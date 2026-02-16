"""Build an unsigned buy transaction with the Exit API.

Before running:
- Replace API key, mint, and wallet placeholders.
- Set `amount_quote_units` in quote-asset atomic units
  (for SOL quote, this is lamports).
"""

from __future__ import annotations

import asyncio

from lasersell_sdk.exit_api import BuildBuyTxRequest, ExitApiClient


async def main() -> None:
    client = ExitApiClient.with_api_key("REPLACE_WITH_API_KEY")

    request = BuildBuyTxRequest(
        mint="REPLACE_WITH_MINT",
        user_pubkey="REPLACE_WITH_WALLET_PUBKEY",
        amount_quote_units=1_000_000,
        slippage_bps=2_000,
    )

    response = await client.build_buy_tx(request)
    print(f"unsigned_tx_b64={response.tx}")
    if response.route is not None:
        print(f"route={response.route}")


if __name__ == "__main__":
    asyncio.run(main())
