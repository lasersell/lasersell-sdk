package lasersell

import (
	"context"
	"errors"
	"fmt"
	"time"
)

// RetryPolicy controls retry attempts and exponential backoff behavior.
type RetryPolicy struct {
	// Maximum number of attempts including the first attempt.
	MaxAttempts int
	// Delay used before the first retry.
	InitialBackoff time.Duration
	// Upper bound for exponential backoff delay growth.
	MaxBackoff time.Duration
	// Maximum random jitter added to each retry delay.
	Jitter time.Duration
}

// LowLatencyRetryPolicy returns defaults suitable for short-lived API requests.
func LowLatencyRetryPolicy() RetryPolicy {
	return RetryPolicy{
		MaxAttempts:    2,
		InitialBackoff: 25 * time.Millisecond,
		MaxBackoff:     100 * time.Millisecond,
		Jitter:         25 * time.Millisecond,
	}
}

// DelayForAttempt computes the delay for the given 1-based attempt index.
func (p RetryPolicy) DelayForAttempt(attempt int) time.Duration {
	normalized := p.normalized()
	delay := normalized.InitialBackoff
	for i := 1; i < attempt; i++ {
		delay *= 2
		if delay > normalized.MaxBackoff {
			delay = normalized.MaxBackoff
		}
	}
	return delay + jitterDuration(normalized.Jitter, attempt)
}

func (p RetryPolicy) normalized() RetryPolicy {
	out := p
	if out.MaxAttempts <= 0 {
		out.MaxAttempts = 1
	}
	if out.InitialBackoff < 0 {
		out.InitialBackoff = 0
	}
	if out.MaxBackoff < 0 {
		out.MaxBackoff = 0
	}
	if out.MaxBackoff > 0 && out.InitialBackoff > out.MaxBackoff {
		out.InitialBackoff = out.MaxBackoff
	}
	if out.Jitter < 0 {
		out.Jitter = 0
	}
	return out
}

// Retry executes op with retries controlled by policy.
//
// op receives the 1-based attempt index. shouldRetry decides whether an error
// should trigger another attempt.
func Retry[T any](
	ctx context.Context,
	policy RetryPolicy,
	op func(attempt int) (T, error),
	shouldRetry func(error) bool,
) (T, error) {
	var zero T
	normalized := policy.normalized()

	for attempt := 1; attempt <= normalized.MaxAttempts; attempt++ {
		value, err := op(attempt)
		if err == nil {
			return value, nil
		}

		if attempt >= normalized.MaxAttempts || !shouldRetry(err) {
			return zero, err
		}

		delay := normalized.DelayForAttempt(attempt)
		if delay <= 0 {
			continue
		}

		timer := time.NewTimer(delay)
		select {
		case <-ctx.Done():
			timer.Stop()
			return zero, ctx.Err()
		case <-timer.C:
		}
	}

	return zero, errors.New("retry reached an unreachable state")
}

// TimeoutError indicates an operation exceeded its deadline.
type TimeoutError struct {
	Timeout time.Duration
}

func (e TimeoutError) Error() string {
	return fmt.Sprintf("operation timed out after %s", e.Timeout)
}

func (e TimeoutError) Is(target error) bool {
	_, ok := target.(TimeoutError)
	return ok
}

// WithTimeout applies timeout to fn while preserving parent cancellation.
func WithTimeout[T any](
	ctx context.Context,
	timeout time.Duration,
	fn func(ctx context.Context) (T, error),
) (T, error) {
	if timeout <= 0 {
		return fn(ctx)
	}

	timeoutCtx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()

	value, err := fn(timeoutCtx)
	if err != nil {
		if errors.Is(timeoutCtx.Err(), context.DeadlineExceeded) {
			var zero T
			return zero, TimeoutError{Timeout: timeout}
		}
		return value, err
	}

	return value, nil
}

func jitterDuration(maxJitter time.Duration, attempt int) time.Duration {
	if maxJitter <= 0 {
		return 0
	}

	limitNanos := uint64(maxJitter.Nanoseconds())
	if limitNanos == 0 {
		return 0
	}

	nowNanos := uint64(time.Now().UnixNano())
	mixed := nowNanos ^ (uint64(attempt) * 0x9e3779b97f4a7c15)
	return time.Duration(mixed % (limitNanos + 1))
}
