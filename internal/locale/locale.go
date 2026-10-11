// Package locale formats times and numbers for display (P6):
// thousands grouping follows LANG/LC_ALL, dates follow the
// locale's field order. Layouts never break on long translated
// strings — callers truncate by cells (tui.TruncateTo), covered
// by the long-string golden below.
package locale

import (
	"os"
	"strings"
	"time"

	"github.com/mattn/go-runewidth"
	"golang.org/x/text/language"
	"golang.org/x/text/message"
)

// Tag resolves the display language from LC_ALL/LANG ("de_DE.UTF-8"
// -> German); unknown or unset means English.
func Tag() language.Tag {
	for _, key := range []string{"LC_ALL", "LANG"} {
		if v := os.Getenv(key); v != "" {
			base := strings.Split(v, ".")[0]
			base = strings.ReplaceAll(base, "_", "-")
			if tag, err := language.Parse(base); err == nil {
				return tag
			}
		}
	}
	return language.English
}

// Printer returns a message printer for the display language.
func Printer() *message.Printer {
	return message.NewPrinter(Tag())
}

// FormatInt groups thousands per locale (1,234 / 1.234).
func FormatInt(n int) string {
	return Printer().Sprintf("%d", n)
}

// dateLayout picks field order by language: en month-first, most
// others day-first (de/fr/es/it), east-asian year-first.
func dateLayout() string {
	base, _ := Tag().Base()
	switch base.String() {
	case "de", "fr", "es", "it", "nl", "pl", "pt", "ru":
		return "02.01.2006 15:04"
	case "ja", "zh", "ko":
		return "2006/01/02 15:04"
	default:
		return "01/02/2006 15:04"
	}
}

// FormatTime renders t for display in the locale order.
func FormatTime(t time.Time) string {
	return t.Format(dateLayout())
}

// TruncateCells cuts s to at most max display cells without
// breaking layout on long translated strings (P6 golden).
func TruncateCells(s string, max int) string {
	if max <= 0 {
		return ""
	}
	if runewidth.StringWidth(s) <= max {
		return s
	}
	return runewidth.Truncate(s, max, "")
}

// displayWidth is exposed for the layout golden.
func displayWidth(s string) int {
	return runewidth.StringWidth(s)
}
