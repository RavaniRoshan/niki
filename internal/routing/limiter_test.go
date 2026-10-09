package routing

import (
	"context"
	"testing"
	"time"
)

func TestRateLimiter(t *testing.T) {
	rl := NewRateLimiter(600) // 10 per second
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	if err := rl.Wait(ctx); err != nil {
		t.Fatalf("first wait failed: %v", err)
	}
	if err := rl.Wait(ctx); err != nil {
		t.Fatalf("second wait failed: %v", err)
	}
}

func TestCostGuard(t *testing.T) {
	cg := NewCostGuard(2.50)
	if err := cg.Check(1.20); err != nil {
		t.Fatalf("unexpected cost error: %v", err)
	}
	if err := cg.Check(2.55); err == nil {
		t.Fatal("expected cost guard error when spend exceeds limit")
	}
}
