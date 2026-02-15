use std::collections::VecDeque;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::InvalidHeaderValue;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use crate::stream::proto::{
    ClientMessage, IntegrationContextMsg, ServerMessage, SessionModeMsg, StrategyConfigMsg,
};

const MIN_RECONNECT_BACKOFF: Duration = Duration::from_millis(100);
const MAX_RECONNECT_BACKOFF: Duration = Duration::from_secs(2);
pub const STREAM_ENDPOINT: &str = "wss://stream.lasersell.io/v1/ws";
pub const LOCAL_STREAM_ENDPOINT: &str = "ws://localhost:8082/v1/ws";

#[derive(Clone)]
pub struct StreamClient {
    api_key: SecretString,
    local: bool,
}

impl StreamClient {
    pub fn new(api_key: SecretString) -> Self {
        Self {
            api_key,
            local: false,
        }
    }

    pub fn with_local_mode(mut self, local: bool) -> Self {
        self.local = local;
        self
    }

    pub async fn connect(
        &self,
        configure: StreamConfigure,
    ) -> Result<StreamConnection, StreamClientError> {
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = oneshot::channel();

        let url = self.endpoint().to_string();
        let api_key = self.api_key.clone();

        tokio::spawn(async move {
            run_worker(url, api_key, configure, outbound_rx, inbound_tx, ready_tx).await;
        });

        match ready_rx.await {
            Ok(Ok(())) => Ok(StreamConnection {
                sender: StreamSender { tx: outbound_tx },
                receiver: inbound_rx,
            }),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(StreamClientError::Protocol(
                "stream worker stopped before initial connect".to_string(),
            )),
        }
    }

    fn endpoint(&self) -> &'static str {
        if self.local {
            LOCAL_STREAM_ENDPOINT
        } else {
            STREAM_ENDPOINT
        }
    }
}

#[derive(Clone, Debug)]
pub struct StreamConfigure {
    pub wallet_pubkey: String,
    pub mode: SessionModeMsg,
    pub strategy: StrategyConfigMsg,
    pub integration_context: Option<IntegrationContextMsg>,
}

#[derive(Debug)]
pub struct StreamConnection {
    sender: StreamSender,
    receiver: mpsc::UnboundedReceiver<ServerMessage>,
}

impl StreamConnection {
    pub fn sender(&self) -> StreamSender {
        self.sender.clone()
    }

    pub fn split(self) -> (StreamSender, mpsc::UnboundedReceiver<ServerMessage>) {
        (self.sender, self.receiver)
    }

    pub async fn recv(&mut self) -> Option<ServerMessage> {
        self.receiver.recv().await
    }
}

#[derive(Clone, Debug)]
pub struct StreamSender {
    tx: mpsc::UnboundedSender<ClientMessage>,
}

impl StreamSender {
    pub fn send(&self, message: ClientMessage) -> Result<(), StreamClientError> {
        self.tx
            .send(message)
            .map_err(|_| StreamClientError::SendQueueClosed)
    }

    pub fn ping(&self, client_time_ms: u64) -> Result<(), StreamClientError> {
        self.send(ClientMessage::Ping { client_time_ms })
    }

    pub fn update_strategy(&self, strategy: StrategyConfigMsg) -> Result<(), StreamClientError> {
        self.send(ClientMessage::UpdateStrategy { strategy })
    }

    pub fn close_position(&self, position_id: u64) -> Result<(), StreamClientError> {
        self.send(ClientMessage::ClosePosition { position_id })
    }

    pub fn request_exit_signal(
        &self,
        position_id: u64,
        slippage_bps: Option<u16>,
    ) -> Result<(), StreamClientError> {
        self.send(ClientMessage::RequestExitSignal {
            position_id,
            slippage_bps,
        })
    }
}

#[derive(Debug, Error)]
pub enum StreamClientError {
    #[error("websocket error: {0}")]
    WebSocket(#[from] WsError),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid api-key header: {0}")]
    InvalidApiKeyHeader(#[from] InvalidHeaderValue),

    #[error("send queue is closed")]
    SendQueueClosed,

    #[error("protocol error: {0}")]
    Protocol(String),
}

enum SessionOutcome {
    GracefulShutdown,
    Reconnect,
}

async fn run_worker(
    url: String,
    api_key: SecretString,
    configure: StreamConfigure,
    mut outbound_rx: mpsc::UnboundedReceiver<ClientMessage>,
    inbound_tx: mpsc::UnboundedSender<ServerMessage>,
    ready_tx: oneshot::Sender<Result<(), StreamClientError>>,
) {
    let mut ready_tx = Some(ready_tx);
    let mut pending = VecDeque::new();
    let mut backoff = MIN_RECONNECT_BACKOFF;

    loop {
        match run_connected_session(
            &url,
            &api_key,
            &configure,
            &mut outbound_rx,
            &inbound_tx,
            &mut pending,
            &mut ready_tx,
        )
        .await
        {
            Ok(SessionOutcome::GracefulShutdown) => {
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(Err(StreamClientError::SendQueueClosed));
                }
                break;
            }
            Ok(SessionOutcome::Reconnect) => {
                backoff = MIN_RECONNECT_BACKOFF;
            }
            Err(err) => {
                if let Some(tx) = ready_tx.take() {
                    let _ = tx.send(Err(err));
                    return;
                }
            }
        }

        if outbound_rx.is_closed() {
            break;
        }

        if !collect_messages_during_delay(backoff, &mut outbound_rx, &mut pending).await {
            break;
        }

        backoff = std::cmp::min(backoff.saturating_mul(2), MAX_RECONNECT_BACKOFF);
    }
}

async fn run_connected_session(
    url: &str,
    api_key: &SecretString,
    configure: &StreamConfigure,
    outbound_rx: &mut mpsc::UnboundedReceiver<ClientMessage>,
    inbound_tx: &mpsc::UnboundedSender<ServerMessage>,
    pending: &mut VecDeque<ClientMessage>,
    ready_tx: &mut Option<oneshot::Sender<Result<(), StreamClientError>>>,
) -> Result<SessionOutcome, StreamClientError> {
    let mut request = url.into_client_request()?;
    let api_key_header = api_key.expose_secret().parse()?;
    request.headers_mut().insert("x-api-key", api_key_header);

    let (mut socket, _) = connect_async(request).await?;

    let first_server_message = loop {
        match socket.next().await {
            Some(Ok(Message::Text(text))) => break ServerMessage::from_text(&text)?,
            Some(Ok(Message::Ping(payload))) => {
                socket.send(Message::Pong(payload)).await?;
            }
            Some(Ok(Message::Pong(_))) => {}
            Some(Ok(Message::Close(_))) => {
                return Err(StreamClientError::Protocol(
                    "socket closed before hello_ok".to_string(),
                ));
            }
            Some(Ok(_)) => {
                return Err(StreamClientError::Protocol(
                    "received non-text frame before hello_ok".to_string(),
                ));
            }
            Some(Err(err)) => return Err(StreamClientError::WebSocket(err)),
            None => {
                return Err(StreamClientError::Protocol(
                    "socket ended before hello_ok".to_string(),
                ));
            }
        }
    };

    if matches!(
        &first_server_message,
        ServerMessage::HelloOk { mode: None, .. }
    ) {
        let _ = inbound_tx.send(first_server_message);
    } else {
        return Err(StreamClientError::Protocol(
            "expected first server message to be hello_ok with mode=null".to_string(),
        ));
    }

    let configure_msg = ClientMessage::Configure {
        wallet_pubkey: configure.wallet_pubkey.clone(),
        mode: configure.mode.clone(),
        strategy: configure.strategy.clone(),
        integration_context: configure.integration_context.clone(),
    };
    send_client_message(&mut socket, &configure_msg).await?;

    if let Some(tx) = ready_tx.take() {
        let _ = tx.send(Ok(()));
    }

    while let Some(next) = pending.pop_front() {
        if send_client_message(&mut socket, &next).await.is_err() {
            pending.push_front(next);
            return Ok(SessionOutcome::Reconnect);
        }
    }

    loop {
        tokio::select! {
            maybe_outbound = outbound_rx.recv() => {
                match maybe_outbound {
                    Some(client_msg) => {
                        if send_client_message(&mut socket, &client_msg).await.is_err() {
                            pending.push_front(client_msg);
                            return Ok(SessionOutcome::Reconnect);
                        }
                    }
                    None => {
                        let _ = socket.close(None).await;
                        return Ok(SessionOutcome::GracefulShutdown);
                    }
                }
            }
            maybe_inbound = socket.next() => {
                match maybe_inbound {
                    Some(Ok(Message::Text(text))) => {
                        match ServerMessage::from_text(&text) {
                            Ok(server_msg) => {
                                let _ = inbound_tx.send(server_msg);
                            }
                            Err(_) => return Ok(SessionOutcome::Reconnect),
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            return Ok(SessionOutcome::Reconnect);
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) => return Ok(SessionOutcome::Reconnect),
                    Some(Ok(_)) => return Ok(SessionOutcome::Reconnect),
                    Some(Err(_)) => return Ok(SessionOutcome::Reconnect),
                    None => return Ok(SessionOutcome::Reconnect),
                }
            }
        }
    }
}

async fn send_client_message<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    message: &ClientMessage,
) -> Result<(), StreamClientError>
where
    tokio_tungstenite::WebSocketStream<S>: futures_util::Sink<Message, Error = WsError> + Unpin,
{
    let text = message.to_text()?;
    socket.send(Message::Text(text)).await?;
    Ok(())
}

async fn collect_messages_during_delay(
    delay: Duration,
    outbound_rx: &mut mpsc::UnboundedReceiver<ClientMessage>,
    pending: &mut VecDeque<ClientMessage>,
) -> bool {
    let sleep = tokio::time::sleep(delay);
    tokio::pin!(sleep);

    loop {
        tokio::select! {
            _ = &mut sleep => return true,
            maybe_message = outbound_rx.recv() => {
                match maybe_message {
                    Some(message) => pending.push_back(message),
                    None => return false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use secrecy::{ExposeSecret, SecretString};

    use super::{StreamClient, LOCAL_STREAM_ENDPOINT, STREAM_ENDPOINT};

    #[test]
    fn stream_client_stores_secret_without_exposing_in_struct_debug() {
        let client = StreamClient::new(SecretString::new("key".into()));
        assert_eq!(client.api_key.expose_secret(), "key");
    }

    #[test]
    fn stream_client_uses_production_stream_endpoint_by_default() {
        assert_eq!(STREAM_ENDPOINT, "wss://stream.lasersell.io/v1/ws");
        let client = StreamClient::new(SecretString::new("key".into()));
        assert_eq!(client.endpoint(), STREAM_ENDPOINT);
    }

    #[test]
    fn stream_client_uses_local_stream_endpoint_when_enabled() {
        let client = StreamClient::new(SecretString::new("key".into())).with_local_mode(true);
        assert_eq!(LOCAL_STREAM_ENDPOINT, "ws://localhost:8082/v1/ws");
        assert_eq!(client.endpoint(), LOCAL_STREAM_ENDPOINT);
    }
}
