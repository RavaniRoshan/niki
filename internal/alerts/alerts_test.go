package alerts

import (
	"strings"
	"testing"
)

func TestAlertNoModel(t *testing.T) {
	a := NewNoModelAlert()
	if a.Type != AlertNoModel {
		t.Fatalf("expected AlertNoModel, got %v", a.Type)
	}
	if !strings.Contains(a.Title, "No AI Model") {
		t.Fatalf("title missing expected text: %q", a.Title)
	}
	if len(a.Guidance) < 3 {
		t.Fatalf("expected at least 3 guidance points, got %d", len(a.Guidance))
	}

	rendered := a.FormatPlain(80)
	if !strings.Contains(rendered, "/connect") {
		t.Fatalf("rendered alert missing /connect guidance: %s", rendered)
	}
	if !strings.Contains(rendered, "! <command>") && !strings.Contains(rendered, "Local shell") {
		t.Fatalf("rendered alert missing local shell guidance: %s", rendered)
	}
}

func TestAlertAuthFailure(t *testing.T) {
	a := NewAuthFailureAlert("anthropic")
	if a.Type != AlertAuthFailure {
		t.Fatalf("expected AlertAuthFailure, got %v", a.Type)
	}
	if !strings.Contains(a.Description, "anthropic") {
		t.Fatalf("expected provider name in description: %q", a.Description)
	}
}

func TestAlertRateLimit(t *testing.T) {
	a := NewRateLimitAlert("openai", "20s")
	if a.Type != AlertRateLimit {
		t.Fatalf("expected AlertRateLimit, got %v", a.Type)
	}
	if !strings.Contains(a.Description, "20s") {
		t.Fatalf("expected retry-after in description: %q", a.Description)
	}
}

func TestAlertContextBudget(t *testing.T) {
	a := NewContextBudgetAlert(110000, 128000)
	if a.Type != AlertContextBudget {
		t.Fatalf("expected AlertContextBudget, got %v", a.Type)
	}
	if a.Percentage < 80 {
		t.Fatalf("expected percentage >= 80, got %d", a.Percentage)
	}
}
