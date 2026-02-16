# Go SDK Examples

These examples mirror the Rust SDK examples and are intended as copy-and-adapt starting points.

## Files

- `build_buy/main.go`: Build an unsigned buy transaction via Exit API.
- `build_sell/main.go`: Build an unsigned sell transaction via Exit API.
- `build_and_send_sell/main.go`: Build sell tx, sign locally, and submit to RPC or Helius Sender.
- `auto_sell/main.go`: Listen for stream `exit_signal_with_tx`, sign, and submit automatically.

## Run

From `lasersell-sdk/go`:

```bash
go run ./examples/build_buy
go run ./examples/build_sell
go run ./examples/build_and_send_sell
go run ./examples/auto_sell
```

Each file contains placeholder values and setup notes near the top.

## Notes

- `build_*` examples only call Exit API and print unsigned tx base64.
- `build_and_send_sell` and `auto_sell` require a local Solana keypair file.
- For local development services, toggle local mode on clients:

```go
exitClient := lasersell.NewExitAPIClientWithAPIKey(apiKey).WithLocalMode(true)
streamClient := stream.NewStreamClient(apiKey).WithLocalMode(true)
```
