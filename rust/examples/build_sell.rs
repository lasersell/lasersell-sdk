//! Build an unsigned sell transaction with the Exit API.
//!
//! Before running:
//! - Replace API key, mint, and wallet placeholders.
//! - Set `amount_tokens_atomic` in the mint's smallest unit.
//!
//! This example prints the unsigned transaction as base64 plus optional route
//! metadata returned by the API.

use std::error::Error;

use lasersell_sdk::exit_api::{BuildSellTxRequest, ExitApiClient, SellOutput};
use secrecy::SecretString;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "REPLACE_WITH_API_KEY".to_string();
    let mint = "REPLACE_WITH_MINT".to_string();
    let user_pubkey = "REPLACE_WITH_WALLET_PUBKEY".to_string();
    // Example: for a 6-decimal mint, 1 whole token = 1_000_000 atomic units.
    let amount_tokens_atomic = 1_000_000_u64;
    let slippage_bps = Some(2_000_u16); // 2,000 bps = 20%

    let client = ExitApiClient::with_api_key(SecretString::new(api_key))?;

    let request = BuildSellTxRequest {
        mint,
        user_pubkey,
        amount_tokens: amount_tokens_atomic,
        slippage_bps,
        mode: None,
        output: Some(SellOutput::Sol),
        referral_id: None,
        market_context: None,
    };

    let response = client.build_sell_tx(&request).await?;
    println!("unsigned_tx_b64={}", response.tx);
    if let Some(route) = response.route {
        println!("route={route}");
    }

    Ok(())
}
