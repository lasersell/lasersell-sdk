// Build an unsigned sell transaction with the Exit API.
//
// Before running:
// - Replace API key, mint, and wallet placeholders.
// - Set amountTokensAtomic in mint atomic units.
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
	amountTokensAtomic := uint64(1_000_000) // Example: for a 6-decimal mint, 1 token = 1_000_000.
	slippageBps := uint16(2_000)            // 2,000 bps = 20%.
	output := lasersell.SellOutputSOL

	client := lasersell.NewExitAPIClientWithAPIKey(apiKey)

	response, err := client.BuildSellTx(ctx, lasersell.BuildSellTxRequest{
		Mint:         mint,
		UserPubkey:   userPubkey,
		AmountTokens: amountTokensAtomic,
		SlippageBps:  lasersell.Ptr(slippageBps),
		Output:       &output,
	})
	if err != nil {
		log.Fatalf("build sell tx: %v", err)
	}

	fmt.Printf("unsigned_tx_b64=%s\n", response.Tx)
	if response.Route != nil {
		fmt.Printf("route=%v\n", response.Route)
	}
	if response.Debug != nil {
		fmt.Printf("debug=%v\n", response.Debug)
	}
}
