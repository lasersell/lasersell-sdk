//! Build an unsigned buy transaction with the Exit API.
//!
//! Before running:
//! - Replace API key, mint, and wallet placeholders.
//! - Set `amount_quote_atomic` in the quote asset's smallest unit (e.g., lamports if buying with SOL).
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
    // Example: 1_000_000 lamports = 0.001 SOL
    let amount_quote_atomic = 1_000_000_u64;
    let slippage_bps = Some(2_000_u16); // 2,000 bps = 20%

    let client = ExitApiClient::with_api_key(SecretString::new(api_key))?;

    let request = BuildBuyTxRequest {
        mint,
        user_pubkey,
        amount_quote_units: amount_quote_atomic,
        slippage_bps,
        mode: None,

        send_mode: None,
        tip_lamports: None,
    };

    let response = client.build_buy_tx(&request).await?;
    println!("unsigned_tx_b64={}", response.tx);
    if let Some(route) = response.route {
        println!("route={route}");
    }

    Ok(())
}
