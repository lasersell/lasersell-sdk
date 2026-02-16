"""Build an unsigned sell transaction with the Exit API.

Before running:
- Replace API key, mint, and wallet placeholders.
- Set `amount_tokens` in mint atomic units.
"""

from __future__ import annotations

import asyncio

from lasersell_sdk.exit_api import BuildSellTxRequest, ExitApiClient, SellOutput


async def main() -> None:
    client = ExitApiClient.with_api_key("REPLACE_WITH_API_KEY")

    request = BuildSellTxRequest(
        mint="REPLACE_WITH_MINT",
        user_pubkey="REPLACE_WITH_WALLET_PUBKEY",
        amount_tokens=1_000_000,
        slippage_bps=2_000,
        output=SellOutput.SOL,
    )

    response = await client.build_sell_tx(request)
    print(f"unsigned_tx_b64={response.tx}")
    if response.route is not None:
        print(f"route={response.route}")


if __name__ == "__main__":
    asyncio.run(main())
