package main

import (
	"testing"

	"github.com/RavaniRoshan/niki/internal/provider"
)

func TestDemoMockProviderOffByDefault(t *testing.T) {
	t.Setenv("NIKICODE_DEMO_TOUR", "")
	t.Setenv("NIKI_DEMO_TOUR", "")
	m := demoMockProvider().(*provider.MockProvider)
	if len(m.ToolScripts) != 0 {
		t.Fatal("demo scripts must be off unless explicitly enabled")
	}
}

func TestDemoMockProviderTour(t *testing.T) {
	t.Setenv("NIKICODE_DEMO_TOUR", "1")
	m := demoMockProvider().(*provider.MockProvider)
	if len(m.ToolScripts) == 0 {
		t.Fatal("tour scripts missing")
	}
}
