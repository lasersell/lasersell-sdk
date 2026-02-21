"""Higher-level stream session wrapper with position tracking."""

from __future__ import annotations

import asyncio
from dataclasses import dataclass

from .client import (
    StreamClient,
    StreamClientError,
    StreamConfigure,
    StreamConnection,
    StreamSender,
    _validate_strategy_and_deadline,
    strategy_config_from_optional,
)
from .proto import ServerMessage, StrategyConfigMsg


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

    def __init__(
        self,
        connection: StreamConnection,
        strategy: StrategyConfigMsg | None = None,
        deadline_timeout_sec: int = 0,
    ) -> None:
        self._connection = connection
        self._positions_by_id: dict[int, PositionHandle] = {}
        self._strategy: StrategyConfigMsg = dict(strategy or _default_strategy())
        self._deadline_timeout_sec = max(0, int(deadline_timeout_sec))
        self._opened_at: dict[str, float] = {}
        self._deadline_tasks: dict[str, asyncio.Task[None]] = {}

    @classmethod
    async def connect(
        cls,
        client: StreamClient,
        configure: StreamConfigure,
    ) -> "StreamSession":
        connection = await client.connect(configure)
        return cls(
            connection,
            configure.strategy,
            configure.deadline_timeout_sec,
        )

    @classmethod
    def from_connection(
        cls,
        connection: StreamConnection,
        strategy: StrategyConfigMsg | None = None,
        deadline_timeout_sec: int = 0,
    ) -> "StreamSession":
        return cls(connection, strategy, deadline_timeout_sec)

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

    def close(self, handle: PositionHandle | None = None) -> None:
        if handle is not None:
            self.sender().close_position(handle.token_account)
            return

        for task in self._deadline_tasks.values():
            task.cancel()
        self._deadline_tasks.clear()
        self._opened_at.clear()
        self._connection.close()

    def request_exit_signal(self, handle: PositionHandle, slippage_bps: int | None = None) -> None:
        self.sender().request_exit_signal(handle.token_account, slippage_bps)

    def update_strategy(
        self,
        strategy: StrategyConfigMsg,
        deadline_timeout_sec: int | None = None,
    ) -> None:
        deadline = self._deadline_timeout_sec if deadline_timeout_sec is None else int(deadline_timeout_sec)
        _validate_strategy_and_deadline(strategy, deadline)
        self._strategy = dict(strategy)
        if deadline_timeout_sec is not None:
            self._deadline_timeout_sec = max(0, int(deadline_timeout_sec))
        self._reschedule_all_deadlines()
        self.sender().update_strategy(strategy)

    def update_strategy_optional(
        self,
        *,
        target_profit_pct: float | None = None,
        stop_loss_pct: float | None = None,
        deadline_timeout_sec: int | None = None,
    ) -> None:
        self.update_strategy(
            strategy_config_from_optional(
                target_profit_pct=target_profit_pct,
                stop_loss_pct=stop_loss_pct,
            ),
            deadline_timeout_sec=deadline_timeout_sec,
        )

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
            self._arm_deadline(handle.token_account)
            return StreamEvent(type="position_opened", handle=handle, message=message)

        if message_type == "position_closed":
            handle = self._remove_position(
                int(message["position_id"]),
                message.get("token_account") if isinstance(message.get("token_account"), str) else None,
            )
            token_account = handle.token_account if handle is not None else message.get("token_account")
            if isinstance(token_account, str):
                self._cancel_deadline_for(token_account)
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

    def _deadline_seconds(self) -> float:
        return float(self._deadline_timeout_sec)

    def _arm_deadline(self, token_account: str) -> None:
        self._cancel_deadline_task(token_account)

        loop = asyncio.get_running_loop()
        opened_at = loop.time()
        self._opened_at[token_account] = opened_at

        deadline_sec = self._deadline_seconds()
        if deadline_sec == 0:
            return

        self._schedule_deadline(token_account, deadline_sec)

    def _reschedule_all_deadlines(self) -> None:
        for task in self._deadline_tasks.values():
            task.cancel()
        self._deadline_tasks.clear()

        deadline_sec = self._deadline_seconds()
        if deadline_sec == 0:
            return

        loop = asyncio.get_running_loop()
        now = loop.time()
        token_accounts = {handle.token_account for handle in self._positions_by_id.values()}
        for token_account in token_accounts:
            opened_at = self._opened_at.get(token_account)
            if opened_at is None:
                opened_at = now
                self._opened_at[token_account] = opened_at

            remaining = opened_at + deadline_sec - now
            if remaining <= 0:
                self._try_request_exit_signal(token_account)
                continue
            self._schedule_deadline(token_account, remaining)

    def _schedule_deadline(self, token_account: str, remaining_sec: float) -> None:
        async def _run() -> None:
            try:
                await asyncio.sleep(remaining_sec)
            except asyncio.CancelledError:
                return
            finally:
                self._deadline_tasks.pop(token_account, None)

            self._try_request_exit_signal(token_account)

        self._deadline_tasks[token_account] = asyncio.create_task(_run())

    def _try_request_exit_signal(self, token_account: str) -> None:
        if not self._has_open_position_for_token(token_account):
            return

        try:
            self.sender().request_exit_signal(token_account, None)
        except (StreamClientError, RuntimeError):
            # Ignore timer callback failures so recv loops keep running.
            return

    def _has_open_position_for_token(self, token_account: str) -> bool:
        return any(handle.token_account == token_account for handle in self._positions_by_id.values())

    def _cancel_deadline_for(self, token_account: str) -> None:
        self._cancel_deadline_task(token_account)
        self._opened_at.pop(token_account, None)

    def _cancel_deadline_task(self, token_account: str) -> None:
        task = self._deadline_tasks.pop(token_account, None)
        if task is not None:
            task.cancel()


__all__ = [
    "PositionHandle",
    "StreamEvent",
    "StreamSession",
    "StreamClient",
    "StreamClientError",
    "StreamConfigure",
    "StreamSender",
]


def _default_strategy() -> StrategyConfigMsg:
    return {
        "target_profit_pct": 0.0,
        "stop_loss_pct": 0.0,
    }
