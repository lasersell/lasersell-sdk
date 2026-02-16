use std::error::Error;

use lasersell_sdk::stream::client::{StreamClient, StreamConfigure};
use lasersell_sdk::stream::proto::{ServerMessage, StrategyConfigMsg};
use lasersell_sdk::stream::session::{StreamEvent, StreamSession};
use lasersell_sdk::tx::{send_via_helius_sender, sign_unsigned_tx};
use secrecy::SecretString;
use solana_sdk::signature::read_keypair_file;

fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "REPLACE_WITH_API_KEY".to_string();
    let wallet_pubkeys = vec![
        "REPLACE_WITH_WALLET_PUBKEY_1".to_string(),
        "REPLACE_WITH_WALLET_PUBKEY_2".to_string(),
    ];
    let keypair_path = "REPLACE_WITH_KEYPAIR_PATH".to_string();
    let close_after_submit = false;

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

        let keypair = read_keypair_file(keypair_path)?;
        let http = reqwest::Client::builder().no_proxy().build()?;
        let mut session = StreamSession::connect(&client, configure).await?;

        loop {
            let Some(event) = session.recv().await else {
                return Err("stream ended unexpectedly".into());
            };

            match event {
                StreamEvent::PositionOpened { handle, .. } => {
                    println!(
                        "tracked position wallet={} mint={} token_account={}",
                        handle.wallet_pubkey, handle.mint, handle.token_account
                    );
                }
                StreamEvent::ExitSignalWithTx {
                    handle,
                    message:
                        ServerMessage::ExitSignalWithTx {
                            position_id,
                            wallet_pubkey,
                            mint,
                            unsigned_tx_b64,
                            ..
                        },
                } => {
                    let signed_tx = sign_unsigned_tx(&unsigned_tx_b64, &keypair)?;
                    let signature = send_via_helius_sender(&http, &signed_tx).await?;
                    println!(
                        "submitted exit tx signature={signature} wallet={wallet_pubkey} mint={mint}"
                    );

                    if close_after_submit {
                        if let Some(handle) = handle {
                            session.close(&handle)?;
                        } else {
                            session.sender().close_by_id(position_id)?;
                        }
                    }
                }
                StreamEvent::Message(ServerMessage::Error { code, message }) => {
                    eprintln!("stream error code={code} message={message}");
                }
                _ => {}
            }
        }
    })
}
