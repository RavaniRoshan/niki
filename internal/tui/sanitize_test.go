package tui

import (
	"strings"
	"testing"
)

func TestSanitize(t *testing.T) {
	in := "hello\x1b]52;b64\x07world\u200b\x00end"
	out := Sanitize(in)
	if strings.ContainsRune(out, '\x1b') || strings.ContainsRune(out, '\x07') || strings.ContainsRune(out, '\x00') || strings.ContainsRune(out, '\u200b') {
		t.Fatalf("unsafe content in %q", out)
	}
	if out != "hello]52;b64worldend" {
		t.Fatalf("got %q", out)
	}
}

func FuzzSanitize(f *testing.F) {
	f.Add("plain")
	f.Add("\x1b[31mred\x1b[0m")
	f.Add("\u202e")
	f.Fuzz(func(t *testing.T, s string) {
		out := Sanitize(s)
		for _, r := range out {
			if r != '\n' && r != '\t' && (r < 0x20 || r == 0x7f || r == '\u200b' || r == '\u2028' || r == '\u2029') {
				t.Fatalf("unsafe rune %U in %q", r, out)
			}
		}
	})
}
