package routing_test

import (
	"context"
	"errors"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/routing"
)

type failProvider struct {
	name   string
	errMsg string
}

func (f *failProvider) Name() string { return f.name }

func (f *failProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	deltaCh := make(chan provider.Delta)
	errCh := make(chan error, 1)
	close(deltaCh)
	errCh <- errors.New(f.errMsg)
	close(errCh)
	return deltaCh, errCh
}

type successProvider struct {
	name string
	text string
}

func (s *successProvider) Name() string { return s.name }

func (s *successProvider) Stream(ctx context.Context, messages []provider.Message) (<-chan provider.Delta, <-chan error) {
	deltaCh := make(chan provider.Delta, 1)
	errCh := make(chan error, 1)
	deltaCh <- provider.Delta{
		Kind: provider.DeltaText,
		Text: s.text,
	}
	close(deltaCh)
	close(errCh)
	return deltaCh, errCh
}

func TestFallbackOnServerError(t *testing.T) {
	primary := &failProvider{name: "primary-500", errMsg: "500 Internal Server Error"}
	fallback := &successProvider{name: "fallback-ok", text: "recovered successfully"}

	router := routing.NewFallbackProvider(primary, fallback)

	dCh, errCh := router.Stream(context.Background(), nil)
	var output string
	for d := range dCh {
		output += d.Text
	}
	if err := <-errCh; err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if output != "recovered successfully" {
		t.Errorf("expected fallback output 'recovered successfully', got %q", output)
	}
}

func TestNoFallbackOnAuthError(t *testing.T) {
	primary := &failProvider{name: "primary-401", errMsg: "401 Unauthorized: invalid api key"}
	fallback := &successProvider{name: "fallback-ok", text: "should not be reached"}

	router := routing.NewFallbackProvider(primary, fallback)

	dCh, errCh := router.Stream(context.Background(), nil)
	var output string
	for d := range dCh {
		output += d.Text
	}
	err := <-errCh
	if err == nil {
		t.Fatal("expected error, got nil")
	}

	if !strings.Contains(err.Error(), "401") && !strings.Contains(err.Error(), "non-fallback") {
		t.Errorf("expected non-fallback auth error, got: %v", err)
	}

	if output != "" {
		t.Errorf("expected no output when auth fails, got: %q", output)
	}
}

func TestMaxThreeFallbacks(t *testing.T) {
	p1 := &failProvider{name: "p1", errMsg: "503 service unavailable"}
	p2 := &failProvider{name: "p2", errMsg: "503 service unavailable"}
	p3 := &failProvider{name: "p3", errMsg: "503 service unavailable"}
	p4 := &failProvider{name: "p4", errMsg: "503 service unavailable"}
	p5 := &successProvider{name: "p5", text: "exceeded limit"}

	// Passing 4 fallbacks (p2, p3, p4, p5)
	router := routing.NewFallbackProvider(p1, p2, p3, p4, p5)

	// Since max fallbacks is 3, router only has [p1, p2, p3, p4] and p5 is trimmed.
	dCh, errCh := router.Stream(context.Background(), nil)
	for range dCh {
	}
	err := <-errCh
	if err == nil {
		t.Fatal("expected error because 4th fallback (p5) should have been truncated")
	}
}

func TestCostAccounting(t *testing.T) {
	// 1000 prompt tokens and 500 completion tokens on gpt-4o
	// Prompt: 1000 / 1M * $2.50 = $0.0025
	// Completion: 500 / 1M * $10.00 = $0.0050
	// Total: $0.0075
	usage := provider.Usage{
		PromptTokens:     1000,
		CompletionTokens: 500,
	}
	cost := routing.CalculateCost("gpt-4o", usage)
	if cost < 0.0074 || cost > 0.0076 {
		t.Errorf("expected ~$0.0075, got %f", cost)
	}

	formatted := routing.FormatCost(cost)
	if formatted != "$0.0075" {
		t.Errorf("expected formatted '$0.0075', got %s", formatted)
	}

	zeroCost := routing.CalculateCost("mock", usage)
	if zeroCost != 0.0 {
		t.Errorf("expected 0.0 for mock, got %f", zeroCost)
	}
	if routing.FormatCost(zeroCost) != "$0.00" {
		t.Errorf("expected '$0.00', got %s", routing.FormatCost(zeroCost))
	}
}
