//! Retry and timeout utilities.
//!
//! The helpers in this module are transport-agnostic and are used by clients
//! that need bounded retries with lightweight jitter.

use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tracing::debug;

/// Policy controlling retry attempts and exponential backoff behavior.
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    /// Maximum number of attempts including the first attempt.
    pub max_attempts: usize,
    /// Delay used before the first retry.
    pub initial_backoff: Duration,
    /// Upper bound for exponential backoff delay growth.
    pub max_backoff: Duration,
    /// Maximum random jitter added to each retry delay.
    pub jitter: Duration,
}

impl RetryPolicy {
    /// Returns a low-latency default suitable for short-lived API requests.
    pub fn low_latency() -> Self {
        Self {
            max_attempts: 2,
            initial_backoff: Duration::from_millis(25),
            max_backoff: Duration::from_millis(100),
            jitter: Duration::from_millis(25),
        }
    }

    /// Computes the delay to apply before the given retry attempt.
    ///
    /// `attempt` is 1-based and should correspond to the current attempt index.
    pub fn delay_for_attempt(&self, attempt: usize) -> Duration {
        let mut delay = self.initial_backoff;
        for _ in 1..attempt {
            delay = std::cmp::min(delay.saturating_mul(2), self.max_backoff);
        }
        delay + jitter_duration(self.jitter, attempt)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self::low_latency()
    }
}

/// Executes an async operation with retry behavior controlled by `policy`.
///
/// `op` receives the 1-based attempt number and must return a future that
/// resolves to the operation result. `should_retry` determines whether each
/// error is retryable.
pub async fn retry_async<T, E, Op, Fut, ShouldRetry>(
    policy: &RetryPolicy,
    mut op: Op,
    mut should_retry: ShouldRetry,
) -> Result<T, E>
where
    Op: FnMut(usize) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    ShouldRetry: FnMut(&E) -> bool,
{
    let max_attempts = policy.max_attempts.max(1);

    for attempt in 1..=max_attempts {
        match op(attempt).await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if attempt >= max_attempts || !should_retry(&error) {
                    return Err(error);
                }

                let delay = policy.delay_for_attempt(attempt);
                debug!(
                    event = "retry_attempt_failed",
                    attempt,
                    max_attempts,
                    delay_ms = delay.as_millis() as u64
                );
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    unreachable!("max_attempts is always at least 1")
}

/// Applies a timeout to an async computation.
pub async fn with_timeout<T, Fut>(
    timeout: Duration,
    future: Fut,
) -> Result<T, tokio::time::error::Elapsed>
where
    Fut: Future<Output = T>,
{
    tokio::time::timeout(timeout, future).await
}

fn jitter_duration(max_jitter: Duration, attempt: usize) -> Duration {
    if max_jitter.is_zero() {
        return Duration::ZERO;
    }

    let limit_nanos = max_jitter.as_nanos().min(u64::MAX as u128) as u64;
    if limit_nanos == 0 {
        return Duration::ZERO;
    }

    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64;
    let mixed = now_nanos ^ ((attempt as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    Duration::from_nanos(mixed % (limit_nanos + 1))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use super::{retry_async, RetryPolicy};

    #[test]
    fn retries_until_success() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let calls = Arc::new(AtomicUsize::new(0));
            let policy = RetryPolicy {
                max_attempts: 3,
                initial_backoff: Duration::from_millis(1),
                max_backoff: Duration::from_millis(2),
                jitter: Duration::from_millis(0),
            };

            let result = retry_async(
                &policy,
                {
                    let calls = Arc::clone(&calls);
                    move |_| {
                        let calls = Arc::clone(&calls);
                        async move {
                            let value = calls.fetch_add(1, Ordering::SeqCst);
                            if value < 2 {
                                Err("retry")
                            } else {
                                Ok("ok")
                            }
                        }
                    }
                },
                |_| true,
            )
            .await;

            assert_eq!(result.expect("success"), "ok");
            assert_eq!(calls.load(Ordering::SeqCst), 3);
        });
    }

    #[test]
    fn stops_when_retry_predicate_rejects() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime");

        runtime.block_on(async {
            let calls = Arc::new(AtomicUsize::new(0));
            let policy = RetryPolicy {
                max_attempts: 5,
                initial_backoff: Duration::from_millis(1),
                max_backoff: Duration::from_millis(2),
                jitter: Duration::from_millis(0),
            };

            let result: Result<(), &str> = retry_async(
                &policy,
                {
                    let calls = Arc::clone(&calls);
                    move |_| {
                        let calls = Arc::clone(&calls);
                        async move {
                            calls.fetch_add(1, Ordering::SeqCst);
                            Err("fatal")
                        }
                    }
                },
                |_| false,
            )
            .await;

            assert_eq!(result.expect_err("expected failure"), "fatal");
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        });
    }
}
