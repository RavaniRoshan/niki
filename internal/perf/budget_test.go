package perf

import (
	"strings"
	"testing"
)

// Q3: performance budgets enforced as a regression gate. Accept: a regression
// above 10 percent fails. These are the deterministic contracts; real numbers
// come from tools/perfbudget in CI.

func TestCheckRegressionAboveToleranceFails(t *testing.T) {
	b := Budget{Name: "niki_version_ms", Baseline: 6.5, Current: 7.2, Tolerance: DefaultTolerance}
	// 7.2 > 6.5 * 1.10 = 7.15 → regression ~10.8% → FAIL.
	vs := Check([]Budget{b})
	if len(vs) != 1 {
		t.Fatalf("[Q3] regression over 10%% should fail, got %d violations", len(vs))
	}
	if vs[0].Reason != "regression" || vs[0].DeltaPct < 10 {
		t.Fatalf("[Q3] wrong violation: %+v", vs[0])
	}
	t.Logf("[Q3] %s", vs[0])
}

func TestCheckWithinTolerancePasses(t *testing.T) {
	b := Budget{Name: "niki_version_ms", Baseline: 6.5, Current: 6.8, Tolerance: DefaultTolerance}
	vs := Check([]Budget{b})
	if len(vs) != 0 {
		t.Fatalf("[Q3] regression within 10%% should pass, got %+v", vs)
	}
	t.Logf("[Q3] 6.5 → 6.8 within 10%% tolerance: clean")
}

func TestCheckAbsoluteLimitFails(t *testing.T) {
	b := Budget{Name: "binary_mb", Baseline: 19.2, Current: 26.0, Limit: 25.0, Tolerance: DefaultTolerance}
	vs := Check([]Budget{b})
	if len(vs) != 1 || vs[0].Reason != "absolute" {
		t.Fatalf("[Q3] over absolute limit should fail: %+v", vs)
	}
	t.Logf("[Q3] %s", vs[0])
}

func TestCheckDisabledBudgetIgnored(t *testing.T) {
	vs := Check([]Budget{{Name: "disabled", Baseline: 0, Current: 999, Tolerance: DefaultTolerance}})
	if len(vs) != 0 {
		t.Fatalf("[Q3] zero-baseline budget should be ignored: %+v", vs)
	}
	t.Logf("[Q3] disabled budget ignored")
}

func TestCheckNoFalsePositiveAtExactTolerance(t *testing.T) {
	// Current == baseline * 1.10 exactly: the contract is "above 10% fails",
	// so the boundary (==) passes.
	b := Budget{Name: "x", Baseline: 100, Current: 110, Tolerance: DefaultTolerance}
	vs := Check([]Budget{b})
	if len(vs) != 0 {
		t.Fatalf("[Q3] at-exact-tolerance boundary should pass: %+v", vs)
	}
	t.Logf("[Q3] at boundary (= baseline+10%%): clean")
}

func TestViolationMessage(t *testing.T) {
	v := Violation{Name: "niki_version_ms", Reason: "regression", DeltaPct: 10.8, Current: 7.2, Baseline: 6.5}
	if !strings.Contains(v.Error(), "[Q3]") || !strings.Contains(v.Error(), "regressed") {
		t.Fatalf("[Q3] bad message: %s", v.Error())
	}
	t.Logf("[Q3] message: %s", v.Error())
}
