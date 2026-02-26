use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::StreamExt;
use lasersell_sdk::exit_api::{BuildSellTxRequest, ExitApiClient, SellOutput};
use lasersell_sdk::stream::client::{StreamClient, StreamConfigure};
use lasersell_sdk::stream::proto::{ClientMessage, LimitsMsg, ServerMessage, StrategyConfigMsg};
use secrecy::SecretString;
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;

const TEST_API_KEY: &str = "test-api-key";
const TEST_WALLET: &str = "11111111111111111111111111111111";
const TEST_MINT: &str = "So11111111111111111111111111111111111111112";
const TEST_POSITION_ID: u64 = 42;
const TEST_SLIPPAGE_BPS: Option<u16> = Some(1234);
const DUMMY_TX_B64: &str = "dGVzdF90eA==";

#[derive(Debug)]
struct WsObserved {
    wallet_pubkey: String,
    position_id: u64,
    slippage_bps: Option<u16>,
}

#[derive(Clone)]
struct WsState {
    expected_api_key: String,
    expected_wallet_pubkey: String,
    expected_strategy: StrategyConfigMsg,
    expected_position_id: u64,
    expected_slippage_bps: Option<u16>,
    observed_tx: Arc<Mutex<Option<oneshot::Sender<Result<WsObserved, String>>>>>,
}

#[derive(Clone)]
struct HttpState {
    expected_api_key: String,
    observed_tx: Arc<Mutex<Option<oneshot::Sender<Result<Value, String>>>>>,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "SDK clients now use fixed endpoints (production/local), so mock URL injection is disabled"]
async fn stream_client_ws_smoke_connect_configure_request_exit_signal() {
    let expected_strategy = StrategyConfigMsg {
        target_profit_pct: 5.0,
        stop_loss_pct: 1.5,
        trailing_stop_pct: 0.0,
        sell_on_graduation: false,
    };
    let (observed_tx, observed_rx) = oneshot::channel();
    let ws_state = WsState {
        expected_api_key: TEST_API_KEY.to_string(),
        expected_wallet_pubkey: TEST_WALLET.to_string(),
        expected_strategy: expected_strategy.clone(),
        expected_position_id: TEST_POSITION_ID,
        expected_slippage_bps: TEST_SLIPPAGE_BPS,
        observed_tx: Arc::new(Mutex::new(Some(observed_tx))),
    };

    let app = Router::new()
        .route("/v1/ws", get(ws_handler))
        .with_state(ws_state);
    let (_addr, shutdown_tx, server_task) = spawn_server(app).await;

    let client = StreamClient::new(SecretString::new(TEST_API_KEY.to_string()));
    let mut connection = client
        .connect(StreamConfigure {
            wallet_pubkeys: vec![TEST_WALLET.to_string()],
            strategy: expected_strategy,
            deadline_timeout_sec: 45,
        })
        .await
        .expect("connect stream client to mock ws server");

    connection
        .sender()
        .request_exit_signal(TEST_POSITION_ID, TEST_SLIPPAGE_BPS)
        .expect("queue request_exit_signal");

    let mut saw_hello_ok = false;
    let mut saw_exit_signal_with_tx = false;
    for _ in 0..4 {
        let maybe_msg = timeout(Duration::from_secs(2), connection.recv())
            .await
            .expect("timed out waiting for stream message");
        let Some(msg) = maybe_msg else {
            break;
        };
        match msg {
            ServerMessage::HelloOk { .. } => saw_hello_ok = true,
            ServerMessage::ExitSignalWithTx {
                unsigned_tx_b64, ..
            } => {
                saw_exit_signal_with_tx = true;
                assert_eq!(unsigned_tx_b64, DUMMY_TX_B64);
                break;
            }
            _ => {}
        }
    }
    assert!(saw_hello_ok, "expected hello_ok from server");
    assert!(
        saw_exit_signal_with_tx,
        "expected exit_signal_with_tx from server"
    );

    let observed = timeout(Duration::from_secs(2), observed_rx)
        .await
        .expect("timed out waiting for ws server observations")
        .expect("ws observation channel closed")
        .expect("ws protocol assertions failed");
    assert_eq!(observed.wallet_pubkey, TEST_WALLET);
    assert_eq!(observed.position_id, TEST_POSITION_ID);
    assert_eq!(observed.slippage_bps, TEST_SLIPPAGE_BPS);

    let _ = shutdown_tx.send(());
    server_task.await.expect("mock ws server task should join");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "SDK clients now use fixed endpoints (production/local), so mock URL injection is disabled"]
async fn exit_api_client_smoke_posts_required_sell_fields_and_parses_envelope() {
    let (observed_tx, observed_rx) = oneshot::channel();
    let http_state = HttpState {
        expected_api_key: TEST_API_KEY.to_string(),
        observed_tx: Arc::new(Mutex::new(Some(observed_tx))),
    };
    let app = Router::new()
        .route("/v1/sell", post(exit_api_sell_handler))
        .with_state(http_state);
    let (_addr, shutdown_tx, server_task) = spawn_server(app).await;

    let client = ExitApiClient::with_api_key(SecretString::new(TEST_API_KEY.to_string()))
        .expect("build exit api client");

    let request = BuildSellTxRequest {
        mint: TEST_MINT.to_string(),
        user_pubkey: TEST_WALLET.to_string(),
        amount_tokens: 25,
        slippage_bps: Some(1500),
        mode: None,
        output: Some(SellOutput::Sol),

        market_context: None,
        send_mode: None,
        tip_lamports: None,
    };

    let response = client
        .build_sell_tx(&request)
        .await
        .expect("build_sell_tx should parse status=ok envelope");
    assert_eq!(response.tx, DUMMY_TX_B64);

    let observed = timeout(Duration::from_secs(2), observed_rx)
        .await
        .expect("timed out waiting for http observation")
        .expect("http observation channel closed")
        .expect("http payload assertions failed");
    assert_eq!(
        observed.get("mint").and_then(Value::as_str),
        Some(TEST_MINT),
        "mint must be present"
    );
    assert_eq!(
        observed.get("user_pubkey").and_then(Value::as_str),
        Some(TEST_WALLET),
        "user_pubkey must be present"
    );
    assert_eq!(
        observed.get("amount_tokens").and_then(Value::as_u64),
        Some(25),
        "amount_tokens must be present"
    );
    assert_eq!(
        observed.get("slippage_bps").and_then(Value::as_u64),
        Some(1500),
        "slippage_bps must be present"
    );
    assert_eq!(
        observed.get("output").and_then(Value::as_str),
        Some("SOL"),
        "output must be present"
    );

    let _ = shutdown_tx.send(());
    server_task
        .await
        .expect("mock http server task should join");
}

async fn ws_handler(
    State(state): State<WsState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let api_key_matches = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.expected_api_key);
    if !api_key_matches {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let expected_wallet = state.expected_wallet_pubkey.clone();
    let expected_strategy = state.expected_strategy.clone();
    let expected_position_id = state.expected_position_id;
    let expected_slippage_bps = state.expected_slippage_bps;
    let observed_tx = state.observed_tx.clone();

    ws.on_upgrade(move |socket| async move {
        let result = run_ws_protocol(
            socket,
            expected_wallet,
            expected_strategy,
            expected_position_id,
            expected_slippage_bps,
        )
        .await;
        if let Some(tx) = observed_tx.lock().await.take() {
            let _ = tx.send(result);
        }
    })
    .into_response()
}

async fn run_ws_protocol(
    mut socket: WebSocket,
    expected_wallet: String,
    expected_strategy: StrategyConfigMsg,
    expected_position_id: u64,
    expected_slippage_bps: Option<u16>,
) -> Result<WsObserved, String> {
    send_server_message(
        &mut socket,
        ServerMessage::HelloOk {
            session_id: 7,
            server_time_ms: 1_700_000_000_000,
            limits: LimitsMsg {
                hi_capacity: 256,
                pnl_flush_ms: 100,
                max_positions_per_session: 256,
                max_wallets_per_session: 8,
                max_positions_per_wallet: 64,
                max_sessions_per_api_key: 1,
            },
        },
    )
    .await?;

    let configure = recv_client_message(&mut socket).await?;
    let (wallet_pubkeys, strategy) = match configure {
        ClientMessage::Configure {
            strategy,
            wallet_pubkeys,
        } => (wallet_pubkeys, strategy),
        other => {
            return Err(format!(
                "expected first client message to be configure, got {other:?}"
            ));
        }
    };
    if wallet_pubkeys != vec![expected_wallet.clone()] {
        return Err("configure wallet_pubkeys did not match expected value".to_string());
    }
    if strategy != expected_strategy {
        return Err("configure strategy did not match expected value".to_string());
    }

    let request_exit_signal = recv_client_message(&mut socket).await?;
    let (position_id, token_account, slippage_bps) = match request_exit_signal {
        ClientMessage::RequestExitSignal {
            position_id,
            token_account,
            slippage_bps,
        } => (position_id, token_account, slippage_bps),
        other => {
            return Err(format!(
                "expected second client message to be request_exit_signal, got {other:?}"
            ));
        }
    };
    if position_id != Some(expected_position_id) {
        return Err("request_exit_signal position_id did not match expected value".to_string());
    }
    if token_account.is_some() {
        return Err(
            "request_exit_signal should not include token_account in smoke test".to_string(),
        );
    }
    if slippage_bps != expected_slippage_bps {
        return Err("request_exit_signal slippage_bps did not match expected value".to_string());
    }

    send_server_message(
        &mut socket,
        ServerMessage::ExitSignalWithTx {
            session_id: 7,
            position_id: expected_position_id,
            wallet_pubkey: expected_wallet.clone(),
            mint: TEST_MINT.to_string(),
            token_account: None,
            token_program: None,
            position_tokens: 25,
            profit_units: 12,
            reason: "manual".to_string(),
            triggered_at_ms: 1_700_000_000_001,
            market_context: None,
            unsigned_tx_b64: DUMMY_TX_B64.to_string(),
        },
    )
    .await?;

    Ok(WsObserved {
        wallet_pubkey: expected_wallet,
        position_id: expected_position_id,
        slippage_bps,
    })
}

async fn recv_client_message(socket: &mut WebSocket) -> Result<ClientMessage, String> {
    loop {
        match socket.next().await {
            Some(Ok(Message::Text(text))) => {
                return ClientMessage::from_text(text.as_ref())
                    .map_err(|err| format!("failed to decode client message: {err}"));
            }
            Some(Ok(Message::Ping(payload))) => {
                socket
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|err| format!("failed to send pong: {err}"))?;
            }
            Some(Ok(Message::Pong(_))) => {}
            Some(Ok(Message::Close(_))) => {
                return Err("websocket closed before expected client message".to_string());
            }
            Some(Ok(_)) => return Err("received unexpected non-text websocket frame".to_string()),
            Some(Err(err)) => return Err(format!("websocket receive error: {err}")),
            None => return Err("websocket stream ended unexpectedly".to_string()),
        }
    }
}

async fn send_server_message(socket: &mut WebSocket, msg: ServerMessage) -> Result<(), String> {
    let payload = msg
        .to_text()
        .map_err(|err| format!("failed to encode server message: {err}"))?;
    socket
        .send(Message::Text(payload.into()))
        .await
        .map_err(|err| format!("failed to send server message: {err}"))
}

async fn exit_api_sell_handler(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let api_key_matches = headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == state.expected_api_key);
    if !api_key_matches {
        if let Some(tx) = state.observed_tx.lock().await.take() {
            let _ = tx.send(Err("missing or invalid x-api-key".to_string()));
        }
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"status":"error","message":"unauthorized"})),
        );
    }

    let required_ok = payload.get("output").and_then(Value::as_str).is_some()
        && payload
            .get("slippage_bps")
            .and_then(Value::as_u64)
            .is_some()
        && payload
            .get("amount_tokens")
            .and_then(Value::as_u64)
            .is_some()
        && payload.get("mint").and_then(Value::as_str).is_some()
        && payload.get("user_pubkey").and_then(Value::as_str).is_some();

    if !required_ok {
        if let Some(tx) = state.observed_tx.lock().await.take() {
            let _ = tx.send(Err(
                "missing required fields: output, slippage_bps, amount_tokens, mint, user_pubkey"
                    .to_string(),
            ));
        }
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"status":"error","message":"missing required fields"})),
        );
    }

    if let Some(tx) = state.observed_tx.lock().await.take() {
        let _ = tx.send(Ok(payload));
    }

    (
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "tx": DUMMY_TX_B64
        })),
    )
}

async fn spawn_server(
    app: Router,
) -> (SocketAddr, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock server listener");
    let addr = listener
        .local_addr()
        .expect("read mock server listener address");
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("mock server should run");
    });
    (addr, shutdown_tx, task)
}
