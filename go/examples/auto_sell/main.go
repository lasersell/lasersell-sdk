// Auto-sell stream example.
//
// This program connects to LaserSell stream, listens for exit_signal_with_tx
// events, signs each unsigned transaction with a local keypair, and submits
// it through Helius Sender.
//
// Before running:
// - Replace API key and wallet placeholders.
// - Point keypairPath to a local Solana keypair file.
//
// Behavior notes:
// - Submission errors are logged and the stream keeps running.
// - If closeAfterSubmit is true, the example requests close after each send.
package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"log"
	"sync"

	"github.com/gagliardetto/solana-go"
	lasersell "github.com/lasersell/lasersell-sdk/go"
	"github.com/lasersell/lasersell-sdk/go/stream"
)

func main() {
	ctx := context.Background()

	apiKey := "REPLACE_WITH_API_KEY"
	walletPubkeys := []string{
		"REPLACE_WITH_WALLET_PUBKEY_1",
		"REPLACE_WITH_WALLET_PUBKEY_2",
	}
	keypairPath := "REPLACE_WITH_KEYPAIR_PATH"
	closeAfterSubmit := false

	privateKey, err := solana.PrivateKeyFromSolanaKeygenFile(keypairPath)
	if err != nil {
		log.Fatalf("read keypair: %v", err)
	}

	client := stream.NewStreamClient(apiKey)
	session, err := stream.ConnectSession(ctx, client, stream.StreamConfigure{
		WalletPubkeys: walletPubkeys,
		Strategy: stream.StrategyConfigMsg{
			TargetProfitPct:    5.0,
			StopLossPct:        1.5,
			DeadlineTimeoutSec: 45,
		},
	})
	if err != nil {
		log.Fatalf("connect stream: %v", err)
	}

	sender := session.Sender()
	var submissions sync.WaitGroup
	var streamErr error

	for {
		event, err := session.Recv(ctx)
		if errors.Is(err, io.EOF) {
			streamErr = errors.New("stream ended unexpectedly")
			break
		}
		if err != nil {
			streamErr = fmt.Errorf("stream receive error: %w", err)
			break
		}

		switch msg := event.Message.(type) {
		case stream.PositionOpenedServerMessage:
			fmt.Printf(
				"tracked position wallet=%s mint=%s token_account=%s\n",
				msg.WalletPubkey,
				msg.Mint,
				msg.TokenAccount,
			)

		case stream.ExitSignalWithTxServerMessage:
			// Submit exits concurrently so RPC latency never blocks websocket receive.
			exitMsg := msg
			submissions.Add(1)
			go func() {
				defer submissions.Done()

				signedTx, err := lasersell.SignUnsignedTx(exitMsg.UnsignedTxB64, privateKey)
				if err != nil {
					log.Printf("sign failed position_id=%d: %v", exitMsg.PositionID, err)
					return
				}

				signature, err := lasersell.SendViaHeliusSender(ctx, nil, signedTx)
				if err != nil {
					log.Printf(
						"send failed position_id=%d wallet=%s mint=%s: %v",
						exitMsg.PositionID,
						exitMsg.WalletPubkey,
						exitMsg.Mint,
						err,
					)
					return
				}

				fmt.Printf(
					"submitted exit tx signature=%s wallet=%s mint=%s\n",
					signature,
					exitMsg.WalletPubkey,
					exitMsg.Mint,
				)

				if closeAfterSubmit {
					if err := sender.CloseByID(exitMsg.PositionID); err != nil {
						log.Printf("close failed position_id=%d: %v", exitMsg.PositionID, err)
					}
				}
			}()

		case stream.ErrorServerMessage:
			log.Printf("stream error code=%s message=%s", msg.Code, msg.Message)
		}
	}

	log.Printf("stream closed; waiting for in-flight submissions to finish")
	submissions.Wait()
	log.Fatal(streamErr)
}
