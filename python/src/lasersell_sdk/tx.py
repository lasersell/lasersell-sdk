"""Transaction signing and submission helpers."""

from __future__ import annotations

import asyncio
import base64
import binascii
import json
import re
import socket
from urllib import error as urllib_error
from urllib import request as urllib_request

ERROR_BODY_SNIPPET_LEN = 220

HELIUS_SENDER_FAST_URL = "https://sender.helius-rpc.com/fast"


class TxSubmitError(Exception):
    """Error raised when signing or submitting a transaction."""

    def __init__(
        self,
        kind: str,
        message: str,
        *,
        target: str | None = None,
        status: int | None = None,
        body: str | None = None,
        cause: BaseException | None = None,
    ) -> None:
        super().__init__(message)
        self.kind = kind
        self.target = target
        self.status = status
        self.body = body
        self.__cause__ = cause

    @classmethod
    def decode_unsigned_tx(cls, cause: BaseException) -> "TxSubmitError":
        return cls("decode_unsigned_tx", f"decode unsigned tx b64: {cause}", cause=cause)

    @classmethod
    def deserialize_unsigned_tx(cls, cause: BaseException) -> "TxSubmitError":
        return cls("deserialize_unsigned_tx", f"deserialize unsigned tx: {cause}", cause=cause)

    @classmethod
    def sign_tx(cls, cause: BaseException | str) -> "TxSubmitError":
        error = cause if isinstance(cause, BaseException) else Exception(str(cause))
        return cls("sign_tx", f"sign tx: {error}", cause=error)

    @classmethod
    def serialize_tx(cls, cause: BaseException) -> "TxSubmitError":
        return cls("serialize_tx", f"serialize tx: {cause}", cause=cause)

    @classmethod
    def request_send(cls, target: str, cause: BaseException | str) -> "TxSubmitError":
        error = cause if isinstance(cause, BaseException) else Exception(str(cause))
        return cls(
            "request_send",
            f"{target} request send failed: {error}",
            target=target,
            cause=error,
        )

    @classmethod
    def response_read(cls, target: str, cause: BaseException | str) -> "TxSubmitError":
        error = cause if isinstance(cause, BaseException) else Exception(str(cause))
        return cls(
            "response_read",
            f"{target} response read failed: {error}",
            target=target,
            cause=error,
        )

    @classmethod
    def http_status(cls, target: str, status: int, body: str) -> "TxSubmitError":
        return cls(
            "http_status",
            f"{target} http {status}: {body}",
            target=target,
            status=status,
            body=body,
        )

    @classmethod
    def decode_response(
        cls,
        target: str,
        body: str,
        cause: BaseException,
    ) -> "TxSubmitError":
        return cls(
            "decode_response",
            f"{target} response decode failed: {cause}. body={body}",
            target=target,
            body=body,
            cause=cause,
        )

    @classmethod
    def rpc_error(cls, target: str, error: str) -> "TxSubmitError":
        return cls("rpc_error", f"{target} returned error: {error}", target=target)

    @classmethod
    def missing_result(cls, target: str, response: str) -> "TxSubmitError":
        return cls(
            "missing_result",
            f"{target} response missing signature: {response}",
            target=target,
        )


def sign_unsigned_tx(unsigned_tx_b64: str, keypair: object) -> object:
    """Decodes and signs an unsigned base64-encoded versioned transaction.

    This helper expects ``solders`` to be installed and a ``solders.keypair.Keypair``
    signer input.
    """

    try:
        raw = _decode_base64_strict(unsigned_tx_b64)
    except Exception as error:
        raise TxSubmitError.decode_unsigned_tx(_coerce_exception(error)) from error

    try:
        from solders.transaction import VersionedTransaction  # type: ignore[import-not-found]
    except ModuleNotFoundError as error:
        raise TxSubmitError.sign_tx(
            "solders dependency is required for signing. Install with `pip install lasersell-sdk[tx]`"
        ) from error

    try:
        unsigned_tx = VersionedTransaction.from_bytes(raw)
    except Exception as error:
        raise TxSubmitError.deserialize_unsigned_tx(_coerce_exception(error)) from error

    try:
        return VersionedTransaction(unsigned_tx.message, [keypair])
    except Exception as constructor_error:
        # Fallback for alternate transaction implementations that expose sign().
        try:
            sign_method = getattr(unsigned_tx, "sign", None)
            if callable(sign_method):
                sign_method([keypair])
                return unsigned_tx
        except Exception as sign_error:
            raise TxSubmitError.sign_tx(_coerce_exception(sign_error)) from sign_error

        raise TxSubmitError.sign_tx(_coerce_exception(constructor_error)) from constructor_error


def encode_signed_tx(tx: object) -> str:
    """Serializes a signed transaction to base64."""

    try:
        if isinstance(tx, (bytes, bytearray, memoryview)):
            raw = bytes(tx)
        else:
            raw = bytes(tx)  # solders objects implement __bytes__
        return base64.b64encode(raw).decode("ascii")
    except Exception as error:
        raise TxSubmitError.serialize_tx(_coerce_exception(error)) from error


async def send_via_helius_sender(tx: object) -> str:
    """Submits a signed transaction to Helius Sender and returns its signature."""

    tx_b64 = encode_signed_tx(tx)
    return await send_via_helius_sender_b64(tx_b64)


async def send_via_rpc(rpc_url: str, tx: object) -> str:
    """Submits a signed transaction to a standard Solana RPC endpoint."""

    tx_b64 = encode_signed_tx(tx)
    return await send_via_rpc_b64(rpc_url, tx_b64)


async def send_via_helius_sender_b64(tx_b64: str) -> str:
    """Submits a base64-encoded signed transaction via Helius Sender."""

    return await _send_transaction_b64(
        endpoint=HELIUS_SENDER_FAST_URL,
        target="helius sender",
        tx_b64=tx_b64,
        include_preflight_commitment=False,
    )


async def send_via_rpc_b64(rpc_url: str, tx_b64: str) -> str:
    """Submits a base64-encoded signed transaction via standard RPC."""

    return await _send_transaction_b64(
        endpoint=rpc_url,
        target="rpc",
        tx_b64=tx_b64,
        include_preflight_commitment=True,
    )


async def _send_transaction_b64(
    endpoint: str,
    target: str,
    tx_b64: str,
    include_preflight_commitment: bool,
) -> str:
    config: dict[str, object] = {
        "encoding": "base64",
        "skipPreflight": True,
        "maxRetries": 0,
    }
    if include_preflight_commitment:
        config["preflightCommitment"] = "processed"

    payload = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sendTransaction",
        "params": [tx_b64, config],
    }

    headers = {"content-type": "application/json"}

    try:
        status, body = await asyncio.to_thread(
            _post_json,
            endpoint,
            payload,
            headers,
            10.0,
        )
    except _RequestSendError as error:
        raise TxSubmitError.request_send(target, error.error) from error.error
    except _ResponseReadError as error:
        raise TxSubmitError.response_read(target, error.error) from error.error

    if not (200 <= status < 300):
        raise TxSubmitError.http_status(target, status, _summarize_body(body))

    try:
        parsed = json.loads(body)
    except json.JSONDecodeError as error:
        raise TxSubmitError.decode_response(target, _summarize_body(body), error) from error

    if not isinstance(parsed, dict):
        raise TxSubmitError.decode_response(
            target,
            _summarize_body(body),
            ValueError("response was not an object"),
        )

    if "error" in parsed:
        raise TxSubmitError.rpc_error(target, json.dumps(parsed["error"], separators=(",", ":")))

    result = parsed.get("result")
    if isinstance(result, str) and result:
        return result

    raise TxSubmitError.missing_result(
        target,
        json.dumps(result if result is not None else parsed, separators=(",", ":")),
    )


class _RequestSendError(Exception):
    def __init__(self, error: Exception) -> None:
        super().__init__(str(error))
        self.error = error


class _ResponseReadError(Exception):
    def __init__(self, error: Exception) -> None:
        super().__init__(str(error))
        self.error = error


def _post_json(
    endpoint: str,
    payload: dict[str, object],
    headers: dict[str, str],
    timeout_s: float,
) -> tuple[int, str]:
    data = json.dumps(payload, separators=(",", ":")).encode("utf-8")
    req = urllib_request.Request(endpoint, data=data, headers=headers, method="POST")
    opener = urllib_request.build_opener(urllib_request.ProxyHandler({}))

    try:
        response = opener.open(req, timeout=timeout_s)
    except urllib_error.HTTPError as error:
        return error.code, _read_http_error_body(error)
    except (OSError, socket.timeout, TimeoutError) as error:
        raise _RequestSendError(_coerce_exception(error)) from error
    except Exception as error:
        raise _RequestSendError(_coerce_exception(error)) from error

    with response:
        try:
            status = int(response.getcode())
            body_bytes = response.read()
        except Exception as error:
            raise _ResponseReadError(_coerce_exception(error)) from error

    return status, body_bytes.decode("utf-8", errors="replace")


def _decode_base64_strict(value: str) -> bytes:
    normalized = value.strip()
    if not normalized:
        raise ValueError("base64 payload is empty")

    if not re.fullmatch(r"[A-Za-z0-9+/]*={0,2}", normalized) or len(normalized) % 4 != 0:
        raise ValueError("invalid base64 encoding")

    try:
        return base64.b64decode(normalized, validate=True)
    except (ValueError, binascii.Error) as error:
        raise ValueError("invalid base64 encoding") from error


def _summarize_body(body: str) -> str:
    return body[:ERROR_BODY_SNIPPET_LEN]


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


__all__ = [
    "HELIUS_SENDER_FAST_URL",
    "TxSubmitError",
    "encode_signed_tx",
    "send_via_helius_sender",
    "send_via_helius_sender_b64",
    "send_via_rpc",
    "send_via_rpc_b64",
    "sign_unsigned_tx",
]
