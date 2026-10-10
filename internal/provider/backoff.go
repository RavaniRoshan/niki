package provider

import (
	"math/rand"
	"strings"
	"time"
)

// JitteredBackoff returns base*2^attempt scaled into [0.5x, 1.5x) of
// that value using rnd (n, expected in [0,1) — rand.Float64 by default).
// Jitter keeps retries from stampeding in lockstep (L6).
func JitteredBackoff(base time.Duration, attempt int, rnd func() float64) time.Duration {
	if attempt < 0 {
		attempt = 0
	}
	d := float64(base) * float64(uint(1)<<uint(attempt))
	if rnd == nil {
		rnd = rand.Float64
	}
	r := rnd()
	if r < 0 {
		r = 0
	}
	if r >= 1 {
		r = 0.999999
	}
	return time.Duration(d * (0.5 + r))
}

// IsRetryable reports whether a stream-level provider error is safe to
// retry at the engine before any output was emitted for the step.
// HTTP 429/5xx are retried inside the providers themselves.
func IsRetryable(err error) bool {
	if err == nil {
		return false
	}
	msg := err.Error()
	return strings.Contains(msg, "stream truncated") ||
		strings.Contains(msg, "connection reset by peer") ||
		strings.Contains(msg, "unexpected EOF") ||
		strings.Contains(msg, "broken pipe")
}
