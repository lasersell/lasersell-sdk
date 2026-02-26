# lasersell-sdk (Python)

Python SDK for the LaserSell API.

Modules:

- `exit_api`: build unsigned buy/sell transactions.
- `stream`: websocket client, protocol types, and session helpers.
- `tx`: sign/encode/send Solana transactions.
- `retry`: shared retry helpers.

## Install

```bash
pip install lasersell-sdk
```

From this repository:

```bash
cd lasersell-sdk/python
python -m pip install -e .
```

## Package layout

- `lasersell_sdk.exit_api`: build unsigned buy/sell transactions.
- `lasersell_sdk.stream.client`: websocket transport and command sender.
- `lasersell_sdk.stream.session`: higher-level stream session with position tracking.
- `lasersell_sdk.stream.proto`: protocol types and JSON encode/decode helpers.
- `lasersell_sdk.tx`: sign/encode/send Solana transactions.
- `lasersell_sdk.retry`: retry/backoff and timeout helpers.

## Build a sell transaction

```python
import asyncio

from lasersell_sdk.exit_api import BuildSellTxRequest, ExitApiClient, SellOutput


async def main() -> None:
    client = ExitApiClient.with_api_key("REPLACE_WITH_API_KEY")

    request = BuildSellTxRequest(
        mint="REPLACE_WITH_MINT",
        user_pubkey="REPLACE_WITH_WALLET_PUBKEY",
        amount_tokens=1_000_000,
        slippage_bps=2_000,
        output=SellOutput.SOL,
    )

    response = await client.build_sell_tx(request)
    print(response.tx)


asyncio.run(main())
```

Notes:

- `amount_tokens` is in token atomic units (smallest unit for the mint).
- `slippage_bps` is basis points (`100` = `1%`, `2000` = `20%`).
- Use `client.with_base_url("https://api-dev.example")` to target a custom base URL.

## Stream + auto-sell flow

```python
import asyncio

from solders.keypair import Keypair

from lasersell_sdk.stream.client import (
    StreamClient,
    single_wallet_stream_configure_optional,
)
from lasersell_sdk.stream.session import StreamSession
from lasersell_sdk.tx import SendTargetHeliusSender, send_transaction, sign_unsigned_tx


async def main() -> None:
    signer = Keypair()
    client = StreamClient("REPLACE_WITH_API_KEY")
    session = await StreamSession.connect(
        client,
        single_wallet_stream_configure_optional(
            "REPLACE_WITH_WALLET_PUBKEY",
            deadline_timeout_sec=45,  # timeout-only strategy is valid
        ),
    )

    while True:
        event = await session.recv()
        if event is None:
            break

        if event.type == "exit_signal_with_tx" and event.message.get("type") == "exit_signal_with_tx":
            signed_tx = sign_unsigned_tx(event.message["unsigned_tx_b64"], signer)
            signature = await send_transaction(SendTargetHeliusSender(), signed_tx)
            print(signature)


asyncio.run(main())
```

`deadline_timeout_sec` is enforced client-side by `StreamSession` timers and is not sent as part of wire strategy.
Use `session.update_strategy(...)` when changing strategy so local deadline timers stay in sync (pass `deadline_timeout_sec=` to change local deadline timing).
Use `single_wallet_stream_configure_optional(...)` / `strategy_config_from_optional(...)` to omit TP/SL fields; at least one of take profit, stop loss, trailing stop, or timeout must be enabled.

### Trailing stop

Lock in profits by tracking a high-water mark. When the position's profit drops by the configured percentage of entry cost from its peak, an exit is triggered.

```python
from lasersell_sdk.stream.client import strategy_config_from_optional

# 20% take profit, 10% stop loss, 5% trailing stop
strategy = strategy_config_from_optional(
    take_profit_pct=20.0,
    stop_loss_pct=10.0,
    trailing_stop_pct=5.0,
)
```

Example: with `trailing_stop_pct=5.0` and an entry of 100 SOL, if profit peaks at 30 SOL, an exit triggers when profit falls below 25 SOL.

### Sell on graduation

Automatically exit a position when its token graduates from a bonding curve to a full DEX (e.g. Pump.fun to PumpSwap). Enable by setting `sell_on_graduation` in the optional strategy config:

```python
from lasersell_sdk.stream.client import strategy_config_from_optional

strategy = strategy_config_from_optional(
    take_profit_pct=20.0,
    stop_loss_pct=10.0,
    sell_on_graduation=True,
)
```

When a graduation event is detected the server sells the position on the new market and reports `"graduation"` as the exit reason.

### Update wallets mid-session

Add or remove wallets without reconnecting:

```python
await session.sender().update_wallets(["WALLET_1_PUBKEY", "WALLET_2_PUBKEY"])
```

The server diffs the new list against the current set and registers/unregisters accordingly.

Notes:

- Use `client.with_endpoint("wss://stream-dev.example/v1/ws")` to target a custom stream endpoint.
- `unsigned_tx_b64` from stream events can be signed with `lasersell_sdk.tx.sign_unsigned_tx`.

## RPC endpoint

The SDK ships with the Solana public mainnet-beta RPC as a default so you can get started immediately:

```python
from lasersell_sdk.tx import SendTargetRpc

target = SendTargetRpc()  # uses mainnet-beta public RPC
```

**A private RPC is highly recommended for production** — the public endpoint is rate-limited and unreliable under load. Free private RPC tiers are available from [Helius](https://www.helius.dev/) and [Chainstack](https://chainstack.com/), among others:

```python
target = SendTargetRpc(url="https://your-private-rpc.example.com")
```

## Examples

See `examples/README.md` for setup and script-by-script instructions.

Quick run commands (from `lasersell-sdk/python`):

```bash
python examples/build_buy.py
python examples/build_sell.py
python examples/build_and_send_sell.py
python examples/auto_sell.py
```

Scripts:

- `build_buy.py`
- `build_sell.py`
- `build_and_send_sell.py`
- `auto_sell.py`
