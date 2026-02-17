# Python SDK Examples

This folder contains runnable example scripts for the LaserSell Python SDK.

## Prerequisites

From `lasersell-sdk/python`:

```bash
python -m pip install -e '.[full]'
```

Required inputs you will replace in scripts:

- `REPLACE_WITH_API_KEY`
- `REPLACE_WITH_MINT`
- `REPLACE_WITH_WALLET_PUBKEY` (or wallet list)
- `REPLACE_WITH_KEYPAIR_PATH` (for tx signing examples)
- `REPLACE_WITH_RPC_URL` (for direct RPC send example)

## Keypair file format

`build_and_send_sell.py` and `auto_sell.py` expect a Solana keypair JSON file containing
an array of integers (the 64-byte secret key), for example:

```json
[12,34,56,...]
```

## Scripts

### `build_buy.py`

Builds an unsigned buy transaction from Exit API (`POST /v1/buy`) and prints:

- `unsigned_tx_b64`
- optional route metadata

Run:

```bash
python examples/build_buy.py
```

### `build_sell.py`

Builds an unsigned sell transaction from Exit API (`POST /v1/sell`) and prints:

- `unsigned_tx_b64`
- optional route metadata

Run:

```bash
python examples/build_sell.py
```

### `build_and_send_sell.py`

Full sell flow:

1. Build unsigned sell tx via Exit API.
2. Sign tx with local keypair (`solders`).
3. Send tx to either:
   - standard RPC (`send_target = "rpc"`)
   - Helius Sender (`send_target = "helius_sender"`)

Run:

```bash
python examples/build_and_send_sell.py
```

### `auto_sell.py`

Stream-driven flow:

1. Connect to stream websocket.
2. Track positions and listen for `exit_signal_with_tx`.
3. Sign and submit each exit tx through Helius Sender.
4. Optionally send `close_position` after successful submission.

Run:

```bash
python examples/auto_sell.py
```

## Development mode endpoints

For local development, enable local mode in your script:

- Exit API: `ExitApiClient.with_api_key(...).with_local_mode(True)` -> `http://localhost:8080`
- Stream: `StreamClient(...).with_local_mode(True)` -> `ws://localhost:8082/v1/ws`

For custom environments, use explicit overrides:

- Exit API: `ExitApiClient.with_api_key(...).with_base_url("https://api-dev.example")`
- Stream: `StreamClient(...).with_endpoint("wss://stream-dev.example/v1/ws")`
