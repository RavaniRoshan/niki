package verifier

import (
	"context"
	"errors"
	"testing"
)

// L3/V3 vocabulary: only "verified" passes; inconclusive never passes.
func TestVerdictPassed(t *testing.T) {
	cases := []struct {
		res    Result
		passed bool
	}{
		{Verified, true},
		{NotVerified, false},
		{Inconclusive, false},
		{Result(""), false},
	}
	for _, c := range cases {
		got := (Verdict{Result: c.res}).Passed()
		if got != c.passed {
			t.Errorf("%q passed=%v want %v", c.res, got, c.passed)
		}
	}
}

func TestFuncAdapter(t *testing.T) {
	v := Func(func(_ context.Context, r Request) (Verdict, error) {
		if r.Claim != "done" {
			return Verdict{Result: NotVerified, Reason: "bad claim"}, nil
		}
		return Verdict{Result: Verified, Reason: "checks passed"}, nil
	})
	got, err := v.Verify(context.Background(), Request{Prompt: "p", Claim: "done", Attempt: 1})
	if err != nil || !got.Passed() {
		t.Fatalf("want verified pass, got %+v err=%v", got, err)
	}
	got, err = v.Verify(context.Background(), Request{Claim: "nope"})
	if err != nil || got.Passed() {
		t.Fatalf("want not-verified, got %+v err=%v", got, err)
	}
}

// An erroring verifier is never a pass.
func TestErrorIsNotAPass(t *testing.T) {
	v := Func(func(context.Context, Request) (Verdict, error) {
		return Verdict{Result: Verified}, errors.New("boom")
	})
	got, err := v.Verify(context.Background(), Request{})
	if err == nil {
		t.Fatal("expected error to surface")
	}
	if got.Passed() {
		t.Error("error must never count as verified")
	}
}
