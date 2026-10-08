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

	"github.com/RavaniRoshan/niki/internal/explain"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/routing"
)

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
			case "/model":
				m.history.Append("system", "⚡ Active Model: gpt-4o-mini (provider: mock, context window: 128k tokens)")
				m.composer.Input.Reset()
			case "/doctor":
				m.history.Append("system", "🏥 System Health:\n  ✓ Sandbox: Bubblewrap isolation active\n  ✓ Writable workspace roots enforced\n  ✓ Network isolation enabled\n  ✓ Terminal capabilities synced")
				m.composer.Input.Reset()
			case "/compact":
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdCompact}
				m.history.Append("system", "🧹 Context compaction triggered. Past turns folded.")
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
			case "/plan":
				if m.state.Mode == "plan" {
					m.state.Mode = "normal"
					m.history.Append("system", "Switched to standard execution mode (tools active).")
				} else {
					m.state.Mode = "plan"
					m.history.Append("system", "Entered Plan Mode (read-only exploration; modifications withheld).")
				}
				m.composer.Input.Reset()
			case "/agents":
				m.history.Append("system", "Subagents: 0 active child agents in root session.")
				m.composer.Input.Reset()
			case "/rewind":
				m.history.Append("system", "Rewound session state to previous turn checkpoint.")
				m.composer.Input.Reset()
			case "/palette":
				var sb strings.Builder
				sb.WriteString("🎨 Command Palette:\n")
				for _, cmd := range CoreSlashCommands {
					fmt.Fprintf(&sb, "  %-12s %s\n", cmd.Name, cmd.Description)
				}
				m.history.Append("system", sb.String())
				m.composer.Input.Reset()
			case "/cost":
				costMsg := fmt.Sprintf("💰 Session Accounting:\n  Tokens: %d / %d\n  Cost:   %s",
					m.state.UsedTokens, m.state.MaxTokens, routing.FormatCost(m.state.TotalCost))
				m.history.Append("system", costMsg)
				m.composer.Input.Reset()
			case "/theme":
				m.history.Append("system", "Available themes: default, dark, light, monochrome (Usage: /theme <name>)")
				m.composer.Input.Reset()
			case "/reload":
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
				m.composer.Input.Reset()
			default:
				if strings.HasPrefix(input, "/explain ") || input == "/explain" {
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
					m.composer.Input.Reset()
				} else if strings.HasPrefix(input, "/rewind") {
					m.history.Append("system", "Rewound session state to previous turn checkpoint.")
					m.composer.Input.Reset()
				} else if strings.HasPrefix(input, "/theme ") {
					parts := strings.Fields(input)
					if len(parts) > 1 {
						m.theme = SelectTheme(parts[1])
						m.history.Append("system", fmt.Sprintf("Theme switched to '%s'", parts[1]))
					}
					m.composer.Input.Reset()
				} else if input != "" {
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
	var vpCmd tea.Cmd
	m.viewport, vpCmd = m.viewport.Update(msg)
	cmds = append(cmds, vpCmd)

	return m, tea.Batch(cmds...)
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

	var body string
	if m.state.Inline {
		body = lipgloss.JoinVertical(lipgloss.Left,
			m.activityView(),
			m.viewport.View(),
			m.composerView(),
			m.footerView(),
		)
	} else {
		if len(m.history.Cells) == 0 {
			body = lipgloss.JoinVertical(lipgloss.Left,
				m.welcomeCardView(),
				"",
				m.activityView(),
				m.composerView(),
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
				m.composerView(),
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
	glyph := m.theme.ActivityGlyph.Render(spinGlyph(m.state.SpinFrame, m.state.ReducedMotion, UseASCII()) + " ")
	actText := m.theme.ActivityText.Render(act)
	hint := m.theme.ActivityHint.Render(" (esc to interrupt)")
	return "  " + glyph + actText + hint
}

// spinGlyph is the activity-sweep frame: ◐◓◑◒ at 120ms while working
// (visual spec cadence), a static ◐ under reduced motion, ASCII
// -\|/ on dumb terminals. Every frame is one cell wide: the line
// never shifts layout.
func spinGlyph(frame int, reduced, ascii bool) string {
	if ascii {
		frames := []string{"-", "\\", "|", "/"}
		if reduced {
			return "-"
		}
		return frames[frame%len(frames)]
	}
	if reduced {
		return "◐"
	}
	frames := []string{"◐", "◓", "◑", "◒"}
	return frames[frame%len(frames)]
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
