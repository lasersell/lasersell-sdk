# lasersell-sdk (Python)

Python SDK for the LaserSell API.

Modules:

- `exit_api`: build unsigned buy/sell transactions.
- `stream`: websocket client, protocol types, and session helpers.
- `tx`: sign/encode/send Solana transactions.
- `retry`: shared retry helpers.

## Install

Published package:

```bash
pip install lasersell-sdk[full]
```

From this repository:

```bash
cd lasersell-sdk/python
python -m pip install -e '.[full]'
```

Install only what you need:

- `lasersell-sdk[stream]` for websocket stream support (`websockets`)
- `lasersell-sdk[tx]` for Solana transaction signing support (`solders`)

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
Use `single_wallet_stream_configure_optional(...)` / `strategy_config_from_optional(...)` to omit TP/SL fields; at least one of take profit, stop loss, or timeout must be enabled.

Notes:

- Use `client.with_endpoint("wss://stream-dev.example/v1/ws")` to target a custom stream endpoint.
- `unsigned_tx_b64` from stream events can be signed with `lasersell_sdk.tx.sign_unsigned_tx`.

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
