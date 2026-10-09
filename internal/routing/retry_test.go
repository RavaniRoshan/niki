package routing

import (
	"net/http"
	"testing"
	"time"
)

func TestEvaluateRetryHeaders(t *testing.T) {
	// Status 429 with retry-after in seconds
	h1 := http.Header{}
	h1.Set("Retry-After", "5")
	dec1 := EvaluateRetry(429, h1, 1)
	if !dec1.ShouldRetry {
		t.Fatalf("expected 429 to retry")
	}
	if dec1.WaitDelay != 5*time.Second {
		t.Fatalf("expected 5s wait delay, got %v", dec1.WaitDelay)
	}

	// Status 429 with retry-after-ms
	h2 := http.Header{}
	h2.Set("retry-after-ms", "1500")
	dec2 := EvaluateRetry(429, h2, 1)
	if dec2.WaitDelay != 1500*time.Millisecond {
		t.Fatalf("expected 1500ms wait delay, got %v", dec2.WaitDelay)
	}

	// Non-retryable 401
	dec3 := EvaluateRetry(401, http.Header{}, 1)
	if dec3.ShouldRetry {
		t.Fatalf("401 must not retry")
	}

	// Max retries exceeded
	dec4 := EvaluateRetry(503, http.Header{}, 6)
	if dec4.ShouldRetry {
		t.Fatalf("attempt 6 should exceed max retries")
	}
}
