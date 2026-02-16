/**
 * Build an unsigned sell transaction with the Exit API.
 *
 * Before running:
 * - Replace API key, mint, and wallet placeholders.
 * - Set `amount_tokens` in mint atomic units.
 *
 * This example prints:
 * - `unsigned_tx_b64`
 * - optional `route` metadata
 */
import {
  ExitApiClient,
  type BuildSellTxRequest,
} from "@lasersell/lasersell-sdk";

async function main(): Promise<void> {
  const client = ExitApiClient.withApiKey("REPLACE_WITH_API_KEY");

  const request: BuildSellTxRequest = {
    mint: "REPLACE_WITH_MINT",
    user_pubkey: "REPLACE_WITH_WALLET_PUBKEY",
    amount_tokens: 1_000_000,
    slippage_bps: 2_000,
    output: "SOL",
  };

  const response = await client.buildSellTx(request);
  console.log(`unsigned_tx_b64=${response.tx}`);
  if (response.route !== undefined) {
    console.log(`route=${JSON.stringify(response.route)}`);
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
