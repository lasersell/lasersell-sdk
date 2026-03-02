"""Build an unsigned buy transaction with the Exit API.

Before running:
- Replace API key, mint, and wallet placeholders.
- Set `amount` as a human-readable decimal (e.g. 0.001 for 0.001 SOL),
  or use `amount_in_total` in atomic units (lamports for SOL).
"""

from __future__ import annotations

import asyncio

from lasersell_sdk.exit_api import BuildBuyTxRequest, ExitApiClient


async def main() -> None:
    client = ExitApiClient.with_api_key("REPLACE_WITH_API_KEY")

    request = BuildBuyTxRequest(
        mint="REPLACE_WITH_MINT",
        user_pubkey="REPLACE_WITH_WALLET_PUBKEY",
        amount=0.001,  # 0.001 SOL
        slippage_bps=2_000,
    )

    response = await client.build_buy_tx(request)
    print(f"unsigned_tx_b64={response.tx}")
    if response.route is not None:
        print(f"route={response.route}")


if __name__ == "__main__":
    asyncio.run(main())
