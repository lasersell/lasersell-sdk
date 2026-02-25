/**
 * Auto-sell stream example.
 *
 * Flow:
 * 1. Connect to LaserSell stream.
 * 2. Listen for `exit_signal_with_tx` events.
 * 3. Sign each unsigned tx and submit via Helius Sender.
 *
 * Before running:
 * - Replace API key and wallet placeholders.
 * - Set `keypairPath` to a local Solana keypair JSON file.
 *
 * Notes:
 * - Submission failures are logged and processing continues.
 * - If the stream ends, the program exits with an error.
 */
import { readFile } from "node:fs/promises";

import { Keypair } from "@solana/web3.js";
import {
  StreamClient,
  StreamSession,
  sendTransaction,
  sendTargetHeliusSender,
  signUnsignedTx,
} from "@lasersell/lasersell-sdk";

interface SubmittedExit {
  wallet_pubkey: string;
  mint: string;
  signature: string;
}

async function main(): Promise<void> {
  const apiKey = "REPLACE_WITH_API_KEY";
  const walletPubkeys = [
    "REPLACE_WITH_WALLET_PUBKEY_1",
    "REPLACE_WITH_WALLET_PUBKEY_2",
  ];
  const keypairPath = "REPLACE_WITH_KEYPAIR_PATH";
  const closeAfterSubmit = false;

  const signer = await readKeypairFile(keypairPath);
  const client = new StreamClient(apiKey);
  const session = await StreamSession.connect(client, {
    wallet_pubkeys: walletPubkeys,
    strategy: {
      target_profit_pct: 5,
      stop_loss_pct: 1.5,
    },
    deadline_timeout_sec: 45,
  });

  const submissions = new Set<Promise<void>>();

  while (true) {
    const event = await session.recv();
    if (event === null) {
      break;
    }

    if (event.type === "position_opened") {
      console.log(
        `tracked position wallet=${event.handle.wallet_pubkey} mint=${event.handle.mint} token_account=${event.handle.token_account}`,
      );
      continue;
    }

    if (
      event.type === "exit_signal_with_tx" &&
      event.message.type === "exit_signal_with_tx"
    ) {
      const submission = submitExitSignal(
        event.message.position_id,
        event.message.wallet_pubkey,
        event.message.mint,
        event.message.unsigned_tx_b64,
        signer,
        closeAfterSubmit,
        session,
      );
      submissions.add(submission);
      void submission.finally(() => {
        submissions.delete(submission);
      });
      continue;
    }

    if (event.type === "message" && event.message.type === "error") {
      console.error(
        `stream error code=${event.message.code} message=${event.message.message}`,
      );
    }
  }

  await Promise.allSettled([...submissions]);
  throw new Error("stream ended unexpectedly");
}

async function submitExitSignal(
  positionId: number,
  walletPubkey: string,
  mint: string,
  unsignedTxB64: string,
  signer: Keypair,
  closeAfterSubmit: boolean,
  session: StreamSession,
): Promise<void> {
  try {
    const signedTx = signUnsignedTx(unsignedTxB64, signer);
    const signature = await sendTransaction(sendTargetHeliusSender(), signedTx);

    if (closeAfterSubmit) {
      session.sender().closeById(positionId);
    }

    const submitted: SubmittedExit = {
      wallet_pubkey: walletPubkey,
      mint,
      signature,
    };
    console.log(
      `submitted exit tx signature=${submitted.signature} wallet=${submitted.wallet_pubkey} mint=${submitted.mint}`,
    );
  } catch (error) {
    console.error(
      `submission error position_id=${positionId} wallet=${walletPubkey} mint=${mint}: ${String(error)}`,
    );
  }
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
