package tui

import (
	tea "charm.land/bubbletea/v2"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// The five-frame classic fixture: scripted engine events producing
// the mockup frames (docs/ui/CLASSIC.md §2). Every row corresponds
// to a real engine event shape — the same replay drives the golden
// frames, the [d] recorded-demo choice, and the row-rendering tests.
var classicStages = []struct {
	name   string
	events []protocol.EngineEvent
	// preKeys are user key presses applied before this stage's
	// events (frame 5 answers the frame 4 gate with n).
	preKeys []tea.KeyPressMsg
}{
	{"frame1", nil, nil},
	{"frame2", []protocol.EngineEvent{
		{Type: protocol.EventTurnStarted, TurnID: "t1", Text: "harden POST /login: reject expired tokens and rate-limit failed logins",
			Contract: []protocol.ContractReq{
				{ID: "R1", State: "queued"}, {ID: "R2", State: "queued"}, {ID: "R3", State: "queued"},
				{ID: "R4", State: "queued"}, {ID: "R5", State: "queued"},
			}},
		{Type: protocol.EventAssistantTextDelta, TurnID: "t1",
			Text:  "Reading the handler and its tests first, then adding a limiter for failed logins.\nContract: 5 requirements. Assumed: limit is per client IP; only failed logins count.",
			Usage: &protocol.Usage{PromptTokens: 5120, TotalTokens: 5120}},
	}, nil},
	{"frame3", []protocol.EngineEvent{
		{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "c1", ToolName: "read_file", Target: "internal/auth/token.go"},
		{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "c1", ToolName: "read_file", Result: "112 lines"},
		{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "c2", ToolName: "read_file", Target: "internal/auth/token_test.go"},
		{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "c2", ToolName: "read_file", Result: "64 lines"},
		{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "c3", ToolName: "grep", Target: `"RateLimit" in internal/**`},
		{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "c3", ToolName: "grep", Result: "3 matches"},
		{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "c4", ToolName: "edit_file", Target: "internal/auth/limit.go"},
		{Type: protocol.EventPlanUpdated, TurnID: "t1", Plan: []protocol.PlanStep{
			{Description: "Read the handler and its tests", Status: "completed"},
			{Description: "Add the limiter", Status: "active"},
			{Description: "run go test ./internal/auth", Status: "pending"},
		}},
		{Type: protocol.EventAssistantTextDelta, TurnID: "t1", Text: "Writing the limiter now, then running the auth tests."},
	}, nil},
	{"frame4", []protocol.EngineEvent{
		{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "c5", ToolName: "shell", Target: "go test ./internal/auth"},
		{Type: protocol.EventToolFailed, TurnID: "t1", CallID: "c5", ToolName: "shell",
			Error: "exit status 1", Result: "exit 1 · 2 failing",
			Excerpt: "--- FAIL: TestRefreshKeepsSession (0.00s)\n" +
				"token_test.go:81: session id changed after refresh"},
		{Type: protocol.EventWarning, TurnID: "t1",
			Text: "stream error: connection reset — retrying 1/4 (jittered backoff)"},
		{Type: protocol.EventQuestionPrompted, TurnID: "t1", Questions: []protocol.UserQuestion{{
			Header:      "needs you",
			Question:    "add golang.org/x/time?",
			Options:     []string{"write the limiter without it", "allow once", "view the diff"},
			AllowCustom: false,
			Safe:        0,
		}}},
	}, nil},
	{"frame5", []protocol.EngineEvent{
		{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "c4", ToolName: "edit_file", Result: "+62 −4"},
		{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "c6", ToolName: "shell", Target: "go test ./..."},
		{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "c6", ToolName: "shell", Result: "31 passed"},
		{Type: protocol.EventVerification, TurnID: "t1", Verification: &protocol.VerificationInfo{
			Attempt: 1, Result: "verified", Tier: "T2",
			Reason: "5 requirement(s), 3 check(s), trust T2 (passed)",
			Requirements: []protocol.ReqStatus{
				{ID: "R1", Text: "Expired tokens are rejected", State: "pass", Evidence: "go test ./... · passed"},
				{ID: "R2", Text: "Refresh keeps the session id", State: "pass", Evidence: "token_test.go:81 · passed"},
				{ID: "R3", Text: "Rate limit returns 429", State: "pass", Evidence: "limit_test.go:12 · passed"},
				{ID: "R4", Text: "Flag is documented", State: "pass", Evidence: "README.md · text check"},
				{ID: "R5", Text: "No new dependency", State: "pass", Evidence: "go.mod unchanged · tamper check clean"},
			},
			NotVerified: []string{"load behavior: no check is configured for it"},
		}},
	}, []tea.KeyPressMsg{{Text: "n", Code: 'n'}}},
}

// classicStageEvents returns the fixture events from frame 2 through
// the named stage inclusive.
func classicStageEvents(stage string) []protocol.EngineEvent {
	var out []protocol.EngineEvent
	for _, st := range classicStages {
		if st.name == "frame1" {
			continue
		}
		out = append(out, st.events...)
		if st.name == stage {
			break
		}
	}
	return out
}

// classicFixtureSetup builds a model at the named fixture stage,
// having submitted the run's prompt through the real composer path
// and applied the stage's user keys where the fixture needs them.
func classicFixtureSetup(w, h int, stage string) AppModel {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	m.SetLayout("classic")
	um, _ := m.Update(tea.WindowSizeMsg{Width: w, Height: h})
	m = um.(AppModel)
	m.state.Directory = "/home/dev/projects/api"
	m.state.GitBranch = "main"
	m.state.Version = "0.1.0"
	m.state.ModelName = "claude-sonnet"
	if stage == "frame1" {
		return m
	}
	m.composer.Input.SetValue("harden POST /login: reject expired tokens and rate-limit failed logins")
	ue, _ := m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	m = ue.(AppModel)
	for _, st := range classicStages {
		if st.name == "frame1" {
			continue
		}
		for _, key := range st.preKeys {
			ke, _ := m.Update(key)
			m = ke.(AppModel)
		}
		for _, evt := range st.events {
			e2, _ := m.Update(engineEventMsg(evt))
			m = e2.(AppModel)
		}
		if st.name == stage {
			break
		}
	}
	m.renderLive()
	return m
}
