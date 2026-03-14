// Demonstrates wallet registration (required for all stream connections)
// and copy trading with watch wallets.
//
// Demonstrates:
// 1. Proving wallet ownership with ProveOwnership (local Ed25519 signature).
// 2. Registering the wallet with the LaserSell API.
// 3. Copy trading via watch wallets.
// 4. Per-position strategy overrides.
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

	apiKey := "REPLACE_WITH_API_KEY"
	keypairPath := "REPLACE_WITH_KEYPAIR_PATH"
	watchWalletPubkey := "REPLACE_WITH_WATCH_WALLET_PUBKEY"

	privateKey, err := solana.PrivateKeyFromSolanaKeygenFile(keypairPath)
	if err != nil {
		log.Fatalf("read keypair: %v", err)
	}

	// 1. Prove ownership (local, no network call)
	proof := lasersell.ProveOwnership(privateKey)
	fmt.Println("Wallet proof generated")

	// 2. Register wallet with the API
	apiClient := lasersell.NewExitAPIClientWithAPIKey(apiKey)
	if err := apiClient.RegisterWallet(ctx, proof, lasersell.Ptr("My Trading Wallet")); err != nil {
		log.Fatalf("register wallet: %v", err)
	}
	fmt.Println("Wallet registered")

	// 3. Build strategy with exit ladder
	strategy := stream.NewStrategyConfigBuilder().
		StopLossPct(10.0).
		TakeProfitLevels([]stream.TakeProfitLevelMsg{
			{ProfitPct: 25.0, SellPct: 30.0, TrailingStopPct: 0.0},
			{ProfitPct: 50.0, SellPct: 50.0, TrailingStopPct: 3.0},
			{ProfitPct: 100.0, SellPct: 100.0, TrailingStopPct: 5.0},
		}).
		LiquidityGuard(true).
		BreakevenTrailPct(2.0).
		Build()

	// 4. Connect stream
	streamClient := stream.NewStreamClient(apiKey)
	sendMode := "helius_sender"
	tipLamports := uint64(1000)
	session, err := stream.ConnectSession(ctx, streamClient, stream.StreamConfigure{
		WalletPubkeys:      []string{privateKey.PublicKey().String()},
		Strategy:           strategy,
		DeadlineTimeoutSec: 120,
		SendMode:           &sendMode,
		TipLamports:        &tipLamports,
	})
	if err != nil {
		log.Fatalf("connect stream: %v", err)
	}
	fmt.Println("Stream connected")

	// 5. Add a watch wallet for copy trading (optional — only needed if mirroring external wallets)
	sender := session.Sender()
	if err := sender.UpdateWatchWallets([]stream.WatchWalletEntryMsg{
		{Pubkey: watchWalletPubkey},
	}); err != nil {
		log.Fatalf("update watch wallets: %v", err)
	}
	fmt.Printf("Watching wallet: %s\n", watchWalletPubkey)

	// 6. Event loop
	for {
		event, err := session.Recv(ctx)
		if err != nil {
			log.Fatal(err)
		}

		switch msg := event.Message.(type) {
		case stream.PositionOpenedServerMessage:
			fmt.Printf("Position opened: %s (watched: %v)\n", msg.Mint, msg.Watched)

			// Apply tighter strategy to copied positions
			if msg.Watched {
				if err := sender.UpdatePositionStrategy(msg.PositionID, stream.StrategyConfigMsg{
					TargetProfitPct: 30.0,
					StopLossPct:     8.0,
					TrailingStopPct: 5.0,
				}); err != nil {
					log.Printf("override failed: %v", err)
				}
				fmt.Printf("Tighter strategy set for copied position %d\n", msg.PositionID)
			}

		case stream.ExitSignalWithTxServerMessage:
			signed, err := lasersell.SignUnsignedTx(msg.UnsignedTxB64, privateKey)
			if err != nil {
				log.Printf("sign failed: %v", err)
				continue
			}
			sig, err := lasersell.SendTransaction(ctx, nil, lasersell.SendTargetHeliusSender(), signed)
			if err != nil {
				log.Printf("send failed: %v", err)
				continue
			}
			fmt.Printf("Exit: %s\n", sig)
		}
	}
}
