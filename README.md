# lasersell-sdk

Language SDKs for LaserSell APIs.

## Layout

- `rust/`: Rust SDK (`lasersell-sdk` crate)
- `js/`: TypeScript SDK (`@lasersell/lasersell-sdk`)
- `python/`: Python SDK (`lasersell-sdk`)
- `go/`: Go SDK (`github.com/lasersell/lasersell-sdk/go`)

## TypeScript SDK

The TypeScript SDK is in `js/` and includes:

- Exit API client (`/v1/buy`, `/v1/sell`)
- Stream websocket client and session helpers
- Stream protocol types (ported from `lasersell-stream-proto` inline in the SDK)
- Transaction signing and submission helpers

See `js/README.md` for usage.
See `js/examples/README.md` for runnable TypeScript example documentation.

## Go SDK

The Go SDK is in `go/` and includes:

- Exit API client (`/v1/buy`, `/v1/sell`)
- Stream websocket client and session helpers
- Stream protocol types (ported from `lasersell-stream-proto` inline in the SDK)
- Transaction signing and submission helpers

See `go/README.md` for usage.
See `go/examples/README.md` for runnable Go example documentation.

## Python SDK

The Python SDK is in `python/` and includes:

- Exit API client (`/v1/buy`, `/v1/sell`)
- Stream websocket client and session helpers
- Stream protocol types (ported from `lasersell-stream-proto` inline in the SDK)
- Transaction signing and submission helpers

See `python/README.md` for usage.
