package tui

import (
	"fmt"
	"testing"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func BenchmarkView80x24(b *testing.B) {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	um, _ := m.Update(tea.WindowSizeMsg{Width: 80, Height: 24})
	m = um.(AppModel)
	for i := 0; i < 100; i++ {
		m.history.Append("user", "hello world, this is a longer message that wraps")
		m.history.AppendDelta("streamed response text that grows the transcript")
	}
	m.viewport.SetContent(m.RenderHistory())
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = m.View()
	}
}

func BenchmarkViewFlatness100vs5000(b *testing.B) {
	for _, n := range []int{100, 5000} {
		cmdChan := make(chan protocol.EngineCommand, 8)
		eventChan := make(chan protocol.EngineEvent, 8)
		m := NewAppModel(cmdChan, eventChan)
		um, _ := m.Update(tea.WindowSizeMsg{Width: 80, Height: 24})
		m = um.(AppModel)
		for i := 0; i < n; i++ {
			m.history.Append("user", "hello world, this is a longer message that wraps")
			m.history.AppendDelta("streamed response text that grows the transcript")
		}
		m.viewport.SetContent(m.RenderHistory())
		b.Run(b.Name()+fmt.Sprintf("_%d", n), func(b *testing.B) {
			for i := 0; i < b.N; i++ {
				_ = m.View()
			}
		})
	}
}

func BenchmarkView120x38(b *testing.B) {
	cmdChan := make(chan protocol.EngineCommand, 8)
	eventChan := make(chan protocol.EngineEvent, 8)
	m := NewAppModel(cmdChan, eventChan)
	um, _ := m.Update(tea.WindowSizeMsg{Width: 120, Height: 38})
	m = um.(AppModel)
	for i := 0; i < 100; i++ {
		m.history.Append("user", "hello world, this is a longer message that wraps")
		m.history.AppendDelta("streamed response text that grows the transcript")
	}
	m.viewport.SetContent(m.RenderHistory())
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = m.View()
	}
}
