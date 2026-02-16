export interface RetryPolicy {
  max_attempts: number;
  initial_backoff_ms: number;
  max_backoff_ms: number;
  jitter_ms: number;
}

export const LOW_LATENCY_RETRY_POLICY: RetryPolicy = {
  max_attempts: 2,
  initial_backoff_ms: 25,
  max_backoff_ms: 100,
  jitter_ms: 25,
};

export function lowLatencyRetryPolicy(): RetryPolicy {
  return { ...LOW_LATENCY_RETRY_POLICY };
}

export function delayForAttempt(policy: RetryPolicy, attempt: number): number {
  let delay = Math.max(0, policy.initial_backoff_ms);

  for (let i = 1; i < attempt; i += 1) {
    delay = Math.min(delay * 2, Math.max(0, policy.max_backoff_ms));
  }

  return delay + jitterDurationMs(Math.max(0, policy.jitter_ms));
}

export async function retryAsync<T, E>(
  policy: RetryPolicy,
  op: (attempt: number) => Promise<T>,
  shouldRetry: (error: E) => boolean,
): Promise<T> {
  const maxAttempts = Math.max(1, policy.max_attempts);

  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    try {
      return await op(attempt);
    } catch (error) {
      const typedError = error as E;
      if (attempt >= maxAttempts || !shouldRetry(typedError)) {
        throw typedError;
      }

      const delay = delayForAttempt(policy, attempt);
      if (delay > 0) {
        await sleep(delay);
      }
    }
  }

  throw new Error("retryAsync reached an unreachable state");
}

export class TimeoutError extends Error {
  readonly timeout_ms: number;

  constructor(timeoutMs: number, message = "operation timed out") {
    super(message);
    this.name = "TimeoutError";
    this.timeout_ms = timeoutMs;
  }
}

export async function withTimeout<T>(
  timeoutMs: number,
  promise: Promise<T>,
): Promise<T> {
  if (timeoutMs <= 0) {
    return promise;
  }

  return await new Promise<T>((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new TimeoutError(timeoutMs));
    }, timeoutMs);

    promise
      .then((value) => {
        clearTimeout(timer);
        resolve(value);
      })
      .catch((error) => {
        clearTimeout(timer);
        reject(error);
      });
  });
}

function jitterDurationMs(maxJitterMs: number): number {
  if (maxJitterMs <= 0) {
    return 0;
  }

  return Math.floor(Math.random() * (maxJitterMs + 1));
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}
