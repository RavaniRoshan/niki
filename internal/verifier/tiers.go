package verifier

// Tier is the trust tier of one verification run (V3). "Verified"
// requires T2 or higher; Inconclusive (T0) is never a pass.
type Tier int

const (
	// T0NothingRan: no check executed — the claim is unevidenced.
	T0NothingRan Tier = 0
	// T1Weak: checks ran but at least one failed or was
	// inconclusive — evidence exists but does not support the claim.
	T1Weak Tier = 1
	// T2Passed: every selected check passed.
	T2Passed Tier = 2
	// T3Clean: every check passed and the integrity snapshot is
	// unchanged (no test/CI/lockfile tampering around the run).
	T3Clean Tier = 3
)

// String names the tier for verdict reasons.
func (t Tier) String() string {
	switch t {
	case T0NothingRan:
		return "T0 (nothing ran)"
	case T1Weak:
		return "T1 (weak)"
	case T2Passed:
		return "T2 (passed)"
	case T3Clean:
		return "T3 (clean)"
	default:
		return "T? (unknown)"
	}
}

// CheckResult is the outcome of one executed check.
type CheckResult struct {
	Check    Check  `json:"check"`
	Passed   bool   `json:"passed"`
	Output   string `json:"output,omitempty"`
	Skipped  bool   `json:"skipped,omitempty"`
	TimedOut bool   `json:"timed_out,omitempty"`
}

// Grade computes the tier from check results and the integrity
// flag: no results → T0; any failure/timeout → T1; all passed →
// T2, promoted to T3 when integrity is confirmed clean.
func Grade(results []CheckResult, integrityClean bool) Tier {
	if len(results) == 0 {
		return T0NothingRan
	}
	for _, r := range results {
		if r.Skipped || r.TimedOut || !r.Passed {
			return T1Weak
		}
	}
	if integrityClean {
		return T3Clean
	}
	return T2Passed
}

// VerdictFor maps a tier to the completion-gate verdict: only T2+
// passes; T0 is Inconclusive and never passes (V3).
func VerdictFor(tier Tier) Verdict {
	switch {
	case tier >= T2Passed:
		return Verdict{Result: Verified, Reason: "trust " + tier.String()}
	case tier == T1Weak:
		return Verdict{Result: NotVerified, Reason: "trust " + tier.String() + ": a check failed or was inconclusive"}
	default:
		return Verdict{Result: Inconclusive, Reason: "trust " + tier.String() + ": no check evidence"}
	}
}
