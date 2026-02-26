# lasersell-go-sdk

Go SDK for the LaserSell API.

## Modules

- `exit_api`: Build unsigned buy/sell transactions.
- `stream`: Websocket client, protocol models, and session helpers.
- `tx`: Sign, encode, and submit Solana transactions.
- `retry`: Shared retry helpers.

## Install

```bash
go get github.com/lasersell/lasersell-sdk/go
```

## Quickstart: Build a Sell Transaction

```go
package main

import (
	"context"
	"fmt"
	"log"

	lasersell "github.com/lasersell/lasersell-sdk/go"
)

func main() {
	ctx := context.Background()
	client := lasersell.NewExitAPIClientWithAPIKey("REPLACE_WITH_API_KEY")

	output := lasersell.SellOutputSOL
	response, err := client.BuildSellTx(ctx, lasersell.BuildSellTxRequest{
		Mint:         "REPLACE_WITH_MINT",
		UserPubkey:   "REPLACE_WITH_WALLET_PUBKEY",
		AmountTokens: 1_000_000,
		SlippageBps:  lasersell.Ptr(uint16(2_000)),
		Output:       &output,
	})
	if err != nil {
		log.Fatal(err)
	}

	fmt.Println(response.Tx)
}
```

## Examples

See `examples/README.md` for details.

From `lasersell-sdk/go`:

```bash
go run ./examples/build_buy
go run ./examples/build_sell
go run ./examples/build_and_send_sell
go run ./examples/auto_sell
```

### Example files

- `examples/build_buy/main.go`
- `examples/build_sell/main.go`
- `examples/build_and_send_sell/main.go`
- `examples/auto_sell/main.go`

## RPC endpoint

The SDK ships with the Solana public mainnet-beta RPC as a default so you can get started immediately:

```go
target := lasersell.SendTargetDefaultRpc()
```

**A private RPC is highly recommended for production** — the public endpoint is rate-limited and unreliable under load. Free private RPC tiers are available from [Helius](https://www.helius.dev/) and [Chainstack](https://chainstack.com/), among others:

```go
target := lasersell.SendTargetRpc("https://your-private-rpc.example.com")
```

## API notes

- Use `context.Context` on all network operations to control cancellation/timeouts.
- API parser supports current and legacy response envelopes.
- Stream client includes reconnect handling and sender/session utilities.
- `StreamConfigure.DeadlineTimeoutSec` is enforced client-side by `stream.StreamSession` timers and is not part of wire strategy.
- Use `session.UpdateStrategy(...)` (instead of sender-only updates) when changing strategy so local deadline timers stay synchronized. Pass optional `deadlineTimeoutSec` if you want to change local deadline timing.
- Use `stream.SingleWalletStreamConfigureOptional(...)` / `stream.StrategyConfigFromOptional(...)` to omit TP/SL fields; at least one of take profit, stop loss, trailing stop, or timeout must be enabled.
- **Trailing stop**: locks in profits by tracking a high-water mark — when profit drops by the configured percentage of entry cost from its peak, an exit is triggered. Set `TrailingStopPct` in `OptionalStrategyConfig` (e.g. `5.0` = exit if profit drops 5% of entry from peak).
- Use `session.Sender().UpdateWallets(...)` to add or remove wallets mid-session without reconnecting.

### Trailing stop example

```go
trailing := 5.0
cfg := stream.StrategyConfigFromOptional(stream.OptionalStrategyConfig{
    TakeProfitPct:  lasersell.Ptr(20.0),
    StopLossPct:    lasersell.Ptr(10.0),
    TrailingStopPct: &trailing,
})
```

With `TrailingStopPct = 5.0` and an entry of 100 SOL: if profit peaks at 30 SOL, an exit triggers when profit falls below 25 SOL.

### Sell on graduation

Automatically exit a position when its token graduates from a bonding curve to a full DEX (e.g. Pump.fun to PumpSwap). Enable by setting `SellOnGraduation` in the optional strategy config:

```go
cfg := stream.StrategyConfigFromOptional(stream.OptionalStrategyConfig{
    TakeProfitPct:    lasersell.Ptr(20.0),
    StopLossPct:      lasersell.Ptr(10.0),
    SellOnGraduation: lasersell.Ptr(true),
})
```

When a graduation event is detected the server sells the position on the new market and reports `"graduation"` as the exit reason (`stream.ExitReasonGraduation`).

- Tx submit helpers support both Helius Sender and standard Solana RPC.

## Error types

- LaserSell API: `*lasersell.ExitAPIError`
- Stream: `*stream.StreamClientError`
- Tx signing/submission: `*lasersell.TxSubmitError`
