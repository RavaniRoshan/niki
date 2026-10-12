package verifier

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// maxIntegrityFiles caps how many files one snapshot hashes.
const maxIntegrityFiles = 200

// IntegritySnapshot hashes the files an agent run must not silently
// change: test files, CI files, lockfiles — plus the exact verifier
// command lines, so a swapped check definition is caught too (V4).
type IntegritySnapshot struct {
	Files map[string]string `json:"files"`
	Cmds  []string          `json:"cmds"`
}

func integrityMatch(path string) bool {
	base := filepath.Base(path)
	switch {
	case strings.HasSuffix(base, "_test.go"):
		return true
	case strings.HasPrefix(base, "test_") && (strings.HasSuffix(base, ".py") || strings.HasSuffix(base, ".js") || strings.HasSuffix(base, ".ts")):
		return true
	case strings.HasSuffix(base, "_test.py"):
		return true
	// Cross-stack test files: a deleted/renamed test in any supported
	// stack is tampering (V4/V7), so integrity tracks them all.
	case strings.HasSuffix(base, "_test.rs"),
		strings.HasSuffix(base, ".test.js"), strings.HasSuffix(base, ".test.ts"),
		strings.HasSuffix(base, ".test.jsx"), strings.HasSuffix(base, ".test.tsx"),
		strings.HasSuffix(base, ".spec.js"), strings.HasSuffix(base, ".spec.ts"):
		return true
	case base == "go.mod" || base == "go.sum" || base == "package-lock.json" || base == "yarn.lock" || base == "Cargo.lock":
		return true
	}
	return strings.Contains(filepath.ToSlash(path), ".github/workflows/")
}

func hashFile(path string) (string, bool) {
	data, err := os.ReadFile(path)
	if err != nil {
		return "", false
	}
	h := sha256.Sum256(data)
	return hex.EncodeToString(h[:]), true
}

// SnapshotIntegrity walks dir (skipping .git, node_modules,
// target, __pycache__) and hashes every integrity-relevant file,
// recording the check argv list alongside.
func SnapshotIntegrity(dir string, checks []Check) IntegritySnapshot {
	snap := IntegritySnapshot{Files: map[string]string{}}
	_ = filepath.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			switch d.Name() {
			case ".git", "node_modules", "target", "__pycache__", ".venv", "dist", "build":
				return filepath.SkipDir
			}
			return nil
		}
		if len(snap.Files) >= maxIntegrityFiles {
			return nil
		}
		if integrityMatch(path) {
			if h, ok := hashFile(path); ok {
				rel, _ := filepath.Rel(dir, path)
				snap.Files[rel] = h
			}
		}
		return nil
	})
	for _, c := range checks {
		snap.Cmds = append(snap.Cmds, strings.Join(c.Argv, " "))
	}
	sort.Strings(snap.Cmds)
	return snap
}

// Diff lists human-readable changes between two snapshots: added,
// removed, or modified files, plus check-definition changes. Empty
// means clean.
func (s IntegritySnapshot) Diff(after IntegritySnapshot) []string {
	var out []string
	for p, h := range s.Files {
		ah, ok := after.Files[p]
		if !ok {
			out = append(out, "removed: "+p)
		} else if ah != h {
			out = append(out, "modified: "+p)
		}
	}
	for p := range after.Files {
		if _, ok := s.Files[p]; !ok {
			out = append(out, "added: "+p)
		}
	}
	beforeCmds := strings.Join(s.Cmds, "\x00")
	afterCmds := strings.Join(after.Cmds, "\x00")
	if beforeCmds != afterCmds {
		out = append(out, "verifier commands changed")
	}
	sort.Strings(out)
	return out
}
