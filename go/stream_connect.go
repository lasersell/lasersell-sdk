package lasersell

import (
	"context"

	"github.com/lasersell/lasersell-sdk/go/stream"
)

// ConnectStream registers wallet proofs via the LaserSell API, then opens a
// stream connection. This is the recommended way to connect. Generate proofs
// with ProveOwnership first — a pure local operation that never touches the
// network.
//
//	proof := lasersell.ProveOwnership(walletKey)
//	conn, err := lasersell.ConnectStream(ctx, "your-api-key", []lasersell.WalletProof{proof}, strategy, 45)
func ConnectStream(
	ctx context.Context,
	apiKey string,
	proofs []WalletProof,
	strategy stream.StrategyConfigMsg,
	deadlineTimeoutSec uint64,
) (*stream.StreamConnection, error) {
	exitClient := NewExitAPIClientWithAPIKey(apiKey)
	for _, proof := range proofs {
		if err := exitClient.RegisterWallet(ctx, proof, nil); err != nil {
			return nil, err
		}
	}

	pubkeys := make([]string, len(proofs))
	for i, p := range proofs {
		pubkeys[i] = p.WalletPubkey
	}

	client := stream.NewStreamClient(apiKey)
	return client.Connect(ctx, stream.StreamConfigure{
		WalletPubkeys:      pubkeys,
		Strategy:           strategy,
		DeadlineTimeoutSec: deadlineTimeoutSec,
	})
}
