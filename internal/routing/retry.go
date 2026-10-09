package routing

import (
	"math"
	"math/rand"
	"net/http"
	"strconv"
	"time"
)

const (
	// DefaultInitialDelay is the baseline backoff interval.
	DefaultInitialDelay = 2000 * time.Millisecond
	// BackoffFactor is the exponential multiplication factor.
	BackoffFactor = 2.0
	// JitterFactor adds up to 25% randomized jitter.
	JitterFactor = 0.25
	// MaxDelayNoHeaders caps exponential backoff when no headers exist.
	MaxDelayNoHeaders = 30 * time.Second
	// MaxRetries is the maximum number of transient retries allowed.
	MaxRetries = 5
)

// RetryDecision contains the outcome of an error inspection.
type RetryDecision struct {
	ShouldRetry bool          `json:"should_retry"`
	WaitDelay   time.Duration `json:"wait_delay"`
	Reason      string        `json:"reason"`
}

// EvaluateRetry checks response status code and headers to determine if and how long to back off.
func EvaluateRetry(status int, headers http.Header, attempt int) RetryDecision {
	if attempt > MaxRetries {
		return RetryDecision{ShouldRetry: false, Reason: "max_retries_exceeded"}
	}

	isTransient := status == 429 || (status >= 500 && status <= 599)
	if !isTransient {
		return RetryDecision{ShouldRetry: false, Reason: "non_transient_status"}
	}

	// Check retry-after-ms header
	if msHeader := headers.Get("retry-after-ms"); msHeader != "" {
		if msVal, err := strconv.ParseFloat(msHeader, 64); err == nil && msVal > 0 {
			return RetryDecision{
				ShouldRetry: true,
				WaitDelay:   time.Duration(msVal) * time.Millisecond,
				Reason:      "retry_after_ms",
			}
		}
	}

	// Check standard Retry-After header
	if retryAfter := headers.Get("Retry-After"); retryAfter != "" {
		if secVal, err := strconv.Atoi(retryAfter); err == nil && secVal > 0 {
			return RetryDecision{
				ShouldRetry: true,
				WaitDelay:   time.Duration(secVal) * time.Second,
				Reason:      "retry_after_seconds",
			}
		}
		// Attempt RFC1123 / RFC850 / ANSIC HTTP date parsing
		if parsedTime, err := http.ParseTime(retryAfter); err == nil {
			wait := time.Until(parsedTime)
			if wait > 0 {
				return RetryDecision{
					ShouldRetry: true,
					WaitDelay:   wait,
					Reason:      "retry_after_date",
				}
			}
		}
	}

	// Fallback to exponential backoff with jitter
	base := float64(DefaultInitialDelay) * math.Pow(BackoffFactor, float64(attempt-1))
	jitter := base * JitterFactor * rand.Float64()
	delay := time.Duration(base + jitter)
	if delay > MaxDelayNoHeaders {
		delay = MaxDelayNoHeaders
	}

	return RetryDecision{
		ShouldRetry: true,
		WaitDelay:   delay,
		Reason:      "exponential_backoff",
	}
}
