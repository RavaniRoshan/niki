package tui

import (
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"github.com/charmbracelet/colorprofile"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// U1: the Workbench shell (docs/tui/WORKBENCH.md). Tests follow the
// rule of proof: every element must be proven absent before its
// event fires and present after, at three widths, in plain and
// ASCII modes, with no mock data anywhere in the package.

// workbenchModel builds a ready workbench model with frozen ambient
// state so frames are hermetic.
func workbenchModel(t *testing.T, w, h int) AppModel {
	t.Helper()
	t.Setenv("TERM", "xterm-256color")
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	m.SetLayout("workbench")
	um, _ := m.Update(tea.WindowSizeMsg{Width: w, Height: h})
	m = um.(AppModel)
	m.state.Directory = "/repo"
	m.state.GitBranch = "main"
	m.state.Version = "0.11.0"
	return m
}

// feedEvents delivers engine events through the real Update path.
func feedEvents(m AppModel, evts ...protocol.EngineEvent) AppModel {
	for _, e := range evts {
		um, _ := m.Update(engineEventMsg(e))
		m = um.(AppModel)
	}
	m.renderLive()
	return m
}

// workbenchFrame renders the model to plain text (the test profile
// downsamples at the output boundary).
func workbenchFrame(m AppModel) string {
	ApplyDisplayEnv(io.Discard)
	return Downsample(m.View().Content)
}

// workbenchFixture replays the classic five-stage fixture in the
// workbench layout (the same real events, through the same Update
// path a live run uses).
func workbenchFixture(t *testing.T, w, h int, stage string) AppModel {
	t.Helper()
	t.Setenv("TERM", "xterm-256color")
	m := classicFixtureSetup(w, h, stage)
	m.SetLayout("workbench")
	m.renderLive()
	return m
}

// U1: all six regions render — header, left rail, centre, right
// rail, composer, status bar — from real state.
func TestWorkbenchShellRendersAllRegions(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	m = feedEvents(m,
		protocol.EngineEvent{Type: protocol.EventPlanUpdated, Plan: []protocol.PlanStep{
			{Description: "Read the handler", Status: "completed"},
			{Description: "Add the limiter", Status: "active"},
		}},
		protocol.EngineEvent{Type: protocol.EventSubagentStarted, Text: "agent-7"},
	)
	view := workbenchFrame(m)
	for _, want := range []string{
		"NikiCode", "v0.11.0", "claude-sonnet", "mode act", "/repo", "main", // header
		"PLAN", "Read the handler", "Add the limiter", // left rail: plan
		"AGENTS", "agent-7", // left rail: agents
		"CONTEXT", "COST", // right rail
		">",        // composer
		"ctrl+k",   // composer palette hint
		"idle",     // status bar state word
		"agents 1", // status bar count
	} {
		if !strings.Contains(view, want) {
			t.Fatalf("[U1] region element %q missing from workbench view:\n%s", want, view)
		}
	}
}

// U1: the header's model field is the em dash before any model is
// configured (P22 — an unknown is never a fake value) and the real
// model after.
func TestWorkbenchModelHeaderAbsentThenPresent(t *testing.T) {
	fresh := workbenchModel(t, 100, 27)
	before := workbenchFrame(fresh)
	if strings.Contains(before, "claude-sonnet") {
		t.Fatalf("[U1] model present before configuration:\n%s", before)
	}
	if !strings.Contains(before, "—") {
		t.Fatalf("[U1] unconfigured model must render the em dash:\n%s", before)
	}
	fresh.state.ModelName = "claude-sonnet"
	after := workbenchFrame(fresh)
	if !strings.Contains(after, "claude-sonnet · mode act") {
		t.Fatalf("[U1] configured model missing from the header:\n%s", after)
	}
}

// U1 rule of proof: each element is ABSENT before its event fires
// and PRESENT after. Absence is a state (P21); unknown instruments
// read "—" (P22).
func TestWorkbenchElementsAbsentBeforePresentAfter(t *testing.T) {
	planEvt := protocol.EngineEvent{Type: protocol.EventPlanUpdated, Plan: []protocol.PlanStep{
		{Description: "Add the limiter", Status: "active"},
	}}
	agentStart := protocol.EngineEvent{Type: protocol.EventSubagentStarted, Text: "agent-7"}
	usageEvt := protocol.EngineEvent{Type: protocol.EventAssistantMessageDone,
		Usage: &protocol.Usage{PromptTokens: 5120, TotalTokens: 5120}}
	turnStart := protocol.EngineEvent{Type: protocol.EventTurnStarted, Contract: []protocol.ContractReq{
		{ID: "R1", State: "queued"}, {ID: "R2", State: "queued"},
	}}
	questionEvt := protocol.EngineEvent{Type: protocol.EventQuestionPrompted, Questions: []protocol.UserQuestion{{
		Header: "needs you", Question: "add golang.org/x/time?",
		Options: []string{"write the limiter without it", "allow once", "view the diff"}, Safe: 0,
	}}}
	errorEvt := protocol.EngineEvent{Type: protocol.EventTurnFailed, Error: "exit status 1"}
	verifyEvt := protocol.EngineEvent{Type: protocol.EventVerification, Verification: &protocol.VerificationInfo{
		Attempt: 1, Result: "verified", Tier: "T2",
		Requirements: []protocol.ReqStatus{{ID: "R1", Text: "Expired tokens are rejected", State: "pass", Evidence: "token_test.go:44"}},
	}}

	cases := []struct {
		name    string
		absent  string
		present string
		feed    []protocol.EngineEvent
		check   func(t *testing.T, before, after string)
	}{
		{"plan rail", "PLAN", "Add the limiter", []protocol.EngineEvent{planEvt}, nil},
		{"agents rail", "AGENTS", "agent-7", []protocol.EngineEvent{agentStart}, nil},
		{"activity line", "thinking", "thinking", []protocol.EngineEvent{turnStart}, nil},
		{"interrupt hint", "esc interrupt", "esc interrupt", []protocol.EngineEvent{turnStart}, nil},
		{"agent count", "agents 1", "agents 1", []protocol.EngineEvent{agentStart}, nil},
		{"contract strip", "contract", "contract", []protocol.EngineEvent{turnStart}, nil},
		{"gate", "needs you", "(default)", []protocol.EngineEvent{questionEvt}, func(t *testing.T, _, after string) {
			t.Helper()
			if !strings.Contains(after, "write the limiter without it (default)") {
				t.Fatalf("[U1] the constraint-keeping option must be the focused default:\n%s", after)
			}
		}},
		{"error row", "exit status 1", "exit status 1", []protocol.EngineEvent{errorEvt}, nil},
		{"verdict card", "verified", "verified", []protocol.EngineEvent{turnStart, verifyEvt}, nil},
		// The instruments' absent state IS the em dash (P22), so the
		// absent check is custom: the numbers must not be there, and
		// the dash must be.
		{"context meter", "", "5.1k / 128k", []protocol.EngineEvent{usageEvt}, func(t *testing.T, before, after string) {
			t.Helper()
			if !strings.Contains(before, "—") {
				t.Fatalf("[U1] an unknown context must render the em dash:\n%s", before)
			}
			if strings.Contains(before, "5.1k") {
				t.Fatalf("[U1] context value present before its usage event:\n%s", before)
			}
		}},
		{"cost meter", "", "$", []protocol.EngineEvent{usageEvt}, func(t *testing.T, before, after string) {
			t.Helper()
			if !strings.Contains(before, "turn —") {
				t.Fatalf("[U1] an unknown turn cost must render the em dash:\n%s", before)
			}
			if strings.Contains(before, "$") {
				t.Fatalf("[U1] cost value present before its usage event:\n%s", before)
			}
		}},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			m := workbenchModel(t, 100, 27)
			m.state.ModelName = "claude-sonnet"
			before := workbenchFrame(m)
			if c.absent != "" && strings.Contains(before, c.absent) {
				t.Fatalf("[%s] %q present before its event fired:\n%s", c.name, c.absent, before)
			}
			m = feedEvents(m, c.feed...)
			after := workbenchFrame(m)
			if !strings.Contains(after, c.present) {
				t.Fatalf("[%s] %q absent after its event fired:\n%s", c.name, c.present, after)
			}
			if c.check != nil {
				c.check(t, before, after)
			}
		})
	}

	// The activity line's interrupt hint is part of the element: an
	// idle workbench shows neither the phrase nor the hint (P21).
	idle := workbenchModel(t, 100, 27)
	idle.state.ModelName = "claude-sonnet"
	if v := workbenchFrame(idle); strings.Contains(v, "esc pause") {
		t.Fatalf("[U1] the activity line rendered while idle:\n%s", v)
	}
}

// U1: the CHECKPOINTS rail section is absent — no checkpoint event
// exists, so the section is never drawn, even after a full run
// (WORKBENCH.md §7).
func TestWorkbenchCheckpointsAbsentUntilEventExists(t *testing.T) {
	m := workbenchFixture(t, 100, 27, "frame5")
	view := workbenchFrame(m)
	if strings.Contains(view, "CHECKPOINTS") {
		t.Fatalf("[U1] CHECKPOINTS rendered without a checkpoint event:\n%s", view)
	}
	if !strings.Contains(view, "verified") {
		t.Fatal("precondition: the verdict card renders in the workbench")
	}
}

// U1: the failed-agent status is legible without color — the plain
// mode renders the state as a word.
func TestWorkbenchAgentStatusWordInPlainMode(t *testing.T) {
	saved := frameProfile
	defer func() { frameProfile = saved }()
	t.Setenv("NIKI_PLAIN", "1")
	ApplyDisplayEnv(io.Discard)
	m := workbenchModel(t, 100, 27)
	m = feedEvents(m,
		protocol.EngineEvent{Type: protocol.EventSubagentStarted, Text: "agent-7"},
		protocol.EngineEvent{Type: protocol.EventSubagentFailed, Text: "agent-7", Error: "boom"},
	)
	view := workbenchFrame(m)
	if !strings.Contains(view, "failed") {
		t.Fatalf("[U1] failed agent must render its word in plain mode:\n%s", view)
	}
}

// U1: the shell is resize-aware at three widths with no overflow
// or clipping — every line fits the width and the frame fits the
// height, with the status bar intact as the last line.
func TestWorkbenchWidthsNoOverflow(t *testing.T) {
	sizes := []struct {
		name string
		w, h int
	}{
		{"100x27", 100, 27},
		{"80x24", 80, 24},
		{"60x20", 60, 20},
	}
	for _, s := range sizes {
		m := workbenchFixture(t, s.w, s.h, "frame5")
		view := workbenchFrame(m)
		lines := strings.Split(view, "\n")
		if len(lines) > s.h {
			t.Fatalf("[%s] frame has %d lines, height is %d", s.name, len(lines), s.h)
		}
		for i, l := range lines {
			if CellWidth(l) > s.w {
				t.Fatalf("[%s] line %d overflows: %d cells > %d: %q", s.name, i+1, CellWidth(l), s.w, l)
			}
		}
		last := lines[len(lines)-1]
		if !strings.Contains(last, "claude-sonnet") {
			t.Fatalf("[%s] the status bar must be the last line, got %q", s.name, last)
		}
		// The run trace's real rows survive inside the centre
		// column: the viewport shows the tail of the trace (the
		// fixture's last tool call and the verdict card).
		if !strings.Contains(view, "go test ./...") {
			t.Fatalf("[%s] centre column missing the tool trace:\n%s", s.name, view)
		}
		if !strings.Contains(view, "verified") {
			t.Fatalf("[%s] centre column missing the verdict card:\n%s", s.name, view)
		}
		t.Logf("[%s] no overflow: %d lines", s.name, len(lines))
	}
}

// U1: a resize preserves state — plan, agents and instruments stay
// rendered at the new size.
func TestWorkbenchResizePreservesState(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	m = feedEvents(m,
		protocol.EngineEvent{Type: protocol.EventPlanUpdated, Plan: []protocol.PlanStep{
			{Description: "Add the limiter", Status: "active"},
		}},
		protocol.EngineEvent{Type: protocol.EventSubagentStarted, Text: "agent-7"},
		protocol.EngineEvent{Type: protocol.EventAssistantMessageDone,
			Usage: &protocol.Usage{PromptTokens: 5120, TotalTokens: 5120}},
	)
	for _, size := range []struct{ w, h int }{{80, 24}, {60, 20}} {
		um, _ := m.Update(tea.WindowSizeMsg{Width: size.w, Height: size.h})
		m = um.(AppModel)
		view := workbenchFrame(m)
		// The rails truncate descriptions at 60 (18-wide rail); the
		// surviving prefix plus the section labels prove the state
		// carried across. The right rail's instruments move to the
		// status bar below 80 columns.
		wants := []string{"PLAN", "Add the", "AGENTS", "agent-7"}
		if size.w >= 80 {
			wants = append(wants, "5.1k / 128k", "Add the limiter")
		} else {
			wants = append(wants, "ctx 4%")
		}
		for _, want := range wants {
			if !strings.Contains(view, want) {
				t.Fatalf("[%dx%d] state %q lost across resize:\n%s", size.w, size.h, want, view)
			}
		}
		lines := strings.Split(view, "\n")
		if len(lines) > size.h {
			t.Fatalf("[%dx%d] frame overflows after resize: %d lines", size.w, size.h, len(lines))
		}
		for i, l := range lines {
			if CellWidth(l) > size.w {
				t.Fatalf("[%dx%d] line %d overflows after resize: %q", size.w, size.h, i+1, l)
			}
		}
	}
}

// U1 (P23): the Stream view renders the same events inline — the
// same run must read the same in both views.
func TestWorkbenchAndStreamSameEvents(t *testing.T) {
	events := []protocol.EngineEvent{
		{Type: protocol.EventToolStarted, TurnID: "t1", CallID: "c1", ToolName: "read_file", Target: "internal/auth/token.go"},
		{Type: protocol.EventToolCompleted, TurnID: "t1", CallID: "c1", ToolName: "read_file", Result: "112 lines"},
		{Type: protocol.EventAssistantTextDelta, TurnID: "t1", Text: "Reading the handler first.",
			Usage: &protocol.Usage{PromptTokens: 5120, TotalTokens: 5120}},
	}
	wb := workbenchModel(t, 100, 27)
	wb.state.ModelName = "claude-sonnet"
	wb = feedEvents(wb, events...)

	st := classicModel(100, 27, false)
	st.state.ModelName = "claude-sonnet"
	st = feedEvents(st, events...)

	wbView := workbenchFrame(wb)
	stView := Downsample(st.View().Content)
	for _, want := range []string{"internal/auth/token.go", "112 lines", "Reading the handler first", "claude-sonnet"} {
		if !strings.Contains(wbView, want) {
			t.Fatalf("[U1] workbench missing %q:\n%s", want, wbView)
		}
		if !strings.Contains(stView, want) {
			t.Fatalf("[U1] stream missing %q:\n%s", want, stView)
		}
	}
}

// U1: no mock data anywhere in the UI package — placeholder
// literals in non-test files fail the build's test.
func TestWorkbenchNoMockDataInUIPackage(t *testing.T) {
	banned := []string{
		"lorem", "coming soon", "sample data", "mock data", "dummy data",
		"placeholder text",
	}
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	if len(files) == 0 {
		t.Fatal("no ui package files found")
	}
	for _, f := range files {
		if strings.HasSuffix(f, "_test.go") {
			continue
		}
		data, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		s := strings.ToLower(string(data))
		for _, b := range banned {
			if strings.Contains(s, strings.ToLower(b)) {
				t.Fatalf("[U1] placeholder literal %q in %s", b, f)
			}
		}
	}
	t.Logf("[U1] ui package files clean of placeholder literals")
}

// U1: View is a pure function of state.
func TestWorkbenchViewPure(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m = feedEvents(m, protocol.EngineEvent{Type: protocol.EventPlanUpdated, Plan: []protocol.PlanStep{
		{Description: "Add the limiter", Status: "active"},
	}})
	a := workbenchFrame(m)
	b := workbenchFrame(m)
	if a != b {
		t.Fatal("workbench View is not deterministic across calls")
	}
}

// U1: ASCII and plain modes stay pure ASCII with words instead of
// glyphs.
func TestWorkbenchPlainAndASCIIModes(t *testing.T) {
	saved := frameProfile
	defer func() { frameProfile = saved }()

	t.Run("ascii", func(t *testing.T) {
		t.Setenv("NIKI_ASCII", "1")
		ApplyDisplayEnv(io.Discard)
		m := workbenchModel(t, 80, 24)
		m.state.ModelName = "claude-sonnet"
		out := workbenchFrame(m)
		for _, r := range out {
			if r > 127 {
				t.Fatalf("[U1 ascii] non-ascii rune %q in frame", r)
			}
		}
		if !strings.Contains(out, "idle") {
			t.Fatalf("[U1 ascii] status word missing:\n%s", out)
		}
	})
	t.Run("plain", func(t *testing.T) {
		t.Setenv("NIKI_PLAIN", "1")
		ApplyDisplayEnv(io.Discard)
		m := workbenchModel(t, 100, 27)
		m = feedEvents(m,
			protocol.EngineEvent{Type: protocol.EventPlanUpdated, Plan: []protocol.PlanStep{
				{Description: "Add the limiter", Status: "active"},
			}},
			protocol.EngineEvent{Type: protocol.EventSubagentStarted, Text: "agent-7"},
		)
		out := workbenchFrame(m)
		if strings.Contains(out, "\x1b[") {
			t.Fatalf("[U1 plain] escape sequences present:\n%q", out)
		}
		for _, r := range out {
			if r > 127 {
				t.Fatalf("[U1 plain] non-ascii rune %q", r)
			}
		}
		for _, want := range []string{"PLAN", "AGENTS", "idle", "running"} {
			if !strings.Contains(out, want) {
				t.Fatalf("[U1 plain] word %q missing:\n%s", want, out)
			}
		}
	})
}

// U1 golden frames: the workbench shell at 100x27, 80x24, 60x20,
// normal and plain. Regenerated only after printing the diff.
func TestWorkbenchGoldenFrames(t *testing.T) {
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
			os.Unsetenv("NIKI_PLAIN")
			os.Unsetenv("NIKI_ASCII")
			os.Unsetenv("NO_COLOR")
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
		for _, stage := range []string{"frame1", "frame2", "frame3", "frame4", "frame5"} {
			for _, size := range sizes {
				mode.setup()
				m := workbenchFixture(t, size.w, size.h, stage)
				got := Downsample(m.View().Content)
				golden := filepath.Join("testdata", "workbench_"+stage+"_"+size.name+"_"+mode.name+".golden")
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
					t.Fatalf("[workbench %s %s %s] drifted from golden; regenerate with NIKI_WRITE_GOLDEN=1 after printing the diff", stage, size.name, mode.name)
				}
				t.Logf("[workbench %s %s %s] golden matches (%d bytes)", stage, size.name, mode.name, len(got))
			}
		}
	}
}

// U1: ANSI frame dumps for owner review (docs/tui/review/), the
// workbench counterpart of the classic C5 dumps.
func TestWriteWorkbenchFrameDumps(t *testing.T) {
	if os.Getenv("NIKI_WRITE_DUMPS") != "1" {
		t.Skip("set NIKI_WRITE_DUMPS=1 to write frame dumps")
	}
	sizes := []struct {
		name string
		w, h int
	}{
		{"100x27", 100, 27},
		{"80x24", 80, 24},
		{"60x20", 60, 20},
	}
	outDir := filepath.Join("..", "..", "docs", "tui", "review")
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		t.Fatal(err)
	}
	saved := frameProfile
	defer func() { frameProfile = saved }()

	write := func(profile colorprofile.Profile, plain bool) {
		t.Setenv("TERM", "xterm-256color")
		if plain {
			t.Setenv("NIKI_PLAIN", "1")
		} else {
			os.Unsetenv("NIKI_PLAIN")
			os.Unsetenv("NIKI_ASCII")
			os.Unsetenv("NO_COLOR")
		}
		frameProfile = profile
		// frame6 is the recorded full run (U2): plan, step headers,
		// the real diff, the reasoning line, the failing test.
		for _, stage := range []string{"frame1", "frame2", "frame3", "frame4", "frame5", "frame6"} {
			for _, size := range sizes {
				var m AppModel
				if stage == "frame6" {
					m = workbenchFullRun(t, size.w, size.h)
				} else {
					m = workbenchFixture(t, size.w, size.h, stage)
				}
				out := Downsample(m.View().Content)
				suffix := ".ansi"
				if plain {
					suffix = ".plain.txt"
				}
				path := filepath.Join(outDir, "workbench_"+stage+"_"+size.name+suffix)
				if err := os.WriteFile(path, []byte(out), 0o644); err != nil {
					t.Fatal(err)
				}
				t.Logf("[U1] wrote %s (%d bytes)", path, len(out))
			}
		}
	}
	write(colorprofile.TrueColor, false)
	write(colorprofile.NoTTY, true)
}
