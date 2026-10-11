package verifier

import (
	"context"
	"fmt"
)

// DefaultFlakeReruns is how many times a check is re-run to detect
// flakiness (V5). A check whose outcome changes across reruns is
// flaky: it is reported as Inconclusive, never as a pass.
const DefaultFlakeReruns = 3

// FlakeResult records one check's behavior across reruns.
type FlakeResult struct {
	Check     Check
	Outcomes  []bool // one per rerun (true = passed)
	Diverged  bool   // outcomes disagree across reruns
	Tier      Tier   // tier if the check is treated as stable
	Inconclus bool   // the check is flaky and must be treated as inconclusive
}

// detectFlake re-runs a check k times and reports whether its outcome
// diverged (V5). A stable check yields its normal tier; a divergent
// (flaky) check yields T0/NothingRan-style evidence and is marked
// inconclusive so the requirement can never pass on a coin flip.
//
// The runner is the same seam Standard uses (tests inject fakes;
// production execs). If a rerun times out or errors it is treated as
// a non-pass outcome for divergence purposes.
func detectFlake(ctx context.Context, runner func(context.Context, Check) CheckResult, c Check, k int) FlakeResult {
	if k < 2 {
		k = DefaultFlakeReruns
	}
	fr := FlakeResult{Check: c}
	for i := 0; i < k; i++ {
		res := runner(ctx, c)
		fr.Outcomes = append(fr.Outcomes, res.Passed && !res.TimedOut && !res.Skipped)
	}
	first := fr.Outcomes[0]
	for _, o := range fr.Outcomes[1:] {
		if o != first {
			fr.Diverged = true
			break
		}
	}
	if fr.Diverged {
		fr.Inconclus = true
		return fr
	}
	fr.Tier = Grade([]CheckResult{{Check: c, Passed: first}}, true)
	return fr
}

// RerunChecks runs each check with flake detection and folds the
// results into a single tier and a per-check explanation (V5).
// Any diverged (flaky) check forces Inconclusive: a requirement that
// passes sometimes and fails sometimes is never "verified".
func RerunChecks(ctx context.Context, runner func(context.Context, Check) CheckResult, checks []Check, k int) (Tier, []string) {
	var notes []string
	stable := make([]CheckResult, 0, len(checks))
	anyDiverged := false
	for _, c := range checks {
		fr := detectFlake(ctx, runner, c, k)
		if fr.Diverged {
			anyDiverged = true
			notes = append(notes, fmt.Sprintf("%s: flaky (outcomes %v) -> inconclusive", c.Name, fr.Outcomes))
			continue
		}
		stable = append(stable, CheckResult{Check: c, Passed: fr.Outcomes[0]})
	}
	if anyDiverged {
		// A flaky check removes all certainty: grade the stable
		// remainder but cap the tier at T1 and never verify.
		return T1Weak, notes
	}
	return Grade(stable, true), notes
}
