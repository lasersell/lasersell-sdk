/**
 * Demonstrates wallet registration (required for all stream connections)
 * and copy trading with watch wallets.
 *
 * Demonstrates:
 * 1. Proving wallet ownership with prove_ownership (local Ed25519 signature).
 * 2. Registering the wallet with the LaserSell API.
 * 3. Connecting to the stream with connectWithWallets (register + connect in one step).
 * 4. Copy trading via watch wallets.
 *
 * Before running:
 * - Replace API key, wallet, and keypair placeholders.
 */
import { readFile } from "node:fs/promises";
import { Keypair } from "@solana/web3.js";
import {
  ExitApiClient,
  StreamClient,
  StrategyConfigBuilder,
  proveOwnership,
  signUnsignedTx,
  sendTransaction,
  sendTargetHeliusSender,
} from "@lasersell/lasersell-sdk";

async function main(): Promise<void> {
  const apiKey = "REPLACE_WITH_API_KEY";
  const keypairPath = "REPLACE_WITH_KEYPAIR_PATH";
  const watchWalletPubkey = "REPLACE_WITH_WATCH_WALLET_PUBKEY";

  // Load keypair
  const raw = await readFile(keypairPath, "utf8");
  const signer = Keypair.fromSecretKey(Uint8Array.from(JSON.parse(raw).map(Number)));
  const walletPubkey = signer.publicKey.toBase58();
  console.log(`Wallet: ${walletPubkey}`);

  // 1. Prove ownership (local, no network call)
  const proof = proveOwnership(signer);
  console.log("Wallet proof generated");

  // 2. Register wallet with the API
  const apiClient = ExitApiClient.withApiKey(apiKey);
  await apiClient.registerWallet(proof, "My Trading Wallet");
  console.log("Wallet registered");

  // 3. Build strategy with exit ladder
  const strategy = new StrategyConfigBuilder()
    .stopLossPct(10)
    .takeProfitLevels([
      { profit_pct: 25, sell_pct: 30, trailing_stop_pct: 0 },
      { profit_pct: 50, sell_pct: 50, trailing_stop_pct: 3 },
      { profit_pct: 100, sell_pct: 100, trailing_stop_pct: 5 },
    ])
    .liquidityGuard(true)
    .breakevenTrailPct(2)
    .build();

  // 4. Connect with registered wallet (convenience method)
  const streamClient = new StreamClient(apiKey);
  const session = await streamClient.connectWithWallets([proof], strategy, 120);
  console.log("Stream connected");

  // 5. Add a watch wallet for copy trading (optional — only needed if mirroring external wallets)
  session.sender().updateWatchWallets([
    { pubkey: watchWalletPubkey },
  ]);
  console.log(`Watching wallet: ${watchWalletPubkey}`);

  // 6. Event loop
  while (true) {
    const event = await session.recv();
    if (event === null) break;

    if (event.type === "position_opened") {
      console.log(`Position opened: ${event.handle.mint} (watched: ${event.message.watched ?? false})`);

      // Apply tighter strategy to copied positions
      if (event.message.watched) {
        session.sender().updatePositionStrategy(event.handle.position_id, {
          target_profit_pct: 30,
          stop_loss_pct: 8,
          trailing_stop_pct: 5,
        });
        console.log(`Tighter strategy set for copied position ${event.handle.position_id}`);
      }
    }

    if (event.type === "exit_signal_with_tx" && event.message.type === "exit_signal_with_tx") {
      const signed = signUnsignedTx(event.message.unsigned_tx_b64, signer);
      const signature = await sendTransaction(sendTargetHeliusSender(), signed);
      console.log(`Exit: ${signature}`);
    }
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
