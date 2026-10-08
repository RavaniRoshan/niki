// Package claimcheck enforces the claim contract (G2): every anchored
// README claim sentence and every tagged --help/TUI surface string must
// trace to a CLAIMS.md row whose probe passes (status PROVEN). A claim
// with no probe is a build failure, not a docs issue.
package claimcheck

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

func repoRoot(t *testing.T) string {
	t.Helper()
	wd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	// internal/claimcheck -> repo root.
	return filepath.Join(wd, "..", "..")
}

// parseClaims returns row id -> proven.
func parseClaims(t *testing.T, path string) map[string]bool {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	rows := map[string]bool{}
	for _, line := range strings.Split(string(data), "\n") {
		line = strings.TrimSpace(line)
		if !strings.HasPrefix(line, "|") {
			continue
		}
		cells := strings.Split(line, "|")
		if len(cells) < 6 {
			continue
		}
		id := strings.TrimSpace(cells[1])
		if !regexp.MustCompile(`^C\d+$`).MatchString(id) {
			continue
		}
		if _, dup := rows[id]; dup {
			t.Errorf("duplicate claim row %s in %s", id, path)
			continue
		}
		result := strings.TrimSpace(cells[5])
		rows[id] = strings.HasPrefix(result, "PROVEN")
	}
	if len(rows) == 0 {
		t.Fatalf("no claim rows parsed from %s", path)
	}
	return rows
}

var anchorPattern = regexp.MustCompile(`<!--\s*claims?:\s*([A-Z0-9 ]+)\s*-->`)
var tagPattern = regexp.MustCompile(`\[(C\d+)\]`)

func checkRefs(t *testing.T, where, text string, rows map[string]bool) {
	t.Helper()
	seen := map[string]bool{}
	var pattern *regexp.Regexp
	if strings.HasSuffix(where, ".md") && !strings.HasSuffix(where, "SURFACE.txt") {
		pattern = anchorPattern
	} else {
		pattern = tagPattern
	}
	for _, m := range pattern.FindAllStringSubmatch(text, -1) {
		for _, id := range strings.Fields(m[1]) {
			seen[id] = true
			proven, ok := rows[id]
			if !ok {
				t.Errorf("%s references unknown claim %s (no CLAIMS.md row)", where, id)
				continue
			}
			if !proven {
				t.Errorf("%s references claim %s with no passing probe (UNPROVEN/BLOCKED)", where, id)
			}
		}
	}
	if len(seen) == 0 {
		t.Errorf("%s has no claim references at all", where)
	}
}

func TestClaimsCoverSurfaces(t *testing.T) {
	root := repoRoot(t)
	rows := parseClaims(t, filepath.Join(root, "docs", "CLAIMS.md"))

	readme, err := os.ReadFile(filepath.Join(root, "README.md"))
	if err != nil {
		t.Fatal(err)
	}
	checkRefs(t, "README.md", string(readme), rows)

	surface, err := os.ReadFile(filepath.Join(root, "docs", "SURFACE.txt"))
	if err != nil {
		t.Fatal(err)
	}
	checkRefs(t, "docs/SURFACE.txt", string(surface), rows)
	if !strings.Contains(string(surface), "nikicode do") {
		t.Error("SURFACE.txt looks stale: no `nikicode do` line; regenerate with `nikicode surfaces > docs/SURFACE.txt`")
	}
}
