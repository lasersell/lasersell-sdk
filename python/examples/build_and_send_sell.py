"""Build, sign, and submit a sell transaction.

Flow:
1. Build unsigned sell tx via Exit API.
2. Sign tx using local keypair.
3. Submit tx via standard RPC or Helius Sender.

Before running:
- Replace all placeholders, including keypair path and API key.
- Set `send_target` to `"rpc"` or `"helius_sender"`.
- If using `"rpc"`, set `rpc_url` to a valid Solana RPC endpoint.
"""

from __future__ import annotations

import asyncio
import json
from pathlib import Path

from solders.keypair import Keypair

from lasersell_sdk.exit_api import BuildSellTxRequest, ExitApiClient, SellOutput
from lasersell_sdk.tx import send_via_helius_sender, send_via_rpc, sign_unsigned_tx


async def main() -> None:
    api_key = "REPLACE_WITH_API_KEY"
    keypair_path = Path("REPLACE_WITH_KEYPAIR_PATH")
    send_target = "rpc"  # "rpc" or "helius_sender"
    rpc_url = "REPLACE_WITH_RPC_URL"

    keypair = read_keypair_file(keypair_path)
    exit_api = ExitApiClient.with_api_key(api_key)

    request = BuildSellTxRequest(
        mint="REPLACE_WITH_MINT",
        user_pubkey="REPLACE_WITH_WALLET_PUBKEY",
        amount_tokens=1_000_000,
        slippage_bps=2_000,
        output=SellOutput.SOL,
    )

    unsigned_tx_b64 = await exit_api.build_sell_tx_b64(request)
    signed_tx = sign_unsigned_tx(unsigned_tx_b64, keypair)

    if send_target == "helius_sender":
        signature = await send_via_helius_sender(signed_tx)
    elif send_target == "rpc":
        signature = await send_via_rpc(rpc_url, signed_tx)
    else:
        raise ValueError(f"send_target must be rpc or helius_sender (got {send_target!r})")

    print(f"signature={signature}")


def read_keypair_file(path: Path) -> Keypair:
    raw = path.read_text(encoding="utf-8")
    parsed = json.loads(raw)
    if not isinstance(parsed, list):
        raise ValueError("keypair file must be an array of numbers")

    secret = bytes(int(value) for value in parsed)
    return Keypair.from_bytes(secret)


if __name__ == "__main__":
    asyncio.run(main())
