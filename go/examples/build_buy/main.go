// Build an unsigned buy transaction with the Exit API.
//
// Before running:
// - Replace API key, mint, and wallet placeholders.
// - Set Amount as a human-readable decimal (e.g. 0.001 for 0.001 SOL).
package main

import (
	"context"
	"fmt"
	"log"

	lasersell "github.com/lasersell/lasersell-sdk/go"
)

func main() {
	ctx := context.Background()

	apiKey := "REPLACE_WITH_API_KEY"
	mint := "REPLACE_WITH_MINT"
	userPubkey := "REPLACE_WITH_WALLET_PUBKEY"
	amount := 0.001                    // 0.001 SOL
	slippageBps := uint16(2_000)       // 2,000 bps = 20%.

	client := lasersell.NewExitAPIClientWithAPIKey(apiKey)

	response, err := client.BuildBuyTx(ctx, lasersell.BuildBuyTxRequest{
		Mint:        mint,
		UserPubkey:  userPubkey,
		Amount:      &amount,
		SlippageBps: slippageBps,
	})
	if err != nil {
		log.Fatalf("build buy tx: %v", err)
	}

	fmt.Printf("unsigned_tx_b64=%s\n", response.Tx)
	if response.Route != nil {
		fmt.Printf("route=%v\n", response.Route)
	}
	if response.Debug != nil {
		fmt.Printf("debug=%v\n", response.Debug)
	}
}
