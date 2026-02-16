//! Build, sign, and submit a sell transaction.
//!
//! Flow:
//! 1. Request an unsigned sell transaction from the Exit API.
//! 2. Sign it with a local Solana keypair file.
//! 3. Submit it to either a standard RPC endpoint or Helius Sender.
//!
//! Before running:
//! - Replace all placeholders, including `keypair_path`.
//! - Set `amount_tokens_atomic` in the mint's smallest unit.
//! - If `send_target` is `rpc`, set `rpc_url` to a valid endpoint.

use std::error::Error;

use lasersell_sdk::exit_api::{BuildSellTxRequest, ExitApiClient, SellOutput};
use lasersell_sdk::tx::{send_via_helius_sender, send_via_rpc, sign_unsigned_tx};
use secrecy::SecretString;
use solana_sdk::signature::read_keypair_file;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "REPLACE_WITH_API_KEY".to_string();
    let mint = "REPLACE_WITH_MINT".to_string();
    let user_pubkey = "REPLACE_WITH_WALLET_PUBKEY".to_string();
    let keypair_path = "REPLACE_WITH_KEYPAIR_PATH".to_string();
    // "rpc" uses a standard Solana RPC endpoint; "helius_sender" uses Helius Sender.
    let send_target = "rpc".to_string();
    let rpc_url = "REPLACE_WITH_RPC_URL".to_string();

    // Example: for a 6-decimal mint, 1 whole token = 1_000_000 atomic units.
    let amount_tokens_atomic = 1_000_000_u64;
    let slippage_bps = Some(2_000_u16); // 2,000 bps = 20%
    let keypair = read_keypair_file(keypair_path)?;

    let exit_api = ExitApiClient::with_api_key(SecretString::new(api_key))?;
    let http = reqwest::Client::builder().no_proxy().build()?;

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

    let unsigned_tx_b64 = exit_api.build_sell_tx_b64(&request).await?;
    let signed_tx = sign_unsigned_tx(&unsigned_tx_b64, &keypair)?;

    let signature = match send_target.as_str() {
        "helius_sender" => send_via_helius_sender(&http, &signed_tx).await?,
        "rpc" => send_via_rpc(&http, &rpc_url, &signed_tx).await?,
        other => {
            return Err(
                format!("send_target must be `rpc` or `helius_sender` (got `{other}`)").into(),
            )
        }
    };

    println!("signature={signature}");
    Ok(())
}
