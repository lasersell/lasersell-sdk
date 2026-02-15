use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use lasersell_sdk::stream::client::{StreamClient, StreamConfigure};
use lasersell_sdk::stream::proto::{ServerMessage, SessionModeMsg, StrategyConfigMsg};
use secrecy::SecretString;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "REPLACE_WITH_API_KEY".to_string();
    let wallet_pubkey = "REPLACE_WITH_WALLET_PUBKEY".to_string();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let client = StreamClient::new(SecretString::new(api_key));
        let configure = StreamConfigure {
            wallet_pubkey,
            mode: SessionModeMsg::Integration,
            strategy: StrategyConfigMsg {
                target_profit_pct: 5.0,
                stop_loss_pct: 1.5,
                deadline_timeout_sec: 45,
            },
            integration_context: None,
        };

        let mut connection = client.connect(configure).await?;
        connection.sender().ping(now_ms())?;

        while let Some(message) = connection.recv().await {
            if let ServerMessage::ExitSignalWithTx {
                position_id,
                unsigned_tx_b64,
                ..
            } = message
            {
                println!("position_id={position_id} unsigned_tx_b64={unsigned_tx_b64}");
                break;
            }
        }

        Ok::<(), Box<dyn Error>>(())
    })
}
