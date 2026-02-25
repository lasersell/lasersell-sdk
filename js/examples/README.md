# TypeScript Examples

These examples mirror the Rust examples in `../rust/examples`.

## Prerequisites

1. Node.js 20+
2. Dependencies installed

```bash
cd lasersell-sdk/js
npm install
```

3. Build the SDK (examples import from `@lasersell/lasersell-sdk`, which resolves through package exports to `dist/`)

```bash
npm run build
```

4. A TypeScript runner for direct `.ts` execution (for example `tsx`)

```bash
npm install --save-dev tsx
```

## Run an Example

From `lasersell-sdk/js`:

```bash
npx tsx examples/build_buy.ts
npx tsx examples/build_sell.ts
npx tsx examples/build_and_send_sell.ts
npx tsx examples/auto_sell.ts
```

## Example Inputs

All examples include placeholder constants near the top of each file.
Replace those placeholders before running.

### `build_buy.ts`

Required values:

- API key
- Token mint
- User wallet pubkey
- `amount_quote_units` (atomic quote units)

Behavior:

- Calls `POST /v1/buy`
- Prints `unsigned_tx_b64`
- Prints optional route metadata

### `build_sell.ts`

Required values:

- API key
- Token mint
- User wallet pubkey
- `amount_tokens` (atomic token units)

Behavior:

- Calls `POST /v1/sell`
- Prints `unsigned_tx_b64`
- Prints optional route metadata

### `build_and_send_sell.ts`

Required values:

- API key
- Token mint
- User wallet pubkey
- Keypair file path (Solana JSON keypair)
- Send target:
  - `rpc` (requires `rpcUrl`)
  - `helius_sender`

Behavior:

1. Builds unsigned sell tx via Exit API
2. Signs tx with local keypair
3. Sends tx
4. Prints transaction signature

### `auto_sell.ts`

Required values:

- API key
- One or more wallet pubkeys
- Keypair file path

Behavior:

1. Connects to stream websocket
2. Tracks opened positions
3. For each `exit_signal_with_tx` message:
   - signs unsigned tx
   - submits via Helius Sender
4. Optionally sends `close_position` after successful submit

## Notes

- Slippage is in basis points (`slippage_bps`).
- Amount fields are atomic units.
- The examples are intentionally minimal and do not include production-safe secret management.
