//! Demonstrates wallet registration (required for all stream connections)
//! and copy trading with watch wallets.
//!
//! Demonstrates:
//! 1. Proving wallet ownership with `prove_ownership` (local Ed25519 signature).
//! 2. Registering the wallet with the LaserSell API.
//! 3. Connecting with `connect_with_wallets`.
//! 4. Copy trading via watch wallets.
//! 5. Per-position strategy overrides.

use std::error::Error;

use lasersell_sdk::exit_api::{prove_ownership, ExitApiClient};
use lasersell_sdk::stream::client::{StreamClient, StrategyConfigBuilder};
use lasersell_sdk::stream::proto::{ServerMessage, TakeProfitLevelMsg, WatchWalletEntryMsg};
use lasersell_sdk::stream::session::{StreamEvent, StreamSession};
use lasersell_sdk::tx::{send_transaction, sign_unsigned_tx, SendTarget};
use secrecy::SecretString;
use solana_sdk::signature::read_keypair_file;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "REPLACE_WITH_API_KEY".to_string();
    let keypair_path = "REPLACE_WITH_KEYPAIR_PATH";
    let watch_wallet_pubkey = "REPLACE_WITH_WATCH_WALLET_PUBKEY";

    let keypair = read_keypair_file(keypair_path)?;
    let http = reqwest::Client::builder().no_proxy().build()?;

    // 1. Prove ownership (local, no network call)
    let proof = prove_ownership(&keypair);
    println!("Wallet proof generated");

    // 2. Register wallet with the API
    let api_client = ExitApiClient::with_api_key(SecretString::new(api_key.clone()))?;
    api_client.register_wallet(&proof, Some("My Trading Wallet")).await?;
    println!("Wallet registered");

    // 3. Build strategy with exit ladder
    let strategy = StrategyConfigBuilder::new()
        .stop_loss_pct(10.0)
        .take_profit_levels(vec![
            TakeProfitLevelMsg { profit_pct: 25.0, sell_pct: 30.0, trailing_stop_pct: 0.0 },
            TakeProfitLevelMsg { profit_pct: 50.0, sell_pct: 50.0, trailing_stop_pct: 3.0 },
            TakeProfitLevelMsg { profit_pct: 100.0, sell_pct: 100.0, trailing_stop_pct: 5.0 },
        ])
        .liquidity_guard(true)
        .breakeven_trail_pct(2.0)
        .build();

    // 4. Connect with registered wallet
    let stream_client = StreamClient::new(SecretString::new(api_key));
    let mut session = stream_client
        .connect_with_wallets(&[proof], strategy, 120)
        .await?;
    println!("Stream connected");

    // 5. Add a watch wallet for copy trading (optional — only needed if mirroring external wallets)
    session.sender().update_watch_wallets(vec![
        WatchWalletEntryMsg {
            pubkey: watch_wallet_pubkey.to_string(),
            auto_buy: None,
        },
    ])?;
    println!("Watching wallet: {watch_wallet_pubkey}");

    // 6. Event loop
    while let Some(event) = session.recv().await {
        match event {
            StreamEvent::PositionOpened { handle, message, .. } => {
                let watched = match &message {
                    ServerMessage::PositionOpened { watched, .. } => watched.unwrap_or(false),
                    _ => false,
                };
                println!("Position opened: {} (watched: {})", handle.mint, watched);

                // Apply tighter strategy to copied positions
                if watched {
                    session.sender().update_position_strategy(
                        handle.position_id,
                        lasersell_sdk::stream::proto::StrategyConfigMsg {
                            target_profit_pct: 30.0,
                            stop_loss_pct: 8.0,
                            trailing_stop_pct: 5.0,
                            ..Default::default()
                        },
                    )?;
                    println!("Tighter strategy set for copied position {}", handle.position_id);
                }
            }
            StreamEvent::ExitSignalWithTx {
                message: ServerMessage::ExitSignalWithTx { unsigned_tx_b64, .. },
                ..
            } => {
                let signed_tx = sign_unsigned_tx(&unsigned_tx_b64, &keypair)?;
                let signature =
                    send_transaction(&http, &SendTarget::HeliusSender, &signed_tx).await?;
                println!("Exit: {signature}");
            }
            _ => {}
        }
    }

    Err("stream ended unexpectedly".into())
}
