package verifier

import (
	"bytes"
	"context"
	"fmt"
	"os/exec"
	"strings"
	"time"
)

// DefaultCheckTimeout bounds one allowlisted check.
const DefaultCheckTimeout = 120 * time.Second

// Standard is the product verifier: requirements become the
// contract (V1), checks come only from the stack allowlist (V2),
// the run is graded into trust tiers (V3), and test/CI/lockfile
// integrity is hashed before and after (V4). A planted
// test-deletion is flagged and blocks T2.
type Standard struct {
	// Dir is the repository under verification.
	Dir string
	// CheckTimeout bounds each check; <=0 means DefaultCheckTimeout.
	CheckTimeout time.Duration
	// Runner executes one check; nil runs the argv in Dir.
	// Tests inject fakes here — production always execs.
	Runner func(ctx context.Context, c Check) CheckResult
}

// Verify implements Verifier.
func (s Standard) Verify(ctx context.Context, r Request) (Verdict, error) {
	reqs := ExtractRequirements(r.Prompt)
	checks := SelectChecks(s.Dir)
	if len(checks) == 0 {
		return Verdict{Result: Inconclusive,
			Reason: fmt.Sprintf("no allowlisted checks for this repo (%d requirement(s) extracted, none verifiable here)", len(reqs))}, nil
	}
	before := SnapshotIntegrity(s.Dir, checks)
	strengthBefore := MeasureTests(s.Dir)

	results := make([]CheckResult, 0, len(checks))
	for _, c := range checks {
		results = append(results, s.runOne(ctx, c))
	}

	after := SnapshotIntegrity(s.Dir, checks)
	drift := before.Diff(after)
	integrityClean := len(drift) == 0

	// V6: a suite "made green" by weakening its tests (removed
	// assertions, new skips, commented-out checks) is not evidence.
	weakened := DetectWeakening(strengthBefore, MeasureTests(s.Dir))

	tier := Grade(results, integrityClean)
	if !integrityClean && tier >= T2Passed {
		// V4: tampering around the run blocks T2 — a green suite
		// over deleted tests is not evidence.
		tier = T1Weak
	}
	if len(weakened) > 0 && tier >= T2Passed {
		// V6: weakening blocks T2 the same way deletion does.
		tier = T1Weak
	}
	v := VerdictFor(tier)
	reason := fmt.Sprintf("%d requirement(s), %d check(s), trust %s%s",
		len(reqs), len(results), tier.String(), driftSuffix(drift))
	if len(weakened) > 0 {
		reason += "; test weakening: " + joinWeakening(weakened)
	}
	v.Reason = reason
	v = structuredVerdict(v, reqs, results, tier)
	return v, nil
}

func joinWeakening(w []WeakeningEvent) string {
	parts := make([]string, 0, len(w))
	for _, e := range w {
		parts = append(parts, e.String())
	}
	if len(parts) > 4 {
		parts = append(parts[:4], "…")
	}
	return strings.Join(parts, ", ")
}

func driftSuffix(drift []string) string {
	if len(drift) == 0 {
		return ""
	}
	return "; integrity drift: " + strings.Join(drift, ", ")
}

func (s Standard) runOne(ctx context.Context, c Check) CheckResult {
	if s.Runner != nil {
		return s.Runner(ctx, c)
	}
	timeout := s.CheckTimeout
	if timeout <= 0 {
		timeout = DefaultCheckTimeout
	}
	cctx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()
	cmd := exec.CommandContext(cctx, c.Argv[0], c.Argv[1:]...)
	cmd.Dir = s.Dir
	var buf bytes.Buffer
	cmd.Stdout, cmd.Stderr = &buf, &buf
	err := cmd.Run()
	out := buf.String()
	const maxOut = 4000
	if len(out) > maxOut {
		out = out[:maxOut] + "\n... (truncated)"
	}
	res := CheckResult{Check: c, Output: out}
	switch {
	case cctx.Err() == context.DeadlineExceeded:
		res.TimedOut = true
	case err != nil:
		res.Passed = false
		if res.Output == "" {
			res.Output = err.Error()
		}
	default:
		res.Passed = true
	}
	return res
}
