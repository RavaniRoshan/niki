package tools

import (
	"fmt"
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

// ResolveSafe resolves path against root and rejects any escape —
// both via ".." and via symlinks (X8 symlink escape, Q6 path-guard
// property). Symlinks are evaluated along the deepest existing
// ancestor, so a link inside the workspace that points outside it is
// refused, while a link that stays inside the workspace is allowed.
// The result is the symlink-resolved real path on success.
func ResolveSafe(root, path string) (string, error) {
	absRoot, err := filepath.EvalSymlinks(root)
	if err != nil {
		// root itself does not exist yet — evaluate what we can.
		absRoot, err = filepath.Abs(root)
		if err != nil {
			return "", fmt.Errorf("bad root: %w", err)
		}
	}
	absRoot = filepath.Clean(absRoot)

	// Materialize the candidate path (absolute, cleaned).
	cand := path
	if !filepath.IsAbs(cand) {
		cand = filepath.Join(absRoot, cand)
	}
	cand = filepath.Clean(cand)

	// Resolve symlinks on the deepest existing prefix so a symlinked
	// escape is caught even when the final component does not exist.
	real := evalExistingPrefix(cand)

	if real != absRoot && !strings.HasPrefix(real, absRoot+string(filepath.Separator)) {
		return "", fmt.Errorf("path escapes workspace: %s resolves outside %s", path, absRoot)
	}
	return real, nil
}

// evalExistingPrefix walks up to the deepest existing ancestor, runs
// EvalSymlinks on it, and rejoins the non-existing remainder.
func evalExistingPrefix(p string) string {
	var suffix []string
	cur := p
	for {
		suffix = append([]string{filepath.Base(cur)}, suffix...)
		parent := filepath.Dir(cur)
		if resolved, err := filepath.EvalSymlinks(cur); err == nil {
			return filepath.Join(append([]string{resolved}, suffix[1:]...)...)
		}
		if parent == cur {
			// Reached the root without an existing component: return
			// the cleaned absolute path as-is.
			return filepath.Clean(p)
		}
		cur = parent
	}
}
