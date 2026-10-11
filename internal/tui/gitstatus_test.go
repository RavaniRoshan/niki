package tui

import (
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/git"
)

// TestSubmodsBadge covers the G4 status-bar integrator without
// shelling out: the badge must be empty when there are no submodules
// (keeping hermetic golden frames stable) and otherwise report counts.
func TestSubmodsBadge(t *testing.T) {
	th := NewDefaultTheme()

	if got := submodsBadge(nil, th); got != "" {
		t.Fatalf("expected empty badge for no submods, got %q", got)
	}
	if got := submodsBadge([]git.Submodule{}, th); got != "" {
		t.Fatalf("expected empty badge for empty submods, got %q", got)
	}

	clean := []git.Submodule{{Path: "a", Status: "clean"}, {Path: "b", Status: "clean"}}
	got := submodsBadge(clean, th)
	if !strings.Contains(got, "+2 subs") || strings.Contains(got, "dirty") {
		t.Fatalf("clean badge unexpected: %q", got)
	}

	mixed := []git.Submodule{
		{Path: "a", Status: "clean"},
		{Path: "b", Status: "modified"},
		{Path: "c", Status: "unregistered"},
	}
	got = submodsBadge(mixed, th)
	if !strings.Contains(got, "+3 subs") || !strings.Contains(got, "1 dirty") || !strings.Contains(got, "1 unreg") {
		t.Fatalf("mixed badge unexpected: %q", got)
	}
}
