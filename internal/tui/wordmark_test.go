package tui

import (
	"strings"
	"testing"
)

// The wordmark renders at three terminal sizes with an ASCII fallback:
// full lockup at 80+, one row at 50-79, name only below 50.
func TestWordmarkSizes(t *testing.T) {
	full := Wordmark(80, false)
	if !strings.Contains(full, "NikiCode") || !strings.Contains(full, "◐") {
		t.Fatalf("full wordmark missing name or orb:\n%s", full)
	}
	if lines := strings.Count(full, "\n"); lines != 2 {
		t.Fatalf("full wordmark = %d lines, want 3 rows", lines+1)
	}
	compact := Wordmark(60, false)
	if compact != "◐ NikiCode" {
		t.Fatalf("compact wordmark = %q", compact)
	}
	if got := Wordmark(40, false); got != "NikiCode" {
		t.Fatalf("narrow wordmark = %q", got)
	}
	// Width never changes the narrow form; it always fits.
	if got := Wordmark(10, false); got != "NikiCode" {
		t.Fatalf("tiny wordmark = %q", got)
	}
}

func TestWordmarkASCIIFallback(t *testing.T) {
	for _, w := range []int{40, 60, 80, 120} {
		got := Wordmark(w, true)
		if !strings.Contains(got, "NikiCode") {
			t.Fatalf("ascii wordmark at %d missing name:\n%s", w, got)
		}
		for _, r := range got {
			if r > 127 {
				t.Fatalf("ascii wordmark at %d has non-ASCII rune %q:\n%s", w, r, got)
			}
		}
	}
	if got := Wordmark(60, true); got != "(o) NikiCode" {
		t.Fatalf("ascii compact = %q", got)
	}
}

func TestBrandLine(t *testing.T) {
	if got := BrandLine(); !strings.Contains(got, "NikiCode") {
		t.Fatalf("brand line = %q", got)
	}
}

func TestCompactMark(t *testing.T) {
	if got := CompactMark(80, false); got != "◐ NikiCode" {
		t.Fatalf("compact = %q", got)
	}
	if got := CompactMark(80, true); got != "(o) NikiCode" {
		t.Fatalf("ascii compact = %q", got)
	}
	if got := CompactMark(30, false); got != "NikiCode" {
		t.Fatalf("narrow compact = %q", got)
	}
}
