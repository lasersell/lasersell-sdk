# @lasersell/lasersell-sdk

TypeScript SDK for the LaserSell API.

Modules:
- `exit-api`: build unsigned buy/sell transactions.
- `stream`: websocket client, protocol types, and session helpers.
- `tx`: sign/encode/send Solana transactions.
- `retry`: shared retry helpers.

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

- LaserSell API client:
  - `ExitApiClient`
  - `BuildBuyTxRequest`, `BuildSellTxRequest`, `BuildTxResponse`
- Stream client and session:
  - `StreamClient`, `StreamSession`
  - `StreamConfigure`, `StreamEvent`, `PositionHandle`
- Stream protocol types (ported inline):
  - `ClientMessage`, `ServerMessage`, `StrategyConfigMsg`, `MarketContextMsg`
- Transaction helpers:
  - `signUnsignedTx`, `encodeSignedTx`
  - `sendTransaction`, `sendTransactionB64To`
  - `sendTargetRpc`, `sendTargetHeliusSender`, `sendTargetAstralane`

## Build a sell transaction

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

## Stream + auto-sell flow

```ts
import {
  StreamClient,
  StreamSession,
  singleWalletStreamConfigureOptional,
  signUnsignedTx,
  sendTransaction,
  sendTargetHeliusSender,
} from "@lasersell/lasersell-sdk";
import { Keypair } from "@solana/web3.js";

const client = new StreamClient("REPLACE_WITH_API_KEY");
const session = await StreamSession.connect(
  client,
  singleWalletStreamConfigureOptional(
    "REPLACE_WITH_WALLET_PUBKEY",
    {},
    45, // timeout-only strategy is valid
  ),
);

const signer = Keypair.generate();

while (true) {
  const event = await session.recv();
  if (event === null) break;

  if (event.type === "exit_signal_with_tx" && event.message.type === "exit_signal_with_tx") {
    const signed = signUnsignedTx(event.message.unsigned_tx_b64, signer);
    const signature = await sendTransaction(sendTargetHeliusSender(), signed);
    console.log(signature);
  }
}
```

`deadline_timeout_sec` is enforced client-side by `StreamSession` timers and is not sent as part of wire strategy.
Use `session.updateStrategy(...)` when changing strategy so local deadline timers stay in sync (pass a second `deadlineTimeoutSec` argument to change local deadline timing).
Use `singleWalletStreamConfigureOptional(...)` / `strategyConfigFromOptional(...)` to omit TP/SL fields; at least one of take profit, stop loss, or timeout must be enabled.

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
