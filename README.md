<p align="center">
  <a href="https://lasersell.io">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://lasersell.io/brand/lasersell-dark-full-1.png">
      <img alt="LaserSell" src="https://lasersell.io/brand/lasersell-light-full-2.png" width="260">
    </picture>
  </a>
</p>

<p align="center">
  Ultra-low latency Solana execution infrastructure.<br>
  Deterministic routing. Adaptive slippage. Non-custodial.
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/@lasersell/lasersell-sdk"><img alt="npm" src="https://img.shields.io/npm/v/@lasersell/lasersell-sdk?label=npm&color=CB3837"></a>
  <a href="https://pypi.org/project/lasersell-sdk/"><img alt="PyPI" src="https://img.shields.io/pypi/v/lasersell-sdk?label=pypi&color=3775A9"></a>
  <a href="https://crates.io/crates/lasersell-sdk"><img alt="crates.io" src="https://img.shields.io/crates/v/lasersell-sdk?label=crates.io&color=dea584"></a>
  <a href="https://pkg.go.dev/github.com/lasersell/lasersell-sdk/go"><img alt="Go Reference" src="https://pkg.go.dev/badge/github.com/lasersell/lasersell-sdk/go.svg"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <br>
  <a href="https://discord.gg/lasersell"><img alt="Discord" src="https://img.shields.io/badge/Discord-Join-5865F2?logo=discord&logoColor=white"></a>
  <a href="https://x.com/lasersellhq"><img alt="X (Twitter)" src="https://img.shields.io/twitter/follow/lasersellhq?style=flat&logo=x&color=000000"></a>
</p>

---

## What is LaserSell?

[LaserSell](https://lasersell.io) is professional Solana execution infrastructure for automated exit strategies. It builds optimized transactions across major Solana DEXs, submits them with adaptive slippage logic, and monitors positions in real time so you can set profit targets, stop losses, and timeouts that fire the millisecond conditions are met.

**Supported DEXs and platforms**: Pump.fun, Raydium (LaunchLab, CPMM), Meteora (DBC, DAMM v2), Bags.fm.

**Non-custodial by design.** Your private keys never leave your machine. LaserSell builds unsigned transactions, you sign locally, you submit.

## What can you build with the SDK?

- **Automated exit strategies** — connect to the LaserSell Exit Intelligence stream, receive real-time exit signals with pre-built transactions, sign and submit in milliseconds.
- **One-shot sells and buys** — build, sign, and submit a single transaction through the LaserSell API with optimized routing and slippage.
- **Custom trading bots** — combine the stream, retry, and transaction modules to build production trading infrastructure.
- **Portfolio management tools** — monitor positions, track PnL, and execute exits programmatically across multiple wallets.

## Install

```bash
# TypeScript / JavaScript
npm install @lasersell/lasersell-sdk

# Python
pip install lasersell-sdk

# Rust
cargo add lasersell-sdk

# Go
go get github.com/lasersell/lasersell-sdk/go
```

## Quick start

### 1. Get an API key

Sign up for free at [app.lasersell.io](https://app.lasersell.io/auth/sign-up) and create an API key from the dashboard. No credit card required.

### 2. Build and send a transaction

**TypeScript**

```ts
import {
  ExitApiClient,
  signUnsignedTx,
  sendTransaction,
  sendTargetRpc,
} from "@lasersell/lasersell-sdk";
import { Keypair } from "@solana/web3.js";

const client = ExitApiClient.withApiKey(process.env.LASERSELL_API_KEY!);

const unsignedTxB64 = await client.buildSellTxB64({
  mint: "TOKEN_MINT_ADDRESS",
  user_pubkey: "YOUR_WALLET_PUBKEY",
  amount_tokens: 1_000_000,
  slippage_bps: 2_000,
  output: "SOL",
});

const signedTx = signUnsignedTx(unsignedTxB64, keypair);
const signature = await sendTransaction(sendTargetRpc(process.env.RPC_URL!), signedTx);
```

**Python**

```python
from lasersell_sdk.exit_api import ExitApiClient, BuildSellTxRequest, SellOutput
from lasersell_sdk.tx import sign_unsigned_tx, send_transaction, SendTargetRpc

client = ExitApiClient.with_api_key("YOUR_API_KEY")

request = BuildSellTxRequest(
    mint="TOKEN_MINT_ADDRESS",
    user_pubkey="YOUR_WALLET_PUBKEY",
    amount_tokens=1_000_000,
    slippage_bps=2_000,
    output=SellOutput.SOL,
)

unsigned_tx_b64 = await client.build_sell_tx_b64(request)
signed_tx = sign_unsigned_tx(unsigned_tx_b64, keypair)
signature = await send_transaction(SendTargetRpc(url="YOUR_RPC_URL"), signed_tx)
```

### 3. Automate exits with the stream

Connect to the LaserSell Exit Intelligence stream to receive real-time exit signals with pre-built transactions:

```ts
import {
  StreamClient,
  StreamSession,
  signUnsignedTx,
  sendTransaction,
  sendTargetHeliusSender,
} from "@lasersell/lasersell-sdk";

const client = new StreamClient("YOUR_API_KEY");
const session = await StreamSession.connect(client, {
  wallet_pubkeys: ["YOUR_WALLET_PUBKEY"],
  strategy: { target_profit_pct: 50, stop_loss_pct: 10 },
  deadline_timeout_sec: 120,
});

while (true) {
  const event = await session.recv();
  if (event === null) break;

  if (event.type === "exit_signal_with_tx" && event.message.type === "exit_signal_with_tx") {
    const signed = signUnsignedTx(event.message.unsigned_tx_b64, signer);
    const signature = await sendTransaction(sendTargetHeliusSender(), signed);
    console.log(`Exit executed: ${signature}`);
  }
}
```

## SDK documentation

Full API reference and examples for each language:

| Language | Package | Docs |
|----------|---------|------|
| TypeScript | [`@lasersell/lasersell-sdk`](https://www.npmjs.com/package/@lasersell/lasersell-sdk) | [README](js/README.md) · [Examples](js/examples/) |
| Python | [`lasersell-sdk`](https://pypi.org/project/lasersell-sdk/) | [README](python/README.md) · [Examples](python/examples/) |
| Rust | [`lasersell-sdk`](https://crates.io/crates/lasersell-sdk) | [README](rust/README.md) · [Examples](rust/examples/) |
| Go | [`github.com/lasersell/lasersell-sdk/go`](https://pkg.go.dev/github.com/lasersell/lasersell-sdk/go) | [README](go/README.md) · [Examples](go/examples/) |

## LaserSell ecosystem

| Resource | Link |
|----------|------|
| Website | [lasersell.io](https://lasersell.io) |
| Dashboard & API keys | [app.lasersell.io](https://app.lasersell.io) |
| Documentation | [docs.lasersell.io](https://docs.lasersell.io) |
| LaserSell CLI (open source) | [github.com/lasersell/lasersell](https://github.com/lasersell/lasersell) |
| SDKs (this repo) | [github.com/lasersell/lasersell-sdk](https://github.com/lasersell/lasersell-sdk) |
| Discord | [discord.gg/lasersell](https://discord.gg/lasersell) |
| X (Twitter) | [@lasersellhq](https://x.com/lasersellhq) |

## Security

LaserSell is non-custodial. The SDK builds unsigned transactions. Your private keys never leave your machine. Transactions are signed locally and submitted directly to the Solana network.

- Private keys are never transmitted to LaserSell servers
- All transaction signing happens client-side
- Open-source SDKs for full auditability

Learn more at [lasersell.io/security](https://www.lasersell.io/security).

## FAQ

### Can I use the SDK to build an AI agent that trades on Solana?

Yes, this is one of the best use cases for the SDK. LaserSell offloads the heavy lifting of DEX routing, slippage optimization, and transaction construction to the API, so your AI agent doesn't need to understand Solana's low-level mechanics. An agent can execute a trade in a single API call: pass a mint address and amount, get back a ready-to-sign transaction. This makes it straightforward to give any LLM or autonomous agent the ability to buy and sell tokens on Solana efficiently and reliably.

### What's the difference between the API client and the stream?

The API client (`ExitApiClient`) is for one-shot operations: build a single buy or sell transaction on demand. The stream (`StreamClient` + `StreamSession`) is for continuous monitoring: it watches your positions in real time and sends exit signals with pre-built transactions the moment your take-profit, stop-loss, or timeout conditions are met.

### What is the Exit Intelligence stream?

The Exit Intelligence stream is not a regular WebSocket feed. It monitors your positions server-side against your take-profit and stop-loss thresholds and delivers ready-to-sign exit transactions the moment conditions are met. No polling, no client-side price math, no separate transaction building step. It is also superior to limit orders: limit orders sit passively and often get skipped entirely during rapid dumps because the price gaps right past them. The Exit Intelligence stream fires an immediate market swap the instant your trigger hits, using real-time on-chain data rather than stale API snapshots. Learn more in the [full comparison](https://docs.lasersell.io/core-concepts/comparison-laser-sell-vs-standard-market-orders).

### How does the LaserSell CLI relate to the SDK?

The [LaserSell CLI](https://github.com/lasersell/lasersell) is an open-source command-line application built on top of this SDK. It's a ready-to-use trading tool. The SDK is for developers who want to build their own applications, bots, or integrations on top of the LaserSell API.

## License

[MIT](LICENSE)
