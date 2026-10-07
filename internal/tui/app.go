package tui

import (
	"os"
	"strings"

	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

type AppModel struct {
	cmdChan   chan<- protocol.EngineCommand
	eventChan <-chan protocol.EngineEvent

	viewport viewport.Model
	composer Composer
	history  History
	theme    Theme
	state    State
}

func NewAppModel(cmdChan chan<- protocol.EngineCommand, eventChan <-chan protocol.EngineEvent) AppModel {
	return AppModel{
		cmdChan:   cmdChan,
		eventChan: eventChan,
		composer:  NewComposer(),
		theme:     NewDefaultTheme(),
	}
}

type engineEventMsg protocol.EngineEvent

func applyEvent(m *AppModel, evt protocol.EngineEvent) {
	switch evt.Type {
	case protocol.EventAssistantTextDelta:
		m.history.AppendDelta(evt.Text)
	case protocol.EventTurnStarted:
		m.state.Busy = true
	case protocol.EventTurnCompleted:
		m.state.Busy = false
	case protocol.EventTurnCancelled:
		m.state.Busy = false
		m.history.Append("system", "[turn cancelled]")
	case protocol.EventTurnFailed:
		m.state.Busy = false
		m.history.Append("error", evt.Error)
	case protocol.EventToolStarted:
		m.history.Append("tool", "started "+evt.ToolName)
	case protocol.EventToolCompleted:
		m.history.Append("tool", evt.ToolName+" done")
	case protocol.EventToolFailed:
		m.history.Append("error", evt.ToolName+": "+evt.Error)
	case protocol.EventError:
		m.history.Append("error", evt.Error)
	case protocol.EventWarning:
		m.history.Append("system", "[warning] "+evt.Text)
	}
}

func waitForEvent(ch <-chan protocol.EngineEvent) tea.Cmd {
	return func() tea.Msg {
		evt, ok := <-ch
		if !ok {
			return nil
		}
		return engineEventMsg(evt)
	}
}

func (m AppModel) Init() tea.Cmd {
	return tea.Batch(waitForEvent(m.eventChan))
}

func (m AppModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case tea.KeyMsg:
		switch msg.Type {
		case tea.KeyCtrlC:
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdShutdown}
			return m, tea.Quit
		case tea.KeyEsc:
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdInterruptTurn}
		case tea.KeyEnter:
			input := m.composer.Input.Value()
			if input != "" {
				m.history.Append("user", input)
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: input}
				m.composer.Input.Reset()
				m.state.Busy = true
			}
		}

	case tea.WindowSizeMsg:
		m.state.Width = msg.Width
		m.state.Height = msg.Height
		if !m.state.Ready {
			m.viewport = viewport.New(msg.Width, msg.Height-4)
			m.state.Ready = true
		} else {
			m.viewport.Width = msg.Width
			m.viewport.Height = msg.Height - 4
		}

	case engineEventMsg:
		evt := protocol.EngineEvent(msg)
		applyEvent(&m, evt)
		// Coalesce any already-queued events into this same frame.
		for drained := true; drained; {
			select {
			case evt, ok := <-m.eventChan:
				if !ok {
					drained = false
					break
				}
				applyEvent(&m, protocol.EngineEvent(evt))
			default:
				drained = false
			}
		}
		m.viewport.SetContent(m.RenderHistory())
		m.viewport.GotoBottom()
		cmds = append(cmds, waitForEvent(m.eventChan))
	}

	var cmd tea.Cmd
	m.composer.Input, cmd = m.composer.Input.Update(msg)
	cmds = append(cmds, cmd)
	var vpCmd tea.Cmd
	m.viewport, vpCmd = m.viewport.Update(msg)
	cmds = append(cmds, vpCmd)

	return m, tea.Batch(cmds...)
}

func (m AppModel) View() string {
	if !m.state.Ready {
		return "Initializing Niki..."
	}
	var header string
	if m.state.Width < 60 {
		header = m.theme.Header.Render("Niki")
	} else {
		header = m.theme.Header.Render("Niki") + "  " + m.theme.Muted.Render("Local Coding Agent")
	}
	if m.state.Busy {
		header += "  " + m.theme.Muted.Render("(working...)")
	}
	composer := m.composer.Input.View()
	if suggestions := composerSuggestions(m.composer.Input.Value()); len(suggestions) > 0 {
		composer += "\n" + m.theme.Muted.Render("  "+strings.Join(suggestions, "  "))
	}
	return lipgloss.JoinVertical(
		lipgloss.Left,
		header,
		m.viewport.View(),
		composer,
	)
}

// composerSuggestions offers the fuzzy command menu (/) and a minimal file
// picker (@) from the real filesystem.
func composerSuggestions(value string) []string {
	switch {
	case strings.HasPrefix(value, "/"):
		cmds := []string{"/exec", "/doctor", "/config", "/skills", "/mcp", "/resume", "/quit"}
		q := strings.TrimPrefix(value, "/")
		var out []string
		for _, c := range cmds {
			if strings.Contains(c, q) {
				out = append(out, c)
			}
		}
		if len(out) > 5 {
			out = out[:5]
		}
		return out
	case strings.HasPrefix(value, "@"):
		entries, err := os.ReadDir(".")
		if err != nil {
			return nil
		}
		var out []string
		for _, e := range entries {
			if strings.HasPrefix(e.Name(), strings.TrimPrefix(value, "@")) {
				out = append(out, "@"+e.Name())
			}
			if len(out) >= 5 {
				break
			}
		}
		return out
	}
	return nil
}
