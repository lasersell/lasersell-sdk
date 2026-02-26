"""Stream protocol messages ported from lasersell-stream-proto."""

from __future__ import annotations

import json
import math
from typing import Any, Literal, NotRequired, Required, TypedDict, cast

MarketTypeMsg = Literal[
    "pump_fun",
    "pump_swap",
    "meteora_dbc",
    "meteora_damm_v2",
    "raydium_launchpad",
    "raydium_cpmm",
]


class PumpFunContextMsg(TypedDict):
    pass


class PumpSwapContextMsg(TypedDict, total=False):
    pool: Required[str]
    global_config: NotRequired[str]


class MeteoraDbcContextMsg(TypedDict):
    pool: str
    config: str
    quote_mint: str


class MeteoraDammV2ContextMsg(TypedDict):
    pool: str


class RaydiumLaunchpadContextMsg(TypedDict):
    pool: str
    config: str
    platform: str
    quote_mint: str
    user_quote_account: str


class RaydiumCpmmContextMsg(TypedDict):
    pool: str
    config: str
    quote_mint: str
    user_quote_account: str


class MarketContextMsg(TypedDict, total=False):
    market_type: Required[MarketTypeMsg]
    pumpfun: NotRequired[PumpFunContextMsg]
    pumpswap: NotRequired[PumpSwapContextMsg]
    meteora_dbc: NotRequired[MeteoraDbcContextMsg]
    meteora_damm_v2: NotRequired[MeteoraDammV2ContextMsg]
    raydium_launchpad: NotRequired[RaydiumLaunchpadContextMsg]
    raydium_cpmm: NotRequired[RaydiumCpmmContextMsg]


class StrategyConfigMsg(TypedDict, total=False):
    target_profit_pct: Required[float]
    stop_loss_pct: Required[float]
    trailing_stop_pct: NotRequired[float]
    sell_on_graduation: NotRequired[bool]


class LimitsMsg(TypedDict):
    hi_capacity: int
    pnl_flush_ms: int
    max_positions_per_session: int
    max_wallets_per_session: int
    max_positions_per_wallet: int
    max_sessions_per_api_key: int


class PingClientMessage(TypedDict):
    type: Literal["ping"]
    client_time_ms: int


class ConfigureClientMessage(TypedDict):
    type: Literal["configure"]
    wallet_pubkeys: list[str]
    strategy: StrategyConfigMsg


class UpdateStrategyClientMessage(TypedDict):
    type: Literal["update_strategy"]
    strategy: StrategyConfigMsg


class ClosePositionClientMessage(TypedDict, total=False):
    type: Required[Literal["close_position"]]
    position_id: NotRequired[int]
    token_account: NotRequired[str]


class RequestExitSignalClientMessage(TypedDict, total=False):
    type: Required[Literal["request_exit_signal"]]
    position_id: NotRequired[int]
    token_account: NotRequired[str]
    slippage_bps: NotRequired[int]


class UpdateWalletsClientMessage(TypedDict):
    type: Literal["update_wallets"]
    wallet_pubkeys: list[str]


ClientMessage = (
    PingClientMessage
    | ConfigureClientMessage
    | UpdateStrategyClientMessage
    | ClosePositionClientMessage
    | RequestExitSignalClientMessage
    | UpdateWalletsClientMessage
)


class HelloOkServerMessage(TypedDict):
    type: Literal["hello_ok"]
    session_id: int
    server_time_ms: int
    limits: LimitsMsg


class PongServerMessage(TypedDict):
    type: Literal["pong"]
    server_time_ms: int


class ErrorServerMessage(TypedDict):
    type: Literal["error"]
    code: str
    message: str


class PnlUpdateServerMessage(TypedDict):
    type: Literal["pnl_update"]
    position_id: int
    profit_units: int
    proceeds_units: int
    server_time_ms: int


class BalanceUpdateServerMessage(TypedDict, total=False):
    type: Required[Literal["balance_update"]]
    wallet_pubkey: Required[str]
    mint: Required[str]
    token_account: NotRequired[str]
    token_program: NotRequired[str]
    tokens: Required[int]
    slot: Required[int]


class PositionOpenedServerMessage(TypedDict, total=False):
    type: Required[Literal["position_opened"]]
    position_id: Required[int]
    wallet_pubkey: Required[str]
    mint: Required[str]
    token_account: Required[str]
    token_program: NotRequired[str]
    tokens: Required[int]
    entry_quote_units: Required[int]
    market_context: NotRequired[MarketContextMsg]
    slot: Required[int]


class PositionClosedServerMessage(TypedDict, total=False):
    type: Required[Literal["position_closed"]]
    position_id: Required[int]
    wallet_pubkey: Required[str]
    mint: Required[str]
    token_account: NotRequired[str]
    reason: Required[str]
    slot: Required[int]


class ExitSignalWithTxServerMessage(TypedDict, total=False):
    type: Required[Literal["exit_signal_with_tx"]]
    session_id: Required[int]
    position_id: Required[int]
    wallet_pubkey: Required[str]
    mint: Required[str]
    token_account: NotRequired[str]
    token_program: NotRequired[str]
    position_tokens: Required[int]
    profit_units: Required[int]
    reason: Required[str]
    triggered_at_ms: Required[int]
    market_context: NotRequired[MarketContextMsg]
    unsigned_tx_b64: Required[str]


ServerMessage = (
    HelloOkServerMessage
    | PongServerMessage
    | ErrorServerMessage
    | PnlUpdateServerMessage
    | BalanceUpdateServerMessage
    | PositionOpenedServerMessage
    | PositionClosedServerMessage
    | ExitSignalWithTxServerMessage
)


def client_message_from_text(text: str) -> ClientMessage:
    """Parses JSON text into a validated client message."""

    try:
        parsed = json.loads(text)
    except json.JSONDecodeError as exc:
        raise ValueError(f"decode client message JSON: {exc}") from exc
    return client_message_from_obj(parsed)


def client_message_from_obj(value: object) -> ClientMessage:
    """Parses a Python object into a validated client message."""

    obj = _as_record(value, "client message")
    message_type = _as_string(obj.get("type"), "client message.type")

    if message_type == "ping":
        return {
            "type": "ping",
            "client_time_ms": _as_int(obj.get("client_time_ms"), "ping.client_time_ms"),
        }

    if message_type == "configure":
        return {
            "type": "configure",
            "wallet_pubkeys": _parse_wallet_pubkeys(obj),
            "strategy": _parse_strategy_config(obj.get("strategy")),
        }

    if message_type == "update_strategy":
        return {
            "type": "update_strategy",
            "strategy": _parse_strategy_config(obj.get("strategy")),
        }

    if message_type == "close_position":
        message: ClosePositionClientMessage = {"type": "close_position"}
        _set_optional_int(
            message,
            "position_id",
            obj.get("position_id"),
            "close_position.position_id",
        )
        _set_optional_str(
            message,
            "token_account",
            obj.get("token_account"),
            "close_position.token_account",
        )
        return message

    if message_type in ("sell_now", "request_exit_signal"):
        message = cast(RequestExitSignalClientMessage, {"type": "request_exit_signal"})
        _set_optional_int(
            message,
            "position_id",
            obj.get("position_id"),
            "request_exit_signal.position_id",
        )
        _set_optional_str(
            message,
            "token_account",
            obj.get("token_account"),
            "request_exit_signal.token_account",
        )
        _set_optional_int(
            message,
            "slippage_bps",
            obj.get("slippage_bps"),
            "request_exit_signal.slippage_bps",
        )
        return message

    if message_type == "update_wallets":
        return {
            "type": "update_wallets",
            "wallet_pubkeys": _parse_wallet_pubkeys(obj),
        }

    raise ValueError(f"unsupported client message type: {message_type}")


def client_message_to_text(message: ClientMessage) -> str:
    """Serializes a client message to JSON text."""

    return json.dumps(message, separators=(",", ":"))


def server_message_from_text(text: str) -> ServerMessage:
    """Parses JSON text into a validated server message."""

    try:
        parsed = json.loads(text)
    except json.JSONDecodeError as exc:
        raise ValueError(f"decode server message JSON: {exc}") from exc
    return server_message_from_obj(parsed)


def server_message_from_obj(value: object) -> ServerMessage:
    """Parses a Python object into a validated server message."""

    obj = _as_record(value, "server message")
    message_type = _as_string(obj.get("type"), "server message.type")

    if message_type == "hello_ok":
        return {
            "type": "hello_ok",
            "session_id": _as_int(obj.get("session_id"), "hello_ok.session_id"),
            "server_time_ms": _as_int(obj.get("server_time_ms"), "hello_ok.server_time_ms"),
            "limits": _parse_limits(obj.get("limits")),
        }

    if message_type == "pong":
        return {
            "type": "pong",
            "server_time_ms": _as_int(obj.get("server_time_ms"), "pong.server_time_ms"),
        }

    if message_type == "error":
        return {
            "type": "error",
            "code": _as_string(obj.get("code"), "error.code"),
            "message": _as_string(obj.get("message"), "error.message"),
        }

    if message_type == "pnl_update":
        return {
            "type": "pnl_update",
            "position_id": _as_int(obj.get("position_id"), "pnl_update.position_id"),
            "profit_units": _as_int(obj.get("profit_units"), "pnl_update.profit_units"),
            "proceeds_units": _as_int(obj.get("proceeds_units"), "pnl_update.proceeds_units"),
            "server_time_ms": _as_int(obj.get("server_time_ms"), "pnl_update.server_time_ms"),
        }

    if message_type == "balance_update":
        message = cast(
            BalanceUpdateServerMessage,
            {
                "type": "balance_update",
                "wallet_pubkey": _as_string(
                    obj.get("wallet_pubkey"),
                    "balance_update.wallet_pubkey",
                ),
                "mint": _as_string(obj.get("mint"), "balance_update.mint"),
                "tokens": _as_int(obj.get("tokens"), "balance_update.tokens"),
                "slot": _as_int(obj.get("slot"), "balance_update.slot"),
            },
        )
        _set_optional_str(
            message,
            "token_account",
            obj.get("token_account"),
            "balance_update.token_account",
        )
        _set_optional_str(
            message,
            "token_program",
            obj.get("token_program"),
            "balance_update.token_program",
        )
        return message

    if message_type == "position_opened":
        message = cast(
            PositionOpenedServerMessage,
            {
                "type": "position_opened",
                "position_id": _as_int(obj.get("position_id"), "position_opened.position_id"),
                "wallet_pubkey": _as_string(
                    obj.get("wallet_pubkey"),
                    "position_opened.wallet_pubkey",
                ),
                "mint": _as_string(obj.get("mint"), "position_opened.mint"),
                "token_account": _as_string(
                    obj.get("token_account"),
                    "position_opened.token_account",
                ),
                "tokens": _as_int(obj.get("tokens"), "position_opened.tokens"),
                "entry_quote_units": _as_int(
                    obj.get("entry_quote_units"),
                    "position_opened.entry_quote_units",
                ),
                "slot": _as_int(obj.get("slot"), "position_opened.slot"),
            },
        )
        _set_optional_str(
            message,
            "token_program",
            obj.get("token_program"),
            "position_opened.token_program",
        )
        _set_optional_market_context(
            message,
            "market_context",
            obj.get("market_context"),
            "position_opened.market_context",
        )
        return message

    if message_type == "position_closed":
        message = cast(
            PositionClosedServerMessage,
            {
                "type": "position_closed",
                "position_id": _as_int(obj.get("position_id"), "position_closed.position_id"),
                "wallet_pubkey": _as_string(
                    obj.get("wallet_pubkey"),
                    "position_closed.wallet_pubkey",
                ),
                "mint": _as_string(obj.get("mint"), "position_closed.mint"),
                "reason": _as_string(obj.get("reason"), "position_closed.reason"),
                "slot": _as_int(obj.get("slot"), "position_closed.slot"),
            },
        )
        _set_optional_str(
            message,
            "token_account",
            obj.get("token_account"),
            "position_closed.token_account",
        )
        return message

    if message_type == "exit_signal_with_tx":
        message = cast(
            ExitSignalWithTxServerMessage,
            {
                "type": "exit_signal_with_tx",
                "session_id": _as_int(obj.get("session_id"), "exit_signal_with_tx.session_id"),
                "position_id": _as_int(
                    obj.get("position_id"),
                    "exit_signal_with_tx.position_id",
                ),
                "wallet_pubkey": _as_string(
                    obj.get("wallet_pubkey"),
                    "exit_signal_with_tx.wallet_pubkey",
                ),
                "mint": _as_string(obj.get("mint"), "exit_signal_with_tx.mint"),
                "position_tokens": _as_int(
                    obj.get("position_tokens"),
                    "exit_signal_with_tx.position_tokens",
                ),
                "profit_units": _as_int(
                    obj.get("profit_units"),
                    "exit_signal_with_tx.profit_units",
                ),
                "reason": _as_string(obj.get("reason"), "exit_signal_with_tx.reason"),
                "triggered_at_ms": _as_int(
                    obj.get("triggered_at_ms"),
                    "exit_signal_with_tx.triggered_at_ms",
                ),
                "unsigned_tx_b64": _as_string(
                    obj.get("unsigned_tx_b64"),
                    "exit_signal_with_tx.unsigned_tx_b64",
                ),
            },
        )
        _set_optional_str(
            message,
            "token_account",
            obj.get("token_account"),
            "exit_signal_with_tx.token_account",
        )
        _set_optional_str(
            message,
            "token_program",
            obj.get("token_program"),
            "exit_signal_with_tx.token_program",
        )
        _set_optional_market_context(
            message,
            "market_context",
            obj.get("market_context"),
            "exit_signal_with_tx.market_context",
        )
        return message

    raise ValueError(f"unsupported server message type: {message_type}")


def server_message_to_text(message: ServerMessage) -> str:
    """Serializes a server message to JSON text."""

    return json.dumps(message, separators=(",", ":"))


def _parse_wallet_pubkeys(obj: dict[str, object]) -> list[str]:
    wallets = obj.get("wallet_pubkeys", obj.get("wallet_pubkey"))

    if isinstance(wallets, str):
        return [wallets]

    if isinstance(wallets, list):
        return [_as_string(item, f"configure.wallet_pubkeys[{idx}]") for idx, item in enumerate(wallets)]

    raise ValueError(
        "configure.wallet_pubkeys must be a string or string[] (legacy wallet_pubkey is supported)"
    )


def _parse_strategy_config(value: object) -> StrategyConfigMsg:
    obj = _as_record(value, "strategy")

    result: StrategyConfigMsg = {
        "target_profit_pct": _as_float(obj.get("target_profit_pct"), "strategy.target_profit_pct"),
        "stop_loss_pct": _as_float(obj.get("stop_loss_pct"), "strategy.stop_loss_pct"),
    }

    trailing_raw = obj.get("trailing_stop_pct")
    if trailing_raw is not None:
        result["trailing_stop_pct"] = _as_float(trailing_raw, "strategy.trailing_stop_pct")

    sell_on_graduation_raw = obj.get("sell_on_graduation")
    if sell_on_graduation_raw is not None:
        if not isinstance(sell_on_graduation_raw, bool):
            raise ValueError("strategy.sell_on_graduation must be a boolean")
        result["sell_on_graduation"] = sell_on_graduation_raw

    return result


def _parse_limits(value: object) -> LimitsMsg:
    obj = _as_record(value, "limits")

    max_wallets_per_session = _optional_int(
        obj.get("max_wallets_per_session"),
        "limits.max_wallets_per_session",
    )
    max_positions_per_wallet = _optional_int(
        obj.get("max_positions_per_wallet"),
        "limits.max_positions_per_wallet",
    )
    max_sessions_per_api_key = _optional_int(
        obj.get("max_sessions_per_api_key"),
        "limits.max_sessions_per_api_key",
    )

    return {
        "hi_capacity": _as_int(obj.get("hi_capacity"), "limits.hi_capacity"),
        "pnl_flush_ms": _as_int(obj.get("pnl_flush_ms"), "limits.pnl_flush_ms"),
        "max_positions_per_session": _as_int(
            obj.get("max_positions_per_session"),
            "limits.max_positions_per_session",
        ),
        "max_wallets_per_session": 0 if max_wallets_per_session is None else max_wallets_per_session,
        "max_positions_per_wallet": 0 if max_positions_per_wallet is None else max_positions_per_wallet,
        "max_sessions_per_api_key": 0
        if max_sessions_per_api_key is None
        else max_sessions_per_api_key,
    }


def _set_optional_market_context(
    obj: dict[str, object],
    key: str,
    value: object,
    path: str,
) -> None:
    parsed = _optional_market_context(value, path)
    if parsed is not None:
        obj[key] = parsed


def _optional_market_context(value: object, path: str) -> MarketContextMsg | None:
    if value is None:
        return None
    return _parse_market_context(value, path)


def _parse_market_context(value: object, path: str) -> MarketContextMsg:
    obj = _as_record(value, path)

    result: MarketContextMsg = {
        "market_type": _parse_market_type(
            obj.get("market_type"),
            f"{path}.market_type",
        )
    }

    _set_optional_object(result, "pumpfun", obj.get("pumpfun"), f"{path}.pumpfun")
    _set_optional_object(result, "pumpswap", obj.get("pumpswap"), f"{path}.pumpswap")
    _set_optional_object(result, "meteora_dbc", obj.get("meteora_dbc"), f"{path}.meteora_dbc")
    _set_optional_object(
        result,
        "meteora_damm_v2",
        obj.get("meteora_damm_v2"),
        f"{path}.meteora_damm_v2",
    )
    _set_optional_object(
        result,
        "raydium_launchpad",
        obj.get("raydium_launchpad"),
        f"{path}.raydium_launchpad",
    )
    _set_optional_object(
        result,
        "raydium_cpmm",
        obj.get("raydium_cpmm"),
        f"{path}.raydium_cpmm",
    )

    return result


def _parse_market_type(value: object, path: str) -> MarketTypeMsg:
    market_type = _as_string(value, path)
    allowed_market_types: set[str] = {
        "pump_fun",
        "pump_swap",
        "meteora_dbc",
        "meteora_damm_v2",
        "raydium_launchpad",
        "raydium_cpmm",
    }
    if market_type not in allowed_market_types:
        raise ValueError(f"invalid market type at {path}: {market_type}")
    return cast(MarketTypeMsg, market_type)


def _set_optional_int(
    obj: dict[str, object],
    key: str,
    value: object,
    path: str,
) -> None:
    parsed = _optional_int(value, path)
    if parsed is not None:
        obj[key] = parsed


def _set_optional_str(
    obj: dict[str, object],
    key: str,
    value: object,
    path: str,
) -> None:
    parsed = _optional_string(value, path)
    if parsed is not None:
        obj[key] = parsed


def _set_optional_object(
    obj: dict[str, object],
    key: str,
    value: object,
    path: str,
) -> None:
    parsed = _optional_object(value, path)
    if parsed is not None:
        obj[key] = parsed


def _optional_string(value: object, path: str) -> str | None:
    if value is None:
        return None
    return _as_string(value, path)


def _optional_int(value: object, path: str) -> int | None:
    if value is None:
        return None
    return _as_int(value, path)


def _optional_object(value: object, path: str) -> dict[str, object] | None:
    if value is None:
        return None
    return _as_record(value, path)


def _as_record(value: object, path: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise ValueError(f"{path} must be an object")
    return cast(dict[str, object], value)


def _as_string(value: object, path: str) -> str:
    if not isinstance(value, str):
        raise ValueError(f"{path} must be a string")
    return value


def _as_float(value: object, path: str) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise ValueError(f"{path} must be a finite number")

    number = float(value)
    if not math.isfinite(number):
        raise ValueError(f"{path} must be a finite number")
    return number


def _as_int(value: object, path: str) -> int:
    if isinstance(value, bool):
        raise ValueError(f"{path} must be an integer")

    if isinstance(value, int):
        return value

    if isinstance(value, float) and value.is_integer() and math.isfinite(value):
        return int(value)

    raise ValueError(f"{path} must be an integer")


__all__ = [
    "BalanceUpdateServerMessage",
    "ClientMessage",
    "ClosePositionClientMessage",
    "ConfigureClientMessage",
    "ErrorServerMessage",
    "ExitSignalWithTxServerMessage",
    "HelloOkServerMessage",
    "LimitsMsg",
    "MarketContextMsg",
    "MarketTypeMsg",
    "PnlUpdateServerMessage",
    "PositionClosedServerMessage",
    "PositionOpenedServerMessage",
    "RequestExitSignalClientMessage",
    "ServerMessage",
    "StrategyConfigMsg",
    "UpdateStrategyClientMessage",
    "UpdateWalletsClientMessage",
    "client_message_from_obj",
    "client_message_from_text",
    "client_message_to_text",
    "server_message_from_obj",
    "server_message_from_text",
    "server_message_to_text",
]
