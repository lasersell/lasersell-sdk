# @lasersell/lasersell-sdk

TypeScript SDK for the LaserSell API.

> **Full documentation:** [docs.lasersell.io/api/sdk/typescript](https://docs.lasersell.io/api/sdk/typescript)

Modules:
- `exit-api`: build unsigned [buy](https://docs.lasersell.io/api/exit-api/buy)/[sell](https://docs.lasersell.io/api/exit-api/sell) transactions.
- `stream`: [Exit Intelligence Stream](https://docs.lasersell.io/api/stream/overview) client, protocol types, and session helpers.
- `tx`: [sign/encode/send](https://docs.lasersell.io/api/transactions/signing) Solana transactions.
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

> **Important:** Connect the stream **before** submitting a buy transaction. See [docs.lasersell.io/api/exit-api/buy](https://docs.lasersell.io/api/exit-api/buy) for details.

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
Use `singleWalletStreamConfigureOptional(...)` / `strategyConfigFromOptional(...)` to omit TP/SL fields; at least one of take profit, stop loss, trailing stop, or timeout must be enabled.

### Trailing stop

Lock in profits by tracking a high-water mark. When the position's profit drops by the configured percentage of entry cost from its peak, an exit is triggered.

```ts
import { strategyConfigFromOptional } from "@lasersell/lasersell-sdk";

// 20% take profit, 10% stop loss, 5% trailing stop
const strategy = strategyConfigFromOptional({ takeProfitPct: 20, stopLossPct: 10, trailingStopPct: 5 });
```

Example: with `trailingStopPct: 5` and an entry of 100 SOL, if profit peaks at 30 SOL, an exit triggers when profit falls below 25 SOL.

### Sell on graduation

Automatically exit a position when its token graduates from a bonding curve to a full DEX (e.g. Pump.fun to PumpSwap). Enable by setting `sellOnGraduation` in the optional strategy config:

```ts
import { strategyConfigFromOptional } from "@lasersell/lasersell-sdk";

const strategy = strategyConfigFromOptional({
  takeProfitPct: 20,
  stopLossPct: 10,
  sellOnGraduation: true,
});
```

When a graduation event is detected the server sells the position on the new market and reports `"graduation"` as the exit reason.

### Update wallets mid-session

Add or remove wallets without reconnecting:

```ts
await session.sender().updateWallets(["WALLET_1_PUBKEY", "WALLET_2_PUBKEY"]);
```

The server diffs the new list against the current set and registers/unregisters accordingly.

## RPC endpoint

The SDK ships with the Solana public mainnet-beta RPC as a default so you can get started immediately:

```ts
import { sendTargetDefaultRpc } from "@lasersell/lasersell-sdk";

const target = sendTargetDefaultRpc();
```

**A private RPC is highly recommended for production** — the public endpoint is rate-limited and unreliable under load. Free private RPC tiers are available from [Helius](https://www.helius.dev/) and [Chainstack](https://chainstack.com/), among others:

```ts
import { sendTargetRpc } from "@lasersell/lasersell-sdk";

const target = sendTargetRpc("https://your-private-rpc.example.com");
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
