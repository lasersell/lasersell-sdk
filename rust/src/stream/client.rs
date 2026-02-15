use std::collections::VecDeque;
use std::time::Duration;

use futures_util::{SinkExt, Stream, StreamExt};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::InvalidHeaderValue;
use tokio_tungstenite::tungstenite::{Error as WsError, Message};

use crate::stream::proto::{ClientMessage, ServerMessage, StrategyConfigMsg};

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
            stream_connection_worker(url, api_key, configure, outbound_rx, inbound_tx, ready_tx)
                .await;
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
    pub wallet_pubkeys: Vec<String>,
    pub strategy: StrategyConfigMsg,
}

impl StreamConfigure {
    pub fn single_wallet(wallet_pubkey: impl Into<String>, strategy: StrategyConfigMsg) -> Self {
        Self {
            wallet_pubkeys: vec![wallet_pubkey.into()],
            strategy,
        }
    }
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PositionSelector {
    TokenAccount(String),
    PositionId(u64),
}

pub trait IntoPositionSelector {
    fn into_position_selector(self) -> PositionSelector;
}

impl IntoPositionSelector for PositionSelector {
    fn into_position_selector(self) -> PositionSelector {
        self
    }
}

impl IntoPositionSelector for String {
    fn into_position_selector(self) -> PositionSelector {
        PositionSelector::TokenAccount(self)
    }
}

impl IntoPositionSelector for &String {
    fn into_position_selector(self) -> PositionSelector {
        PositionSelector::TokenAccount(self.clone())
    }
}

impl IntoPositionSelector for &str {
    fn into_position_selector(self) -> PositionSelector {
        PositionSelector::TokenAccount(self.to_string())
    }
}

impl IntoPositionSelector for u64 {
    fn into_position_selector(self) -> PositionSelector {
        PositionSelector::PositionId(self)
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

    pub fn close_position<S>(&self, selector: S) -> Result<(), StreamClientError>
    where
        S: IntoPositionSelector,
    {
        self.send(close_message(selector.into_position_selector()))
    }

    pub fn close_by_id(&self, position_id: u64) -> Result<(), StreamClientError> {
        self.close_position(PositionSelector::PositionId(position_id))
    }

    pub fn request_exit_signal<S>(
        &self,
        selector: S,
        slippage_bps: Option<u16>,
    ) -> Result<(), StreamClientError>
    where
        S: IntoPositionSelector,
    {
        self.send(request_exit_signal_message(
            selector.into_position_selector(),
            slippage_bps,
        ))
    }

    pub fn request_exit_signal_by_id(
        &self,
        position_id: u64,
        slippage_bps: Option<u16>,
    ) -> Result<(), StreamClientError> {
        self.request_exit_signal(PositionSelector::PositionId(position_id), slippage_bps)
    }
}

fn close_message(selector: PositionSelector) -> ClientMessage {
    match selector {
        PositionSelector::TokenAccount(token_account) => ClientMessage::ClosePosition {
            position_id: None,
            token_account: Some(token_account),
        },
        PositionSelector::PositionId(position_id) => ClientMessage::ClosePosition {
            position_id: Some(position_id),
            token_account: None,
        },
    }
}

fn request_exit_signal_message(
    selector: PositionSelector,
    slippage_bps: Option<u16>,
) -> ClientMessage {
    match selector {
        PositionSelector::TokenAccount(token_account) => ClientMessage::RequestExitSignal {
            position_id: None,
            token_account: Some(token_account),
            slippage_bps,
        },
        PositionSelector::PositionId(position_id) => ClientMessage::RequestExitSignal {
            position_id: Some(position_id),
            token_account: None,
            slippage_bps,
        },
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

async fn stream_connection_worker(
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

    let first_server_message = recv_server_message_before_configure(&mut socket).await?;

    if !matches!(&first_server_message, ServerMessage::HelloOk { .. }) {
        return Err(StreamClientError::Protocol(
            "expected first server message to be hello_ok".to_string(),
        ));
    }
    let _ = inbound_tx.send(first_server_message);

    let configure_msg = ClientMessage::Configure {
        wallet_pubkeys: configure.wallet_pubkeys.clone(),
        strategy: configure.strategy.clone(),
    };
    send_client_message(&mut socket, &configure_msg).await?;

    let configured_message = recv_server_message_after_configure(&mut socket).await?;
    let _ = inbound_tx.send(configured_message);

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
                        match parse_server_message(&text) {
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

async fn recv_server_message_before_configure<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> Result<ServerMessage, StreamClientError>
where
    tokio_tungstenite::WebSocketStream<S>: futures_util::Sink<Message, Error = WsError>
        + Stream<Item = Result<Message, WsError>>
        + Unpin,
{
    loop {
        match socket.next().await {
            Some(Ok(Message::Text(text))) => return parse_server_message(&text),
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
    }
}

async fn recv_server_message_after_configure<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> Result<ServerMessage, StreamClientError>
where
    tokio_tungstenite::WebSocketStream<S>: futures_util::Sink<Message, Error = WsError>
        + Stream<Item = Result<Message, WsError>>
        + Unpin,
{
    loop {
        match socket.next().await {
            Some(Ok(Message::Text(text))) => return parse_server_message(&text),
            Some(Ok(Message::Ping(payload))) => {
                socket.send(Message::Pong(payload)).await?;
            }
            Some(Ok(Message::Pong(_))) => {}
            Some(Ok(Message::Close(_))) => {
                return Err(StreamClientError::Protocol(
                    "socket closed before configure acknowledgement".to_string(),
                ));
            }
            Some(Ok(_)) => {
                return Err(StreamClientError::Protocol(
                    "received non-text frame before configure acknowledgement".to_string(),
                ));
            }
            Some(Err(err)) => return Err(StreamClientError::WebSocket(err)),
            None => {
                return Err(StreamClientError::Protocol(
                    "socket ended before configure acknowledgement".to_string(),
                ));
            }
        }
    }
}

fn parse_server_message(text: &str) -> Result<ServerMessage, StreamClientError> {
    serde_json::from_str(text).map_err(StreamClientError::Json)
}

async fn send_client_message<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    message: &ClientMessage,
) -> Result<(), StreamClientError>
where
    tokio_tungstenite::WebSocketStream<S>: futures_util::Sink<Message, Error = WsError> + Unpin,
{
    let text = serde_json::to_string(message)?;
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
