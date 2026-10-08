package sanitize

import (
	"strings"
	"unicode"
)

// Sanitize strips control characters and bidi/zero-width tricks from
// untrusted text before it reaches the terminal or the model context.
// Only newline and tab survive among control runes.
func Sanitize(s string) string {
	return strings.Map(func(r rune) rune {
		switch {
		case r == '\n' || r == '\t':
			return r
		case unicode.IsControl(r):
			return -1
		case r == '‌' || r == '‎' || r == '‏' || r == ' ' || r == ' ':
			return -1
		}
		return r
	}, s)
}
