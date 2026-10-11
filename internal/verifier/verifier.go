// Package verifier defines the completion gate (L3): a run ends as
// done only through a verifier verdict. A model's claim of completion
// is recorded and handed to the verifier; only Result=Verified passes.
// Inconclusive never passes (V3).
package verifier

import "context"

// Result is a verification outcome.
type Result string

const (
	Verified     Result = "verified"
	NotVerified  Result = "not-verified"
	Inconclusive Result = "inconclusive"
)

// Verdict is the outcome of one verification attempt.
type Verdict struct {
	Result Result `json:"result"`
	Reason string `json:"reason,omitempty"`

	// Tier is the graded trust tier code ("T0".."T3") (C3).
	Tier string `json:"tier,omitempty"`
	// Requirements is the per-requirement evidence in prompt order
	// (C3): the verdict card renders exactly this.
	Requirements []ReqEvidence `json:"requirements,omitempty"`
}

// Passed reports whether the verdict ends the run as done.
// Only Verified passes; Inconclusive never does (V3).
func (v Verdict) Passed() bool { return v.Result == Verified }

// Request describes what is being verified.
type Request struct {
	Prompt  string `json:"prompt"`
	Claim   string `json:"claim"`
	Attempt int    `json:"attempt"`
}

// Verifier runs checks for one completion claim.
type Verifier interface {
	Verify(ctx context.Context, r Request) (Verdict, error)
}

// Func adapts a function to the Verifier interface.
type Func func(ctx context.Context, r Request) (Verdict, error)

// Verify implements Verifier. An erroring verifier never passes: the
// returned verdict is downgraded to NotVerified before the error.
func (f Func) Verify(ctx context.Context, r Request) (Verdict, error) {
	v, err := f(ctx, r)
	if err != nil {
		return Verdict{Result: NotVerified, Reason: "verifier error: " + err.Error()}, err
	}
	return v, nil
}
