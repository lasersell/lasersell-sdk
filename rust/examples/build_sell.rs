use std::error::Error;

use lasersell_sdk::exit_api::{BuildSellTxRequest, ExitApiClient, SellOutput};
use secrecy::SecretString;

fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "REPLACE_WITH_API_KEY".to_string();
    let mint = "REPLACE_WITH_MINT".to_string();
    let user_pubkey = "REPLACE_WITH_WALLET_PUBKEY".to_string();
    let amount_tokens = 1_u64;
    let slippage_bps = None;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let client = ExitApiClient::with_api_key(SecretString::new(api_key))?;

        let request = BuildSellTxRequest {
            mint,
            user_pubkey,
            amount_tokens,
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

        Ok::<(), Box<dyn Error>>(())
    })
}
