package tools

import (
	"path/filepath"
	"regexp"
	"strings"
)

// Redact scrubs likely credentials from output before it is shown or logged.
func Redact(s string) string {
	patterns := []*regexp.Regexp{
		regexp.MustCompile(`sk-[A-Za-z0-9_-]{10,}`),
		regexp.MustCompile(`Bearer [A-Za-z0-9._-]+`),
		regexp.MustCompile(`(?i)api[_-]?key["'\s:=]+[A-Za-z0-9._-]+`),
	}
	out := s
	for _, p := range patterns {
		out = p.ReplaceAllString(out, "[REDACTED]")
	}
	return out
}

// SafeJoin joins root and path, rejecting escapes via "..".
func SafeJoin(root, path string) (string, bool) {
	joined := filepath.Clean(filepath.Join(root, path))
	absRoot, err := filepath.Abs(root)
	if err != nil {
		return "", false
	}
	absJoined, err := filepath.Abs(joined)
	if err != nil {
		return "", false
	}
	if absJoined != absRoot && !strings.HasPrefix(absJoined, absRoot+string(filepath.Separator)) {
		return "", false
	}
	return joined, true
}
