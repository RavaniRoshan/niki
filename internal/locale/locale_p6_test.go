package locale

import (
	"strings"
	"testing"
	"time"
)

// P6: locale-aware time and number display; no layout breakage
// with long translated strings.
func TestLocaleNumbersAndDates(t *testing.T) {
	t.Setenv("LC_ALL", "de_DE.UTF-8")
	t.Setenv("LANG", "de_DE.UTF-8")
	if got := FormatInt(1234567); got != "1.234.567" {
		t.Fatalf("[P6] de number = %q, want 1.234.567", got)
	}
	when := time.Date(2026, 10, 11, 14, 30, 0, 0, time.UTC)
	if got := FormatTime(when); !strings.HasPrefix(got, "11.10.2026") {
		t.Fatalf("[P6] de date = %q, want day-first", got)
	}
	t.Logf("[P6] de_DE: 1234567 -> 1.234.567, date day-first")

	t.Setenv("LC_ALL", "en_US.UTF-8")
	t.Setenv("LANG", "en_US.UTF-8")
	if got := FormatInt(1234567); got != "1,234,567" {
		t.Fatalf("[P6] en number = %q, want 1,234,567", got)
	}
	if got := FormatTime(when); !strings.HasPrefix(got, "10/11/2026") {
		t.Fatalf("[P6] en date = %q, want month-first", got)
	}
	t.Logf("[P6] en_US: 1234567 -> 1,234,567, date month-first")
}

// Long translated strings must not break layouts: truncation is by
// cells (golden expectations frozen here).
func TestLongStringLayoutGolden(t *testing.T) {
	long := strings.Repeat("An extremely long status line withGremlins ", 10)
	cases := map[int]string{}
	for _, w := range []int{20, 40, 80} {
		cases[w] = TruncateCells(long, w)
	}
	if cases[20] != "An extremely long st" {
		t.Fatalf("[P6] 20-cell golden = %q", cases[20])
	}
	for w, s := range cases {
		if displayWidth(s) > w {
			t.Fatalf("[P6] width %d overflow: %q", w, s)
		}
	}
	t.Logf("[P6] long strings truncate cleanly at 20/40/80 cells")
}
