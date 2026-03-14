"""Auto-sell stream example.

Behavior:
- Connects to stream websocket and tracks positions.
- On each `exit_signal_with_tx`, signs and submits transaction.
- Keeps running after individual submission failures.

Before running:
- Replace API key and wallet placeholders.
- Set `keypair_path` to a local Solana keypair JSON file.
"""

from __future__ import annotations

import asyncio
import json
from pathlib import Path

from solders.keypair import Keypair

from lasersell_sdk.exit_api import ExitApiClient, prove_ownership
from lasersell_sdk.stream.client import StreamClient, StreamConfigure, StrategyConfigBuilder
from lasersell_sdk.stream.session import StreamSession
from lasersell_sdk.tx import SendTargetHeliusSender, send_transaction, sign_unsigned_tx


async def main() -> None:
    api_key = "REPLACE_WITH_API_KEY"
    wallet_pubkeys = [
        "REPLACE_WITH_WALLET_PUBKEY_1",
        "REPLACE_WITH_WALLET_PUBKEY_2",
    ]
    keypair_path = Path("REPLACE_WITH_KEYPAIR_PATH")
    close_after_submit = False

    signer = read_keypair_file(keypair_path)

    # Register wallet (required for stream connection)
    api_client = ExitApiClient.with_api_key(api_key)
    proof = prove_ownership(signer)
    await api_client.register_wallet(proof)

    client = StreamClient(api_key)
    session = await StreamSession.connect(
        client,
        StreamConfigure(
            wallet_pubkeys=wallet_pubkeys,
            strategy=StrategyConfigBuilder()
                .target_profit_pct(5.0)
                .stop_loss_pct(1.5)
                .take_profit_levels([
                    {"profit_pct": 3, "sell_pct": 30, "trailing_stop_pct": 0},
                    {"profit_pct": 5, "sell_pct": 100, "trailing_stop_pct": 1},
                ])
                .build(),
            deadline_timeout_sec=45,
            send_mode="helius_sender",
            tip_lamports=1000,
        ),
    )

    pending_submissions: set[asyncio.Task[None]] = set()

    while True:
        event = await session.recv()
        if event is None:
            break

        if event.type == "position_opened" and event.handle is not None:
            print(
                "tracked position "
                f"wallet={event.handle.wallet_pubkey} "
                f"mint={event.handle.mint} "
                f"token_account={event.handle.token_account}"
            )
            continue

        if event.type == "exit_signal_with_tx" and event.message.get("type") == "exit_signal_with_tx":
            message = event.message
            task = asyncio.create_task(
                submit_exit_signal(
                    position_id=int(message["position_id"]),
                    wallet_pubkey=str(message["wallet_pubkey"]),
                    mint=str(message["mint"]),
                    unsigned_tx_b64=str(message["unsigned_tx_b64"]),
                    signer=signer,
                    close_after_submit=close_after_submit,
                    session=session,
                )
            )
            pending_submissions.add(task)
            task.add_done_callback(pending_submissions.discard)
            continue

        if event.type == "message" and event.message.get("type") == "error":
            print(
                "stream error "
                f"code={event.message.get('code')} "
                f"message={event.message.get('message')}"
            )

    await asyncio.gather(*pending_submissions, return_exceptions=True)
    raise RuntimeError("stream ended unexpectedly")


async def submit_exit_signal(
    position_id: int,
    wallet_pubkey: str,
    mint: str,
    unsigned_tx_b64: str,
    signer: Keypair,
    close_after_submit: bool,
    session: StreamSession,
) -> None:
    try:
        signed_tx = sign_unsigned_tx(unsigned_tx_b64, signer)
        signature = await send_transaction(SendTargetHeliusSender(), signed_tx)

        if close_after_submit:
            session.sender().close_by_id(position_id)

        print(f"submitted exit tx signature={signature} wallet={wallet_pubkey} mint={mint}")
    except Exception as error:
        print(
            "submission error "
            f"position_id={position_id} wallet={wallet_pubkey} mint={mint}: {error}"
        )


def read_keypair_file(path: Path) -> Keypair:
    raw = path.read_text(encoding="utf-8")
    parsed = json.loads(raw)
    if not isinstance(parsed, list):
        raise ValueError("keypair file must be an array of numbers")

    secret = bytes(int(value) for value in parsed)
    return Keypair.from_bytes(secret)


if __name__ == "__main__":
    asyncio.run(main())
