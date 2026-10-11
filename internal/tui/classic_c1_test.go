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

// classicModel builds a ready model in classic layout with frozen
// ambient state so frames are hermetic.
func classicModel(w, h int, inline bool) AppModel {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	m.SetLayout("classic")
	um, _ := m.Update(tea.WindowSizeMsg{Width: w, Height: h})
	m = um.(AppModel)
	m.state.Directory = "/home/dev/projects/api"
	m.state.GitBranch = "main"
	m.state.Version = "0.1.0"
	return m
}

// C0: classic is the default layout and renders the orb header;
// ledger keeps the previous chrome; an unknown value fails closed to
// classic.
func TestLayoutDispatch(t *testing.T) {
	m := classicModel(100, 27, false)
	view := m.View().Content
	if !strings.Contains(view, "niki") || !strings.Contains(view, "0.1.0") {
		t.Fatalf("classic header missing: %q", view)
	}
	if !strings.Contains(view, "approvals") {
		t.Fatalf("classic identity row missing: %q", view)
	}

	ledger := classicModel(100, 27, false)
	ledger.SetLayout("ledger")
	ledger.state.ModelName = "claude-sonnet"
	lview := ledger.View().Content
	if strings.Contains(lview, "Type a request") {
		t.Fatalf("ledger must not render the classic tip: %q", lview)
	}

	unknown := classicModel(100, 27, false)
	unknown.SetLayout("quantum")
	if unknown.View().Content != view {
		t.Fatal("unknown layout must fail closed to classic")
	}
}

// C0: View is a pure function of state — two consecutive calls with
// no Update between them are byte-identical and mutate nothing.
func TestViewPure(t *testing.T) {
	m := classicModel(100, 27, false)
	m.history.Append("user", "harden POST /login")
	first := m.View().Content
	second := m.View().Content
	if first != second {
		t.Fatal("View is not deterministic across calls")
	}
	if m.telemetry.Events != 0 || m.telemetry.Deltas != 0 {
		t.Fatal("View mutated telemetry")
	}
	// Purity also holds in inline mode.
	inl := classicModel(100, 27, true)
	inlA := inl.View().Content
	inlB := inl.View().Content
	if inlA != inlB {
		t.Fatal("inline View is not deterministic")
	}
}

// C1: the SETUP block shows only when no model is configured, and
// lists only real choices.
func TestSetupBlockOnlyWhenUnconfigured(t *testing.T) {
	m := classicModel(100, 27, false)
	if !strings.Contains(m.View().Content, "SETUP") {
		t.Fatal("SETUP block must show when unconfigured")
	}
	m.state.ModelName = "claude-sonnet"
	if strings.Contains(m.View().Content, "SETUP") {
		t.Fatal("SETUP block must hide once a model is configured")
	}
}

// C1: every SETUP choice dispatches a real action. [k] opens the
// connect flow; [o] returns the Ollama config command (the command
// itself is not executed here — it writes user config).
func TestSetupChoicesDispatch(t *testing.T) {
	m := classicModel(100, 27, false)
	if !m.needsSetup() {
		t.Fatal("precondition: no model configured")
	}
	um, _ := m.Update(tea.KeyPressMsg{Text: "k", Code: 'k'})
	after := um.(AppModel)
	if !after.state.Palette.Open || after.state.Palette.Mode != "palette" || after.state.Palette.Query != "connect" {
		t.Fatalf("k must open the connect flow, got open=%v mode=%q query=%q",
			after.state.Palette.Open, after.state.Palette.Mode, after.state.Palette.Query)
	}
	m2 := classicModel(100, 27, false)
	um2, cmd := m2.Update(tea.KeyPressMsg{Text: "o", Code: 'o'})
	_ = um2.(AppModel)
	if cmd == nil {
		t.Fatal("o must dispatch the Ollama configuration command")
	}
	// d replays the recorded five-frame fixture (no model call).
	m4 := classicModel(100, 27, false)
	um4, cmd4 := m4.Update(tea.KeyPressMsg{Text: "d", Code: 'd'})
	if cmd4 == nil {
		t.Fatal("d must dispatch the recorded-demo replay")
	}
	m4 = um4.(AppModel)
	if !m4.state.Demo || m4.state.DemoStep != 0 {
		t.Fatal("d must start the recorded demo at step 0")
	}
	// The replay advances on its own ticks and finishes.
	for i := 0; i < len(demoEvents())+2 && m4.state.Demo; i++ {
		um5, _ := m4.Update(demoTickMsg{})
		m4 = um5.(AppModel)
	}
	if m4.state.Demo {
		t.Fatal("the recorded demo must finish")
	}

	// The choices type text when the composer is not empty (they
	// are not global hotkeys).
	m3 := classicModel(100, 27, false)
	m3.composer.Input.SetValue("keep")
	um3, _ := m3.Update(tea.KeyPressMsg{Text: "k", Code: 'k'})
	if um3.(AppModel).state.Palette.Open {
		t.Fatal("k must not open the palette while typing")
	}
	if got := um3.(AppModel).composer.Input.Value(); got != "keepk" {
		t.Fatalf("k must type while composing, got %q", got)
	}
}

// C1: footer hints come from the key registry and every hint
// dispatches in its state (the H2 pattern extended to the classic
// footer).
func TestFooterHintsDispatch(t *testing.T) {
	states := []struct {
		name  string
		arm   func(*AppModel)
		state string
		proof func(t *testing.T, before, after AppModel, cmd tea.Cmd, cmdLen int)
	}{
		{"idle", func(m *AppModel) {}, hintIdle, func(t *testing.T, before, after AppModel, cmd tea.Cmd, cmdLen int) {
			t.Helper()
			dispatched := cmd != nil ||
				len(after.history.Cells) != len(before.history.Cells) ||
				after.state.Palette.Open != before.state.Palette.Open ||
				after.state.ExpandToolOutput != before.state.ExpandToolOutput ||
				after.state.PermissionMode != before.state.PermissionMode ||
				cmdLen > 0
			if !dispatched {
				t.Fatal("idle hint key did not dispatch")
			}
		}},
		{"running", func(m *AppModel) { m.state.Busy = true; m.state.Activity = "writing the limiter" }, hintRunning, func(t *testing.T, before, after AppModel, cmd tea.Cmd, cmdLen int) {
			t.Helper()
			dispatched := cmd != nil ||
				len(after.history.Cells) != len(before.history.Cells) ||
				after.state.ExpandToolOutput != before.state.ExpandToolOutput ||
				cmdLen > 0
			if !dispatched {
				t.Fatal("running hint key did not dispatch")
			}
		}},
	}
	for _, st := range states {
		m := classicModel(100, 27, false)
		st.arm(&m)
		footer := m.classicFooter()
		for _, kb := range FooterHints(st.state) {
			if !strings.Contains(footer, kb.Hint) {
				t.Fatalf("[%s] hint %q missing from footer %q", st.name, kb.Hint, footer)
			}
			// Press the key and require observable dispatch.
			armed := m
			switch kb.ID {
			case "ctrl+k", "ctrl+p":
				armed.state.Palette.Open = false
			case "?":
				armed.composer.Input.SetValue("")
			}
			before := armed
			um, cmd := armed.Update(kb.Key)
			after := um.(AppModel)
			st.proof(t, before, after, cmd, len(armed.cmdChan))
		}
		t.Logf("[C1 %s] footer hints all dispatch: %q", st.name, footer)
	}
	// ctx% hides until the provider reports usage (design §1.4).
	m := classicModel(100, 27, false)
	if strings.Contains(m.classicFooter(), "ctx") {
		t.Fatal("ctx must not render without usage")
	}
	m.state.UsedTokens = 12800
	m.state.MaxTokens = 128000
	if !strings.Contains(m.classicFooter(), "ctx 10%") {
		t.Fatalf("ctx%% missing with usage: %q", m.classicFooter())
	}
}

// C1: the raised user row never truncates and carries the prompt
// glyph; say text is indented and capped at two lines.
func TestClassicRows(t *testing.T) {
	m := classicModel(100, 27, false)
	long := strings.Repeat("x", 200)
	row := Downsample(m.classicUserRow(long))
	if !strings.HasPrefix(row, promptGlyph()) {
		t.Fatalf("user row must start with the prompt glyph: %q", row[:10])
	}
	if !strings.Contains(strings.ReplaceAll(row, "\n", ""), long) {
		t.Fatal("user row must wrap, never truncate")
	}
	say := m.classicSay("line one\nline two\nline three\nline four")
	if strings.Count(say, "\n") != 1 {
		t.Fatalf("say text must cap at two lines: %q", say)
	}
	if !strings.HasPrefix(say, "  ") {
		t.Fatalf("say text must indent two columns: %q", say)
	}
}

// The classic frames 1-2 (C1): first launch and submitted. Frames
// 3-5 use the shared five-stage fixture (classic_fixture.go) once
// their features land (C2/C3).
func classicFrames() []classicFrame {
	return []classicFrame{
		{"frame1", func(m *AppModel) {}},
		{"frame2", func(m *AppModel) {
			m.state.ModelName = "claude-sonnet"
			m.history.Append("user", "harden POST /login: reject expired tokens and rate-limit failed logins")
			m.state.Busy = true
			m.state.Activity = "reading the project"
			m.state.UsedTokens = 5120
			m.state.MaxTokens = 128000
		}},
	}
}

type classicFrame struct {
	name  string
	setup func(*AppModel)
}

// C1 golden frames: 100x27, 80x24, 60x20, plus plain mode. Goldens
// are regenerated only after printing the diff.
func TestClassicGoldenFrames(t *testing.T) {
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
		for _, frame := range classicFrames() {
			for _, size := range sizes {
				mode.setup()
				m := classicModel(size.w, size.h, false)
				frame.setup(&m)
				m.renderLive()
				got := Downsample(m.View().Content)
				golden := filepath.Join("testdata", "classic_"+frame.name+"_"+size.name+"_"+mode.name+".golden")
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
					t.Fatalf("[classic %s %s %s] drifted from golden; regenerate with NIKI_WRITE_GOLDEN=1 after printing the diff", frame.name, size.name, mode.name)
				}
				t.Logf("[classic %s %s %s] golden matches (%d bytes)", frame.name, size.name, mode.name, len(got))
			}
		}
	}
}

// Plain mode (NIKI_PLAIN=1) renders every glyph with a word and no
// escape sequences.
func TestClassicPlainMode(t *testing.T) {
	t.Setenv("NIKI_PLAIN", "1")
	ApplyDisplayEnv(io.Discard)
	m := classicModel(100, 27, false)
	m.state.ModelName = "claude-sonnet"
	m.history.Append("user", "harden the login handler")
	out := Downsample(m.View().Content)
	if strings.Contains(out, "\x1b[") {
		t.Fatalf("plain mode must strip escapes: %q", out)
	}
	for _, r := range out {
		if r > 127 {
			t.Fatalf("plain mode must be ASCII-only, found %q", r)
		}
	}
	if !strings.Contains(out, "ready") {
		t.Fatalf("plain activity line missing its word: %q", out)
	}
}
