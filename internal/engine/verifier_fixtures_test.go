package engine

import (
	"context"

	"github.com/RavaniRoshan/niki/internal/verifier"
)

// passVerifier ends claims as verified. Fixture tests that are not
// about the L3 gate itself install this so their runs can complete;
// the gate is exercised in verifier_gate_test.go.
func passVerifier() verifier.Verifier {
	return verifier.Func(func(context.Context, verifier.Request) (verifier.Verdict, error) {
		return verifier.Verdict{Result: verifier.Verified, Reason: "fixture pass"}, nil
	})
}
