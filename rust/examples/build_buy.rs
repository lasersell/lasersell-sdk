//! Build an unsigned buy transaction with the Exit API.
//!
//! Before running:
//! - Replace API key, mint, and wallet placeholders.
//! - Set `amount` as a human-readable decimal (e.g. 0.001 for 0.001 SOL),
//!   or use `amount_in_total` in atomic units (e.g. lamports for SOL).
//! This example prints the unsigned transaction as base64 plus optional route
//! metadata returned by the API.

use std::error::Error;

use lasersell_sdk::exit_api::{BuildBuyTxRequest, ExitApiClient};
use secrecy::SecretString;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "REPLACE_WITH_API_KEY".to_string();
    let mint = "REPLACE_WITH_MINT".to_string();
    let user_pubkey = "REPLACE_WITH_WALLET_PUBKEY".to_string();
    let slippage_bps = Some(2_000_u16); // 2,000 bps = 20%

    let client = ExitApiClient::with_api_key(SecretString::new(api_key))?;

    let request = BuildBuyTxRequest {
        mint,
        user_pubkey,
        amount: Some(0.001), // 0.001 SOL
        slippage_bps: slippage_bps.unwrap_or(2_000),
        ..Default::default()
    };

    let response = client.build_buy_tx(&request).await?;
    println!("unsigned_tx_b64={}", response.tx);
    if let Some(route) = response.route {
        println!("route={route}");
    }

    Ok(())
}
