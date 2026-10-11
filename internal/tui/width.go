package tui

import (
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
