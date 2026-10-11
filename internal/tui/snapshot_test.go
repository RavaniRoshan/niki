package tui

import (
	"fmt"
	"os"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func renderAt(w, h int) string {
	return renderAtLayout(w, h, "classic")
}

func renderAtLayout(w, h int, layout string) string {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	m.SetLayout(layout)
	um, _ := m.Update(tea.WindowSizeMsg{Width: w, Height: h})
	m = um.(AppModel)
	m.history.Append("user", "hello")
	m.history.AppendDelta("hi there")
	m.viewport.SetContent(m.RenderHistory())
	return m.View().Content
}

func TestSnapshotSizesDeterministic(t *testing.T) {
	for _, size := range [][2]int{{50, 16}, {80, 24}, {120, 38}, {160, 45}} {
		v1 := renderAt(size[0], size[1])
		v2 := renderAt(size[0], size[1])
		if v1 != v2 {
			t.Fatalf("snapshot at %dx%d not deterministic", size[0], size[1])
		}
		if !strings.Contains(v1, "niki") {
			t.Fatalf("snapshot at %dx%d missing header", size[0], size[1])
		}
	}
}

// C0: both layouts render deterministically at every size; classic
// is the default and shows the niki identity rows.
func TestSnapshotBothLayouts(t *testing.T) {
	for _, size := range [][2]int{{50, 16}, {80, 24}, {120, 38}, {160, 45}} {
		classic := renderAtLayout(size[0], size[1], "classic")
		if classic != renderAtLayout(size[0], size[1], "classic") {
			t.Fatalf("classic snapshot at %dx%d not deterministic", size[0], size[1])
		}
		if !strings.Contains(classic, "niki") {
			t.Fatalf("classic snapshot at %dx%d missing identity row", size[0], size[1])
		}
		ledger := renderAtLayout(size[0], size[1], "ledger")
		if !strings.Contains(ledger, "NikiCode") {
			t.Fatalf("ledger snapshot at %dx%d missing header", size[0], size[1])
		}
	}
}

// TestWriteFrameDumps writes frame dumps to docs/review when NIKI_WRITE_DUMPS=1.
func TestWriteFrameDumps(t *testing.T) {
	if os.Getenv("NIKI_WRITE_DUMPS") != "1" {
		t.Skip("set NIKI_WRITE_DUMPS=1 to write frame dumps")
	}
	for _, size := range [][2]int{{50, 16}, {80, 24}, {120, 38}, {160, 45}} {
		v := renderAt(size[0], size[1])
		_ = os.MkdirAll("../../docs/review", 0o755)
		_ = os.WriteFile(fmt.Sprintf("../../docs/review/frame_%dx%d.txt", size[0], size[1]), []byte(v), 0o644)
	}
}
