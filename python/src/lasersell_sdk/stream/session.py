"""Higher-level stream session wrapper with position tracking."""

from __future__ import annotations

from dataclasses import dataclass

from .client import StreamClient, StreamClientError, StreamConfigure, StreamConnection, StreamSender
from .proto import ServerMessage


@dataclass(slots=True)
class PositionHandle:
    """Snapshot of a tracked stream position."""

    position_id: int
    token_account: str
    wallet_pubkey: str
    mint: str
    token_program: str | None
    tokens: int
    entry_quote_units: int


@dataclass(slots=True)
class StreamEvent:
    """Session-level event emitted by ``StreamSession.recv``."""

    type: str
    message: ServerMessage
    handle: PositionHandle | None = None


class StreamSession:
    """Stateful wrapper around a stream connection."""

    def __init__(self, connection: StreamConnection) -> None:
        self._connection = connection
        self._positions_by_id: dict[int, PositionHandle] = {}

    @classmethod
    async def connect(
        cls,
        client: StreamClient,
        configure: StreamConfigure,
    ) -> "StreamSession":
        connection = await client.connect(configure)
        return cls(connection)

    @classmethod
    def from_connection(cls, connection: StreamConnection) -> "StreamSession":
        return cls(connection)

    def sender(self) -> StreamSender:
        return self._connection.sender()

    def positions(self) -> list[PositionHandle]:
        return [
            PositionHandle(
                position_id=handle.position_id,
                token_account=handle.token_account,
                wallet_pubkey=handle.wallet_pubkey,
                mint=handle.mint,
                token_program=handle.token_program,
                tokens=handle.tokens,
                entry_quote_units=handle.entry_quote_units,
            )
            for handle in self._positions_by_id.values()
        ]

    def positions_for_wallet_mint(self, wallet: str, mint: str) -> list[PositionHandle]:
        return [
            handle
            for handle in self.positions()
            if handle.wallet_pubkey == wallet and handle.mint == mint
        ]

    def close(self, handle: PositionHandle) -> None:
        self.sender().close_position(handle.token_account)

    def request_exit_signal(self, handle: PositionHandle, slippage_bps: int | None = None) -> None:
        self.sender().request_exit_signal(handle.token_account, slippage_bps)

    async def recv(self) -> StreamEvent | None:
        message = await self._connection.recv()
        if message is None:
            return None
        return self._apply_message(message)

    def _apply_message(self, message: ServerMessage) -> StreamEvent:
        message_type = message.get("type")

        if message_type == "position_opened":
            position_id = int(message["position_id"])
            handle = PositionHandle(
                position_id=position_id,
                token_account=str(message["token_account"]),
                wallet_pubkey=str(message["wallet_pubkey"]),
                mint=str(message["mint"]),
                token_program=message.get("token_program") if isinstance(message.get("token_program"), str) else None,
                tokens=int(message["tokens"]),
                entry_quote_units=int(message["entry_quote_units"]),
            )
            self._positions_by_id[position_id] = handle
            return StreamEvent(type="position_opened", handle=handle, message=message)

        if message_type == "position_closed":
            handle = self._remove_position(
                int(message["position_id"]),
                message.get("token_account") if isinstance(message.get("token_account"), str) else None,
            )
            return StreamEvent(type="position_closed", handle=handle, message=message)

        if message_type == "exit_signal_with_tx":
            handle = self._find_position(
                int(message["position_id"]),
                message.get("token_account") if isinstance(message.get("token_account"), str) else None,
            )
            return StreamEvent(type="exit_signal_with_tx", handle=handle, message=message)

        if message_type == "pnl_update":
            handle = self._find_position(int(message["position_id"]))
            return StreamEvent(type="pnl_update", handle=handle, message=message)

        return StreamEvent(type="message", message=message)

    def _find_position(self, position_id: int, token_account: str | None = None) -> PositionHandle | None:
        handle = self._positions_by_id.get(position_id)
        if handle is not None:
            return handle

        if token_account is None:
            return None

        for candidate in self._positions_by_id.values():
            if candidate.token_account == token_account:
                return candidate

        return None

    def _remove_position(
        self,
        position_id: int,
        token_account: str | None = None,
    ) -> PositionHandle | None:
        handle = self._positions_by_id.pop(position_id, None)
        if handle is not None:
            return handle

        if token_account is None:
            return None

        for candidate_id, candidate in list(self._positions_by_id.items()):
            if candidate.token_account == token_account:
                self._positions_by_id.pop(candidate_id, None)
                return candidate

        return None


__all__ = [
    "PositionHandle",
    "StreamEvent",
    "StreamSession",
    "StreamClient",
    "StreamClientError",
    "StreamConfigure",
    "StreamSender",
]
