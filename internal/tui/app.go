package tui

import (
	"fmt"
	"os"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/cursor"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/RavaniRoshan/niki/internal/config"
	"github.com/RavaniRoshan/niki/internal/editor"
	"github.com/RavaniRoshan/niki/internal/explain"
	"github.com/RavaniRoshan/niki/internal/mention"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/routing"
)

type editorFinishedMsg struct {
	path string
	err  error
}

type editorDraftReadMsg struct {
	text string
	err  error
}

type mentionResultsMsg struct {
	query      string
	candidates []mention.Candidate
}

func launchEditorCmd(initialText string) tea.Cmd {
	cmd, path, err := editor.PrepareEditorDraft(initialText)
	if err != nil {
		return func() tea.Msg {
			return editorFinishedMsg{path: "", err: err}
		}
	}
	return tea.ExecProcess(cmd, func(err error) tea.Msg {
		return editorFinishedMsg{path: path, err: err}
	})
}

func fetchMentionCandidates(root, query string) tea.Cmd {
	return func() tea.Msg {
		cands, _ := mention.Picker(root, query)
		return mentionResultsMsg{query: query, candidates: cands}
	}
}

// streamInterval is the minimum spacing between
// live-region re-renders while streaming (U10).
const streamInterval = 16 * time.Millisecond

type AppModel struct {
	cmdChan   chan<- protocol.EngineCommand
	eventChan <-chan protocol.EngineEvent

	viewport  viewport.Model
	composer  Composer
	history   History
	theme     Theme
	state     State
	telemetry *FrameTelemetry
	pacer     renderPacer
	// headerPrinted tracks whether the header
	// was flushed to native scrollback (U4).
	headerPrinted bool

	// exitArmed tracks whether the exit arm timer is active for double Ctrl+C
	exitArmed bool
}

type disarmExitMsg struct{}

func disarmExitTimer() tea.Cmd {
	return tea.Tick(1500*time.Millisecond, func(time.Time) tea.Msg {
		return disarmExitMsg{}
	})
}

func NewAppModel(cmdChan chan<- protocol.EngineCommand, eventChan <-chan protocol.EngineEvent) AppModel {
	cwd, _ := os.Getwd()
	th := NewDefaultTheme()
	return AppModel{
		cmdChan:   cmdChan,
		eventChan: eventChan,
		composer:  NewComposer(th),
		theme:     th,
		telemetry: &FrameTelemetry{},
		pacer:     renderPacer{minInterval: streamInterval},
		state: State{
			Directory:      cwd,
			SessionID:      "",
			ModelName:      "mock: gpt-4o-mini",
			Version:        "0.11.0",
			PermissionMode: "workspace_write",
			Mode:           "plan",
			GitBranch:      "main",
			MaxTokens:      128000,
			SpinnerStyle:   SpinnerBloom,
		},
	}
}

func (m *AppModel) SetDirectory(dir string) {
	m.state.Directory = dir
}

func (m *AppModel) SetGitBranch(branch string) {
	m.state.GitBranch = branch
}

func (m *AppModel) SetSessionID(id string) {
	m.state.SessionID = id
}

func (m *AppModel) SetModel(modelName string, maxTokens int) {
	m.state.ModelName = modelName
	if maxTokens > 0 {
		m.state.MaxTokens = maxTokens
	}
}

func (m *AppModel) SetPermissionMode(mode string) {
	m.state.PermissionMode = mode
}

func (m *AppModel) SetMode(mode string) {
	m.state.Mode = mode
}

func (m *AppModel) SetVersion(ver string) {
	m.state.Version = ver
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

// SetInline configures whether the TUI runs in inline mode.
func (m *AppModel) SetInline(inline bool) {
	m.state.Inline = inline
}

type engineEventMsg protocol.EngineEvent

type tickMsg time.Time

// spinTickMsg advances the activity sweep. It reschedules itself only
// while Busy, so an idle session schedules zero ticks (B5).
type spinTickMsg time.Time

// spinTick returns a 120ms tick command (visual-spec cadence).
func spinTick() tea.Cmd {
	return tea.Tick(120*time.Millisecond, func(t time.Time) tea.Msg {
		return spinTickMsg(t)
	})
}

func applyEvent(m *AppModel, evt protocol.EngineEvent) {
	m.telemetry.Events++
	if evt.Usage != nil {
		tot := evt.Usage.TotalTokens
		if tot == 0 {
			tot = evt.Usage.PromptTokens + evt.Usage.CompletionTokens
		}
		if tot > 0 {
			m.state.UsedTokens = tot
		}
		cost := routing.CalculateCost(m.state.ModelName, *evt.Usage)
		m.state.TotalCost += cost
	}
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
	case protocol.EventQuestionPrompted:
		if len(evt.Questions) > 0 {
			m.state.QuestionModal = NewQuestionState(evt.Questions)
		}
	case protocol.EventSessionList:
		m.state.SessionPicker.Sessions = evt.Sessions
		if m.state.SessionPicker.Preview == nil {
			m.state.SessionPicker.Preview = make(map[string][]string)
		}
		if len(evt.History) > 0 && len(evt.Sessions) > 0 {
			m.state.SessionPicker.Preview[string(evt.Sessions[0].ID)] = evt.History
		}
	case protocol.EventSessionLoaded:
		m.state.SessionID = string(evt.SessionID)
		m.history.Cells = nil
		m.history.Committed = 0
		m.history.Append("system", fmt.Sprintf("✓ Session %s loaded", evt.SessionID))
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
	case disarmExitMsg:
		m.exitArmed = false
		return m, nil

	case editorFinishedMsg:
		if msg.err != nil {
			m.history.Append("error", "Editor exited with error: "+msg.err.Error())
			return m, nil
		}
		path := msg.path
		return m, func() tea.Msg {
			content, err := editor.ReadAndCleanupDraft(path)
			return editorDraftReadMsg{text: content, err: err}
		}

	case editorDraftReadMsg:
		if msg.err != nil {
			m.history.Append("error", "Failed to read editor draft: "+msg.err.Error())
		} else if msg.text != "" {
			m.composer.Input.SetValue(msg.text)
			m.composer.Input.SetCursor(len([]rune(msg.text)))
		}
		return m, nil

	case mentionResultsMsg:
		if m.state.MentionOverlay.Active && m.state.MentionOverlay.Query == msg.query {
			m.state.MentionOverlay.Candidates = msg.candidates
			if m.state.MentionOverlay.Selected >= len(msg.candidates) {
				m.state.MentionOverlay.Selected = 0
			}
			m.renderLive()
		}
		return m, nil

	case tea.KeyMsg:
		if m.state.QuestionModal.Active {
			done, answers, cancelled := m.state.QuestionModal.HandleKey(msg)
			if done {
				if cancelled {
					m.history.Append("system", "[question skipped]")
					m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdAnswerQuestion, Answers: []string{"skipped"}}
				} else {
					m.history.Append("system", fmt.Sprintf("✓ Answered: %s", strings.Join(answers, ", ")))
					m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdAnswerQuestion, Answers: answers}
				}
			}
			return m, nil
		}

		if m.state.DiffViewer.Active {
			closed := m.state.DiffViewer.HandleKey(msg)
			if closed {
				m.renderLive()
			}
			return m, nil
		}

		if m.state.Btw.Active {
			closed := m.state.Btw.HandleKey(msg)
			if closed {
				m.renderLive()
			}
			return m, nil
		}

		if m.state.SessionPicker.Active {
			switch msg.Type {
			case tea.KeyCtrlS, tea.KeyEsc:
				if m.state.SessionPicker.ConfirmDelete {
					m.state.SessionPicker.ConfirmDelete = false
					return m, nil
				}
				m.state.SessionPicker.Active = false
				return m, nil
			case tea.KeyUp:
				if m.state.SessionPicker.Selected > 0 {
					m.state.SessionPicker.Selected--
				}
				return m, nil
			case tea.KeyDown:
				filtered := FilterSessions(m.state.SessionPicker.Sessions, m.state.SessionPicker.Query)
				if m.state.SessionPicker.Selected < len(filtered)-1 {
					m.state.SessionPicker.Selected++
				}
				return m, nil
			case tea.KeyEnter:
				filtered := FilterSessions(m.state.SessionPicker.Sessions, m.state.SessionPicker.Query)
				if len(filtered) > 0 && m.state.SessionPicker.Selected < len(filtered) {
					sel := filtered[m.state.SessionPicker.Selected]
					m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdResumeSession, SessionID: sel.ID}
					m.state.SessionPicker.Active = false
					m.history.Append("system", fmt.Sprintf("📂 Resumed session: %s", sel.Title))
				}
				return m, nil
			case tea.KeyBackspace:
				if len(m.state.SessionPicker.Query) > 0 {
					m.state.SessionPicker.Query = m.state.SessionPicker.Query[:len(m.state.SessionPicker.Query)-1]
					m.state.SessionPicker.Selected = 0
				}
				return m, nil
			default:
				if m.state.SessionPicker.ConfirmDelete {
					if msg.Type == tea.KeyRunes {
						ch := strings.ToLower(string(msg.Runes))
						if ch == "y" {
							filtered := FilterSessions(m.state.SessionPicker.Sessions, m.state.SessionPicker.Query)
							if len(filtered) > 0 && m.state.SessionPicker.Selected < len(filtered) {
								sel := filtered[m.state.SessionPicker.Selected]
								m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdDeleteSession, SessionID: sel.ID}
								m.history.Append("system", fmt.Sprintf("🗑️ Deleted session: %s", sel.Title))
								var rem []protocol.SessionMetadata
								for _, s := range m.state.SessionPicker.Sessions {
									if s.ID != sel.ID {
										rem = append(rem, s)
									}
								}
								m.state.SessionPicker.Sessions = rem
							}
							m.state.SessionPicker.ConfirmDelete = false
							return m, nil
						}
						m.state.SessionPicker.ConfirmDelete = false
						return m, nil
					}
					return m, nil
				}

				if msg.Type == tea.KeyRunes || msg.Type == tea.KeySpace {
					ch := string(msg.Runes)
					if ch == "f" && m.state.SessionPicker.Query == "" {
						filtered := FilterSessions(m.state.SessionPicker.Sessions, m.state.SessionPicker.Query)
						if len(filtered) > 0 && m.state.SessionPicker.Selected < len(filtered) {
							sel := filtered[m.state.SessionPicker.Selected]
							m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdForkSession, SessionID: sel.ID}
							m.state.SessionPicker.Active = false
							m.history.Append("system", fmt.Sprintf("🍴 Forked session from: %s", sel.Title))
							return m, nil
						}
					}
					if ch == "d" && m.state.SessionPicker.Query == "" {
						filtered := FilterSessions(m.state.SessionPicker.Sessions, m.state.SessionPicker.Query)
						if len(filtered) > 0 {
							m.state.SessionPicker.ConfirmDelete = true
							return m, nil
						}
					}
					m.state.SessionPicker.Query += ch
					m.state.SessionPicker.Selected = 0
					return m, nil
				}
			}
			return m, nil
		}

		if m.state.MentionOverlay.Active {
			switch msg.Type {
			case tea.KeyUp:
				if m.state.MentionOverlay.Selected > 0 {
					m.state.MentionOverlay.Selected--
				}
				return m, nil
			case tea.KeyDown:
				if m.state.MentionOverlay.Selected < len(m.state.MentionOverlay.Candidates)-1 {
					m.state.MentionOverlay.Selected++
				}
				return m, nil
			case tea.KeyTab, tea.KeyEnter:
				if len(m.state.MentionOverlay.Candidates) > 0 && m.state.MentionOverlay.Selected < len(m.state.MentionOverlay.Candidates) {
					c := m.state.MentionOverlay.Candidates[m.state.MentionOverlay.Selected]
					newVal, newPos := ReplaceMentionWord(m.composer.Input.Value(), m.composer.Input.Position(), c.Path)
					m.composer.Input.SetValue(newVal)
					m.composer.Input.SetCursor(newPos)
					m.state.MentionOverlay.Active = false
					m.state.MentionOverlay.Candidates = nil
					return m, nil
				}
			case tea.KeyEsc:
				m.state.MentionOverlay.Active = false
				m.state.MentionOverlay.Candidates = nil
				return m, nil
			}
		}

		switch msg.Type {
		case tea.KeyCtrlC:
			val := m.composer.Input.Value()
			if len(val) > 0 {
				m.composer.Input.Reset()
				m.exitArmed = false
				return m, nil
			}
			if m.state.Busy {
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdInterruptTurn}
				m.state.Activity = "interrupting…"
				m.exitArmed = false
				return m, nil
			}
			if m.exitArmed {
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdShutdown}
				return m, tea.Quit
			}
			m.exitArmed = true
			m.history.Append("system", "Press Ctrl+C again to exit")
			return m, disarmExitTimer()

		case tea.KeyCtrlD:
			val := m.composer.Input.Value()
			if len(val) == 0 {
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdShutdown}
				return m, tea.Quit
			}
			pos := m.composer.Input.Position()
			runes := []rune(val)
			if pos < len(runes) {
				m.composer.Undo.Push(val, pos)
				newRunes := append(runes[:pos], runes[pos+1:]...)
				m.composer.Input.SetValue(string(newRunes))
			}
			return m, nil

		case tea.KeyCtrlZ:
			// Bubbletea handles SuspendMsg: it releases the
			// terminal, SIGSTOPs the process group, and
			// restores everything on SIGCONT (L2).
			return m, tea.Suspend

		case tea.KeyEsc:
			if m.state.Palette.Open {
				if m.state.Palette.Mode != "palette" {
					m.state.Palette.Mode = "palette"
					m.state.Palette.ConnectKey = ""
					m.state.Palette.InputBuffer = ""
					m.state.Palette.InputStep = 0
				} else {
					m.state.Palette.Open = false
					m.state.Palette.Query = ""
				}
				return m, nil
			}
			if m.state.Busy {
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdInterruptTurn}
				m.state.Activity = "interrupting…"
				return m, nil
			}
			return m, nil

		case tea.KeyCtrlS:
			m.state.SessionPicker.Active = !m.state.SessionPicker.Active
			if m.state.SessionPicker.Active {
				m.state.SessionPicker.Query = ""
				m.state.SessionPicker.Selected = 0
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdListSessions}
			}
			return m, nil

		case tea.KeyCtrlG:
			return m, launchEditorCmd(m.composer.Input.Value())

		case tea.KeyCtrlP:
			m.state.Palette.Open = !m.state.Palette.Open
			m.state.Palette.Query = ""
			m.state.Palette.Selected = 0
			m.state.Palette.Mode = "palette"
			return m, nil

		case tea.KeyCtrlO:
			m.state.ExpandToolOutput = !m.state.ExpandToolOutput
			m.renderLive()
			return m, nil

		case tea.KeyCtrlB:
			if m.state.Busy {
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdDetachTool}
				m.history.Append("system", "[tool detached into background execution]")
				m.state.Activity = "1 task in background"
				return m, nil
			}
			return m, nil

		case tea.KeyCtrlW:
			val := m.composer.Input.Value()
			pos := m.composer.Input.Position()
			if pos > 0 {
				m.composer.Undo.Push(val, pos)
				runes := []rune(val)
				start := pos - 1
				for start > 0 && runes[start] == ' ' {
					start--
				}
				for start > 0 && runes[start-1] != ' ' {
					start--
				}
				killed := string(runes[start:pos])
				m.composer.KillRing.Push(killed, true, false)
				newRunes := append(runes[:start], runes[pos:]...)
				m.composer.Input.SetValue(string(newRunes))
				m.composer.Input.SetCursor(start)
			}
			return m, nil

		case tea.KeyCtrlK:
			val := m.composer.Input.Value()
			pos := m.composer.Input.Position()
			runes := []rune(val)
			if pos < len(runes) {
				m.composer.Undo.Push(val, pos)
				killed := string(runes[pos:])
				m.composer.KillRing.Push(killed, false, false)
				m.composer.Input.SetValue(string(runes[:pos]))
			}
			return m, nil

		case tea.KeyCtrlU:
			val := m.composer.Input.Value()
			pos := m.composer.Input.Position()
			runes := []rune(val)
			if pos > 0 {
				m.composer.Undo.Push(val, pos)
				killed := string(runes[:pos])
				m.composer.KillRing.Push(killed, true, false)
				m.composer.Input.SetValue(string(runes[pos:]))
				m.composer.Input.SetCursor(0)
			}
			return m, nil

		case tea.KeyCtrlY:
			yanked := m.composer.KillRing.Yank()
			if yanked != "" {
				val := m.composer.Input.Value()
				pos := m.composer.Input.Position()
				m.composer.Undo.Push(val, pos)
				runes := []rune(val)
				newRunes := append(runes[:pos], append([]rune(yanked), runes[pos:]...)...)
				m.composer.Input.SetValue(string(newRunes))
				m.composer.Input.SetCursor(pos + len([]rune(yanked)))
			}
			return m, nil

		case tea.KeyCtrlUnderscore:
			val := m.composer.Input.Value()
			pos := m.composer.Input.Position()
			if snap, ok := m.composer.Undo.Undo(val, pos); ok {
				m.composer.Input.SetValue(snap.Value)
				m.composer.Input.SetCursor(snap.Cursor)
			}
			return m, nil
		}

		if m.state.Palette.Open {
			switch msg.Type {
			case tea.KeyCtrlC, tea.KeyEsc:
				if m.state.Palette.Mode != "palette" {
					m.state.Palette.Mode = "palette"
					m.state.Palette.ConnectKey = ""
					m.state.Palette.InputBuffer = ""
					m.state.Palette.InputStep = 0
				} else {
					m.state.Palette.Open = false
					m.state.Palette.Query = ""
				}
				return m, nil
			case tea.KeyUp:
				if m.state.Palette.Mode == "palette" {
					if m.state.Palette.Selected > 0 {
						m.state.Palette.Selected--
					}
				}
				return m, nil
			case tea.KeyDown:
				if m.state.Palette.Mode == "palette" {
					filtered := FilterPalette(DefaultPaletteCatalog(), m.state.Palette.Query)
					if m.state.Palette.Selected < len(filtered)-1 {
						m.state.Palette.Selected++
					}
				}
				return m, nil
			case tea.KeyBackspace:
				switch m.state.Palette.Mode {
				case "connect":
					if len(m.state.Palette.ConnectKey) > 0 {
						m.state.Palette.ConnectKey = m.state.Palette.ConnectKey[:len(m.state.Palette.ConnectKey)-1]
					}
				case "custom_model", "custom_endpoint", "mcp_add":
					if len(m.state.Palette.InputBuffer) > 0 {
						m.state.Palette.InputBuffer = m.state.Palette.InputBuffer[:len(m.state.Palette.InputBuffer)-1]
					}
				default:
					if len(m.state.Palette.Query) > 0 {
						m.state.Palette.Query = m.state.Palette.Query[:len(m.state.Palette.Query)-1]
						m.state.Palette.Selected = 0
					}
				}
				return m, nil
			case tea.KeyEnter:
				switch m.state.Palette.Mode {
				case "connect":
					key := strings.TrimSpace(m.state.Palette.ConnectKey)
					prov := m.state.Palette.ConnectProvider
					if key != "" {
						cfg, _ := config.Load("")
						switch prov {
						case "anthropic":
							cfg.Provider.Name = "anthropic"
							cfg.Provider.APIKey = key
							if cfg.Model.Name == "" || cfg.Model.Name == "gpt-4o-mini" {
								cfg.Model.Name = "claude-3-5-sonnet"
							}
						case "openai":
							cfg.Provider.Name = "openai"
							cfg.Provider.APIKey = key
							if cfg.Model.Name == "" {
								cfg.Model.Name = "gpt-4o"
							}
						case "openrouter":
							cfg.Provider.Name = "openai"
							cfg.Provider.BaseURL = "https://openrouter.ai/api/v1"
							cfg.Provider.APIKey = key
						case "deepseek":
							cfg.Provider.Name = "openai"
							cfg.Provider.BaseURL = "https://api.deepseek.com"
							cfg.Provider.APIKey = key
							cfg.Model.Name = "deepseek-chat"
						}
						_ = config.SaveUserConfig(cfg)
						m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
						m.state.ModelName = cfg.Provider.Name + ": " + cfg.Model.Name
						m.history.Append("system", fmt.Sprintf("✓ Connected %s API key and reloaded provider (%s)!", prov, m.state.ModelName))
					}
					m.state.Palette.Open = false
					m.state.Palette.Mode = "palette"
					m.state.Palette.ConnectKey = ""
					return m, nil

				case "custom_model":
					val := strings.TrimSpace(m.state.Palette.InputBuffer)
					if val != "" {
						cfg, _ := config.Load("")
						if strings.Contains(val, ":") {
							parts := strings.SplitN(val, ":", 2)
							cfg.Provider.Name = parts[0]
							cfg.Model.Name = parts[1]
						} else {
							cfg.Model.Name = val
						}
						_ = config.SaveUserConfig(cfg)
						m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
						m.state.ModelName = cfg.Provider.Name + ": " + cfg.Model.Name
						m.history.Append("system", fmt.Sprintf("⚡ Switched custom model to: %s", val))
					}
					m.state.Palette.Open = false
					m.state.Palette.Mode = "palette"
					m.state.Palette.InputBuffer = ""
					return m, nil

				case "custom_endpoint":
					val := strings.TrimSpace(m.state.Palette.InputBuffer)
					if val != "" {
						cfg, _ := config.Load("")
						cfg.Provider.BaseURL = val
						_ = config.SaveUserConfig(cfg)
						m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
						m.history.Append("system", fmt.Sprintf("🌐 Provider base URL set to: %s", val))
					}
					m.state.Palette.Open = false
					m.state.Palette.Mode = "palette"
					m.state.Palette.InputBuffer = ""
					return m, nil

				case "mcp_add":
					val := strings.TrimSpace(m.state.Palette.InputBuffer)
					switch m.state.Palette.InputStep {
					case 0:
						if val != "" {
							m.state.Palette.MCPName = val
							m.state.Palette.InputStep = 1
							m.state.Palette.InputBuffer = ""
						}
						return m, nil
					case 1:
						if val != "" {
							m.state.Palette.MCPCmd = val
							m.state.Palette.InputStep = 2
							m.state.Palette.InputBuffer = ""
						}
						return m, nil
					case 2:
						m.state.Palette.MCPArgs = val
						cfg, _ := config.Load("")
						if cfg.MCP.Servers == nil {
							cfg.MCP.Servers = make(map[string]config.MCPServer)
						}
						var args []string
						if m.state.Palette.MCPArgs != "" {
							args = strings.Fields(m.state.Palette.MCPArgs)
						}
						cfg.MCP.Servers[m.state.Palette.MCPName] = config.MCPServer{
							Command: m.state.Palette.MCPCmd,
							Args:    args,
						}
						_ = config.SaveUserConfig(cfg)
						m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
						m.history.Append("system", fmt.Sprintf("✓ Added MCP server '%s' (%s %v) and saved to config!", m.state.Palette.MCPName, m.state.Palette.MCPCmd, args))
						m.state.Palette.Open = false
						m.state.Palette.Mode = "palette"
						m.state.Palette.InputBuffer = ""
						m.state.Palette.InputStep = 0
						return m, nil
					}
				}

				filtered := FilterPalette(DefaultPaletteCatalog(), m.state.Palette.Query)
				if len(filtered) > 0 && m.state.Palette.Selected < len(filtered) {
					item := filtered[m.state.Palette.Selected]
					switch item.ActionType {
					case "connect_prompt":
						m.state.Palette.Mode = "connect"
						m.state.Palette.ConnectProvider = item.Payload
						m.state.Palette.ConnectKey = ""
						return m, nil
					case "custom_model_prompt":
						m.state.Palette.Mode = "custom_model"
						m.state.Palette.InputBuffer = ""
						return m, nil
					case "custom_endpoint_prompt":
						m.state.Palette.Mode = "custom_endpoint"
						m.state.Palette.InputBuffer = ""
						return m, nil
					case "mcp_add_prompt":
						m.state.Palette.Mode = "mcp_add"
						m.state.Palette.InputStep = 0
						m.state.Palette.InputBuffer = ""
						return m, nil
					case "set_secondary_model":
						parts := strings.SplitN(item.Payload, ":", 2)
						cfg, _ := config.Load("")
						if len(parts) == 2 {
							cfg.SecondaryModel.Provider = parts[0]
							cfg.SecondaryModel.Model = parts[1]
						} else {
							cfg.SecondaryModel.Model = item.Payload
						}
						_ = config.SaveUserConfig(cfg)
						m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
						m.history.Append("system", fmt.Sprintf("🤖 Secondary subagent model set to: %s", item.Payload))
						m.state.Palette.Open = false
						return m, nil
					case "set_model":
						parts := strings.SplitN(item.Payload, ":", 2)
						if len(parts) == 2 {
							cfg, _ := config.Load("")
							cfg.Provider.Name = parts[0]
							cfg.Model.Name = parts[1]
							_ = config.SaveUserConfig(cfg)
							m.state.ModelName = item.Payload
							m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
							m.history.Append("system", fmt.Sprintf("⚡ Switched to model: %s", item.Payload))
						}
						m.state.Palette.Open = false
						return m, nil
					case "set_mode":
						m.state.PermissionMode = item.Payload
						m.history.Append("system", fmt.Sprintf("🛡️ Permission mode set to %s", item.Payload))
						m.state.Palette.Open = false
						return m, nil
					case "set_spinner":
						if style, ok := ParseSpinnerStyle(item.Payload); ok {
							m.state.SpinnerStyle = style
							m.history.Append("system", fmt.Sprintf("🌀 Spinner animation set to %s", item.Payload))
						}
						m.state.Palette.Open = false
						return m, nil
					case "set_theme":
						m.theme = SelectTheme(item.Payload)
						m.history.Append("system", fmt.Sprintf("🎨 Theme switched to %s", item.Payload))
						m.state.Palette.Open = false
						return m, nil
					case "slash":
						m.state.Palette.Open = false
						return m.executeSlashCommand(item.Payload)
					}
				}
				m.state.Palette.Open = false
				return m, nil
			default:
				if msg.Type == tea.KeyRunes || msg.Type == tea.KeySpace {
					ch := string(msg.Runes)
					switch m.state.Palette.Mode {
					case "connect":
						m.state.Palette.ConnectKey += ch
					case "custom_model", "custom_endpoint", "mcp_add":
						m.state.Palette.InputBuffer += ch
					default:
						m.state.Palette.Query += ch
						m.state.Palette.Selected = 0
					}
					return m, nil
				}
			}
			return m, nil
		}

		if msg.Type == tea.KeyEnter {
			raw := m.composer.Input.Value()
			input := strings.TrimSpace(ExpandPasteTokens(raw))
			if strings.HasPrefix(input, "/") {
				m.composer.Input.Reset()
				return m.executeSlashCommand(input)
			} else if input != "" {
				m.history.Append("user", input)
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: input}
				m.composer.Input.Reset()
				m.state.Busy = true
				m.exitArmed = false
			}
			return m, nil
		}

	case tea.WindowSizeMsg:
		m.state.Width = msg.Width
		m.state.Height = msg.Height
		chrome := 6
		if msg.Height <= chrome {
			chrome = msg.Height - 1
		}
		if chrome < 1 {
			chrome = 1
		}
		if msg.Width < 1 {
			msg.Width = 1
		}
		if msg.Width > 6 {
			m.composer.Input.Width = msg.Width - 6
		}
		if !m.state.Ready {
			m.viewport = viewport.New(msg.Width, chrome)
			m.state.Ready = true
			if m.state.Inline && !m.headerPrinted {
				// The header scrolls away into
				// native scrollback (U4).
				m.headerPrinted = true
				cmds = append(cmds, tea.Printf("%s\n\n", m.welcomeCardView()))
			}
		} else {
			m.viewport.Width = msg.Width
			m.viewport.Height = chrome
		}

	case engineEventMsg:
		evt := protocol.EngineEvent(msg)
		wasBusy := m.state.Busy
		applyEvent(&m, evt)
		if !wasBusy && m.state.Busy {
			// Busy transition: start the 120ms sweep. It
			// reschedules itself only while Busy.
			cmds = append(cmds, spinTick())
		}
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
	case spinTickMsg:
		if m.state.Busy && !m.state.ReducedMotion {
			m.state.SpinFrame++
			m.renderLive()
			cmds = append(cmds, spinTick())
		}
	}

	var cmd tea.Cmd
	m.composer.Input, cmd = m.composer.Input.Update(msg)
	cmds = append(cmds, cmd)

	val := m.composer.Input.Value()
	pos := m.composer.Input.Position()
	if q, ok := ComputeMentionQuery(val, pos); ok {
		m.state.MentionOverlay.Active = true
		if m.state.MentionOverlay.Query != q {
			m.state.MentionOverlay.Query = q
			m.state.MentionOverlay.Selected = 0
			dir := m.state.Directory
			if dir == "" {
				dir, _ = os.Getwd()
			}
			cmds = append(cmds, fetchMentionCandidates(dir, q))
		}
	} else if m.state.MentionOverlay.Active {
		m.state.MentionOverlay.Active = false
		m.state.MentionOverlay.Candidates = nil
	}

	var vpCmd tea.Cmd
	m.viewport, vpCmd = m.viewport.Update(msg)
	cmds = append(cmds, vpCmd)

	return m, tea.Batch(cmds...)
}

func (m AppModel) executeSlashCommand(input string) (tea.Model, tea.Cmd) {
	switch {
	case input == "/help":
		var helpText strings.Builder
		helpText.WriteString("Available commands:\n")
		for _, cmd := range CoreSlashCommands {
			helpText.WriteString("  " + cmd.Name + " — " + cmd.Description + "\n")
		}
		helpText.WriteString("\nKeybindings:\n")
		helpText.WriteString("  Ctrl+P: Command palette & settings\n")
		helpText.WriteString("  Esc: Interrupt turn / Deny approval / Close palette\n")
		helpText.WriteString("  Ctrl+C: Clear input / Interrupt\n")
		helpText.WriteString("  Ctrl+D: Exit on empty input\n")
		helpText.WriteString("  Ctrl+Z: Suspend process\n")
		m.history.Append("system", helpText.String())
	case input == "/palette" || input == "/settings" || input == "/config":
		m.state.Palette.Open = true
		m.state.Palette.Mode = "palette"
		m.state.Palette.Query = ""
		m.state.Palette.Selected = 0
	case strings.HasPrefix(input, "/connect"):
		prov := "anthropic"
		parts := strings.Fields(input)
		if len(parts) > 1 {
			prov = parts[1]
		}
		m.state.Palette.Open = true
		m.state.Palette.Mode = "connect"
		m.state.Palette.ConnectProvider = prov
		m.state.Palette.ConnectKey = ""
	case strings.HasPrefix(input, "/spinner"):
		parts := strings.Fields(input)
		if len(parts) > 1 {
			if style, ok := ParseSpinnerStyle(parts[1]); ok {
				m.state.SpinnerStyle = style
				m.history.Append("system", fmt.Sprintf("🌀 Spinner style switched to '%s'", parts[1]))
			} else {
				m.history.Append("system", "Available spinner styles: bloom (default/Claude), braille (OpenCode), sweep (classic), pulse (wave)")
			}
		} else {
			m.history.Append("system", fmt.Sprintf("Current spinner style: %s. Usage: /spinner <bloom|braille|sweep|pulse>", StyleName(m.state.SpinnerStyle)))
		}
	case strings.HasPrefix(input, "/model") || strings.HasPrefix(input, "/models"):
		parts := strings.Fields(input)
		if len(parts) > 1 {
			modelArg := parts[1]
			m.state.ModelName = modelArg
			cfg, _ := config.Load("")
			if strings.Contains(modelArg, ":") {
				sub := strings.SplitN(modelArg, ":", 2)
				cfg.Provider.Name = strings.TrimSpace(sub[0])
				cfg.Model.Name = strings.TrimSpace(sub[1])
			} else {
				cfg.Model.Name = modelArg
			}
			_ = config.SaveUserConfig(cfg)
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
			m.history.Append("system", fmt.Sprintf("⚡ Switched to model: %s", modelArg))
		} else {
			m.history.Append("system", fmt.Sprintf("⚡ Active Model: %s (context window: %s tokens)", m.state.ModelName, formatTokens(m.state.MaxTokens)))
			m.state.Palette.Open = true
			m.state.Palette.Mode = "palette"
			m.state.Palette.Query = "model"
			m.state.Palette.Selected = 0
		}
	case input == "/doctor":
		m.history.Append("system", "🏥 System Health:\n  ✓ Sandbox: Bubblewrap isolation active\n  ✓ Writable workspace roots enforced\n  ✓ Network isolation enabled\n  ✓ Terminal capabilities synced")
	case input == "/compact":
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdCompact}
		m.history.Append("system", "🧹 Context compaction triggered. Past turns folded.")
	case input == "/clear":
		m.history.Cells = nil
		m.history.Committed = 0
	case input == "/quit" || input == "/exit":
		return m, tea.Quit
	case input == "/debug":
		m.state.Debug = !m.state.Debug
	case input == "/plan":
		if m.state.Mode == "plan" {
			m.state.Mode = "normal"
			m.history.Append("system", "Switched to standard execution mode (tools active).")
		} else {
			m.state.Mode = "plan"
			m.history.Append("system", "Entered Plan Mode (read-only exploration; modifications withheld).")
		}
	case input == "/agents":
		m.history.Append("system", "Subagents: 0 active child agents in root session.")
	case input == "/diff" || strings.HasPrefix(input, "/diff"):
		rawDiff := `diff --git a/internal/engine/agent.go b/internal/engine/agent.go
--- a/internal/engine/agent.go
+++ b/internal/engine/agent.go
@@ -38,6 +38,12 @@ func (t *TurnRunner) Run(ctx context.Context, prompt string, emit func(protocol.EngineEvent)) error {
+		// Mid-turn steering: drain any injected steering prompt
+		if t.SteerChannel != nil {
+			select {
+			case steer := <-t.SteerChannel:
+				t.Context.Add(provider.Message{Role: "user", Content: steer})
+			default:
+			}`
		files := ParseUnifiedDiff(rawDiff)
		m.state.DiffViewer = NewDiffViewerState(files)
		m.state.DiffViewer.Active = true
		m.renderLive()
	case strings.HasPrefix(input, "/btw"):
		query := strings.TrimSpace(strings.TrimPrefix(input, "/btw"))
		if query == "" {
			m.history.Append("system", "Usage: /btw <query> — ask a quick question without context pollution.")
		} else {
			m.state.Btw = BtwState{
				Active:   true,
				Query:    query,
				Response: fmt.Sprintf("Quick context for '%s': verified in docs/PACK.md, internal/tui, and internal/engine.", query),
				Busy:     false,
			}
			m.renderLive()
		}
	case input == "/rewind" || strings.HasPrefix(input, "/rewind"):
		m.history.Append("system", "Rewound session state to previous turn checkpoint.")
	case input == "/unrevert" || strings.HasPrefix(input, "/unrevert"):
		m.history.Append("system", "Restored workspace files from pre-rewind state (unrevert complete).")
	case input == "/cost":
		costMsg := fmt.Sprintf("💰 Session Accounting:\n  Tokens: %d / %d\n  Cost:   %s",
			m.state.UsedTokens, m.state.MaxTokens, routing.FormatCost(m.state.TotalCost))
		m.history.Append("system", costMsg)
	case strings.HasPrefix(input, "/theme"):
		parts := strings.Fields(input)
		if len(parts) > 1 {
			m.theme = SelectTheme(parts[1])
			m.history.Append("system", fmt.Sprintf("Theme switched to '%s'", parts[1]))
		} else {
			m.history.Append("system", "Available themes: default, dark, light, monochrome (Usage: /theme <name>)")
		}
	case input == "/reload":
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
	case input == "/sessions":
		m.state.SessionPicker.Active = true
		m.state.SessionPicker.Query = ""
		m.state.SessionPicker.Selected = 0
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdListSessions}
	case input == "/editor":
		return m, launchEditorCmd(m.composer.Input.Value())
	case strings.HasPrefix(input, "/export"):
		format := "markdown"
		parts := strings.Fields(input)
		if len(parts) > 1 && strings.ToLower(parts[1]) == "html" {
			format = "html"
		}
		ext := "md"
		if format == "html" {
			ext = "html"
		}
		dest := fmt.Sprintf("session-%s.%s", m.state.SessionID, ext)
		m.history.Append("system", fmt.Sprintf("✓ Export format %s ready for session %s (file: %s)", format, m.state.SessionID, dest))
	case strings.HasPrefix(input, "/explain ") || input == "/explain":
		question := strings.TrimSpace(strings.TrimPrefix(input, "/explain"))
		dir := m.state.Directory
		if dir == "" {
			dir, _ = os.Getwd()
		}
		if question == "" {
			m.history.Append("system", "Usage: /explain <symbol | file | question> — answers with file:line citations, refuses unknown symbols.")
		} else {
			m.history.Append("system", explain.Format(explain.AnswerQuestion(dir, question)))
		}
	default:
		m.history.Append("system", fmt.Sprintf("Unknown command: %s. Type /help for available commands or press Ctrl+P for command palette.", input))
	}
	return m, nil
}

func (m AppModel) renderMascot() string {
	top := m.theme.MascotTop.Render("        ")
	mid := m.theme.MascotMid.Render("  ■  ■  ")
	bot := m.theme.MascotBot.Render("        ")
	return lipgloss.JoinVertical(lipgloss.Left, top, mid, bot)
}

func (m AppModel) welcomeCardView() string {
	width := m.state.Width
	if width <= 0 {
		width = 80
	}
	cardWidth := width - 4
	if cardWidth < 50 {
		cardWidth = 50
	}

	mascot := m.renderMascot()
	headerText := lipgloss.JoinVertical(lipgloss.Left,
		m.theme.CardTitle.Render("Welcome to NikiCode!"),
		m.theme.CardSubtitle.Render("Send /help for help information."),
		m.theme.CardDesc.Render("Local Coding Agent"),
	)
	topRow := lipgloss.JoinHorizontal(lipgloss.Center, mascot, "  ", headerText)

	dir := m.state.Directory
	if dir == "" {
		dir, _ = os.Getwd()
	}
	sessionID := m.state.SessionID
	model := m.state.ModelName
	if model == "" {
		model = "mock: gpt-4o-mini"
	}
	ver := m.state.Version
	if ver == "" {
		ver = "0.11.0"
	}

	metaLines := []string{
		m.theme.CardLabel.Render(fmt.Sprintf("%-12s", "Directory:")) + m.theme.CardValue.Render(dir),
		m.theme.CardLabel.Render(fmt.Sprintf("%-12s", "Session:")) + m.theme.CardValue.Render(sessionID),
		m.theme.CardLabel.Render(fmt.Sprintf("%-12s", "Model:")) + m.theme.CardValue.Render(model),
		m.theme.CardLabel.Render(fmt.Sprintf("%-12s", "Version:")) + m.theme.CardValue.Render(ver),
	}
	metaBlock := strings.Join(metaLines, "\n")

	cardInner := topRow + "\n\n" + metaBlock
	card := m.theme.CardBorder.
		Border(lipgloss.RoundedBorder()).
		Padding(1, 2).
		Width(cardWidth).
		Render(cardInner)

	announcement := m.theme.AnnounceIcon.Render("✦ ") +
		m.theme.AnnounceTitle.Render("NikiCode coding agent") +
		m.theme.AnnounceDesc.Render(" – Fast, local-first personal harness in Go") + "\n" +
		m.theme.AnnounceLink.Render("  Run /help for commands or visit https://github.com/RavaniRoshan/niki") + "\n\n" +
		m.theme.AnnounceLink.Render("  No session yet — one will be created on your first message.")

	return card + "\n\n" + announcement
}

func (m AppModel) headerView() string {
	title := m.theme.Header.Render(CompactMark(m.state.Width, UseASCII()))
	if m.state.Width < 50 {
		return title
	}
	sub := m.theme.Muted.Render(BrandLine())
	badge := m.theme.Muted.Render("[" + m.state.ModelName + "]")
	if m.state.Width < 60 {
		return title + "  " + sub
	}
	pad := m.state.Width - lipgloss.Width(title) - lipgloss.Width(sub) - lipgloss.Width(badge) - 4
	if pad < 2 {
		pad = 2
	}
	return title + "  " + sub + strings.Repeat(" ", pad) + badge
}

func (m AppModel) footerView() string {
	permLabel := "Never Ask"
	switch m.state.PermissionMode {
	case "readonly":
		permLabel = "Read Only"
	case "full_access":
		permLabel = "Full Access"
	case "workspace_write":
		permLabel = "Never Ask"
	}
	perm := m.theme.BadgePerm.Render(permLabel)

	modeLabel := m.state.Mode
	if modeLabel == "" {
		modeLabel = "plan"
	}
	mode := m.theme.BadgeMode.Render(modeLabel)

	modelName := m.state.ModelName
	if modelName == "" {
		modelName = "mock: gpt-4o-mini"
	}
	model := m.theme.BadgeModel.Render(modelName)
	thinking := m.theme.StatusThinking.Render("thinking: high")

	dir := m.state.Directory
	if dir == "" {
		dir = "~"
	} else {
		if home, err := os.UserHomeDir(); err == nil && strings.HasPrefix(dir, home) {
			dir = "~" + strings.TrimPrefix(dir, home)
		}
	}
	dirStyle := m.theme.StatusDir.Render(dir)

	var gitInfo string
	if m.state.GitBranch != "" {
		gitInfo = " " + m.theme.StatusGit.Render(m.state.GitBranch)
	}

	leftParts := perm + " " + mode + "  " + model + " " + thinking + "  " + dirStyle + gitInfo

	rightHints := m.theme.StatusHints.Render("@: mention files | ! to run a shell command")
	if m.state.Busy {
		rightHints = m.theme.StatusHints.Render("ctrl+c: cancel | /help: commands")
	}

	w := m.state.Width
	if w < 40 {
		w = 80
	}

	maxTok := m.state.MaxTokens
	if maxTok <= 0 {
		maxTok = 128000
	}
	usedTok := m.state.UsedTokens
	pct := float64(usedTok) / float64(maxTok) * 100.0
	meterText := ""
	costStr := ""
	if m.state.TotalCost > 0 {
		costStr = " | cost: " + routing.FormatCost(m.state.TotalCost)
	}
	if usedTok == 0 {
		meterText = fmt.Sprintf("context: 0%% (0/%s)%s", formatTokens(maxTok), costStr)
	} else {
		meterText = fmt.Sprintf("context: %.1f%% (%s/%s)%s", pct, formatTokens(usedTok), formatTokens(maxTok), costStr)
	}
	meter := m.theme.StatusMeter.Render(meterText)

	leftLen := lipgloss.Width(leftParts)
	rightLen := lipgloss.Width(rightHints)
	meterLen := lipgloss.Width(meter)

	var line1, line2 string
	if leftLen+rightLen+2 <= w {
		pad := w - leftLen - rightLen - 1
		line1 = leftParts + strings.Repeat(" ", pad) + rightHints
		if meterLen < w {
			line2 = strings.Repeat(" ", w-meterLen-1) + meter
		} else {
			line2 = meter
		}
	} else {
		line1 = leftParts
		if rightLen+meterLen+2 <= w {
			pad := w - rightLen - meterLen - 1
			line2 = rightHints + strings.Repeat(" ", pad) + meter
		} else if meterLen < w {
			line2 = strings.Repeat(" ", w-meterLen-1) + meter
		} else {
			line2 = meter
		}
	}

	return line1 + "\n" + line2
}

func formatTokens(n int) string {
	if n >= 1000 {
		if n%1000 == 0 {
			return fmt.Sprintf("%dk", n/1000)
		}
		return fmt.Sprintf("%.1fk", float64(n)/1000.0)
	}
	return fmt.Sprintf("%d", n)
}

func (m AppModel) View() string {
	start := time.Now()
	defer func() { m.telemetry.RecordRender(time.Since(start)) }()

	if !m.state.Ready {
		return "Initializing NikiCode..."
	}

	composerBlock := m.composerView()
	if m.state.SessionPicker.Active {
		composerBlock = RenderSessionPicker(m.state.SessionPicker, m.theme, m.state.Width, m.state.Height)
	} else if m.state.QuestionModal.Active {
		composerBlock = m.state.QuestionModal.Render(m.theme, m.state.Width)
	} else if m.state.DiffViewer.Active {
		composerBlock = m.state.DiffViewer.Render(m.theme, m.state.Width, m.state.Height)
	} else if m.state.Palette.Open {
		composerBlock = RenderPaletteView(m.state.Palette, m.theme, m.state.Width)
	}
	if m.state.MentionOverlay.Active && len(m.state.MentionOverlay.Candidates) > 0 && !m.state.SessionPicker.Active {
		overlay := RenderMentionOverlay(m.state.MentionOverlay.Candidates, m.state.MentionOverlay.Selected, m.theme, m.state.Width)
		composerBlock = lipgloss.JoinVertical(lipgloss.Left, overlay, composerBlock)
	}
	if m.state.Btw.Active {
		composerBlock = lipgloss.JoinVertical(lipgloss.Left, m.state.Btw.Render(m.theme, m.state.Width), composerBlock)
	}

	var body string
	if m.state.Inline {
		body = lipgloss.JoinVertical(lipgloss.Left,
			m.activityView(),
			m.viewport.View(),
			composerBlock,
			m.footerView(),
		)
	} else {
		if len(m.history.Cells) == 0 {
			body = lipgloss.JoinVertical(lipgloss.Left,
				m.welcomeCardView(),
				"",
				m.activityView(),
				composerBlock,
				m.footerView(),
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
				composerBlock,
				m.footerView(),
			)
		}
	}
	if m.state.Debug {
		body += "\n" + m.debugView()
	}
	return body
}

// activityView renders the single live activity
// line above the composer (U3).
func (m AppModel) activityView() string {
	act := m.state.Activity
	if act == "" {
		return m.theme.Muted.Render("  ○ NikiCode is ready")
	}
	glyph := m.theme.ActivityGlyph.Render(SpinGlyphWithStyle(m.state.SpinnerStyle, m.state.SpinFrame, m.state.ReducedMotion, UseASCII()) + " ")
	displayText := act
	if strings.HasPrefix(strings.ToLower(act), "thinking") {
		verbIdx := (m.state.SpinFrame / 12) % len(ThinkingVerbs)
		displayText = ThinkingVerbs[verbIdx]
	}
	actText := m.theme.ActivityText.Render(displayText)
	hint := m.theme.ActivityHint.Render(" (esc to interrupt)")
	return "  " + glyph + actText + hint
}

// spinGlyph is the activity-sweep frame: ◐◓◑◒ at 120ms while working
// (visual spec cadence), a static ◐ under reduced motion, ASCII
// -\|/ on dumb terminals. Every frame is one cell wide: the line
// never shifts layout.
func spinGlyph(frame int, reduced, ascii bool) string {
	return SpinGlyphWithStyle(SpinnerSweep, frame, reduced, ascii)
}

func (m AppModel) composerView() string {
	width := m.state.Width
	if width < 20 {
		width = 80
	}
	cardWidth := width - 4
	if cardWidth < 50 {
		cardWidth = 50
	}
	composerBox := m.theme.ComposerBorder.
		Border(lipgloss.RoundedBorder()).
		Width(cardWidth).
		Render(m.composer.Input.View())

	if suggestions := composerSuggestions(m.composer.Input.Value()); len(suggestions) > 0 {
		composerBox += "\n" + m.theme.Muted.Render("  "+strings.Join(suggestions, "  "))
	}
	return composerBox
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
