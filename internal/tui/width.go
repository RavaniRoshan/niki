package tui

import (
	"strings"

	"github.com/mattn/go-runewidth"
)

// CellWidth returns the display width in cells.
func CellWidth(s string) int {
	return runewidth.StringWidth(s)
}

// TruncateTo cuts s to at most max display cells.
func TruncateTo(s string, max int) string {
	if max <= 0 {
		return ""
	}
	if runewidth.StringWidth(s) <= max {
		return s
	}
	ell := Ellipsis()
	ellW := runewidth.StringWidth(ell)
	if max <= ellW {
		return runewidth.Truncate(ell, max, "")
	}
	return runewidth.Truncate(s, max-ellW, "") + ell
}

// ansiWidth measures display width ignoring ANSI escape sequences
// (CSI: ESC [ ... final; OSC: ESC ] ... BEL|ESC \; lone ESC + char).
// CellWidth counts escape bytes as runes, so it over-measures styled
// strings; layout code that truncates styled text must use this.
func ansiWidth(s string) int {
	w := 0
	runes := []rune(s)
	for i := 0; i < len(runes); i++ {
		if runes[i] == 0x1b {
			_, n := ansiSeq(runes[i:])
			i += n - 1
			continue
		}
		w += runewidth.RuneWidth(runes[i])
	}
	return w
}

// ansiSeq returns the escape sequence at the start of runes and its
// length in runes. Unknown shapes consume the ESC and one rune.
func ansiSeq(runes []rune) (string, int) {
	if len(runes) < 2 {
		return string(runes), len(runes)
	}
	switch runes[1] {
	case '[':
		for i := 2; i < len(runes); i++ {
			if runes[i] >= 0x40 && runes[i] <= 0x7e {
				return string(runes[:i+1]), i + 1
			}
		}
	case ']':
		for i := 2; i < len(runes); i++ {
			if runes[i] == 0x07 {
				return string(runes[:i+1]), i + 1
			}
			if runes[i] == 0x1b && i+1 < len(runes) && runes[i+1] == '\\' {
				return string(runes[:i+2]), i + 2
			}
		}
	}
	return string(runes[:2]), 2
}

// TruncateStyledTo cuts a possibly ANSI-styled string to max display
// cells, preserving escape sequences verbatim and appending the
// ellipsis. Plain strings truncate exactly like TruncateTo.
func TruncateStyledTo(s string, max int) string {
	if max <= 0 {
		return ""
	}
	if ansiWidth(s) <= max {
		return s
	}
	ell := Ellipsis()
	ellW := runewidth.StringWidth(ell)
	if max <= ellW {
		return runewidth.Truncate(ell, max, "")
	}
	limit := max - ellW
	var b strings.Builder
	w := 0
	runes := []rune(s)
	for i := 0; i < len(runes); i++ {
		r := runes[i]
		if r == 0x1b {
			seq, n := ansiSeq(runes[i:])
			b.WriteString(seq)
			i += n - 1
			continue
		}
		rw := runewidth.RuneWidth(r)
		if w+rw > limit {
			break
		}
		b.WriteRune(r)
		w += rw
	}
	return b.String() + ell
}
