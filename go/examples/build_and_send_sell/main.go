// Build, sign, and submit a sell transaction.
//
// Flow:
// 1. Request unsigned tx bytes from Exit API.
// 2. Sign locally with your Solana keypair.
// 3. Submit to RPC or Helius Sender.
//
// Before running:
// - Replace all placeholders, including keypair path.
// - Set amountTokensAtomic in mint atomic units.
// - If sendTarget == "rpc", provide a valid rpcURL.
package main

import (
	"context"
	"fmt"
	"log"

	"github.com/gagliardetto/solana-go"
	lasersell "github.com/lasersell/lasersell-sdk/go"
)

func main() {
	ctx := context.Background()

	apiKey := "REPLACE_WITH_API_KEY"
	mint := "REPLACE_WITH_MINT"
	userPubkey := "REPLACE_WITH_WALLET_PUBKEY"
	keypairPath := "REPLACE_WITH_KEYPAIR_PATH"
	sendTarget := "rpc" // or "helius_sender"
	rpcURL := "REPLACE_WITH_RPC_URL"

	amountTokensAtomic := uint64(1_000_000)
	slippageBps := uint16(2_000)
	output := lasersell.SellOutputSOL

	privateKey, err := solana.PrivateKeyFromSolanaKeygenFile(keypairPath)
	if err != nil {
		log.Fatalf("read keypair: %v", err)
	}

	exitAPI := lasersell.NewExitAPIClientWithAPIKey(apiKey)
	unsignedTxB64, err := exitAPI.BuildSellTxB64(ctx, lasersell.BuildSellTxRequest{
		Mint:         mint,
		UserPubkey:   userPubkey,
		AmountTokens: amountTokensAtomic,
		SlippageBps:  lasersell.Ptr(slippageBps),
		Output:       &output,
	})
	if err != nil {
		log.Fatalf("build sell tx: %v", err)
	}

	signedTx, err := lasersell.SignUnsignedTx(unsignedTxB64, privateKey)
	if err != nil {
		log.Fatalf("sign tx: %v", err)
	}

	var signature string
	switch sendTarget {
	case "helius_sender":
		signature, err = lasersell.SendViaHeliusSender(ctx, nil, signedTx)
	case "rpc":
		signature, err = lasersell.SendViaRPC(ctx, nil, rpcURL, signedTx)
	default:
		log.Fatalf("sendTarget must be rpc or helius_sender (got %q)", sendTarget)
	}
	if err != nil {
		log.Fatalf("submit tx: %v", err)
	}

	fmt.Printf("signature=%s\n", signature)
}
