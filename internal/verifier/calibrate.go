package verifier

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// Calibration (V7): a seeded suite of cases, each run through the real
// Standard verifier with an injected runner, checks the verifier's
// behavior against a known ground truth. Two invariants are absolute:
//   - a "clean pass" case must verify;
//   - a "tamper" (test deleted) or "weakening" (assertions removed /
//     skips added) case must NEVER verify, regardless of the checks.
//
// The suite spans four stacks (go, node, rust, python) so no single
// language's check shape can hide a bug. Rates are rendered into
// docs/verifier/TRUST_MODEL.md.

// CaseKind labels a calibration case's ground truth.
type CaseKind string

const (
	KindCleanPass CaseKind = "clean-pass" // must verify
	KindHardFail  CaseKind = "hard-fail"  // must not verify (a check failed)
	KindTamper    CaseKind = "tamper"     // must never verify (test deleted mid-run)
	KindWeakening CaseKind = "weakening"  // must never verify (tests weakened mid-run)
	KindNoChecks  CaseKind = "no-checks"  // inconclusive (unknown stack)
)

// Case is one seeded calibration scenario.
type Case struct {
	Name   string
	Stack  Stack
	Kind   CaseKind
	Runner func(ctx context.Context, c Check) CheckResult
}

// CalibrationResult is the outcome of running one case.
type CalibrationResult struct {
	Case       Case
	Verdict    Verdict
	PassedGate bool // true = the verifier behaved correctly for this kind
}

// stackManifest writes the manifest file that makes DetectStack pick
// the case's stack inside a fresh dir.
func stackManifest(stack Stack) map[string]string {
	switch stack {
	case StackGo:
		return map[string]string{"go.mod": "module calib\n\ngo 1.21\n"}
	case StackNode:
		return map[string]string{"package.json": `{"name":"calib","scripts":{"test":"node --test"}}`}
	case StackRust:
		return map[string]string{"Cargo.toml": "[package]\nname = \"calib\"\nversion = \"0.1.0\"\n"}
	case StackPython:
		return map[string]string{"pyproject.toml": "[project]\nname = \"calib\"\nversion = \"0.1.0\"\n"}
	}
	return nil
}

// stackTestFile writes a real test file for the stack so weakening and
// integrity detection have something to measure.
func stackTestFile(stack Stack) (name, content string) {
	switch stack {
	case StackGo:
		return "calib_test.go", "package calib\n\nimport \"testing\"\n\nfunc TestA(t *testing.T) {\n\tassert(t, 1+1 == 2)\n\tassert(t, 2+2 == 4)\n}\n\nfunc assert(t *testing.T, ok bool) {\n\tif !ok { t.Fatal(\"x\") }\n}\n"
	case StackNode:
		return "calib.test.js", "const assert = require('assert');\ntest('a', () => { assert.equal(1,1); assert.equal(2,2); });\n"
	case StackRust:
		return "calib_test.rs", "#[test]\nfn a() { assert_eq!(1+1, 2); assert_eq!(2+2, 4); }\n"
	case StackPython:
		return "test_calib.py", "def test_a():\n    assert 1 + 1 == 2\n    assert 2 + 2 == 4\n"
	}
	return "", ""
}

// weaken removes one assertion and adds a skip, per stack, so the
// weakening detector has a concrete signal.
func weaken(content string) string {
	lines := strings.Split(content, "\n")
	out := make([]string, 0, len(lines)+1)
	dropped := false
	for _, ln := range lines {
		if !dropped && strings.Contains(ln, "assert") && !strings.Contains(ln, "import") && !strings.Contains(ln, "func assert") && !strings.Contains(ln, "def ") {
			dropped = true
			continue // drop one assertion line
		}
		out = append(out, ln)
	}
	// Add a skip marker appropriate to the file's language.
	skip := "\tt.Skip(\"flaky\")\n"
	switch {
	case strings.Contains(content, "def test_"):
		skip = "    import pytest; pytest.mark.skip\n"
	case strings.Contains(content, "require("):
		skip = "it.skip('a', () => {});\n"
	}
	return strings.Join(out, "\n") + skip
}

// selectedFirst returns the name of the first check SelectChecks would
// run in dir (stack check, before any git diff-check), so a hard-fail
// case fails a check the verifier actually selected.
func selectedFirst(dir string) string {
	checks := SelectChecks(dir)
	if len(checks) == 0 {
		return ""
	}
	return checks[0].Name
}

func removeQuiet(path string) error { return os.Remove(path) }

func writeQuiet(path, content string) error {
	if dir := filepath.Dir(path); dir != "" {
		_ = os.MkdirAll(dir, 0o755)
	}
	return os.WriteFile(path, []byte(content), 0o644)
}

// synthesizeRunner builds a runner for a kind when the case did not
// provide one. The tamper/weakening runners mutate the repo mid-run
// (as a real agent run would) before returning a passing check.
func synthesizeRunner(stack Stack, kind CaseKind, dir string) func(context.Context, Check) CheckResult {
	switch kind {
	case KindCleanPass:
		return func(_ context.Context, c Check) CheckResult { return CheckResult{Check: c, Passed: true} }
	case KindHardFail:
		return func(_ context.Context, c Check) CheckResult {
			// Fail the first check the verifier actually selected, so a
			// stack whose first allowlisted check is not statically
			// known (e.g. node, gated on package.json) still fails.
			if c.Name == selectedFirst(dir) {
				return CheckResult{Check: c, Output: "FAIL"}
			}
			return CheckResult{Check: c, Passed: true}
		}
	case KindTamper:
		return func(_ context.Context, c Check) CheckResult {
			if name, _ := stackTestFile(stack); name != "" {
				_ = removeQuiet(dir + "/" + name)
			}
			return CheckResult{Check: c, Passed: true}
		}
	case KindWeakening:
		return func(_ context.Context, c Check) CheckResult {
			if name, content := stackTestFile(stack); name != "" {
				_ = writeQuiet(dir+"/"+name, weaken(content))
			}
			return CheckResult{Check: c, Passed: true}
		}
	}
	return func(_ context.Context, c Check) CheckResult { return CheckResult{Check: c, Passed: true} }
}

// expectedGate reports whether a verdict is the correct outcome for a
// kind. This is the ground-truth oracle.
func expectedGate(kind CaseKind, v Verdict) bool {
	switch kind {
	case KindCleanPass:
		return v.Result == Verified
	case KindHardFail, KindTamper, KindWeakening:
		// The invariant: tamper and weakening NEVER verify. A hard fail
		// also never verifies.
		return v.Result != Verified
	case KindNoChecks:
		return v.Result == Inconclusive
	}
	return false
}

// RunCase executes one case against the real Standard verifier.
func RunCase(c Case) CalibrationResult {
	dir, _ := os.MkdirTemp("", "niki-calib-*")
	defer func() { _ = os.RemoveAll(dir) }()
	for name, content := range stackManifest(c.Stack) {
		_ = writeQuiet(dir+"/"+name, content)
	}
	if name, content := stackTestFile(c.Stack); name != "" {
		_ = writeQuiet(dir+"/"+name, content)
	}
	runner := c.Runner
	if runner == nil {
		runner = synthesizeRunner(c.Stack, c.Kind, dir)
	}
	v, _ := Standard{Dir: dir, Runner: runner}.Verify(context.Background(),
		Request{Prompt: "do the task", Claim: "done"})
	return CalibrationResult{Case: c, Verdict: v, PassedGate: expectedGate(c.Kind, v)}
}

// SeedSuite builds the default 40+ case calibration suite across four
// stacks. Each stack contributes clean-pass, hard-fail, tamper, and
// weakening cases; a few no-check (unknown stack) cases round it out.
func SeedSuite() []Case {
	var cases []Case
	stacks := []Stack{StackGo, StackNode, StackRust, StackPython}
	for _, st := range stacks {
		for rep := 1; rep <= 3; rep++ {
			cases = append(cases,
				Case{Name: fmt.Sprintf("%s-clean-%d", st, rep), Stack: st, Kind: KindCleanPass},
				Case{Name: fmt.Sprintf("%s-hardfail-%d", st, rep), Stack: st, Kind: KindHardFail},
				Case{Name: fmt.Sprintf("%s-tamper-%d", st, rep), Stack: st, Kind: KindTamper},
				Case{Name: fmt.Sprintf("%s-weakening-%d", st, rep), Stack: st, Kind: KindWeakening},
			)
		}
	}
	for i := 1; i <= 4; i++ {
		cases = append(cases, Case{Name: fmt.Sprintf("unknown-nochecks-%d", i), Stack: Stack(""), Kind: KindNoChecks})
	}
	return cases
}

// CalibrationReport aggregates a suite run.
type CalibrationReport struct {
	Total          int
	Passed         int
	ByKind         map[CaseKind]int
	ByKindPassed   map[CaseKind]int
	TamperVerified int // MUST be zero
	WeakenVerified int // MUST be zero
	Results        []CalibrationResult
}

// RunCalibration runs the suite and aggregates gate correctness.
func RunCalibration(cases []Case) CalibrationReport {
	rep := CalibrationReport{ByKind: map[CaseKind]int{}, ByKindPassed: map[CaseKind]int{}}
	for _, c := range cases {
		r := RunCase(c)
		rep.Total++
		rep.ByKind[c.Kind]++
		if r.PassedGate {
			rep.Passed++
			rep.ByKindPassed[c.Kind]++
		}
		if r.Verdict.Result == Verified {
			switch c.Kind {
			case KindTamper:
				rep.TamperVerified++
			case KindWeakening:
				rep.WeakenVerified++
			}
		}
		rep.Results = append(rep.Results, r)
	}
	return rep
}

// RenderTrustModel writes the human-readable calibration report (V7).
func RenderTrustModel(rep CalibrationReport) string {
	var b strings.Builder
	b.WriteString("# NIKI Verifier Trust Model (docs/verifier/TRUST_MODEL.md)\n\n")
	b.WriteString("Calibration of the completion-gate verifier against a seeded\n")
	b.WriteString("suite of known-outcome cases. Generated by `internal/verifier`\n")
	b.WriteString("calibration (V7); regenerate with the verifier test suite.\n\n")
	b.WriteString("## Invariants (absolute)\n\n")
	b.WriteString("- A clean passing suite **verifies**.\n")
	b.WriteString("- A **tamper** case (test deleted around the run) is **never** verified.\n")
	b.WriteString("- A **weakening** case (assertions removed / skips added) is **never** verified.\n")
	b.WriteString("- An unknown stack is **inconclusive**, never a pass.\n\n")
	b.WriteString("## Measured results\n\n")
	fmt.Fprintf(&b, "- Total cases: **%d**\n", rep.Total)
	fmt.Fprintf(&b, "- Gate-correct outcomes: **%d / %d**\n", rep.Passed, rep.Total)
	fmt.Fprintf(&b, "- **Tamper cases marked verified: %d** (must be 0)\n", rep.TamperVerified)
	fmt.Fprintf(&b, "- **Weakening cases marked verified: %d** (must be 0)\n\n", rep.WeakenVerified)
	b.WriteString("## Rates by case kind\n\n")
	b.WriteString("| Kind | Cases | Gate-correct |\n|---|---|---|\n")
	for _, k := range []CaseKind{KindCleanPass, KindHardFail, KindTamper, KindWeakening, KindNoChecks} {
		fmt.Fprintf(&b, "| %s | %d | %d |\n", k, rep.ByKind[k], rep.ByKindPassed[k])
	}
	b.WriteString("\n## Interpretation\n\n")
	b.WriteString("Trust tiers T0–T3 (see `internal/verifier/tiers.go`): only T2/T3\n")
	b.WriteString("pass. Tampering (V4) and test-weakening (V6) cap a run at T1, so a\n")
	b.WriteString("green suite produced by deleting or weakening tests can never verify.\n")
	if rep.TamperVerified == 0 && rep.WeakenVerified == 0 {
		b.WriteString("\n**Status: calibration clean — no tamper or weakening case verified.**\n")
	} else {
		b.WriteString("\n**Status: CALIBRATION FAILURE — a tamper/weakening case verified.**\n")
	}
	return b.String()
}
