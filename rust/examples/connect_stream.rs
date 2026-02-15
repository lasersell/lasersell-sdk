use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use lasersell_sdk::stream::client::{StreamClient, StreamConfigure};
use lasersell_sdk::stream::proto::{ServerMessage, StrategyConfigMsg};
use lasersell_sdk::stream::session::{StreamEvent, StreamSession};
use secrecy::SecretString;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "REPLACE_WITH_API_KEY".to_string();
    let wallet_pubkeys = vec![
        "REPLACE_WITH_WALLET_PUBKEY_1".to_string(),
        "REPLACE_WITH_WALLET_PUBKEY_2".to_string(),
    ];

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let client = StreamClient::new(SecretString::new(api_key));
        let configure = StreamConfigure {
            wallet_pubkeys,
            strategy: StrategyConfigMsg {
                target_profit_pct: 5.0,
                stop_loss_pct: 1.5,
                deadline_timeout_sec: 45,
            },
        };

        let mut session = StreamSession::connect(&client, configure).await?;
        session.sender().ping(now_ms())?;

        while let Some(event) = session.recv().await {
            match event {
                StreamEvent::ExitSignalWithTx { handle, message } => {
                    if let ServerMessage::ExitSignalWithTx { unsigned_tx_b64, .. } = message {
                        if let Some(handle) = handle {
                            println!(
                                "wallet={} mint={} token_account={} unsigned_tx_b64={unsigned_tx_b64}",
                                handle.wallet_pubkey, handle.mint, handle.token_account,
                            );
                        } else {
                            println!("unsigned_tx_b64={unsigned_tx_b64}");
                        }
                        break;
                    }
                }
                StreamEvent::Message(ServerMessage::HelloOk { session_id, .. }) => {
                    println!("connected session_id={session_id}");
                }
                _ => {}
            }
        }

        Ok::<(), Box<dyn Error>>(())
    })
}
