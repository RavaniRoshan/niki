package verifier

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// V7: verifier calibration — a seeded suite of 40+ cases across 4+
// stacks. Tamper and weakening cases are NEVER marked verified. The
// rates are written to docs/verifier/TRUST_MODEL.md.
func TestCalibrationSuiteAndTrustModel(t *testing.T) {
	cases := SeedSuite()
	if len(cases) < 40 {
		t.Fatalf("[V7] seed suite must have >=40 cases, got %d", len(cases))
	}
	stacks := map[Stack]bool{}
	for _, c := range cases {
		stacks[c.Stack] = true
	}
	if len(stacks) < 4 {
		t.Fatalf("[V7] suite must span >=4 stacks, got %d", len(stacks))
	}

	rep := RunCalibration(cases)

	// The absolute invariant: zero tamper/weakening cases verified.
	if rep.TamperVerified != 0 {
		t.Fatalf("[V7] %d tamper case(s) verified — invariant broken", rep.TamperVerified)
	}
	if rep.WeakenVerified != 0 {
		t.Fatalf("[V7] %d weakening case(s) verified — invariant broken", rep.WeakenVerified)
	}
	if rep.Passed != rep.Total {
		// Surface the first failure for debugging.
		for _, r := range rep.Results {
			if !r.PassedGate {
				t.Fatalf("[V7] gate failure: %s (%s) -> %s", r.Case.Name, r.Case.Kind, r.Verdict.Result)
			}
		}
	}
	t.Logf("[V7] %d cases across %d stacks: %d/%d gate-correct; tamper-verified=%d weakening-verified=%d",
		rep.Total, len(stacks), rep.Passed, rep.Total, rep.TamperVerified, rep.WeakenVerified)

	// Write the trust model to docs/verifier/TRUST_MODEL.md.
	doc := RenderTrustModel(rep)
	outDir := filepath.Join("..", "..", "docs", "verifier")
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		t.Fatal(err)
	}
	outPath := filepath.Join(outDir, "TRUST_MODEL.md")
	if err := os.WriteFile(outPath, []byte(doc), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(outPath); err != nil {
		t.Fatalf("[V7] TRUST_MODEL.md not written: %v", err)
	}
	if !strings.Contains(doc, "calibration clean") {
		t.Fatalf("[V7] trust model not clean:\n%s", doc)
	}
	t.Logf("[V7] wrote %s (%d bytes): calibration clean", outPath, len(doc))
}
