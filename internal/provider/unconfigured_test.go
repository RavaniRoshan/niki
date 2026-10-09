package provider

import (
	"context"
	"strings"
	"testing"
)

func TestUnconfiguredProviderName(t *testing.T) {
	u := NewUnconfiguredProvider()
	if u.Name() != "none" {
		t.Fatalf("expected provider name 'none', got %q", u.Name())
	}
}

func TestUnconfiguredProviderReturnsDiagnosticError(t *testing.T) {
	u := NewUnconfiguredProvider()
	deltas, errs := u.Stream(context.Background(), []Message{{Role: "user", Content: "hello"}})

	for d := range deltas {
		t.Fatalf("expected no deltas from unconfigured provider, got Kind=%v", d.Kind)
	}

	err, ok := <-errs
	if !ok || err == nil {
		t.Fatal("expected diagnostic error from unconfigured provider, got nil")
	}

	errStr := err.Error()
	if !strings.Contains(errStr, "no AI model configured") && !strings.Contains(errStr, "/connect") {
		t.Fatalf("unexpected error message: %q", errStr)
	}
}
