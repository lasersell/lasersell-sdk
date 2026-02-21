//! Auto-sell stream example.
//!
//! This program connects to the LaserSell stream, listens for
//! `ExitSignalWithTx` events, signs each unsigned transaction with a local
//! keypair, and submits it through Helius Sender.
//!
//! Before running:
//! - Replace the API key and wallet placeholders below.
//! - Point `keypair_path` to a local Solana keypair file.
//!
//! Behavior notes:
//! - Transaction submission failures are logged and the stream keeps running.
//! - If the stream disconnects, the example returns an error.

use std::error::Error;

use lasersell_sdk::stream::client::{StreamClient, StreamConfigure};
use lasersell_sdk::stream::proto::{ServerMessage, StrategyConfigMsg};
use lasersell_sdk::stream::session::{StreamEvent, StreamSession};
use lasersell_sdk::tx::{send_via_helius_sender, sign_unsigned_tx};
use secrecy::SecretString;
use solana_sdk::signature::read_keypair_file;
use tokio::task::JoinSet;

struct SubmittedExit {
    wallet_pubkey: String,
    mint: String,
    signature: String,
}

/// Logs the result of one finished submission task.
///
/// Submission failures are intentionally non-fatal so one failed send does not
/// stop monitoring and handling other positions.
fn handle_submission_result(result: Result<Result<SubmittedExit, String>, tokio::task::JoinError>) {
    match result {
        Ok(Ok(submission)) => {
            println!(
                "submitted exit tx signature={} wallet={} mint={}",
                submission.signature, submission.wallet_pubkey, submission.mint
            );
        }
        Ok(Err(message)) => {
            eprintln!("submission error: {message}");
        }
        Err(error) => {
            eprintln!("submission task failed: {error}");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "REPLACE_WITH_API_KEY".to_string();
    let wallet_pubkeys = vec![
        "REPLACE_WITH_WALLET_PUBKEY_1".to_string(),
        "REPLACE_WITH_WALLET_PUBKEY_2".to_string(),
    ];
    let keypair_path = "REPLACE_WITH_KEYPAIR_PATH".to_string();
    let close_after_submit = false;

    let client = StreamClient::new(SecretString::new(api_key));
    let configure = StreamConfigure {
        wallet_pubkeys,
        strategy: StrategyConfigMsg {
            target_profit_pct: 5.0,
            stop_loss_pct: 1.5,
        },
        deadline_timeout_sec: 45,
    };

    let keypair = read_keypair_file(keypair_path)?;
    let http = reqwest::Client::builder().no_proxy().build()?;
    let mut session = StreamSession::connect(&client, configure).await?;
    let mut submissions = JoinSet::new();

    while let Some(event) = session.recv().await {
        match event {
            StreamEvent::PositionOpened { handle, .. } => {
                println!(
                    "tracked position wallet={} mint={} token_account={}",
                    handle.wallet_pubkey, handle.mint, handle.token_account
                );
            }
            StreamEvent::ExitSignalWithTx {
                handle: _,
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
                let http = http.clone();
                let sender = session.sender();
                // Submit each exit in its own task so sends can overlap.
                submissions.spawn(async move {
                    let signature = send_via_helius_sender(&http, &signed_tx).await.map_err(
                        |error| {
                            format!(
                                "send failed position_id={position_id} wallet={wallet_pubkey} mint={mint}: {error}"
                            )
                        },
                    )?;

                    if close_after_submit {
                        sender.close_by_id(position_id).map_err(|error| {
                            format!("close failed position_id={position_id}: {error}")
                        })?;
                    }

                    Ok::<SubmittedExit, String>(SubmittedExit {
                        wallet_pubkey,
                        mint,
                        signature,
                    })
                });
            }
            StreamEvent::Message(ServerMessage::Error { code, message }) => {
                eprintln!("stream error code={code} message={message}");
            }
            _ => {}
        }

        // Surface completed task results promptly while the stream is active.
        while let Some(result) = submissions.try_join_next() {
            handle_submission_result(result);
        }
    }

    // Drain any in-flight submissions before exiting on stream termination.
    while let Some(result) = submissions.join_next().await {
        handle_submission_result(result);
    }

    Err("stream ended unexpectedly".into())
}
