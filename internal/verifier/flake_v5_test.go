package verifier

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func goFixtureDir(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "go.mod"), []byte("module flaketest\n\ngo 1.21\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	return dir
}

// V5: flake handling — a check rerun k times whose outcome diverges
// makes the requirement inconclusive; a stable check grades normally.
func TestFlakyCheckIsInconclusive(t *testing.T) {
	checks := SelectChecks(goFixtureDir(t))

	// Flaky runner: go-test alternates pass/fail across reruns.
	call := 0
	flaky := func(_ context.Context, c Check) CheckResult {
		if c.Name == "go-test" {
			call++
			return CheckResult{Check: c, Passed: call%2 == 1}
		}
		return CheckResult{Check: c, Passed: true}
	}
	tier, notes := RerunChecks(context.Background(), flaky, checks, 3)
	if tier >= T2Passed {
		t.Fatalf("[V5] flaky check must never reach T2: tier=%s notes=%v", tier, notes)
	}
	if len(notes) == 0 || !strings.Contains(notes[0], "flaky") {
		t.Fatalf("[V5] flake not reported: %v", notes)
	}
	t.Logf("[V5] flaky go-test -> %s; %s", tier, notes[0])

	// Stable runner: every check always passes -> verified tier.
	stable := func(_ context.Context, c Check) CheckResult { return CheckResult{Check: c, Passed: true} }
	tier, notes = RerunChecks(context.Background(), stable, checks, 3)
	if tier < T2Passed {
		t.Fatalf("[V5] stable checks must reach T2+: tier=%s notes=%v", tier, notes)
	}
	if len(notes) != 0 {
		t.Fatalf("[V5] stable run should emit no flake notes: %v", notes)
	}
	t.Logf("[V5] stable checks reran 3x, no divergence -> %s", tier)

	// A stable-failing check stays a hard fail (not a flake).
	failing := func(_ context.Context, c Check) CheckResult { return CheckResult{Check: c} }
	tier, _ = RerunChecks(context.Background(), failing, checks, 3)
	if tier != T1Weak {
		t.Fatalf("[V5] consistently failing check -> want T1, got %s", tier)
	}
	t.Logf("[V5] consistently failing check -> %s (deterministic fail, not flake)", tier)
}
