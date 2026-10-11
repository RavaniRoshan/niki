// Package perf holds the performance-budget model (Q3): a baseline, an
// absolute cap, and a regression tolerance. A check fails when the current
// measurement exceeds the baseline by more than the tolerance, or when it
// breaches the absolute cap. The math is the testable, deterministic core;
// the tools/perfbudget binary feeds it real numbers in CI.
package perf

import "fmt"

// Budget is one performance contract row.
type Budget struct {
	Name      string  // metric name, e.g. "niki_version_ms"
	Baseline  float64 // recorded baseline; 0 disables the regression check
	Current   float64 // measured this run
	Limit     float64 // absolute cap; 0 disables
	Tolerance float64 // fractional regression that fails (0.10 = 10%)
}

// Violation is why one budget failed.
type Violation struct {
	Name     string
	Reason   string // "regression" or "absolute"
	DeltaPct float64
	Current  float64
	Baseline float64
	Limit    float64
}

func (v Violation) Error() string {
	switch v.Reason {
	case "absolute":
		return fmt.Sprintf("[Q3] %s over absolute limit: %.3f > %.3f", v.Name, v.Current, v.Limit)
	default:
		return fmt.Sprintf("[Q3] %s regressed %.1f%%: baseline %.3f → current %.3f (limit %.3f)", v.Name, v.DeltaPct, v.Baseline, v.Current, v.Limit)
	}
}

// Check reports every budget that fails its regression or absolute limit.
// A budget with both Baseline==0 and Limit==0 is ignored (disabled).
func Check(budgets []Budget) []Violation {
	var out []Violation
	for _, b := range budgets {
		if b.Baseline == 0 && b.Limit == 0 {
			continue
		}
		// Absolute cap fires first; it is an independent quality gate.
		if b.Limit > 0 && b.Current > b.Limit {
			out = append(out, Violation{Name: b.Name, Reason: "absolute", Current: b.Current, Limit: b.Limit})
			continue
		}
		// Regression check only when a baseline exists and tolerance > 0.
		if b.Baseline > 0 && b.Tolerance > 0 && b.Current > b.Baseline*(1+b.Tolerance) {
			delta := 100 * (b.Current - b.Baseline) / b.Baseline
			out = append(out, Violation{
				Name: b.Name, Reason: "regression", DeltaPct: delta,
				Current: b.Current, Baseline: b.Baseline, Limit: b.Limit,
			})
		}
	}
	return out
}

// DefaultTolerance is the foundation-pack regression tolerance (Q3).
const DefaultTolerance = 0.10
