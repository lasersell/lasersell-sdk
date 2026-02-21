# @lasersell/lasersell-sdk

TypeScript SDK for LaserSell Exit API and stream websocket.

This package ports the Rust SDK surfaces:
- `exit-api`: build unsigned buy/sell transactions.
- `stream`: websocket client, protocol types, and session helpers.
- `tx`: sign/encode/send Solana transactions.
- `retry`: shared retry helpers.

`stream/proto` types from `lasersell-stream-proto` are included directly in this SDK (not a separate npm package).

## Install

```bash
npm install @lasersell/lasersell-sdk
```

For local development in this repository:

```bash
cd lasersell-sdk/js
npm install
```

## API Surface

- Exit API client:
  - `ExitApiClient`
  - `BuildBuyTxRequest`, `BuildSellTxRequest`, `BuildTxResponse`
- Stream client and session:
  - `StreamClient`, `StreamSession`
  - `StreamConfigure`, `StreamEvent`, `PositionHandle`
- Stream protocol types (ported inline):
  - `ClientMessage`, `ServerMessage`, `StrategyConfigMsg`, `MarketContextMsg`
- Transaction helpers:
  - `signUnsignedTx`, `encodeSignedTx`
  - `sendViaHeliusSender`, `sendViaRpc`

## Exit API (build sell)

```ts
import { ExitApiClient, type BuildSellTxRequest } from "@lasersell/lasersell-sdk";

const client = ExitApiClient.withApiKey("REPLACE_WITH_API_KEY");

const request: BuildSellTxRequest = {
  mint: "REPLACE_WITH_MINT",
  user_pubkey: "REPLACE_WITH_WALLET_PUBKEY",
  amount_tokens: 1_000_000,
  slippage_bps: 2_000,
  output: "SOL",
};

const response = await client.buildSellTx(request);
console.log(response.tx);
```

Use local backend endpoints instead of production:

```ts
const client = ExitApiClient.withApiKey("REPLACE_WITH_API_KEY").withLocalMode(true);
```

## Stream + auto-sell flow

```ts
import { StreamClient, StreamSession, signUnsignedTx, sendViaHeliusSender } from "@lasersell/lasersell-sdk";
import { Keypair } from "@solana/web3.js";

const client = new StreamClient("REPLACE_WITH_API_KEY");
const session = await StreamSession.connect(client, {
  wallet_pubkeys: ["REPLACE_WITH_WALLET_PUBKEY"],
  strategy: {
    target_profit_pct: 5,
    stop_loss_pct: 1.5,
  },
  deadline_timeout_sec: 45,
});

const signer = Keypair.generate();

while (true) {
  const event = await session.recv();
  if (event === null) break;

  if (event.type === "exit_signal_with_tx" && event.message.type === "exit_signal_with_tx") {
    const signed = signUnsignedTx(event.message.unsigned_tx_b64, signer);
    const signature = await sendViaHeliusSender(signed);
    console.log(signature);
  }
}
```

`deadline_timeout_sec` is enforced client-side by `StreamSession` timers and is not sent as part of wire strategy.
Use `session.updateStrategy(...)` when changing strategy so local deadline timers stay in sync (pass a second `deadlineTimeoutSec` argument to change local deadline timing).

Use local stream endpoint instead of production:

```ts
const client = new StreamClient("REPLACE_WITH_API_KEY").withLocalMode(true);
```

## Examples

Runnable examples live in `examples/`:

- `examples/build_buy.ts`: build unsigned buy tx
- `examples/build_sell.ts`: build unsigned sell tx
- `examples/build_and_send_sell.ts`: build, sign, and submit sell tx
- `examples/auto_sell.ts`: stream listener that signs and submits exits

See `examples/README.md` for required placeholders/inputs and exact run commands.

## Build

```bash
npm run build
```
