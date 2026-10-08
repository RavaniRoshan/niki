package tui

import (
	"os"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/cursor"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// streamInterval is the minimum spacing between
// live-region re-renders while streaming (U10).
const streamInterval = 16 * time.Millisecond

type AppModel struct {
	cmdChan   chan<- protocol.EngineCommand
	eventChan <-chan protocol.EngineEvent

	viewport viewport.Model
	composer Composer
	history  History
	theme    Theme
	state    State
	telemetry *FrameTelemetry
	pacer    renderPacer
	// headerPrinted tracks whether the header
	// was flushed to native scrollback (U4).
	headerPrinted bool
}

func NewAppModel(cmdChan chan<- protocol.EngineCommand, eventChan <-chan protocol.EngineEvent) AppModel {
	return AppModel{
		cmdChan:   cmdChan,
		eventChan: eventChan,
		composer:  NewComposer(),
		theme:     NewDefaultTheme(),
		telemetry: &FrameTelemetry{},
		pacer:     renderPacer{minInterval: streamInterval},
	}
}

// SetReducedMotion applies the reduced-motion
// preference (U8): the composer cursor becomes
// static instead of blinking, removing the
// periodic wakeups animation would cause.
func (m *AppModel) SetReducedMotion(reduced bool) {
	m.state.ReducedMotion = reduced
	if reduced {
		m.composer.Input.Cursor.SetMode(cursor.CursorStatic)
	} else {
		m.composer.Input.Cursor.SetMode(cursor.CursorBlink)
	}
}

type engineEventMsg protocol.EngineEvent

type tickMsg time.Time

func applyEvent(m *AppModel, evt protocol.EngineEvent) {
	m.telemetry.Events++
	switch evt.Type {
	case protocol.EventAssistantTextDelta:
		m.history.AppendDelta(evt.Text)
		m.telemetry.Deltas++
		m.state.Activity = "streaming…"
	case protocol.EventTurnStarted:
		m.state.Busy = true
		m.state.Activity = "thinking…"
	case protocol.EventTurnCompleted:
		m.state.Busy = false
		m.state.Activity = ""
	case protocol.EventTurnCancelled:
		m.state.Busy = false
		m.state.Activity = ""
		m.history.Append("system", "[turn cancelled]")
	case protocol.EventTurnFailed:
		m.state.Busy = false
		m.state.Activity = ""
		m.history.Append("error", evt.Error)
	case protocol.EventToolStarted:
		m.history.Append("tool", "started "+evt.ToolName)
		m.state.Activity = "running " + evt.ToolName
	case protocol.EventToolCompleted:
		m.history.Append("tool", evt.ToolName+" done")
		if m.state.Activity != "" && strings.HasPrefix(m.state.Activity, "running ") {
			m.state.Activity = ""
		}
	case protocol.EventToolFailed:
		m.history.Append("error", evt.ToolName+": "+evt.Error)
		m.state.Activity = ""
	case protocol.EventError:
		m.history.Append("error", evt.Error)
	case protocol.EventWarning:
		m.history.Append("system", "[warning] "+evt.Text)
	case protocol.EventContextCompacted:
		m.history.Append("system", "[context compacted]")
	case protocol.EventSubagentStarted:
		m.state.Activity = "subagent " + evt.Text
	case protocol.EventSubagentCompleted, protocol.EventSubagentFailed:
		if m.state.Activity != "" && strings.HasPrefix(m.state.Activity, "subagent ") {
			m.state.Activity = ""
		}
	case protocol.EventMcpServerStarting:
		m.state.Activity = "mcp " + evt.ToolName + " starting"
	case protocol.EventMcpServerReady:
		m.history.Append("system", "[mcp] "+evt.ToolName+" ready")
		if m.state.Activity != "" && strings.HasPrefix(m.state.Activity, "mcp ") {
			m.state.Activity = ""
		}
	case protocol.EventMcpServerFailed:
		m.history.Append("error", "mcp "+evt.ToolName+": "+evt.Error)
		if m.state.Activity != "" && strings.HasPrefix(m.state.Activity, "mcp ") {
			m.state.Activity = ""
		}
	case protocol.EventBootPhase:
		m.telemetry.BootPhases = append(m.telemetry.BootPhases, BootPhase{
			Name:     strings.TrimSuffix(evt.Text, ":ready"),
			Duration: evt.Duration,
		})
	case protocol.EventSkillDiscovered:
		m.history.Append("system", "[skills] "+evt.Text)
	case protocol.EventConfigReloaded:
		m.history.Append("system", "[config] "+evt.Text)
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

// renderPacer throttles live-region re-renders
// while streaming with hysteresis (U10): deltas
// arriving inside the interval coalesce into the
// next paced frame; the boundary always flushes.
// Renders counts repaint decisions, for telemetry.
type renderPacer struct {
	minInterval time.Duration
	last        time.Time
	pending     bool
	Renders     int
}

func (p *renderPacer) allow() bool {
	now := time.Now()
	if now.Sub(p.last) >= p.minInterval {
		p.last = now
		p.pending = false
		p.Renders++
		return true
	}
	p.pending = true
	return false
}

func (p *renderPacer) force() {
	p.last = time.Now()
	p.pending = false
	p.Renders++
}

// renderLive repaints the viewport from the
// live region only. Committed history is never
// re-rendered here (U9/U10).
func (m *AppModel) renderLive() {
	cells := m.history.Live()
	if !m.state.Inline {
		cells = m.history.Cells
	}
	m.viewport.SetContent(renderCells(*m, cells))
	m.viewport.GotoBottom()
}

func (m AppModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case tea.KeyMsg:
		switch msg.Type {
		case tea.KeyCtrlC:
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdShutdown}
			return m, tea.Quit
		case tea.KeyCtrlD:
			m.state.Debug = !m.state.Debug
		case tea.KeyCtrlZ:
			// Bubbletea handles SuspendMsg: it releases the
			// terminal, SIGSTOPs the process group, and
			// restores everything on SIGCONT (L2).
			return m, tea.Suspend
		case tea.KeyEsc:
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdInterruptTurn}
		case tea.KeyEnter:
			input := strings.TrimSpace(m.composer.Input.Value())
			switch input {
			case "/help":
				var helpText strings.Builder
				helpText.WriteString("Available commands:\n")
				for _, cmd := range CoreSlashCommands {
					helpText.WriteString("  " + cmd.Name + " — " + cmd.Description + "\n")
				}
				helpText.WriteString("\nKeybindings:\n")
				helpText.WriteString("  Esc: Interrupt turn / Deny approval\n")
				helpText.WriteString("  Ctrl+C: Clear input / Interrupt\n")
				helpText.WriteString("  Ctrl+D: Exit on empty input\n")
				helpText.WriteString("  Ctrl+Z: Suspend process\n")
				m.history.Append("system", helpText.String())
				m.composer.Input.Reset()
			case "/clear":
				m.history.Cells = nil
				m.history.Committed = 0
				m.composer.Input.Reset()
			case "/quit", "/exit":
				return m, tea.Quit
			case "/debug":
				m.state.Debug = !m.state.Debug
				m.composer.Input.Reset()
			case "/reload":
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
				m.composer.Input.Reset()
			default:
				if input != "" {
					m.history.Append("user", input)
					m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: input}
					m.composer.Input.Reset()
					m.state.Busy = true
				}
			}
		}

	case tea.WindowSizeMsg:
		m.state.Width = msg.Width
		m.state.Height = msg.Height
		// Resize safety (L3): the chrome (composer + footer)
		// reserves four lines; degenerate sizes clamp rather
		// than panic.
		chrome := 4
		if msg.Height <= chrome {
			chrome = msg.Height - 1
		}
		if chrome < 1 {
			chrome = 1
		}
		if msg.Width < 1 {
			msg.Width = 1
		}
		if !m.state.Ready {
			m.viewport = viewport.New(msg.Width, chrome)
			m.state.Ready = true
			if m.state.Inline && !m.headerPrinted {
				// The header scrolls away into
				// native scrollback (U4).
				m.headerPrinted = true
				cmds = append(cmds, tea.Printf("%s", m.headerView()))
			}
		} else {
			m.viewport.Width = msg.Width
			m.viewport.Height = chrome
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

		turnEnded := evt.Type == protocol.EventTurnCompleted ||
			evt.Type == protocol.EventTurnFailed ||
			evt.Type == protocol.EventTurnCancelled
		if turnEnded {
			// Finalize the turn: committed cells
			// flush to native scrollback in inline
			// mode and leave the live region (U9).
			committed := m.history.Finalize()
			m.pacer.force()
			m.renderLive()
			if m.state.Inline && len(committed) > 0 {
				cmds = append(cmds, tea.Printf("%s", renderCells(m, committed)))
			}
		} else if m.pacer.allow() {
			m.renderLive()
		} else {
			// Paced: schedule a flush at the
			// interval boundary (U10).
			cmds = append(cmds, tea.Tick(m.pacer.minInterval, func(t time.Time) tea.Msg {
				return tickMsg(t)
			}))
		}
		cmds = append(cmds, waitForEvent(m.eventChan))

	case tickMsg:
		if m.pacer.pending {
			m.pacer.force()
			m.renderLive()
		}
	}

	var cmd tea.Cmd
	m.composer.Input, cmd = m.composer.Input.Update(msg)
	cmds = append(cmds, cmd)
	var vpCmd tea.Cmd
	m.viewport, vpCmd = m.viewport.Update(msg)
	cmds = append(cmds, vpCmd)

	return m, tea.Batch(cmds...)
}

func (m AppModel) headerView() string {
	if m.state.Width < 60 {
		return m.theme.Header.Render("Niki")
	}
	return m.theme.Header.Render("Niki") + "  " + m.theme.Muted.Render("Local Coding Agent")
}

func (m AppModel) View() string {
	start := time.Now()
	defer func() { m.telemetry.RecordRender(time.Since(start)) }()

	if !m.state.Ready {
		return "Initializing Niki..."
	}

	var body string
	if m.state.Inline {
		// Inline mode: no header in the live
		// view (it scrolled away), one
		// activity line, then the composer —
		// the only bordered element (U4).
		body = lipgloss.JoinVertical(lipgloss.Left,
			m.activityView(),
			m.viewport.View(),
			m.composerView(),
		)
	} else {
		header := m.headerView()
		if m.state.Busy {
			header += "  " + m.theme.Muted.Render("(working...)")
		}
		body = lipgloss.JoinVertical(lipgloss.Left,
			header,
			m.activityView(),
			m.viewport.View(),
			m.composerView(),
		)
	}
	if m.state.Debug {
		body += "\n" + m.debugView()
	}
	return body
}

// activityView renders the single live activity
// line above the composer (U3).
func (m AppModel) activityView() string {
	if m.state.Activity == "" {
		return ""
	}
	return m.theme.Muted.Render("  " + m.state.Activity)
}

func (m AppModel) composerView() string {
	composer := lipgloss.NewStyle().Border(lipgloss.NormalBorder()).Render(m.composer.Input.View())
	if suggestions := composerSuggestions(m.composer.Input.Value()); len(suggestions) > 0 {
		composer += "\n" + m.theme.Muted.Render("  "+strings.Join(suggestions, "  "))
	}
	return composer
}

// debugView renders boot-phase timings and frame
// telemetry (B9).
func (m AppModel) debugView() string {
	var b strings.Builder
	b.WriteString(m.theme.Muted.Render("── debug ─────────────────────────────\n"))
	if len(m.telemetry.BootPhases) > 0 {
		b.WriteString(m.theme.Muted.Render("boot phases:\n"))
		for _, p := range m.telemetry.BootPhases {
			b.WriteString(m.theme.Muted.Render("  ") + p.Name + ": " + p.Duration.String() + "\n")
		}
	} else {
		b.WriteString(m.theme.Muted.Render("boot: no phases recorded\n"))
	}
	b.WriteString(m.theme.Muted.Render("frames: "))
	b.WriteString(m.theme.Muted.Render(itoa(m.telemetry.Frames)))
	b.WriteString("  events: ")
	b.WriteString(m.theme.Muted.Render(itoa(m.telemetry.Events)))
	b.WriteString("  deltas: ")
	b.WriteString(m.theme.Muted.Render(itoa(m.telemetry.Deltas)))
	b.WriteString("\n")
	b.WriteString(m.theme.Muted.Render("render last: " + m.telemetry.LastRender.String() +
		"  p95: " + m.telemetry.RenderP95().String() +
		"  max: " + m.telemetry.MaxRenderCost().String() + "\n"))
	return b.String()
}

func itoa(n int) string {
	if n == 0 {
		return "0"
	}
	var buf [20]byte
	i := len(buf)
	for n > 0 {
		i--
		buf[i] = byte('0' + n%10)
		n /= 10
	}
	return string(buf[i:])
}

// composerSuggestions offers the fuzzy command menu (/) and a minimal file
// picker (@) from the real filesystem.
func composerSuggestions(value string) []string {
	switch {
	case strings.HasPrefix(value, "/"):
		out := SuggestSlashCommands(value)
		if len(out) > 6 {
			out = out[:6]
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
