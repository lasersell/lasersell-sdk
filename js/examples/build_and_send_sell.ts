/**
 * Build, sign, and submit a sell transaction.
 *
 * Flow:
 * 1. Build unsigned tx with Exit API.
 * 2. Sign tx using a local Solana keypair JSON file.
 * 3. Send tx via either standard RPC or Helius Sender.
 *
 * Before running:
 * - Replace all placeholders, including `keypairPath`.
 * - For `sendTarget = "rpc"`, provide a valid `rpcUrl`.
 */
import { readFile } from "node:fs/promises";

import { Keypair } from "@solana/web3.js";
import {
  ExitApiClient,
  sendTransaction,
  sendTargetHeliusSender,
  sendTargetRpc,
  signUnsignedTx,
  type BuildSellTxRequest,
} from "@lasersell/lasersell-sdk";

async function main(): Promise<void> {
  const apiKey = "REPLACE_WITH_API_KEY";
  const keypairPath = "REPLACE_WITH_KEYPAIR_PATH";
  const sendTargetInput: string = "rpc";
  const sendTarget: "rpc" | "helius_sender" =
    sendTargetInput === "helius_sender" ? "helius_sender" : "rpc";
  const rpcUrl = "REPLACE_WITH_RPC_URL";

  const keypair = await readKeypairFile(keypairPath);

  const client = ExitApiClient.withApiKey(apiKey);
  const request: BuildSellTxRequest = {
    mint: "REPLACE_WITH_MINT",
    user_pubkey: "REPLACE_WITH_WALLET_PUBKEY",
    amount_tokens: 1_000_000,
    slippage_bps: 2_000,
    output: "SOL",
  };

  const unsignedTxB64 = await client.buildSellTxB64(request);
  const signedTx = signUnsignedTx(unsignedTxB64, keypair);

  const target =
    sendTarget === "helius_sender"
      ? sendTargetHeliusSender()
      : sendTargetRpc(rpcUrl);
  const signature = await sendTransaction(target, signedTx);

  console.log(`signature=${signature}`);
}

async function readKeypairFile(path: string): Promise<Keypair> {
  const raw = await readFile(path, "utf8");
  const parsed = JSON.parse(raw) as unknown;
  if (!Array.isArray(parsed)) {
    throw new Error("keypair file must be an array of numbers");
  }

  const bytes = Uint8Array.from(parsed.map((value) => Number(value)));
  return Keypair.fromSecretKey(bytes);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
