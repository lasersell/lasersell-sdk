# lasersell-sdk

Rust SDK for the LaserSell API.

> **Full documentation:** [docs.lasersell.io/api/sdk/rust](https://docs.lasersell.io/api/sdk/rust)

## Modules

- `exit_api`: Build unsigned [buy](https://docs.lasersell.io/api/exit-api/buy)/[sell](https://docs.lasersell.io/api/exit-api/sell) transactions.
- `stream`: [Exit Intelligence Stream](https://docs.lasersell.io/api/stream/overview) client, protocol types, and session helpers.
- `tx`: [Sign, encode, and submit](https://docs.lasersell.io/api/transactions/signing) Solana transactions.
- `retry`: Shared retry helpers.

## Install

```bash
cargo add lasersell-sdk
```

## Build a sell transaction

```rust
use lasersell_sdk::exit_api::{BuildSellTxRequest, ExitApiClient, SellOutput};
use secrecy::SecretString;

let client = ExitApiClient::with_api_key(SecretString::new("REPLACE_WITH_API_KEY".to_string()))?;

let request = BuildSellTxRequest {
    mint: "REPLACE_WITH_MINT".to_string(),
    user_pubkey: "REPLACE_WITH_WALLET_PUBKEY".to_string(),
    amount_tokens: 1_000_000,
    slippage_bps: Some(2_000),
    output: Some(SellOutput::Sol),
    mode: None,
    market_context: None,
    send_mode: None,
    tip_lamports: None,
};

let response = client.build_sell_tx(&request).await?;
println!("{}", response.tx);
```

## Stream + auto-sell flow

> **Important:** Connect the stream **before** submitting a buy transaction. See [docs.lasersell.io/api/exit-api/buy](https://docs.lasersell.io/api/exit-api/buy) for details.

```rust
use lasersell_sdk::stream::client::{StreamClient, StreamConfigure, strategy_config_from_optional};
use lasersell_sdk::stream::session::{StreamSession, StreamEvent};
use lasersell_sdk::stream::proto::ServerMessage;
use lasersell_sdk::tx::{sign_unsigned_tx, send_transaction, SendTarget};
use secrecy::SecretString;
use solana_sdk::signature::read_keypair_file;

let keypair = read_keypair_file("REPLACE_WITH_KEYPAIR_PATH")?;
let http = reqwest::Client::builder().no_proxy().build()?;

let client = StreamClient::new(SecretString::new("REPLACE_WITH_API_KEY".to_string()));
let configure = StreamConfigure {
    wallet_pubkeys: vec!["REPLACE_WITH_WALLET_PUBKEY".to_string()],
    strategy: strategy_config_from_optional(None, None, None, None),
    deadline_timeout_sec: 45,
};

let mut session = StreamSession::connect(&client, configure).await?;

while let Some(event) = session.recv().await {
    if let StreamEvent::ExitSignalWithTx {
        message: ServerMessage::ExitSignalWithTx { unsigned_tx_b64, .. },
        ..
    } = event
    {
        let signed_tx = sign_unsigned_tx(&unsigned_tx_b64, &keypair)?;
        let signature = send_transaction(&http, &SendTarget::HeliusSender, &signed_tx).await?;
        println!("signature={signature}");
    }
}
```

## Strategy options

The `strategy_config_from_optional` helper builds a `StrategyConfigMsg` from optional fields. At least one exit condition must be enabled: take profit, stop loss, trailing stop, or deadline timeout.

```rust
use lasersell_sdk::stream::client::strategy_config_from_optional;

// Take profit at 20%, stop loss at 10%, trailing stop at 5%
let strategy = strategy_config_from_optional(Some(20.0), Some(10.0), Some(5.0), None);
```

**Trailing stop** locks in profits by tracking a high-water mark. When the position's profit drops by the configured percentage of entry cost from its peak, an exit is triggered. For example, with `trailing_stop_pct = 5.0` and an entry of 100 SOL: if profit peaks at 30 SOL, an exit triggers when profit falls below 25 SOL.

### Sell on graduation

Automatically exit a position when its token graduates from a bonding curve to a full DEX (e.g. Pump.fun to PumpSwap). Enable by setting `sell_on_graduation` on the strategy config:

```rust
use lasersell_sdk::stream::proto::StrategyConfigMsg;

let strategy = StrategyConfigMsg {
    target_profit_pct: 20.0,
    stop_loss_pct: 10.0,
    trailing_stop_pct: 0.0,
    sell_on_graduation: true,
};
```

When a graduation event is detected the server sells the position on the new market and reports `"graduation"` as the exit reason.

## Update wallets mid-session

Add or remove wallets without reconnecting:

```rust
session.sender().update_wallets(vec![
    "WALLET_1_PUBKEY".to_string(),
    "WALLET_2_PUBKEY".to_string(),
]).await?;
```

The server diffs the new list against the current set and registers/unregisters accordingly.

## RPC endpoint

The SDK ships with the Solana public mainnet-beta RPC as a default so you can get started immediately:

```rust
let target = SendTarget::default_rpc();
```

**A private RPC is highly recommended for production** — the public endpoint is rate-limited and unreliable under load. Free private RPC tiers are available from [Helius](https://www.helius.dev/) and [Chainstack](https://chainstack.com/), among others:

```rust
let target = SendTarget::Rpc { url: "https://your-private-rpc.example.com".to_string() };
```

## Examples

See `examples/` for runnable programs:

- `examples/build_buy.rs`: Build unsigned buy tx
- `examples/build_sell.rs`: Build unsigned sell tx
- `examples/build_and_send_sell.rs`: Build, sign, and submit sell tx
- `examples/auto_sell.rs`: Stream listener that signs and submits exits

```bash
cargo run --example build_sell
cargo run --example build_and_send_sell
cargo run --example auto_sell
```

## Error types

- `ExitApiError`: API transport and response errors
- `TxSubmitError`: Transaction signing and submission errors

Both implement `std::error::Error` and can be matched by variant for structured error handling.

## License

[MIT](../LICENSE)
