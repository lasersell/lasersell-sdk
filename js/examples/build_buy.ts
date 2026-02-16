/**
 * Build an unsigned buy transaction with the Exit API.
 *
 * Before running:
 * - Replace API key, mint, and wallet placeholders.
 * - Set `amount_quote_units` in quote-asset atomic units
 *   (for SOL, this is lamports).
 *
 * This example prints:
 * - `unsigned_tx_b64`
 * - optional `route` metadata
 */
import {
  ExitApiClient,
  type BuildBuyTxRequest,
} from "@lasersell/lasersell-sdk";

async function main(): Promise<void> {
  const client = ExitApiClient.withApiKey("REPLACE_WITH_API_KEY");

  const request: BuildBuyTxRequest = {
    mint: "REPLACE_WITH_MINT",
    user_pubkey: "REPLACE_WITH_WALLET_PUBKEY",
    amount_quote_units: 1_000_000,
    slippage_bps: 2_000,
  };

  const response = await client.buildBuyTx(request);
  console.log(`unsigned_tx_b64=${response.tx}`);
  if (response.route !== undefined) {
    console.log(`route=${JSON.stringify(response.route)}`);
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
