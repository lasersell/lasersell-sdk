use std::error::Error;

use lasersell_sdk::stream::client::{StreamClient, StreamConfigure};
use lasersell_sdk::stream::proto::{SessionModeMsg, StrategyConfigMsg};
use secrecy::SecretString;

fn main() -> Result<(), Box<dyn Error>> {
    let api_key = "REPLACE_WITH_API_KEY".to_string();
    let wallet_pubkey = "REPLACE_WITH_WALLET_PUBKEY".to_string();
    let position_id = 1_u64;
    let slippage_bps = None;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let client = StreamClient::new(SecretString::new(api_key));
        let configure = StreamConfigure {
            wallet_pubkey,
            mode: SessionModeMsg::Integration,
            strategy: StrategyConfigMsg {
                target_profit_pct: 5.0,
                stop_loss_pct: 1.5,
                deadline_timeout_sec: 45,
            },
            integration_context: None,
        };

        let connection = client.connect(configure).await?;
        connection
            .sender()
            .request_exit_signal(position_id, slippage_bps)?;

        println!("sent request_exit_signal for position_id={position_id}");
        Ok::<(), Box<dyn Error>>(())
    })
}
