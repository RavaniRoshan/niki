package tui

import (
	"context"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/verifier"
)

// buildTamperedRun drives a REAL tampered run through the product
// verifier: the check runner deletes the test file mid-run (the
// plant) and reports the suite green. Integrity drift caps the tier
// at T1, so the verdict is not-verified — the engine emits exactly
// what this returns.
func buildTamperedRun(t *testing.T) verifier.Verdict {
	t.Helper()
	dir := t.TempDir()
	write := func(rel, content string) {
		p := filepath.Join(dir, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write("go.mod", "module victim\n\ngo 1.21\n")
	write("calc_test.go", "package victim\n\nimport \"testing\"\n\nfunc TestCalc(t *testing.T) {}\n")
	write(".github/workflows/ci.yml", "jobs: {}\n")

	s := verifier.Standard{Dir: dir, Runner: func(_ context.Context, c verifier.Check) verifier.CheckResult {
		// The plant: the run deletes the test file to go green.
		_ = os.Remove(filepath.Join(dir, "calc_test.go"))
		return verifier.CheckResult{Check: c, Passed: true}
	}}
	v, err := s.Verify(context.Background(), verifier.Request{
		Prompt: "Expired tokens are rejected. Refresh keeps the session id. Add a limiter for failed logins.",
		Claim:  "all done",
	})
	if err != nil {
		t.Fatalf("verify: %v", err)
	}
	return v
}

// verificationEventFromVerdict builds the verification event the
// engine emits for a real verdict (the same field mapping as
// internal/engine/agent.go).
func verificationEventFromVerdict(v verifier.Verdict) protocol.EngineEvent {
	ver := &protocol.VerificationInfo{
		Attempt: 1, Result: string(v.Result), Reason: v.Reason, Tier: v.Tier,
	}
	for _, r := range v.Requirements {
		ver.Requirements = append(ver.Requirements, protocol.ReqStatus{
			ID: r.ID, Text: r.Text, Evidence: r.Evidence, State: r.State,
		})
	}
	ver.NotVerified = verifier.NotVerifiedLines(v.Requirements)
	for i, state := range verifier.ContractStates(v.Requirements) {
		ver.Contract = append(ver.Contract, protocol.ContractReq{
			ID: "R" + itoa(i+1), State: state,
		})
	}
	return protocol.EngineEvent{Type: protocol.EventVerification, Verification: ver}
}

// C3 rule: a planted tampered run must render "not verified", and
// the word "verified" must never appear for it. This test predates
// the verdict card: it fails until the card obeys the tier rule.
func TestTamperedRunRendersNotVerified(t *testing.T) {
	v := buildTamperedRun(t)
	if v.Result == verifier.Verified {
		t.Fatal("precondition: the tampered run must not verify")
	}
	if v.Tier == "T2" || v.Tier == "T3" {
		t.Fatalf("precondition: tampering must cap the tier, got %s", v.Tier)
	}

	m := classicModel(100, 27, false)
	m.state.ModelName = "claude-sonnet"
	m.state.Verdict = verdictFromEvent(verificationEventFromVerdict(v))
	m.renderLive()

	view := Downsample(m.View().Content)
	if !strings.Contains(view, "not verified") {
		t.Fatalf("tampered run must render 'not verified':\n%s", view)
	}
	// The word "verified" may appear only inside "not verified".
	for _, line := range strings.Split(view, "\n") {
		if trimmed := strings.TrimSpace(line); strings.Contains(trimmed, "verified") {
			if !strings.Contains(trimmed, "not verified") {
				t.Fatalf("tampered run rendered a bare 'verified': %q", line)
			}
		}
	}
	if !strings.Contains(view, "calc_test.go") && !strings.Contains(view, "removed") {
		t.Logf("tamper drift detail not in card reason: %q", v.Reason)
	}
}

// C3 rule: a genuinely passing run at T2+ with every required
// requirement evidenced renders "verified".
func TestVerifiedRunRendersVerified(t *testing.T) {
	ver := &protocol.VerificationInfo{
		Result: "verified", Tier: "T2", Reason: "3 requirement(s), 1 check(s), trust T2 (passed)",
		Requirements: []protocol.ReqStatus{
			{ID: "R1", Text: "Expired tokens are rejected", State: "pass", Evidence: "go test ./... · passed"},
			{ID: "R2", Text: "Refresh keeps the session id", State: "pass", Evidence: "go test ./... · passed"},
			{ID: "R3", Text: "Rate limit returns 429", State: "pass", Evidence: "go test ./... · passed"},
		},
	}
	m := classicModel(100, 27, false)
	m.state.Verdict = verdictFromEvent(protocol.EngineEvent{Type: protocol.EventVerification, Verification: ver})
	m.renderLive()
	view := Downsample(m.View().Content)
	if !strings.Contains(view, "verified") {
		t.Fatalf("passing T2 run must render 'verified':\n%s", view)
	}
	if !strings.Contains(view, "3 of 3 required") {
		t.Fatalf("card must count the required requirements:\n%s", view)
	}
	if !strings.Contains(view, "trust T2") {
		t.Fatalf("verdict card must name the trust tier:\n%s", view)
	}
	if strings.Contains(view, "not verified") {
		t.Fatalf("a fully verified card must not list anything as not verified:\n%s", view)
	}
}

// C3 rule: at T2+ a required requirement with no check is not
// passing evidence — the headline reads "not verified" and the card
// says which requirement and why.
func TestVerifiedTierWithUnevidencedRequirementStaysNotVerified(t *testing.T) {
	ver := &protocol.VerificationInfo{
		Result: "verified", Tier: "T2", Reason: "3 requirement(s), 1 check(s), trust T2 (passed)",
		Requirements: []protocol.ReqStatus{
			{ID: "R1", Text: "Expired tokens are rejected", State: "pass", Evidence: "go test ./... · passed"},
			{ID: "R2", Text: "Refresh keeps the session id", State: "pass", Evidence: "go test ./... · passed"},
			{ID: "R3", Text: "Rate limit returns 429", State: "queued", Evidence: "no check is configured for it"},
		},
		NotVerified: []string{"R3 Rate limit returns 429: no check is configured for it"},
	}
	m := classicModel(100, 27, false)
	m.state.Verdict = verdictFromEvent(protocol.EngineEvent{Type: protocol.EventVerification, Verification: ver})
	view := Downsample(m.View().Content)
	if !strings.Contains(view, "not verified") {
		t.Fatalf("an unevidenced required requirement must block the verified headline:\n%s", view)
	}
	if !strings.Contains(view, "R3") || !strings.Contains(view, "no check is configured") {
		t.Fatalf("the card must name the requirement and why:\n%s", view)
	}
	for _, line := range strings.Split(view, "\n") {
		if trimmed := strings.TrimSpace(line); strings.Contains(trimmed, "verified") &&
			!strings.Contains(trimmed, "not verified") {
			t.Fatalf("bare 'verified' with an unevidenced requirement: %q", line)
		}
	}
}

// C3 rule: T1 (or below) never renders the word "verified", even
// with a "verified" result string — the tier is the gate.
func TestWeakTierNeverRendersVerified(t *testing.T) {
	ver := &protocol.VerificationInfo{
		Result: "verified", Tier: "T1", Reason: "1 requirement(s), 1 check(s), trust T1 (weak)",
		Requirements: []protocol.ReqStatus{
			{ID: "R1", Text: "Expired tokens are rejected", State: "fail", Evidence: "go test ./... · FAIL"},
		},
		NotVerified: []string{"R1 Expired tokens are rejected: go test ./... · FAIL"},
	}
	m := classicModel(100, 27, false)
	m.state.Verdict = verdictFromEvent(protocol.EngineEvent{Type: protocol.EventVerification, Verification: ver})
	view := Downsample(m.View().Content)
	if strings.Contains(view, "verified") && !strings.Contains(view, "not verified") {
		t.Fatalf("T1 must not render 'verified':\n%s", view)
	}
	for _, line := range strings.Split(view, "\n") {
		if trimmed := strings.TrimSpace(line); strings.Contains(trimmed, "verified") &&
			!strings.Contains(trimmed, "not verified") {
			t.Fatalf("T1 rendered a bare 'verified': %q", line)
		}
	}
}

// C3: the gate defaults focus to the option that keeps the
// constraint — the event's safe index, never the first option by
// accident.
func TestGateDefaultsToConstraintKeepingOption(t *testing.T) {
	// Safe is the SECOND option here; focus must land there.
	evt := protocol.EngineEvent{Type: protocol.EventQuestionPrompted, Questions: []protocol.UserQuestion{{
		Header:   "needs you",
		Question: "add golang.org/x/time?",
		Options:  []string{"allow once", "write the limiter without it"},
		Safe:     1,
	}}}
	cmdChan := make(chan protocol.EngineCommand, 4)
	eventChan := make(chan protocol.EngineEvent, 4)
	m := NewAppModel(cmdChan, eventChan)
	m.SetLayout("classic")
	sz, _ := m.Update(tea.WindowSizeMsg{Width: 100, Height: 27})
	m = sz.(AppModel)
	um, _ := m.Update(engineEventMsg(evt))
	m = um.(AppModel)
	if !m.state.Gate.Active {
		t.Fatal("question event must open the gate")
	}
	if m.state.Gate.Selected != 1 {
		t.Fatalf("gate focus = %d, want 1 (the constraint-keeping option)", m.state.Gate.Selected)
	}
	block := Downsample(m.classicGate())
	if !strings.Contains(block, "(default)") {
		t.Fatalf("the safe option must be marked default: %q", block)
	}
	// The (default) marker sits on the safe option, not the other.
	// Check the options line only (the hint line mentions both
	// option words by design).
	var optionsLine string
	for _, l := range strings.Split(block, "\n") {
		if strings.Contains(l, "[n]") {
			optionsLine = l
		}
	}
	if optionsLine == "" {
		t.Fatalf("no options line in gate block: %q", block)
	}
	for _, seg := range strings.Split(optionsLine, "    ") {
		if strings.Contains(seg, "allow once") && strings.Contains(seg, "(default)") {
			t.Fatalf("the unsafe option must not be the default: %q", seg)
		}
		if strings.Contains(seg, "write the limiter without it") && !strings.Contains(seg, "(default)") {
			t.Fatalf("the safe option must carry the default marker: %q", seg)
		}
	}
	// n answers with the safe option; y/v answer theirs.
	um2, _ := m.Update(tea.KeyPressMsg{Text: "n", Code: 'n'})
	m = um2.(AppModel)
	if m.state.Gate.Active {
		t.Fatal("answering must close the gate")
	}
	select {
	case c := <-cmdChan:
		if c.Type != protocol.CmdAnswerQuestion || len(c.Answers) != 1 || c.Answers[0] != "write the limiter without it" {
			t.Fatalf("n must answer the safe option, got %+v", c)
		}
	default:
		t.Fatal("n must dispatch the answer to the engine")
	}
}

// C3: gate footer hints come from the registry and dispatch.
func TestGateFooterHintsDispatch(t *testing.T) {
	m := classicModel(100, 27, false)
	evt := protocol.EngineEvent{Type: protocol.EventQuestionPrompted, Questions: []protocol.UserQuestion{{
		Header:   "needs you",
		Question: "add golang.org/x/time?",
		Options:  []string{"write the limiter without it", "allow once", "view the diff"},
		Safe:     0,
	}}}
	um, _ := m.Update(engineEventMsg(evt))
	m = um.(AppModel)
	if !m.state.Gate.Active {
		t.Fatal("precondition: gate open")
	}
	footer := m.classicFooter()
	for _, kb := range FooterHints(hintGate) {
		if !strings.Contains(footer, kb.Hint) {
			t.Fatalf("gate hint %q missing: %q", kb.Hint, footer)
		}
		um, _ := m.Update(kb.Key)
		after := um.(AppModel)
		dispatched := !after.state.Gate.Active ||
			len(after.history.Cells) > len(m.history.Cells)
		if !dispatched {
			t.Fatalf("gate hint key %q did not dispatch", kb.ID)
		}
		m = after
	}
}

// C3: the contract strip renders only from the contract event and
// counts-only under 80 columns.
func TestContractStripFromEvents(t *testing.T) {
	m := classicModel(100, 27, false)
	if m.classicContractLine() != "" {
		t.Fatal("no contract without an event")
	}
	evt := protocol.EngineEvent{Type: protocol.EventTurnStarted, Text: "one. two.",
		Contract: []protocol.ContractReq{
			{ID: "R1", State: "pass"}, {ID: "R2", State: "fail"}, {ID: "R3", State: "running"},
		}}
	um, _ := m.Update(engineEventMsg(evt))
	m = um.(AppModel)
	strip := Downsample(m.classicContractLine())
	if !strings.Contains(strip, "R1") || !strings.Contains(strip, "R2") || !strings.Contains(strip, "R3") {
		t.Fatalf("strip missing requirement ids: %q", strip)
	}
	if !strings.Contains(strip, "1 of 3") {
		t.Fatalf("strip must count passing requirements: %q", strip)
	}
	narrow := classicModel(70, 27, false)
	um2, _ := narrow.Update(engineEventMsg(evt))
	narrow = um2.(AppModel)
	if got := Downsample(narrow.classicContractLine()); !strings.Contains(got, "1 of 3") || strings.Contains(got, "R2") {
		t.Fatalf("under 80 columns the strip must be counts-only: %q", got)
	}
}

// C3 golden frames 4-5: failure+gate and verdict at 100x27, 80x24,
// 60x20, plus plain mode.
func TestClassicFrames45Golden(t *testing.T) {
	sizes := []struct {
		name string
		w, h int
	}{
		{"100x27", 100, 27},
		{"80x24", 80, 24},
		{"60x20", 60, 20},
	}
	modes := []struct {
		name  string
		setup func()
	}{
		{"normal", func() {
			t.Setenv("TERM", "xterm-256color")
			os.Unsetenv("NO_COLOR")
			os.Unsetenv("NIKI_PLAIN")
			os.Unsetenv("NIKI_ASCII")
			ApplyDisplayEnv(io.Discard)
		}},
		{"plain", func() {
			t.Setenv("TERM", "xterm-256color")
			t.Setenv("NIKI_PLAIN", "1")
			os.Unsetenv("NIKI_ASCII")
			ApplyDisplayEnv(io.Discard)
		}},
	}
	for _, mode := range modes {
		for _, stage := range []string{"frame4", "frame5"} {
			for _, size := range sizes {
				mode.setup()
				m := classicFixtureSetup(size.w, size.h, stage)
				got := Downsample(m.View().Content)
				golden := filepath.Join("testdata", "classic_"+stage+"_"+size.name+"_"+mode.name+".golden")
				if os.Getenv("NIKI_WRITE_GOLDEN") == "1" {
					if err := os.WriteFile(golden, []byte(got), 0o644); err != nil {
						t.Fatal(err)
					}
					continue
				}
				want, err := os.ReadFile(golden)
				if err != nil {
					if os.IsNotExist(err) {
						t.Fatalf("missing golden %s (generate with NIKI_WRITE_GOLDEN=1)", golden)
					}
					t.Fatal(err)
				}
				if string(want) != got {
					t.Fatalf("[classic %s %s %s] drifted from golden; regenerate with NIKI_WRITE_GOLDEN=1 after printing the diff", stage, size.name, mode.name)
				}
				t.Logf("[classic %s %s %s] golden matches (%d bytes)", stage, size.name, mode.name, len(got))
			}
		}
	}
}
