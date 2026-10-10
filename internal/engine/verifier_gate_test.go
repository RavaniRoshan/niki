package engine

import (
	"context"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
	"github.com/RavaniRoshan/niki/internal/tools"
	"github.com/RavaniRoshan/niki/internal/verifier"
)

// claimFixture always answers with a completion claim — no tools — so
// every round trips the verifier gate (L3).
type claimFixture struct{}

func (claimFixture) Name() string { return "claim-fixture" }

func (claimFixture) Stream(ctx context.Context, _ []provider.Message) (<-chan provider.Delta, <-chan error) {
	deltas := make(chan provider.Delta, 3)
	errs := make(chan error, 1)
	go func() {
		defer close(deltas)
		defer close(errs)
		claim := "done — everything is complete and all checks pass"
		select {
		case <-ctx.Done():
			errs <- ctx.Err()
			return
		case deltas <- provider.Delta{Kind: provider.DeltaText, Text: claim}:
		}
		select {
		case <-ctx.Done():
			errs <- ctx.Err()
			return
		case deltas <- provider.Delta{Kind: provider.DeltaUsage, Usage: &provider.Usage{PromptTokens: 10, CompletionTokens: 5}}:
		}
		deltas <- provider.Delta{Kind: provider.DeltaDone}
	}()
	return deltas, errs
}

// L3 acceptance: a fixture where the model says "done" with a failing
// check ends not-verified. The claim is recorded but never ends the run
// by itself.
func TestModelDoneClaimWithFailingCheckEndsNotVerified(t *testing.T) {
	eng, cmdChan, eventChan := NewEngine(100, claimFixture{}, tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeFullAccess))
	attempts := 0
	eng.SetVerifier(verifier.Func(func(context.Context, verifier.Request) (verifier.Verdict, error) {
		attempts++
		return verifier.Verdict{Result: verifier.NotVerified, Reason: "planted check failure"}, nil
	}), 2) // verifier.max_loops = 2
	go eng.Run()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "finish the task"}

	var (
		verifyNotOK int
		sawTerminal *protocol.EngineEvent
		sawDone     bool
	)
	deadline := time.After(15 * time.Second)
	for sawTerminal == nil {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("closed")
			}
			switch evt.Type {
			case protocol.EventVerification:
				if evt.Verification == nil {
					t.Fatal("verification event missing payload")
				}
				if evt.Verification.Result == string(verifier.NotVerified) {
					verifyNotOK++
				}
			case protocol.EventTurnNotVerified:
				e := evt
				sawTerminal = &e
			case protocol.EventTurnCompleted:
				sawDone = true
			}
		case <-deadline:
			t.Fatalf("timeout (verifications not-verified=%d)", verifyNotOK)
		}
	}
	time.Sleep(50 * time.Millisecond)

	if sawDone {
		t.Fatal("model claim ended the run as done despite failing checks")
	}
	if verifyNotOK < 2 {
		t.Errorf("want two failing verification attempts (max_loops=2), got %d", verifyNotOK)
	}
	if sawTerminal.Verification == nil && !strings.Contains(sawTerminal.Text, "not") && sawTerminal.Text == "" {
		t.Error("terminal not-verified event must carry the reason")
	}
	if attempts != 2 {
		t.Errorf("verifier called %d times, want 2", attempts)
	}
	// The claim was recorded (context + session) before any verdict.
	claimInContext := false
	for _, m := range eng.runner.Context.Snapshot() {
		if m.Role == "assistant" && strings.Contains(m.Content, "done — everything is complete") {
			claimInContext = true
		}
	}
	if !claimInContext {
		t.Error("completion claim not recorded in context")
	}
	// Feedback was fed back after the first failure (loop, not end).
	feedback := false
	for _, m := range eng.runner.Context.Snapshot() {
		if m.Role == "user" && strings.Contains(strings.ToLower(m.Content), "verification") {
			feedback = true
		}
	}
	if !feedback {
		t.Error("failing check must feed verification feedback back into the run")
	}
	claimInSession := false
	for _, e := range eng.session.Events {
		if e.Type == protocol.EventAssistantTextDelta && strings.Contains(e.Text, "done — everything") {
			claimInSession = true
		}
	}
	if !claimInSession {
		t.Error("completion claim not recorded in session")
	}
	t.Logf("[L3 failing check] claim recorded; %d verification attempts (not-verified); ended %s (no turn_completed); feedback fed back",
		verifyNotOK, sawTerminal.Type)
}

// Passing checks end the run as done — through the verifier only.
func TestVerifiedClaimEndsRunDone(t *testing.T) {
	eng, cmdChan, eventChan := NewEngine(100, claimFixture{}, tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeFullAccess))
	eng.SetVerifier(verifier.Func(func(context.Context, verifier.Request) (verifier.Verdict, error) {
		return verifier.Verdict{Result: verifier.Verified, Reason: "all checks pass"}, nil
	}), 0)
	go eng.Run()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "finish the task"}

	var sawVerify, sawDone, sawNotVerified bool
	deadline := time.After(10 * time.Second)
	for !sawDone {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("closed")
			}
			switch evt.Type {
			case protocol.EventVerification:
				sawVerify = true
			case protocol.EventTurnCompleted:
				sawDone = true
			case protocol.EventTurnNotVerified:
				sawNotVerified = true
			}
		case <-deadline:
			t.Fatal("timeout waiting for verified completion")
		}
	}
	if !sawVerify {
		t.Error("completion must pass through a verification event")
	}
	if sawNotVerified {
		t.Error("verified run must not emit turn_not_verified")
	}
	t.Log("[L3 verified path] claim -> verification(verified) -> turn_completed")
}

// No verifier configured: the claim cannot end the run as done.
func TestNoVerifierNeverMarksDone(t *testing.T) {
	eng, cmdChan, eventChan := NewEngine(100, claimFixture{}, tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeFullAccess))
	// SetVerifier not called: no verifier wired.
	go eng.Run()
	defer eng.Stop()

	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "finish the task"}

	var terminal, done bool
	deadline := time.After(10 * time.Second)
	for !terminal {
		select {
		case evt, ok := <-eventChan:
			if !ok {
				t.Fatal("closed")
			}
			switch evt.Type {
			case protocol.EventTurnNotVerified:
				terminal = true
			case protocol.EventTurnCompleted:
				done = true
				terminal = true
			}
		case <-deadline:
			t.Fatal("timeout")
		}
	}
	if done {
		t.Fatal("run ended done without a verifier (claim ended the run by itself)")
	}
	t.Log("[L3 no verifier] claim recorded; ended turn_not_verified (inconclusive is never a pass)")
}
