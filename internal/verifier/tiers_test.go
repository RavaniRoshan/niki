package verifier

import (
	"testing"
)

// V3: trust tiers T0-T3. Verified requires T2+; Inconclusive never
// passes. One case per tier.
func TestTrustTiers(t *testing.T) {
	pass := CheckResult{Check: Check{Name: "go-test"}, Passed: true}
	fail := CheckResult{Check: Check{Name: "go-test"}, Passed: false, Output: "FAIL"}
	cases := []struct {
		name     string
		results  []CheckResult
		clean    bool
		wantTier Tier
		wantVerd Result
		wantPass bool
	}{
		{"T0 nothing ran", nil, true, T0NothingRan, Inconclusive, false},
		{"T1 a check failed", []CheckResult{pass, fail}, true, T1Weak, NotVerified, false},
		{"T1 timeout", []CheckResult{{Check: Check{Name: "x"}, TimedOut: true}}, true, T1Weak, NotVerified, false},
		{"T2 passed, integrity unknown", []CheckResult{pass}, false, T2Passed, Verified, true},
		{"T3 passed and clean", []CheckResult{pass}, true, T3Clean, Verified, true},
	}
	for _, c := range cases {
		tier := Grade(c.results, c.clean)
		if tier != c.wantTier {
			t.Fatalf("[V3 %s] tier = %s, want %s", c.name, tier, c.wantTier)
		}
		v := VerdictFor(tier)
		if v.Result != c.wantVerd || v.Passed() != c.wantPass {
			t.Fatalf("[V3 %s] verdict = %+v, want %s pass=%v", c.name, v, c.wantVerd, c.wantPass)
		}
		t.Logf("[V3 %s] tier=%s verdict=%s pass=%v", c.name, tier, v.Result, v.Passed())
	}
	// Inconclusive is never a pass, even directly.
	if (Verdict{Result: Inconclusive}).Passed() {
		t.Fatalf("[V3] inconclusive must never pass")
	}
}
