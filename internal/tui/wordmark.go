package tui

import (
	"os"
	"strings"
)

// Wordmark renders the original NikiCode brand lockup: the orb motif
// (single eye, designed for NikiCode) beside the name. Nothing is
// borrowed: no competitor name, tagline, logo, or asset.
//
//	width >= 80: three-row lockup with tagline.
//	50 <= width < 80: compact one-row form.
//	width < 50: name only (always fits a narrow viewport).
//
// ascii selects the pure-ASCII fallback (TERM=dumb terminals): the
// half-block orb becomes bracket art and the eye becomes "o".
func Wordmark(width int, ascii bool) string {
	const name = "NikiCode"
	const tag = "personal coding agent"
	if width < 50 {
		return name
	}
	if width < 80 {
		if ascii {
			return "(o) " + name
		}
		return "◐ " + name
	}
	if ascii {
		return strings.Join([]string{
			"  _____",
			" /  o  \\   " + name,
			" \\_____/   " + tag,
		}, "\n")
	}
	return strings.Join([]string{
		"  ▄███▄",
		" ███◐███   " + name,
		"  ▀███▀   " + tag,
	}, "\n")
}

// CompactMark is the one-row header form: orb + name, or the bare
// name below 50 columns. Headers always use this, never the full lockup.
func CompactMark(width int, ascii bool) string {
	if width < 50 {
		return "NikiCode"
	}
	if ascii {
		return "(o) NikiCode"
	}
	return "◐ NikiCode"
}

// BrandLine is the in-TUI brand line shown under the header.
func BrandLine() string {
	return "NikiCode — personal coding agent"
}

// UseASCII reports whether the terminal needs the ASCII fallback:
// TERM=dumb has no unicode block/orb glyphs worth emitting.
func UseASCII() bool {
	return os.Getenv("TERM") == "dumb"
}
