"""Demonstrates wallet registration (required for all stream connections)
and copy trading with watch wallets.

Demonstrates:
1. Proving wallet ownership with prove_ownership (local Ed25519 signature).
2. Registering the wallet with the LaserSell API.
3. Connecting to the stream with connect_with_wallets.
4. Copy trading via watch wallets.
5. Per-position strategy overrides.

Before running:
- Replace API key, wallet, and keypair placeholders.
"""

from __future__ import annotations

import asyncio
import json
from pathlib import Path

from solders.keypair import Keypair

from lasersell_sdk.exit_api import ExitApiClient, prove_ownership
from lasersell_sdk.stream.client import StreamClient, StrategyConfigBuilder
from lasersell_sdk.tx import SendTargetHeliusSender, send_transaction, sign_unsigned_tx


async def main() -> None:
    api_key = "REPLACE_WITH_API_KEY"
    keypair_path = Path("REPLACE_WITH_KEYPAIR_PATH")
    watch_wallet_pubkey = "REPLACE_WITH_WATCH_WALLET_PUBKEY"

    # Load keypair
    raw = json.loads(keypair_path.read_text(encoding="utf-8"))
    signer = Keypair.from_bytes(bytes(int(v) for v in raw))
    wallet_pubkey = str(signer.pubkey())
    print(f"Wallet: {wallet_pubkey}")

    # 1. Prove ownership (local, no network call)
    proof = prove_ownership(signer)
    print("Wallet proof generated")

    # 2. Register wallet with the API
    api_client = ExitApiClient.with_api_key(api_key)
    await api_client.register_wallet(proof, label="My Trading Wallet")
    print("Wallet registered")

    # 3. Build strategy with exit ladder
    strategy = (
        StrategyConfigBuilder()
        .stop_loss_pct(10.0)
        .take_profit_levels([
            {"profit_pct": 25, "sell_pct": 30, "trailing_stop_pct": 0},
            {"profit_pct": 50, "sell_pct": 50, "trailing_stop_pct": 3},
            {"profit_pct": 100, "sell_pct": 100, "trailing_stop_pct": 5},
        ])
        .liquidity_guard(True)
        .breakeven_trail_pct(2.0)
        .build()
    )

    # 4. Connect with registered wallet
    stream_client = StreamClient(api_key)
    session = await stream_client.connect_with_wallets([proof], strategy, deadline_timeout_sec=120)
    print("Stream connected")

    # 5. Add a watch wallet for copy trading (optional — only needed if mirroring external wallets)
    session.sender().update_watch_wallets([
        {"pubkey": watch_wallet_pubkey},
    ])
    print(f"Watching wallet: {watch_wallet_pubkey}")

    # 6. Event loop
    while True:
        event = await session.recv()
        if event is None:
            break

        if event.type == "position_opened" and event.handle is not None:
            watched = getattr(event.message, "watched", False) or False
            print(f"Position opened: {event.handle.mint} (watched: {watched})")

            # Apply tighter strategy to copied positions
            if watched:
                session.sender().update_position_strategy(event.handle.position_id, {
                    "target_profit_pct": 30.0,
                    "stop_loss_pct": 8.0,
                    "trailing_stop_pct": 5.0,
                })
                print(f"Tighter strategy set for copied position {event.handle.position_id}")

        if event.type == "exit_signal_with_tx" and event.message.get("type") == "exit_signal_with_tx":
            signed = sign_unsigned_tx(event.message["unsigned_tx_b64"], signer)
            signature = await send_transaction(SendTargetHeliusSender(), signed)
            print(f"Exit: {signature}")


if __name__ == "__main__":
    asyncio.run(main())
