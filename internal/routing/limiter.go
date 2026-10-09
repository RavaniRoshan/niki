package routing

import (
	"context"
	"fmt"
	"sync"
	"time"
)

// RateLimiter manages request cadence using a token-bucket algorithm to prevent 429 errors.
type RateLimiter struct {
	mu          sync.Mutex
	ratePerMin  int
	tokens      float64
	lastRefill  time.Time
	tokenPerSec float64
}

// NewRateLimiter creates a RateLimiter with requests-per-minute cap.
func NewRateLimiter(rpm int) *RateLimiter {
	if rpm <= 0 {
		rpm = 60
	}
	return &RateLimiter{
		ratePerMin:  rpm,
		tokens:      float64(rpm),
		tokenPerSec: float64(rpm) / 60.0,
		lastRefill:  time.Now(),
	}
}

// Wait blocks until a request token is available or context is cancelled.
func (rl *RateLimiter) Wait(ctx context.Context) error {
	for {
		rl.mu.Lock()
		now := time.Now()
		elapsed := now.Sub(rl.lastRefill).Seconds()
		rl.tokens += elapsed * rl.tokenPerSec
		if rl.tokens > float64(rl.ratePerMin) {
			rl.tokens = float64(rl.ratePerMin)
		}
		rl.lastRefill = now

		if rl.tokens >= 1.0 {
			rl.tokens -= 1.0
			rl.mu.Unlock()
			return nil
		}

		missing := 1.0 - rl.tokens
		sleepTime := time.Duration((missing / rl.tokenPerSec) * float64(time.Second))
		rl.mu.Unlock()

		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-time.After(sleepTime):
		}
	}
}

// CostGuard enforces maximum session cost bounds.
type CostGuard struct {
	mu          sync.Mutex
	maxSpendUSD float64
}

// NewCostGuard initializes a CostGuard.
func NewCostGuard(maxSpendUSD float64) *CostGuard {
	return &CostGuard{
		maxSpendUSD: maxSpendUSD,
	}
}

// Check verifies whether accumulated cost exceeds max spend.
func (cg *CostGuard) Check(accumulatedCost float64) error {
	cg.mu.Lock()
	defer cg.mu.Unlock()

	if cg.maxSpendUSD > 0 && accumulatedCost >= cg.maxSpendUSD {
		return fmt.Errorf("session cost guard triggered: spend $%.4f exceeds limit $%.2f", accumulatedCost, cg.maxSpendUSD)
	}
	return nil
}
