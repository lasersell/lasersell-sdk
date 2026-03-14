# lasersell-go-sdk

Go SDK for the LaserSell API.

> **Full documentation:** [docs.lasersell.io/api/sdk/go](https://docs.lasersell.io/api/sdk/go)

## Modules

- `exit_api`: Build unsigned [buy](https://docs.lasersell.io/api/exit-api/buy)/[sell](https://docs.lasersell.io/api/exit-api/sell) transactions.
- `stream`: [Exit Intelligence Stream](https://docs.lasersell.io/api/stream/overview) client, protocol models, and session helpers.
- `tx`: [Sign, encode, and submit](https://docs.lasersell.io/api/transactions/signing) Solana transactions.
- `retry`: Shared retry helpers.

> **Using AI to code?** Add the [LaserSell MCP server](https://docs.lasersell.io/ai-agents/mcp-server) to your editor so your AI assistant can search LaserSell documentation in real time.

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

	response, err := client.BuildSellTx(ctx, lasersell.BuildSellTxRequest{
		Mint:         "REPLACE_WITH_MINT",
		UserPubkey:   "REPLACE_WITH_WALLET_PUBKEY",
		AmountTokens: 1_000_000,
		Output:       lasersell.SellOutputSOL,
		SlippageBps:  2_000,
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

## Stream + auto-sell flow

> **Important:** Connect the stream **before** submitting a buy transaction.

```go
package main

import (
	"context"
	"fmt"
	"log"

	"github.com/gagliardetto/solana-go"
	lasersell "github.com/lasersell/lasersell-sdk/go"
	"github.com/lasersell/lasersell-sdk/go/stream"
)

func main() {
	ctx := context.Background()
	privateKey, _ := solana.PrivateKeyFromSolanaKeygenFile("keypair.json")

	client := stream.NewStreamClient("YOUR_API_KEY")
	sendMode := "helius_sender"
	tipLamports := uint64(1000)
	session, _ := stream.ConnectSession(ctx, client, stream.StreamConfigure{
		WalletPubkeys:      []string{privateKey.PublicKey().String()},
		Strategy:           stream.StrategyConfigMsg{TargetProfitPct: 50, StopLossPct: 10, TrailingStopPct: 5},
		DeadlineTimeoutSec: 120,
		SendMode:           &sendMode,
		TipLamports:        &tipLamports,
	})

	for {
		event, err := session.Recv(ctx)
		if err != nil {
			log.Fatal(err)
		}
		switch msg := event.Message.(type) {
		case stream.ExitSignalWithTxServerMessage:
			signed, _ := lasersell.SignUnsignedTx(msg.UnsignedTxB64, privateKey)
			sig, _ := lasersell.SendTransaction(ctx, nil, lasersell.SendTargetHeliusSender(), signed)
			fmt.Printf("exit submitted: %s\n", sig)
		case stream.PositionOpenedServerMessage:
			fmt.Printf("tracking: %s\n", msg.Mint)
		}
	}
}
```

## Exit ladder

Sell partial amounts at multiple profit thresholds:

```go
strategy := stream.NewStrategyConfigBuilder().
    StopLossPct(10.0).
    TakeProfitLevels([]stream.TakeProfitLevelMsg{
        {ProfitPct: 25.0, SellPct: 30.0, TrailingStopPct: 0.0},
        {ProfitPct: 50.0, SellPct: 50.0, TrailingStopPct: 3.0},
        {ProfitPct: 100.0, SellPct: 100.0, TrailingStopPct: 5.0},
    }).
    Build()
```

## Liquidity guard

Prevent exits into thin liquidity:

```go
strategy := stream.NewStrategyConfigBuilder().
    TargetProfitPct(50.0).
    StopLossPct(10.0).
    LiquidityGuard(true).
    Build()
```

## Breakeven trail

A trailing stop that activates at breakeven:

```go
strategy := stream.NewStrategyConfigBuilder().
    TargetProfitPct(50.0).
    StopLossPct(10.0).
    BreakevenTrailPct(2.0).
    Build()
```

## Per-position strategy override

Override the global strategy for a single position:

```go
sender.UpdatePositionStrategy(positionID, stream.StrategyConfigMsg{
    TargetProfitPct: 200.0,
    StopLossPct:     5.0,
    TrailingStopPct: 10.0,
})
```

## Wallet registration

All wallets must be registered before connecting to the Exit Intelligence Stream:

```go
proof := lasersell.ProveOwnership(privateKey)
apiClient.RegisterWallet(ctx, proof, lasersell.Ptr("My Wallet"))
```

## Watch wallets (copy trading)

Mirror trades from external wallets:

```go
sender.UpdateWatchWallets([]stream.WatchWalletEntryMsg{
    {Pubkey: "WalletToWatch..."},
})
```

## RPC endpoint

The SDK ships with the Solana public RPC as a default so you can get started immediately:

```go
target := lasersell.SendTargetDefaultRpc()
```

**A private RPC is highly recommended for production** — the public endpoint is rate-limited and unreliable under load. Free private RPC tiers are available from [Helius](https://www.helius.dev/) and [Chainstack](https://chainstack.com/), among others:

```go
target := lasersell.SendTargetRpc("https://your-private-rpc.example.com")
```

## API notes

- **Connect the stream before buying.** The stream detects positions by watching on-chain token arrivals. If the stream is not connected when a buy lands, the position will not be tracked. See [docs.lasersell.io/api/exit-api/buy](https://docs.lasersell.io/api/exit-api/buy).
- Use `context.Context` on all network operations to control cancellation/timeouts.
- API parser supports current and legacy response envelopes.
- Stream client includes reconnect handling and sender/session utilities.
- `StreamConfigure.SendMode` and `StreamConfigure.TipLamports` are optional pointers that control how the server optimizes and prices exit transactions. Values for `SendMode`: `"rpc"`, `"helius_sender"`, `"astralane"`. Default tip: 1000 lamports.
- `StreamConfigure.DeadlineTimeoutSec` is enforced client-side by `stream.StreamSession` timers and is not part of wire strategy.
- Use `session.UpdateStrategy(...)` (instead of sender-only updates) when changing strategy so local deadline timers stay synchronized. Pass optional `deadlineTimeoutSec` if you want to change local deadline timing.
- Use `stream.SingleWalletStreamConfigureOptional(...)` / `stream.StrategyConfigFromOptional(...)` to omit TP/SL fields; at least one of take profit, stop loss, trailing stop, or timeout must be enabled.
- **Trailing stop**: locks in profits by tracking a high-water mark — when profit drops by the configured percentage of entry cost from its peak, an exit is triggered. Set `TrailingStopPct` in `OptionalStrategyConfig` (e.g. `5.0` = exit if profit drops 5% of entry from peak).
- Use `session.Sender().UpdateWallets(...)` to add or remove wallets mid-session without reconnecting.
- **Liquidity snapshots (Tier 1+)**: Professional and Advanced tier subscribers receive real time slippage bands and liquidity trend data ([announcement](https://www.lasersell.io/blog/liquidity-snapshots-and-sdk-0-3)). Query via `session.GetSlippageBands(positionID)`, `session.GetMaxSellAtSlippage(positionID, 500)`, and `session.GetLiquidityTrend(positionID)`.
- **Partial sell**: Use `client.BuildPartialSellTx(ctx, handle, amountTokens, 500, lasersell.SellOutputSOL)` to sell a subset of a position's tokens. Combine with slippage bands for optimal sizing.

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
