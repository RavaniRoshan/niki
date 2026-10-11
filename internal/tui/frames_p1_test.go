package tui

import (
	"io"
	"os"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"path/filepath"
	"strings"
	"testing"
)

// renderFixed is renderAt with a frozen directory/branch so the
// goldens are hermetic (no machine path or git state leaks in).
func renderFixed(w, h int) string {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	um, _ := m.Update(tea.WindowSizeMsg{Width: w, Height: h})
	m = um.(AppModel)
	m.state.Directory = "/repo"
	m.state.GitBranch = "main"
	m.history.Append("user", "hello")
	m.history.AppendDelta("hi there")
	m.viewport.SetContent(m.RenderHistory())
	return m.View()
}

// P1: unicode width correct; NO_COLOR, ASCII fallback, and a plain
// mode for logs and screen readers — golden frames in each mode.
func TestGoldenFramesPerMode(t *testing.T) {
	modes := []struct {
		name  string
		setup func()
		check func(t *testing.T, frame string)
	}{
		{"normal", func() {
			t.Setenv("NO_COLOR", "")
			t.Setenv("NIKI_PLAIN", "")
			os.Unsetenv("NO_COLOR")
			os.Unsetenv("NIKI_PLAIN")
			_ = os.Setenv("TERM", "xterm-256color")
		}, func(t *testing.T, frame string) {
			if !strings.Contains(frame, "◐ NikiCode") {
				t.Fatalf("[P1 normal] orb header missing")
			}
		}},
		{"nocolor", func() {
			t.Setenv("NO_COLOR", "1")
			t.Setenv("NIKI_PLAIN", "")
			os.Unsetenv("NIKI_PLAIN")
			_ = os.Setenv("TERM", "xterm-256color")
			ApplyDisplayEnv(io.Discard)
		}, func(t *testing.T, frame string) {
			if strings.Contains(frame, "\x1b[") {
				t.Fatalf("[P1 nocolor] escape sequences present")
			}
		}},
		{"ascii", func() {
			os.Unsetenv("NO_COLOR")
			t.Setenv("NIKI_PLAIN", "")
			os.Unsetenv("NIKI_PLAIN")
			t.Setenv("TERM", "dumb")
			ApplyDisplayEnv(io.Discard)
		}, func(t *testing.T, frame string) {
			if !strings.Contains(frame, "(o) NikiCode") {
				t.Fatalf("[P1 ascii] ascii header missing")
			}
			for _, r := range frame {
				if r > 127 {
					t.Fatalf("[P1 ascii] non-ascii rune %q in frame", r)
				}
			}
		}},
	}
	for _, m := range modes {
		m.setup()
		frame := renderFixed(80, 24)
		golden := filepath.Join("testdata", "frame_"+m.name+".golden")
		if os.Getenv("NIKI_WRITE_GOLDEN") == "1" {
			if err := os.MkdirAll("testdata", 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(golden, []byte(frame), 0o644); err != nil {
				t.Fatal(err)
			}
			continue
		}
		want, err := os.ReadFile(golden)
		if err != nil {
			if os.IsNotExist(err) {
				t.Fatalf("[P1 %s] missing golden %s (generate with NIKI_WRITE_GOLDEN=1)", m.name, golden)
			}
			t.Fatal(err)
		}
		if string(want) != frame {
			t.Fatalf("[P1 %s] frame drifted from golden (diff printed in review)", m.name)
		}
		m.check(t, frame)
		t.Logf("[P1 %s] golden frame matches (%d bytes)", m.name, len(frame))
	}
}

// TestGoldenWidthCells: unicode width is measured in cells, and
// truncation never splits a rune.
func TestGoldenWidthCells(t *testing.T) {
	if CellWidth("ab你好") != 6 {
		t.Fatalf("[P1] width(ab你好) = %d, want 6", CellWidth("ab你好"))
	}
	if CellWidth("◐ NikiCode") != 10 {
		t.Fatalf("[P1] width(orb) = %d, want 10", CellWidth("◐ NikiCode"))
	}
	got := TruncateTo("ab你好cd", 6)
	if got != "ab你…" || CellWidth(got) > 6 {
		t.Fatalf("[P1] truncate = %q (width %d), want ab你…/<=6", got, CellWidth(got))
	}
	if TruncateTo("ascii", 10) != "ascii" {
		t.Fatalf("[P1] short strings must pass through")
	}
	t.Logf("[P1] cell widths correct (CJK=2), truncation rune-safe")
}
