"""Retry and timeout utilities."""

from __future__ import annotations

import asyncio
import random
from dataclasses import dataclass
from typing import Awaitable, Callable, TypeVar

T = TypeVar("T")


@dataclass(slots=True)
class RetryPolicy:
    """Policy controlling retry attempts and exponential backoff behavior."""

    max_attempts: int
    initial_backoff_ms: int
    max_backoff_ms: int
    jitter_ms: int


LOW_LATENCY_RETRY_POLICY = RetryPolicy(
    max_attempts=2,
    initial_backoff_ms=25,
    max_backoff_ms=100,
    jitter_ms=25,
)


def low_latency_retry_policy() -> RetryPolicy:
    """Returns a low-latency default suitable for short-lived API requests."""

    return RetryPolicy(
        max_attempts=LOW_LATENCY_RETRY_POLICY.max_attempts,
        initial_backoff_ms=LOW_LATENCY_RETRY_POLICY.initial_backoff_ms,
        max_backoff_ms=LOW_LATENCY_RETRY_POLICY.max_backoff_ms,
        jitter_ms=LOW_LATENCY_RETRY_POLICY.jitter_ms,
    )


def delay_for_attempt(policy: RetryPolicy, attempt: int) -> int:
    """Computes the delay in milliseconds to apply before a retry attempt."""

    delay = max(0, policy.initial_backoff_ms)
    max_backoff_ms = max(0, policy.max_backoff_ms)

    for _ in range(1, attempt):
        delay = min(delay * 2, max_backoff_ms)

    return delay + _jitter_duration_ms(max(0, policy.jitter_ms))


async def retry_async(
    policy: RetryPolicy,
    op: Callable[[int], Awaitable[T]],
    should_retry: Callable[[BaseException], bool],
) -> T:
    """Executes an async operation with retry behavior controlled by ``policy``."""

    max_attempts = max(1, policy.max_attempts)

    for attempt in range(1, max_attempts + 1):
        try:
            return await op(attempt)
        except Exception as error:
            if attempt >= max_attempts or not should_retry(error):
                raise

            delay_ms = delay_for_attempt(policy, attempt)
            if delay_ms > 0:
                await asyncio.sleep(delay_ms / 1000)

    raise RuntimeError("retry_async reached an unreachable state")


class TimeoutError(Exception):
    """Error raised by ``with_timeout`` when the operation times out."""

    def __init__(self, timeout_seconds: float, message: str = "operation timed out") -> None:
        super().__init__(message)
        self.timeout_seconds = timeout_seconds


async def with_timeout(timeout_seconds: float, awaitable: Awaitable[T]) -> T:
    """Applies a timeout to an async computation."""

    if timeout_seconds <= 0:
        return await awaitable

    try:
        return await asyncio.wait_for(awaitable, timeout_seconds)
    except asyncio.TimeoutError as exc:
        raise TimeoutError(timeout_seconds) from exc


def _jitter_duration_ms(max_jitter_ms: int) -> int:
    if max_jitter_ms <= 0:
        return 0
    return random.randint(0, max_jitter_ms)


__all__ = [
    "LOW_LATENCY_RETRY_POLICY",
    "RetryPolicy",
    "TimeoutError",
    "delay_for_attempt",
    "low_latency_retry_policy",
    "retry_async",
    "with_timeout",
]
