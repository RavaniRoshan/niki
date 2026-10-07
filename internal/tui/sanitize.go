package tui

import (
	"strings"
	"unicode"
)

// Sanitize strips control characters and bidi/zero-width tricks from
// untrusted text before it reaches the terminal (L6).
func Sanitize(s string) string {
	return strings.Map(func(r rune) rune {
		switch {
		case r == '\n' || r == '\t':
			return r
		case unicode.IsControl(r):
			return -1
		case r == '\u200b' || r == '\u200e' || r == '\u200f' || r == '\u2028' || r == '\u2029':
			return -1
		}
		return r
	}, s)
}
