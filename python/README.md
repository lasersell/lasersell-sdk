# lasersell-sdk (Python)

Python SDK for LaserSell Exit API and stream websocket.

This package ports the Rust SDK surfaces:

- `exit_api`: build unsigned buy/sell transactions.
- `stream`: websocket client, protocol types, and session helpers.
- `tx`: sign/encode/send Solana transactions.
- `retry`: shared retry helpers.

`stream/proto` types from `lasersell-stream-proto` are included directly in this SDK (not a separate package).

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

## Exit API (build sell)

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
- Use `client.with_local_mode(True)` for local Exit API development (`http://localhost:8080`).
- Use `client.with_base_url("https://api-dev.example")` to target a custom Exit API base URL.

## Stream + auto-sell flow

```python
import asyncio

from solders.keypair import Keypair

from lasersell_sdk.stream.client import StreamClient, StreamConfigure
from lasersell_sdk.stream.session import StreamSession
from lasersell_sdk.tx import send_via_helius_sender, sign_unsigned_tx


async def main() -> None:
    signer = Keypair()
    client = StreamClient("REPLACE_WITH_API_KEY")
    session = await StreamSession.connect(
        client,
        StreamConfigure(
            wallet_pubkeys=["REPLACE_WITH_WALLET_PUBKEY"],
            strategy={
                "target_profit_pct": 5.0,
                "stop_loss_pct": 1.5,
                "deadline_timeout_sec": 45,
            },
        ),
    )

    while True:
        event = await session.recv()
        if event is None:
            break

        if event.type == "exit_signal_with_tx" and event.message.get("type") == "exit_signal_with_tx":
            signed_tx = sign_unsigned_tx(event.message["unsigned_tx_b64"], signer)
            signature = await send_via_helius_sender(signed_tx)
            print(signature)


asyncio.run(main())
```

Notes:

- Use `client.with_local_mode(True)` for local stream development (`ws://localhost:8082/v1/ws`).
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
