package sanitize

import (
	"strings"
	"testing"
)

func TestSanitizeStripsControlAndBidi(t *testing.T) {
	in := "hello\x00world\x1b[31mred\u200c\u200e\u200f\u2028\u2029\x07bell"
	out := Sanitize(in)
	if strings.ContainsAny(out, "\x00\x1b\x07") {
		t.Fatalf("control chars survived: %q", out)
	}
	for _, r := range []rune{'\u200c', '\u200e', '\u200f', '\u2028', '\u2029'} {
		if strings.ContainsRune(out, r) {
			t.Fatalf("zero-width/bidi rune %U survived", r)
		}
	}
	if !strings.Contains(out, "hello") || !strings.Contains(out, "world") {
		t.Fatalf("plain text damaged: %q", out)
	}
}

func TestSanitizeKeepsNewlineAndTab(t *testing.T) {
	in := "line1\nline2\tend"
	if out := Sanitize(in); out != in {
		t.Fatalf("newline/tab must survive: %q", out)
	}
}
