# lasersell-sdk

Rust SDK for the LaserSell API.

## Modules

- `exit_api`: Build unsigned buy/sell transactions.
- `stream`: Websocket client, protocol types, and session helpers.
- `tx`: Sign, encode, and submit Solana transactions.
- `retry`: Shared retry helpers.

## Install

```toml
[dependencies]
lasersell-sdk = "0.1"
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
    strategy: strategy_config_from_optional(None, None),
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
