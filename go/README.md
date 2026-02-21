# lasersell-go-sdk

Go SDK for LaserSell Exit API and stream websocket.

## What this SDK includes

- `exit_api`: Build unsigned buy/sell transactions.
- `stream`: Websocket client, protocol models, and session helpers.
- `tx`: Sign, encode, and submit Solana transactions.
- `retry`: Shared retry helpers.

Stream protocol types from `lasersell-stream-proto` are embedded directly in this SDK under `stream` (not a separate Go package).

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

## Local mode

To target local development services:

```go
exitClient := lasersell.NewExitAPIClientWithAPIKey(apiKey).WithLocalMode(true)
streamClient := stream.NewStreamClient(apiKey).WithLocalMode(true)
```

This switches endpoints to:

- Exit API: `http://localhost:8080`
- Stream: `ws://localhost:8082/v1/ws`

To target custom environments directly, use explicit overrides:

```go
exitClient := lasersell.NewExitAPIClientWithAPIKey(apiKey).WithBaseURL("https://api-dev.example")
streamClient := stream.NewStreamClient(apiKey).WithEndpoint("wss://stream-dev.example/v1/ws")
```

## API notes

- Use `context.Context` on all network operations to control cancellation/timeouts.
- Exit API parser supports current and legacy response envelopes.
- Stream client includes reconnect handling and sender/session utilities.
- `StreamConfigure.DeadlineTimeoutSec` is enforced client-side by `stream.StreamSession` timers and is not part of wire strategy.
- Use `session.UpdateStrategy(...)` (instead of sender-only updates) when changing strategy so local deadline timers stay synchronized. Pass optional `deadlineTimeoutSec` if you want to change local deadline timing.
- Tx submit helpers support both Helius Sender and standard Solana RPC.

## Error types

- Exit API: `*lasersell.ExitAPIError`
- Stream: `*stream.StreamClientError`
- Tx signing/submission: `*lasersell.TxSubmitError`
