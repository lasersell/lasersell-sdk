use std::error::Error;

use lasersell_sdk::stream::client::{StreamClient, StreamConfigure};
use lasersell_sdk::stream::proto::StrategyConfigMsg;
use secrecy::SecretString;

fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "REPLACE_WITH_API_KEY".to_string();
    let wallet_pubkeys = vec![
        "REPLACE_WITH_WALLET_PUBKEY_1".to_string(),
        "REPLACE_WITH_WALLET_PUBKEY_2".to_string(),
    ];
    let token_account = "REPLACE_WITH_TOKEN_ACCOUNT".to_string();
    let fallback_position_id = None::<u64>;
    let slippage_bps = None;

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

        let connection = client.connect(configure).await?;
        connection
            .sender()
            .request_exit_signal(&token_account, slippage_bps)?;
        connection.sender().close_position(&token_account)?;

        if let Some(position_id) = fallback_position_id {
            connection.sender().close_by_id(position_id)?;
            println!("sent close_position fallback selector=position_id position_id={position_id}");
        }

        println!("sent request_exit_signal selector=token_account token_account={token_account}");
        println!("sent close_position selector=token_account token_account={token_account}");
        Ok::<(), Box<dyn Error>>(())
    })
}
