package tui

import (
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// C2: tool rows render only from engine events. A running call shows
// the running glyph; its own completion sets the result; its own
// failure sets the failed glyph and the excerpt. A row never shows a
// state its events did not report.
func TestToolRowsFromEventsOnly(t *testing.T) {
	m := classicModel(100, 27, false)
	feed := func(evt protocol.EngineEvent) {
		m.eventChanFeed(evt)
	}
	feed(protocol.EngineEvent{Type: protocol.EventToolStarted, CallID: "c1", ToolName: "read_file", Target: "internal/auth/token.go"})
	cell := m.history.OpenToolCell("c1")
	if cell == nil || cell.State != stateRunning || cell.Target != "internal/auth/token.go" {
		t.Fatalf("started call did not open a running row: %+v", cell)
	}
	feed(protocol.EngineEvent{Type: protocol.EventToolCompleted, CallID: "c1", ToolName: "read_file", Result: "112 lines"})
	cell = m.history.OpenToolCell("c1")
	if cell != nil {
		t.Fatal("completed call must not stay open")
	}
	row := m.classicToolRow(m.history.Cells[0])
	if !strings.Contains(row, "112 lines") {
		t.Fatalf("completed row missing its result: %q", row)
	}
	if !strings.Contains(row, "read") || !strings.Contains(row, "internal/auth/token.go") {
		t.Fatalf("row missing verb/target: %q", row)
	}
	// The result is right-aligned at 100 columns (checked on the
	// downsampled line — the styled line ends with a reset).
	line := strings.Split(Downsample(row), "\n")[0]
	if !strings.HasSuffix(line, "112 lines") {
		t.Fatalf("result not right-aligned: %q", line)
	}

	// A failed call gets the excerpt from its own event.
	feed(protocol.EngineEvent{Type: protocol.EventToolStarted, CallID: "c2", ToolName: "shell", Target: "go test ./internal/auth"})
	feed(protocol.EngineEvent{
		Type: protocol.EventToolFailed, CallID: "c2", ToolName: "shell",
		Error: "exit status 1", Result: "exit 1 · 2 failing",
		Excerpt: "--- FAIL: TestRefreshKeepsSession (0.00s)\n" +
			"token_test.go:81: session id changed after refresh",
	})
	var excerpt string
	for _, c := range m.history.Cells {
		if c.Kind == "tool_excerpt" {
			excerpt = m.classicExcerpt(c)
		}
	}
	if !strings.Contains(excerpt, "--- FAIL: TestRefreshKeepsSession") || !strings.Contains(excerpt, "token_test.go:81") {
		t.Fatalf("excerpt not rendered from the event: %q", excerpt)
	}
	failedRow := m.classicToolRow(m.history.Cells[1])
	if !strings.Contains(failedRow, "exit 1") {
		t.Fatalf("failed row missing its result: %q", failedRow)
	}
}

// eventChanFeed applies an event to the model the way Update does
// for engineEventMsg.
func (m *AppModel) eventChanFeed(evt protocol.EngineEvent) {
	applyEvent(m, evt)
}

// C2: loop rows come only from real retry/repetition warnings.
func TestLoopRowsFromWarningsOnly(t *testing.T) {
	m := classicModel(100, 27, false)
	m.eventChanFeed(protocol.EngineEvent{Type: protocol.EventWarning, Text: "stream error: EOF — retrying 1/3 (jittered backoff)"})
	last := m.history.Cells[len(m.history.Cells)-1]
	if last.Kind != "loop" {
		t.Fatalf("retry warning must make a loop row, got kind=%q", last.Kind)
	}
	row := m.classicLoopRow(last)
	if !strings.Contains(row, "retrying 1/3") {
		t.Fatalf("loop row missing its reason: %q", row)
	}
	// A non-loop warning stays a system note.
	m.eventChanFeed(protocol.EngineEvent{Type: protocol.EventWarning, Text: "mcp server is slow"})
	last = m.history.Cells[len(m.history.Cells)-1]
	if last.Kind == "loop" {
		t.Fatal("a plain warning must not become a loop row")
	}
}

// C2: the next line renders only from a real plan event.
func TestNextLineFromPlanOnly(t *testing.T) {
	m := classicModel(100, 27, false)
	if next := m.classicPlanLine(); next != "" {
		t.Fatalf("next line without a plan event: %q", next)
	}
	m.eventChanFeed(protocol.EngineEvent{Type: protocol.EventPlanUpdated,
		Plan: []protocol.PlanStep{
			{Description: "Add the limiter", Status: "active"},
			{Description: "run go test ./internal/auth", Status: "pending"},
		}})
	next := m.classicPlanLine()
	if !strings.Contains(next, "run go test ./internal/auth") {
		t.Fatalf("next line missing the pending plan step: %q", next)
	}
	if strings.Contains(next, "Add the limiter") {
		t.Fatalf("next line must name the pending step, not the active one: %q", next)
	}
	// All-pending-exhausted plans show no next line.
	m.eventChanFeed(protocol.EngineEvent{Type: protocol.EventPlanUpdated,
		Plan: []protocol.PlanStep{{Description: "Add the limiter", Status: "completed"}}})
	if next := m.classicPlanLine(); next != "" {
		t.Fatalf("no pending steps must show no next line: %q", next)
	}
}

// C2: the five-frame fixture replays through Update and every
// promised row renders.
func TestFiveFrameFixtureReplay(t *testing.T) {
	m := classicModel(100, 27, false)
	m.state.ModelName = "claude-sonnet"
	m.composer.Input.SetValue("harden POST /login: reject expired tokens and rate-limit failed logins")
	um, _ := m.Update(tea.KeyPressMsg{Code: tea.KeyEnter})
	m = um.(AppModel)
	for _, evt := range classicStageEvents("frame3") {
		um, _ := m.Update(engineEventMsg(evt))
		m = um.(AppModel)
	}
	m.renderLive()
	view := Downsample(m.View().Content)
	for _, want := range []string{
		"harden POST /login",          // frame 2: raised user row
		"Reading the handler and its", // say text
		"112 lines",                   // frame 3: tool result right-aligned
		"64 lines",                    //
		"3 matches",                   //
		"read",                        // verb
		"search",                      //
		"edit",                        //
		"internal/auth/limit.go",      // target
		"run go test ./internal/auth", // plan next line
	} {
		if !strings.Contains(view, want) {
			t.Fatalf("fixture replay missing %q in:\n%s", want, view)
		}
	}
	if !strings.Contains(view, "run go test ./internal/auth") {
		t.Fatalf("fixture replay missing the plan next line in:\n%s", view)
	}
}

// C2 golden frame 3: tools in flight at 100x27, 80x24, 60x20, plus
// plain mode.
func TestClassicFrame3Golden(t *testing.T) {
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
		for _, size := range sizes {
			mode.setup()
			m := classicFixtureSetup(size.w, size.h, "frame3")
			got := Downsample(m.View().Content)
			golden := filepath.Join("testdata", "classic_frame3_"+size.name+"_"+mode.name+".golden")
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
				t.Fatalf("[classic frame3 %s %s] drifted from golden; regenerate with NIKI_WRITE_GOLDEN=1 after printing the diff", size.name, mode.name)
			}
			t.Logf("[classic frame3 %s %s] golden matches (%d bytes)", size.name, mode.name, len(got))
		}
	}
}
