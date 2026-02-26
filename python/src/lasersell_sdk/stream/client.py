"""Low-level stream websocket client and outbound command sender."""

from __future__ import annotations

import asyncio
import inspect
import math
from collections import deque
from dataclasses import dataclass
from typing import Any, AsyncIterator, Callable, Mapping, TypedDict, cast

from .proto import (
    ClientMessage,
    RequestExitSignalClientMessage,
    ServerMessage,
    StrategyConfigMsg,
    client_message_to_text,
    server_message_from_text,
)

MIN_RECONNECT_BACKOFF_MS = 100
MAX_RECONNECT_BACKOFF_MS = 2_000
FRAME_IDLE_SLEEP_MS = 10

STREAM_ENDPOINT = "wss://stream.lasersell.io/v1/ws"
LOCAL_STREAM_ENDPOINT = "ws://localhost:8082/v1/ws"


@dataclass(slots=True)
class StreamConfigure:
    """Stream configuration sent during initial websocket setup."""

    wallet_pubkeys: list[str]
    strategy: StrategyConfigMsg
    deadline_timeout_sec: int = 0


class OptionalStrategyConfig(TypedDict, total=False):
    target_profit_pct: float
    stop_loss_pct: float
    trailing_stop_pct: float


def strategy_config_from_optional(
    *,
    target_profit_pct: float | None = None,
    stop_loss_pct: float | None = None,
    trailing_stop_pct: float | None = None,
    sell_on_graduation: bool = False,
) -> StrategyConfigMsg:
    result: StrategyConfigMsg = {
        "target_profit_pct": float(target_profit_pct) if target_profit_pct is not None else 0.0,
        "stop_loss_pct": float(stop_loss_pct) if stop_loss_pct is not None else 0.0,
    }
    if trailing_stop_pct is not None and trailing_stop_pct > 0:
        result["trailing_stop_pct"] = float(trailing_stop_pct)
    if sell_on_graduation:
        result["sell_on_graduation"] = True
    return result


def single_wallet_stream_configure(
    wallet_pubkey: str,
    strategy: StrategyConfigMsg,
    deadline_timeout_sec: int = 0,
) -> StreamConfigure:
    """Convenience constructor for a single-wallet stream configuration."""

    return StreamConfigure(
        wallet_pubkeys=[wallet_pubkey],
        strategy=strategy,
        deadline_timeout_sec=deadline_timeout_sec,
    )


def single_wallet_stream_configure_optional(
    wallet_pubkey: str,
    *,
    target_profit_pct: float | None = None,
    stop_loss_pct: float | None = None,
    trailing_stop_pct: float | None = None,
    deadline_timeout_sec: int | None = None,
) -> StreamConfigure:
    return StreamConfigure(
        wallet_pubkeys=[wallet_pubkey],
        strategy=strategy_config_from_optional(
            target_profit_pct=target_profit_pct,
            stop_loss_pct=stop_loss_pct,
            trailing_stop_pct=trailing_stop_pct,
        ),
        deadline_timeout_sec=0 if deadline_timeout_sec is None else int(deadline_timeout_sec),
    )


class StreamClientError(Exception):
    """Error raised by stream transport and protocol handling."""

    def __init__(self, kind: str, message: str, cause: BaseException | None = None) -> None:
        super().__init__(message)
        self.kind = kind
        self.__cause__ = cause

    @classmethod
    def websocket(cls, cause: BaseException | str) -> "StreamClientError":
        return cls("websocket", f"websocket error: {cause}", _as_exception(cause))

    @classmethod
    def json(cls, cause: BaseException | str) -> "StreamClientError":
        return cls("json", f"json error: {cause}", _as_exception(cause))

    @classmethod
    def invalid_api_key_header(cls, cause: BaseException | str) -> "StreamClientError":
        return cls(
            "invalid_api_key_header",
            f"invalid api-key header: {cause}",
            _as_exception(cause),
        )

    @classmethod
    def send_queue_closed(cls) -> "StreamClientError":
        return cls("send_queue_closed", "send queue is closed")

    @classmethod
    def protocol(cls, message: str) -> "StreamClientError":
        return cls("protocol", f"protocol error: {message}")


class StreamClient:
    """Entry point for creating stream connections."""

    def __init__(self, api_key: str) -> None:
        self._api_key = api_key
        self._local = False
        self._endpoint_override: str | None = None

    def with_local_mode(self, local: bool) -> "StreamClient":
        """Enables or disables local mode endpoint routing."""

        self._local = local
        return self

    def with_endpoint(self, endpoint: str) -> "StreamClient":
        """Sets an explicit stream endpoint override."""

        self._endpoint_override = endpoint.rstrip()
        return self

    async def connect(self, configure: StreamConfigure) -> "StreamConnection":
        """Opens a configured stream connection."""

        _validate_strategy_and_deadline(configure.strategy, configure.deadline_timeout_sec)
        worker = StreamConnectionWorker(self._endpoint(), self._api_key, configure)
        await worker.wait_ready()
        return StreamConnection(worker)

    def _endpoint(self) -> str:
        if self._endpoint_override is not None:
            return self._endpoint_override
        if self._local:
            return LOCAL_STREAM_ENDPOINT
        return STREAM_ENDPOINT


class StreamConnection:
    """Active stream connection wrapper."""

    def __init__(self, worker: "StreamConnectionWorker") -> None:
        self._worker = worker

    def sender(self) -> "StreamSender":
        return StreamSender(self._worker)

    def split(self) -> tuple["StreamSender", "StreamReceiver"]:
        return self.sender(), StreamReceiver(self._worker)

    async def recv(self) -> ServerMessage | None:
        return await self._worker.recv()

    def close(self) -> None:
        self._worker.close()


class StreamReceiver:
    """Receiver view for inbound stream messages."""

    def __init__(self, worker: "StreamConnectionWorker") -> None:
        self._worker = worker

    async def recv(self) -> ServerMessage | None:
        return await self._worker.recv()

    def __aiter__(self) -> AsyncIterator[ServerMessage]:
        return self

    async def __anext__(self) -> ServerMessage:
        message = await self.recv()
        if message is None:
            raise StopAsyncIteration
        return message


class StreamSender:
    """Cloneable sender for outbound stream client messages."""

    def __init__(self, worker: "StreamConnectionWorker") -> None:
        self._worker = worker

    def send(self, message: ClientMessage) -> None:
        self._worker.enqueue(message)

    def ping(self, client_time_ms: int) -> None:
        self.send({"type": "ping", "client_time_ms": client_time_ms})

    def update_strategy(self, strategy: StrategyConfigMsg) -> None:
        self.send({"type": "update_strategy", "strategy": strategy})

    def close_position(self, selector: PositionSelectorInput) -> None:
        self.send(_close_message(_normalize_position_selector(selector)))

    def close_by_id(self, position_id: int) -> None:
        self.close_position(position_id)

    def request_exit_signal(
        self,
        selector: PositionSelectorInput,
        slippage_bps: int | None = None,
    ) -> None:
        self.send(
            _request_exit_signal_message(
                _normalize_position_selector(selector),
                slippage_bps,
            )
        )

    def request_exit_signal_by_id(
        self,
        position_id: int,
        slippage_bps: int | None = None,
    ) -> None:
        self.request_exit_signal(position_id, slippage_bps)

    def update_wallets(self, wallet_pubkeys: list[str]) -> None:
        self.send({"type": "update_wallets", "wallet_pubkeys": wallet_pubkeys})


PositionSelector = dict[str, int | str]
PositionSelectorInput = int | str | Mapping[str, object]


class StreamConnectionWorker:
    def __init__(self, endpoint: str, api_key: str, configure: StreamConfigure) -> None:
        self._endpoint = endpoint
        self._api_key = api_key
        self._configure = configure

        self._inbound: _AsyncQueue[ServerMessage] = _AsyncQueue()
        self._outbound: deque[ClientMessage] = deque()
        self._pending: deque[ClientMessage] = deque()

        self._current_socket: Any = None
        self._stopped = False

        self._ready: asyncio.Future[None] = asyncio.get_running_loop().create_future()
        self._task = asyncio.create_task(self._run())

    async def wait_ready(self) -> None:
        await self._ready

    def enqueue(self, message: ClientMessage) -> None:
        if self._stopped:
            raise StreamClientError.send_queue_closed()
        self._outbound.append(message)

    async def recv(self) -> ServerMessage | None:
        return await self._inbound.shift()

    def close(self) -> None:
        if self._stopped:
            return

        self._stopped = True
        socket = self._current_socket
        if socket is None:
            return

        try:
            loop = asyncio.get_running_loop()
        except RuntimeError:
            return

        loop.create_task(socket.close())

    async def _run(self) -> None:
        backoff_ms = MIN_RECONNECT_BACKOFF_MS

        while not self._stopped:
            try:
                outcome = await self._run_connected_session()
                if outcome == "graceful_shutdown":
                    break
                backoff_ms = MIN_RECONNECT_BACKOFF_MS
            except Exception as error:
                mapped_error = _as_stream_client_error(error)
                if not self._ready.done():
                    self._ready.set_exception(mapped_error)
                    break

            if self._stopped:
                break

            should_continue = await _sleep_with_stop(backoff_ms, lambda: self._stopped)
            if not should_continue:
                break

            backoff_ms = min(backoff_ms * 2, MAX_RECONNECT_BACKOFF_MS)

        if not self._ready.done():
            self._ready.set_exception(StreamClientError.send_queue_closed())

        self._inbound.close()

    async def _run_connected_session(self) -> str:
        socket = await _open_socket(self._endpoint, self._api_key)
        self._current_socket = socket

        try:
            first_server_message = await _recv_server_message_before_configure(socket)
            if first_server_message.get("type") != "hello_ok":
                raise StreamClientError.protocol("expected first server message to be hello_ok")
            self._inbound.push(first_server_message)

            configure_message: ClientMessage = {
                "type": "configure",
                "wallet_pubkeys": list(self._configure.wallet_pubkeys),
                "strategy": dict(self._configure.strategy),
            }
            await _send_client_message(socket, configure_message)

            configured_message = await _recv_server_message_after_configure(socket)
            self._inbound.push(configured_message)

            if not self._ready.done():
                self._ready.set_result(None)

            while not self._stopped:
                next_outbound = self._pop_next_outbound_message()
                if next_outbound is not None:
                    try:
                        await _send_client_message(socket, next_outbound)
                    except Exception:
                        self._pending.appendleft(next_outbound)
                        return "reconnect"
                    continue

                try:
                    frame = await asyncio.wait_for(
                        socket.recv(),
                        timeout=FRAME_IDLE_SLEEP_MS / 1000,
                    )
                except asyncio.TimeoutError:
                    continue
                except Exception:
                    return "reconnect"

                if isinstance(frame, str):
                    try:
                        parsed = server_message_from_text(frame)
                    except ValueError:
                        return "reconnect"

                    self._inbound.push(parsed)
                    continue

                return "reconnect"

            return "graceful_shutdown"
        finally:
            self._current_socket = None
            try:
                await socket.close()
            except Exception:
                pass

    def _pop_next_outbound_message(self) -> ClientMessage | None:
        if self._pending:
            return self._pending.popleft()

        if self._outbound:
            return self._outbound.popleft()

        return None


async def _open_socket(url: str, api_key: str) -> Any:
    websockets = _import_websockets()
    headers = {"x-api-key": api_key}

    try:
        connect_signature = inspect.signature(websockets.connect)
    except (TypeError, ValueError):
        connect_signature = None

    connect_kwargs: dict[str, object] = {
        "open_timeout": 10,
        "close_timeout": 3,
        "max_queue": 64,
    }

    if connect_signature is not None and "additional_headers" in connect_signature.parameters:
        connect_kwargs["additional_headers"] = headers
    else:
        connect_kwargs["extra_headers"] = headers

    try:
        return await websockets.connect(url, **connect_kwargs)
    except TypeError:
        # Handle runtime variations in connect() kwargs across websockets versions.
        try:
            fallback_kwargs = dict(connect_kwargs)
            fallback_kwargs.pop("additional_headers", None)
            fallback_kwargs["extra_headers"] = headers
            return await websockets.connect(url, **fallback_kwargs)
        except Exception as fallback_error:
            raise StreamClientError.invalid_api_key_header(fallback_error) from fallback_error
    except Exception as error:
        raise StreamClientError.websocket(error) from error


async def _recv_server_message_before_configure(socket: Any) -> ServerMessage:
    while True:
        try:
            frame = await socket.recv()
        except Exception as error:
            raise StreamClientError.websocket(error) from error

        if isinstance(frame, str):
            try:
                return server_message_from_text(frame)
            except ValueError as error:
                raise StreamClientError.json(error) from error

        raise StreamClientError.protocol("received non-text frame before hello_ok")


async def _recv_server_message_after_configure(socket: Any) -> ServerMessage:
    while True:
        try:
            frame = await socket.recv()
        except Exception as error:
            raise StreamClientError.websocket(error) from error

        if isinstance(frame, str):
            try:
                return server_message_from_text(frame)
            except ValueError as error:
                raise StreamClientError.json(error) from error

        raise StreamClientError.protocol(
            "received non-text frame before configure acknowledgement"
        )


async def _send_client_message(socket: Any, message: ClientMessage) -> None:
    text = client_message_to_text(message)
    try:
        await socket.send(text)
    except Exception as error:
        raise StreamClientError.websocket(error) from error


def _close_message(selector: PositionSelector) -> ClientMessage:
    if "token_account" in selector:
        return {
            "type": "close_position",
            "token_account": cast(str, selector["token_account"]),
        }

    return {
        "type": "close_position",
        "position_id": cast(int, selector["position_id"]),
    }


def _request_exit_signal_message(
    selector: PositionSelector,
    slippage_bps: int | None,
) -> RequestExitSignalClientMessage:
    message: RequestExitSignalClientMessage = {"type": "request_exit_signal"}

    if "token_account" in selector:
        message["token_account"] = cast(str, selector["token_account"])
    else:
        message["position_id"] = cast(int, selector["position_id"])

    if slippage_bps is not None:
        message["slippage_bps"] = slippage_bps

    return message


def _normalize_position_selector(selector: PositionSelectorInput) -> PositionSelector:
    if isinstance(selector, bool):
        raise StreamClientError.protocol(
            "position selector must be token account string or position id number"
        )

    if isinstance(selector, str):
        return {"token_account": selector}

    if isinstance(selector, int):
        return {"position_id": selector}

    if not isinstance(selector, Mapping):
        raise StreamClientError.protocol(
            "position selector must be token account string or position id number"
        )

    token_account = selector.get("token_account")
    if isinstance(token_account, str):
        return {"token_account": token_account}

    position_id = selector.get("position_id")
    if isinstance(position_id, int) and not isinstance(position_id, bool):
        return {"position_id": position_id}

    camel_token_account = selector.get("tokenAccount")
    if isinstance(camel_token_account, str):
        return {"token_account": camel_token_account}

    camel_position_id = selector.get("positionId")
    if isinstance(camel_position_id, int) and not isinstance(camel_position_id, bool):
        return {"position_id": camel_position_id}

    raise StreamClientError.protocol(
        "position selector must be token account string or position id number"
    )


class _AsyncQueue(asyncio.Queue[ServerMessage | None]):
    def __init__(self) -> None:
        super().__init__()
        self._closed = False

    def push(self, item: ServerMessage) -> None:
        if self._closed:
            return
        self.put_nowait(item)

    def close(self) -> None:
        if self._closed:
            return
        self._closed = True
        self.put_nowait(None)

    async def shift(self) -> ServerMessage | None:
        item = await self.get()
        return item


def _import_websockets() -> Any:
    try:
        import websockets

        return websockets
    except ModuleNotFoundError as error:
        raise StreamClientError.protocol(
            "websockets dependency is required for stream support. "
            "Install with `pip install lasersell-sdk[stream]`"
        ) from error


def _as_stream_client_error(error: BaseException) -> StreamClientError:
    if isinstance(error, StreamClientError):
        return error
    return StreamClientError.websocket(error)


async def _sleep_with_stop(ms: int, should_stop: Callable[[], bool]) -> bool:
    interval_ms = 20
    remaining = ms

    while remaining > 0:
        if should_stop():
            return False

        step = min(interval_ms, remaining)
        await asyncio.sleep(step / 1000)
        remaining -= step

    return not should_stop()


def _as_exception(value: BaseException | str) -> BaseException | None:
    if isinstance(value, BaseException):
        return value
    return None


def _validate_strategy_and_deadline(strategy: StrategyConfigMsg, deadline_timeout_sec: int) -> None:
    _validate_strategy_value(strategy["target_profit_pct"], "strategy.target_profit_pct")
    _validate_strategy_value(strategy["stop_loss_pct"], "strategy.stop_loss_pct")
    trailing = strategy.get("trailing_stop_pct", 0.0)
    if trailing:
        _validate_strategy_value(trailing, "strategy.trailing_stop_pct")

    deadline = int(deadline_timeout_sec)
    if deadline < 0:
        raise StreamClientError.protocol("deadline_timeout_sec must be >= 0")
    if strategy["target_profit_pct"] > 0 or strategy["stop_loss_pct"] > 0 or trailing > 0 or deadline > 0:
        return

    raise StreamClientError.protocol(
        "at least one of strategy.target_profit_pct, strategy.stop_loss_pct, strategy.trailing_stop_pct, or deadline_timeout_sec must be > 0"
    )


def _validate_strategy_value(value: float, field: str) -> None:
    if not math.isfinite(value):
        raise StreamClientError.protocol(f"{field} must be a finite number")
    if value < 0:
        raise StreamClientError.protocol(f"{field} must be >= 0")


__all__ = [
    "LOCAL_STREAM_ENDPOINT",
    "STREAM_ENDPOINT",
    "OptionalStrategyConfig",
    "PositionSelectorInput",
    "StreamClient",
    "StreamClientError",
    "StreamConfigure",
    "StreamConnection",
    "StreamReceiver",
    "StreamSender",
    "strategy_config_from_optional",
    "single_wallet_stream_configure",
    "single_wallet_stream_configure_optional",
]
