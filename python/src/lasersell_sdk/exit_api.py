"""Exit API client and request/response types."""

from __future__ import annotations

import asyncio
import json
import socket
from dataclasses import dataclass, field
from enum import Enum
from urllib import error as urllib_error
from urllib import request as urllib_request

from .retry import RetryPolicy, retry_async
from .stream.proto import MarketContextMsg

ERROR_BODY_SNIPPET_LEN = 220

EXIT_API_BASE_URL = "https://api.lasersell.io"
LOCAL_EXIT_API_BASE_URL = "http://localhost:8080"


class ExitApiDefaults:
    """Default tuning values used by ``ExitApiClientOptions``."""

    CONNECT_TIMEOUT_S = 0.2
    ATTEMPT_TIMEOUT_S = 0.9
    MAX_ATTEMPTS = 2
    BACKOFF_MS = 25
    JITTER_MS = 25


@dataclass(slots=True)
class ExitApiClientOptions:
    """Configuration options for constructing an ``ExitApiClient``."""

    connect_timeout_s: float = ExitApiDefaults.CONNECT_TIMEOUT_S
    attempt_timeout_s: float = ExitApiDefaults.ATTEMPT_TIMEOUT_S
    retry_policy: RetryPolicy = field(
        default_factory=lambda: RetryPolicy(
            max_attempts=ExitApiDefaults.MAX_ATTEMPTS,
            initial_backoff_ms=ExitApiDefaults.BACKOFF_MS,
            max_backoff_ms=ExitApiDefaults.BACKOFF_MS,
            jitter_ms=ExitApiDefaults.JITTER_MS,
        )
    )


class SellOutput(str, Enum):
    """Preferred output asset for sell requests."""

    SOL = "SOL"
    USD1 = "USD1"


@dataclass(slots=True)
class BuildSellTxRequest:
    """Request payload for ``POST /v1/sell``."""

    mint: str
    user_pubkey: str
    amount_tokens: int
    slippage_bps: int | None = None
    mode: str | None = None
    output: SellOutput | str | None = None
    referral_id: str | None = None
    market_context: MarketContextMsg | None = None

    def to_payload(self) -> dict[str, object]:
        payload: dict[str, object] = {
            "mint": self.mint,
            "user_pubkey": self.user_pubkey,
            "amount_tokens": self.amount_tokens,
        }

        if self.slippage_bps is not None:
            payload["slippage_bps"] = self.slippage_bps
        if self.mode is not None:
            payload["mode"] = self.mode
        if self.output is not None:
            payload["output"] = self.output.value if isinstance(self.output, SellOutput) else self.output
        if self.referral_id is not None:
            payload["referral_id"] = self.referral_id
        if self.market_context is not None:
            payload["market_context"] = dict(self.market_context)

        return payload


@dataclass(slots=True)
class BuildBuyTxRequest:
    """Request payload for ``POST /v1/buy``."""

    mint: str
    user_pubkey: str
    amount_quote_units: int
    slippage_bps: int | None = None
    mode: str | None = None
    referral_id: str | None = None

    def to_payload(self) -> dict[str, object]:
        payload: dict[str, object] = {
            "mint": self.mint,
            "user_pubkey": self.user_pubkey,
            "amount_quote_units": self.amount_quote_units,
        }

        if self.slippage_bps is not None:
            payload["slippage_bps"] = self.slippage_bps
        if self.mode is not None:
            payload["mode"] = self.mode
        if self.referral_id is not None:
            payload["referral_id"] = self.referral_id

        return payload


@dataclass(slots=True)
class BuildTxResponse:
    """Common response payload returned by buy/sell build endpoints."""

    tx: str
    route: object | None = None
    debug: object | None = None


class ExitApiError(Exception):
    """Exit API request/response error."""

    def __init__(
        self,
        kind: str,
        message: str,
        *,
        status: int | None = None,
        body: str | None = None,
        detail: str | None = None,
        cause: BaseException | None = None,
    ) -> None:
        super().__init__(message)
        self.kind = kind
        self.status = status
        self.body = body
        self.detail = detail
        self.__cause__ = cause

    @classmethod
    def transport(cls, cause: BaseException) -> "ExitApiError":
        return cls("transport", f"request failed: {cause}", cause=cause)

    @classmethod
    def http_status(cls, status: int, body: str) -> "ExitApiError":
        return cls("http_status", f"http status {status}: {body}", status=status, body=body)

    @classmethod
    def envelope_status(cls, status: str, detail: str) -> "ExitApiError":
        return cls(
            "envelope_status",
            f"exit-api status {status}: {detail}",
            detail=detail,
        )

    @classmethod
    def parse(cls, message: str, cause: BaseException | None = None) -> "ExitApiError":
        return cls("parse", f"failed to parse response: {message}", cause=cause)

    def is_retryable(self) -> bool:
        """Returns ``True`` when the error is likely transient and safe to retry."""

        if self.kind == "transport":
            return True

        if self.kind == "http_status":
            return (self.status is not None and self.status >= 500) or self.status == 429

        return False


class ExitApiClient:
    """HTTP client for building unsigned buy/sell transactions."""

    def __init__(
        self,
        api_key: str | None = None,
        options: ExitApiClientOptions | None = None,
    ) -> None:
        self._api_key = api_key
        resolved_options = options or ExitApiClientOptions()

        self._connect_timeout_s = resolved_options.connect_timeout_s
        self._attempt_timeout_s = resolved_options.attempt_timeout_s
        self._retry_policy = resolved_options.retry_policy
        self._local = False
        self._base_url_override: str | None = None

        # Keep parity with Rust no_proxy client behavior.
        self._opener = urllib_request.build_opener(urllib_request.ProxyHandler({}))

    @classmethod
    def with_api_key(cls, api_key: str) -> "ExitApiClient":
        return cls(api_key=api_key)

    @classmethod
    def with_options(
        cls,
        api_key: str | None,
        options: ExitApiClientOptions,
    ) -> "ExitApiClient":
        return cls(api_key=api_key, options=options)

    def with_local_mode(self, local: bool) -> "ExitApiClient":
        """Enables or disables local mode endpoint routing."""

        self._local = local
        return self

    def with_base_url(self, base_url: str) -> "ExitApiClient":
        """Sets an explicit Exit API base URL override."""

        self._base_url_override = base_url.rstrip("/")
        return self

    async def build_sell_tx(self, request: BuildSellTxRequest) -> BuildTxResponse:
        """Builds an unsigned sell transaction."""

        return await self._build_tx("/v1/sell", request.to_payload())

    async def build_sell_tx_b64(self, request: BuildSellTxRequest) -> str:
        """Builds an unsigned sell transaction and returns only base64 tx data."""

        response = await self.build_sell_tx(request)
        return response.tx

    async def build_buy_tx(self, request: BuildBuyTxRequest) -> BuildTxResponse:
        """Builds an unsigned buy transaction."""

        return await self._build_tx("/v1/buy", request.to_payload())

    async def _build_tx(self, path: str, payload: dict[str, object]) -> BuildTxResponse:
        endpoint = self._endpoint(path)

        async def attempt(_: int) -> BuildTxResponse:
            return await self._send_attempt(endpoint, payload)

        try:
            return await retry_async(
                self._retry_policy,
                attempt,
                lambda error: isinstance(error, ExitApiError) and error.is_retryable(),
            )
        except ExitApiError:
            raise
        except Exception as error:
            raise ExitApiError.transport(_coerce_exception(error)) from error

    def _endpoint(self, path: str) -> str:
        return f"{self._base_url()}{path}"

    def _base_url(self) -> str:
        if self._base_url_override is not None:
            return self._base_url_override
        if self._local:
            return LOCAL_EXIT_API_BASE_URL
        return EXIT_API_BASE_URL

    async def _send_attempt(self, endpoint: str, payload: dict[str, object]) -> BuildTxResponse:
        headers = {"content-type": "application/json"}
        if self._api_key is not None:
            headers["x-api-key"] = self._api_key

        timeout_s = max(self._connect_timeout_s, self._attempt_timeout_s)

        try:
            status, body = await asyncio.to_thread(
                self._post_json,
                endpoint,
                payload,
                headers,
                timeout_s,
            )
        except _TransportError as error:
            raise ExitApiError.transport(error.error) from error.error

        if not (200 <= status < 300):
            raise ExitApiError.http_status(status, summarize_error_body(body))

        return parse_build_tx_response(body)

    def _post_json(
        self,
        endpoint: str,
        payload: dict[str, object],
        headers: dict[str, str],
        timeout_s: float,
    ) -> tuple[int, str]:
        data = json.dumps(payload, separators=(",", ":")).encode("utf-8")
        req = urllib_request.Request(endpoint, data=data, headers=headers, method="POST")

        try:
            response = self._opener.open(req, timeout=timeout_s)
        except urllib_error.HTTPError as error:
            body = _read_http_error_body(error)
            return error.code, body
        except (OSError, socket.timeout, TimeoutError) as error:
            raise _TransportError(_coerce_exception(error)) from error
        except Exception as error:
            raise _TransportError(_coerce_exception(error)) from error

        with response:
            try:
                status = int(response.getcode())
                body_bytes = response.read()
            except Exception as error:
                raise _TransportError(_coerce_exception(error)) from error

        return status, body_bytes.decode("utf-8", errors="replace")


class _TransportError(Exception):
    def __init__(self, error: Exception) -> None:
        super().__init__(str(error))
        self.error = error


def parse_build_tx_response(body: str) -> BuildTxResponse:
    """Parses buy/sell build responses across supported server schemas."""

    try:
        parsed = json.loads(body)
    except json.JSONDecodeError as error:
        raise ExitApiError.parse("response was not valid JSON", error) from error

    if not isinstance(parsed, dict):
        raise ExitApiError.parse("response must be an object")

    tagged_status = _as_optional_string(parsed.get("status"))
    if tagged_status is not None:
        if tagged_status.lower() == "ok":
            tx = _as_optional_string(parsed.get("tx")) or _as_optional_string(
                parsed.get("unsigned_tx_b64")
            )
            if tx is None:
                raise ExitApiError.parse("status=ok payload missing tx")
            return BuildTxResponse(tx=tx, route=parsed.get("route"), debug=parsed.get("debug"))

        detail = (
            _as_optional_string(parsed.get("reason"))
            or _as_optional_string(parsed.get("message"))
            or _as_optional_string(parsed.get("error"))
            or "unknown failure"
        )
        raise ExitApiError.envelope_status(tagged_status, detail)

    legacy_tx = _as_optional_string(parsed.get("unsigned_tx_b64"))
    if legacy_tx is not None:
        return BuildTxResponse(tx=legacy_tx, route=parsed.get("route"), debug=parsed.get("debug"))

    bare_tx = _as_optional_string(parsed.get("tx"))
    if bare_tx is not None:
        return BuildTxResponse(tx=bare_tx, route=parsed.get("route"), debug=parsed.get("debug"))

    raise ExitApiError.parse("response did not match any supported schema")


def summarize_error_body(body: str) -> str:
    """Extracts concise error details from non-success HTTP responses."""

    try:
        parsed = json.loads(body)
    except json.JSONDecodeError:
        parsed = None

    if isinstance(parsed, dict):
        message = (
            _as_optional_string(parsed.get("error"))
            or _as_optional_string(parsed.get("message"))
            or _as_optional_string(parsed.get("reason"))
        )
        if message is not None:
            return message

    return body[:ERROR_BODY_SNIPPET_LEN]


def _as_optional_string(value: object) -> str | None:
    if isinstance(value, str):
        return value
    return None


def _read_http_error_body(error: urllib_error.HTTPError) -> str:
    try:
        body = error.read()
    except BaseException:
        return ""

    if isinstance(body, bytes):
        return body.decode("utf-8", errors="replace")

    return str(body)


def _coerce_exception(error: BaseException) -> Exception:
    if isinstance(error, Exception):
        return error
    return Exception(str(error))


DEFAULT_RETRY_POLICY = RetryPolicy(
    max_attempts=ExitApiDefaults.MAX_ATTEMPTS,
    initial_backoff_ms=ExitApiDefaults.BACKOFF_MS,
    max_backoff_ms=ExitApiDefaults.BACKOFF_MS,
    jitter_ms=ExitApiDefaults.JITTER_MS,
)


__all__ = [
    "BuildBuyTxRequest",
    "BuildSellTxRequest",
    "BuildTxResponse",
    "DEFAULT_RETRY_POLICY",
    "EXIT_API_BASE_URL",
    "ExitApiClient",
    "ExitApiClientOptions",
    "ExitApiDefaults",
    "ExitApiError",
    "LOCAL_EXIT_API_BASE_URL",
    "SellOutput",
    "parse_build_tx_response",
    "summarize_error_body",
]
