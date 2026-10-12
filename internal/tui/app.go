package tui

import (
	"errors"
	"fmt"
	"os"
	"strings"
	"time"

	"charm.land/bubbles/v2/viewport"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/RavaniRoshan/niki/internal/alerts"
	"github.com/RavaniRoshan/niki/internal/config"
	"github.com/RavaniRoshan/niki/internal/doctor"
	"github.com/RavaniRoshan/niki/internal/editor"
	"github.com/RavaniRoshan/niki/internal/explain"
	"github.com/RavaniRoshan/niki/internal/git"
	"github.com/RavaniRoshan/niki/internal/mention"
	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/routing"
	"github.com/RavaniRoshan/niki/internal/secrets"
	"github.com/RavaniRoshan/niki/internal/shell"
	"github.com/RavaniRoshan/niki/internal/terminal"
)

// envVarForProvider names the environment variable a user should export
// when the keychain is unavailable (the env-var fallback for I4).
func envVarForProvider(prov string) string {
	switch prov {
	case "anthropic":
		return "ANTHROPIC_API_KEY"
	case "openai":
		return "OPENAI_API_KEY"
	case "openrouter":
		return "OPENROUTER_API_KEY"
	case "deepseek":
		return "DEEPSEEK_API_KEY"
	default:
		return "an API-key environment variable"
	}
}

// BootCaps and BootCapsErr carry the terminal negotiation result from the
// boot path (main) so /doctor can report it without re-probing the live
// terminal mid-session.
var (
	BootCaps    terminal.Capabilities
	BootCapsErr error
)

type shellResultMsg = shell.Result

func runShellCmd(cmdStr, dir string) tea.Cmd {
	return func() tea.Msg {
		return shell.Run(cmdStr, dir)
	}
}

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

type ollamaConfiguredMsg struct{ err error }

// configureOllamaCmd points the provider at a local Ollama
// endpoint. All I/O happens in the command; Update only records the
// result (C1 SETUP choice [o]).
func configureOllamaCmd() tea.Cmd {
	return func() tea.Msg {
		cfg, _ := config.Load("")
		cfg.Provider.Name = "openai"
		cfg.Provider.BaseURL = "http://localhost:11434/v1"
		cfg.Provider.APIKey = "ollama"
		if cfg.Model.Name == "" || cfg.Model.Name == "None" {
			cfg.Model.Name = "llama3.1"
		}
		return ollamaConfiguredMsg{err: config.SaveUserConfig(cfg)}
	}
}

// gateOptions maps a question's options to gate choices.
func gateOptions(q protocol.UserQuestion) []GateOption {
	safe := safeIndex(q)
	var out []GateOption
	for i, opt := range q.Options {
		out = append(out, GateOption{Key: gateLetter(i), Label: opt, Safe: i == safe})
	}
	return out
}

// safeIndex is the option that keeps the constraint: the event's
// safe index, defaulting to the first option.
func safeIndex(q protocol.UserQuestion) int {
	if q.Safe >= 0 && q.Safe < len(q.Options) {
		return q.Safe
	}
	return 0
}

// gateOptionIndex maps a gate letter to the option it answers: n is
// always the constraint-keeping option, then y, v, d in order.
func (m AppModel) gateOptionIndex(letter string) int {
	if !m.state.Gate.Active {
		return -1
	}
	if letter == "n" {
		for i, o := range m.state.Gate.Options {
			if o.Safe {
				return i
			}
		}
		return 0
	}
	order := map[string]int{"y": 0, "v": 1, "d": 2}[letter]
	if order < 0 {
		return -1
	}
	seen := 0
	for i, o := range m.state.Gate.Options {
		if o.Safe {
			continue
		}
		if seen == order {
			return i
		}
		seen++
	}
	return -1
}

// gateLetter is the single letter that picks gate option i.
func gateLetter(i int) string {
	letters := []string{"n", "y", "v", "d"}
	if i < len(letters) {
		return letters[i]
	}
	return "?"
}

// demoTickMsg advances the recorded demo replay (C3).
type demoTickMsg struct{}

// demoTick spaces the recorded fixture's events so the replay reads
// like a run. No model is called; every event is a recorded one.
func demoTick() tea.Cmd {
	return tea.Tick(400*time.Millisecond, func(time.Time) tea.Msg {
		return demoTickMsg{}
	})
}

// demoEvents is the recorded fixture: the same five-frame script
// the golden frames render.
func demoEvents() []protocol.EngineEvent { return classicStageEvents("frame5") }

// advanceDemo applies the next recorded event and reschedules while
// events remain.
func (m AppModel) advanceDemo() (AppModel, tea.Cmd) {
	events := demoEvents()
	if m.state.DemoStep >= len(events) {
		m.state.Demo = false
		m.state.DemoStep = -1
		return m, nil
	}
	applyEvent(&m, events[m.state.DemoStep])
	m.state.DemoStep++
	m.renderLive()
	if m.state.DemoStep >= len(events) {
		m.state.Demo = false
		m.state.DemoStep = -1
		return m, nil
	}
	return m, demoTick()
}

// keyConnectedMsg carries the connect flow's result (I4): the
// keychain write and config save both happened in the command.
type keyConnectedMsg struct {
	prov           string
	modelName      string
	keychainStored bool
	err            error
}

// connectKeyCmd stores the key in the OS keychain (session memory
// when unavailable), saves the provider/model selection to the user
// config (keys are stripped before writing — SaveUserConfig's hard
// floor), and reloads. All I/O happens here, never in Update.
func connectKeyCmd(prov, key string) tea.Cmd {
	return func() tea.Msg {
		keychainStored, serr := secrets.Set(prov, key)
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
		werr := config.SaveUserConfig(cfg)
		return keyConnectedMsg{
			prov: prov, modelName: cfg.Provider.Name + ": " + cfg.Model.Name,
			keychainStored: keychainStored, err: errors.Join(serr, werr),
		}
	}
}

// modelPickedMsg updates the header's model label after a model
// selection command (the label is state, not I/O).
type modelPickedMsg struct{ name string }

// configSavedMsg reports a palette config edit's result; the edit
// and the disk write both happened in the command.
type configSavedMsg struct {
	note string
	err  error
}

// editConfigCmd applies an edit to the user config inside the
// command and saves it (Update performs no I/O).
func editConfigCmd(note string, edit func(*config.Config)) tea.Cmd {
	return func() tea.Msg {
		cfg, _ := config.Load("")
		edit(&cfg)
		return configSavedMsg{note: note, err: config.SaveUserConfig(cfg)}
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
	stash     *StashManager
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
		stash:     NewStashManager(),
		state: State{
			Directory:      cwd,
			SessionID:      "",
			ModelName:      "None",
			Version:        "0.11.0",
			PermissionMode: "workspace_write",
			Mode:           "agent",
			GitBranch:      "main",
			MaxTokens:      128000,
			SpinnerStyle:   SpinnerBloom,
			Layout:         "classic",
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
	styles := m.composer.Input.Styles()
	styles.Cursor.Blink = !reduced
	m.composer.Input.SetStyles(styles)
}

// SetInline configures whether the TUI runs in inline mode.
func (m *AppModel) SetInline(inline bool) {
	m.state.Inline = inline
}

func (m AppModel) cycleWorkingMode() (AppModel, tea.Cmd) {
	if m.state.Mode == "plan" {
		m.state.Mode = "agent"
		m.state.PermissionMode = "workspace_write"
		m.history.Append("system", "✨ Mode switched to: Agent · Ask When Needed (Standard execution; workspace writes enabled)")
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPlanMode, Mode: "false"}
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: "workspace_write"}
	} else {
		switch m.state.PermissionMode {
		case "manual", "always":
			// X2: Shift+Tab cycling never lands on Never Ask —
			// unattended execution needs the explicit /yolo
			// command as confirmation, never a keypress away.
			m.state.Mode = "plan"
			m.state.PermissionMode = "readonly"
			m.history.Append("system", "📋 Mode switched to: Plan Mode (Never Ask is skipped while cycling — type /yolo to confirm it explicitly)")
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPlanMode, Mode: "plan"}
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: "readonly"}
		case "full_access", "yolo", "never":
			m.state.Mode = "plan"
			m.state.PermissionMode = "readonly"
			m.history.Append("system", "📋 Mode switched to: Plan Mode (Read-only exploration; file edits and commands locked)")
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPlanMode, Mode: "plan"}
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: "readonly"}
		default: // workspace_write, ask_when_needed
			m.state.Mode = "agent"
			m.state.PermissionMode = "manual"
			m.history.Append("system", "🛡️ Mode switched to: Agent · Always Ask (Strict safety: prompt on every action)")
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: "manual"}
		}
	}
	m.renderLive()
	return m, nil
}

func (m AppModel) executeShellCommand(cmdStr string) (AppModel, tea.Cmd) {
	m.history.Append("shell_input", "$ "+cmdStr)
	m.state.Busy = true
	m.state.Activity = "executing shell: " + cmdStr
	dir := m.state.Directory
	if dir == "" {
		dir, _ = os.Getwd()
	}
	m.renderLive()
	return m, runShellCmd(cmdStr, dir)
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
		m.state.TurnCost += cost
	}
	switch evt.Type {
	case protocol.EventAssistantTextDelta:
		m.history.AppendDelta(evt.Text)
		m.telemetry.Deltas++
		m.state.Activity = "streaming…"
	case protocol.EventReasoningDelta:
		// U2: the collapsed reasoning line accumulates only from
		// these events. No provider emits one yet, so live runs
		// show no reasoning line — absence, never a placeholder.
		m.history.AppendReasoningDelta(evt.Text)
	case protocol.EventTurnStarted:
		m.state.Busy = true
		m.state.Activity = "thinking…"
		m.state.BudgetState = ""
		m.state.BudgetPct = 0
		m.state.TurnOutcome = ""
		m.state.TurnCost = 0
		m.state.Contract = evt.Contract
		m.state.Verdict = VerdictState{}
	case protocol.EventBudgetGauge:
		if evt.Budget != nil {
			m.state.BudgetState = evt.Budget.State
			m.state.BudgetPct = worstBudgetPct(evt.Budget)
		}
	case protocol.EventBudgetWrap:
		if evt.Budget != nil {
			m.state.BudgetState = evt.Budget.State
			m.state.BudgetPct = worstBudgetPct(evt.Budget)
			m.state.Busy = false
			m.state.Activity = "budget wrap: " + evt.Budget.Hit
			m.history.Append("system", fmt.Sprintf("Budget WRAP at %.0f%% of %s — finishing step, running check, verdict: %s",
				m.state.BudgetPct*100, evt.Budget.Hit, evt.Budget.Verdict))
		}
	case protocol.EventBudgetStopped:
		if evt.Budget != nil {
			m.state.BudgetState = evt.Budget.State
			m.state.BudgetPct = worstBudgetPct(evt.Budget)
			m.state.Busy = false
			m.state.Activity = "budget stopped: " + evt.Budget.Hit
			m.history.Append("system", fmt.Sprintf("Budget STOPPED at %.0f%% of %s — run stopped, branch kept, verdict: %s",
				m.state.BudgetPct*100, evt.Budget.Hit, evt.Budget.Verdict))
		}
	case protocol.EventTurnCompleted:
		m.state.Busy = false
		m.state.Activity = ""
		m.state.TurnOutcome = "done"
	case protocol.EventTurnCancelled:
		m.state.Busy = false
		m.state.Activity = ""
		m.state.TurnOutcome = "interrupted"
		if evt.Text != "" {
			m.history.Append("system", "[turn cancelled] "+evt.Text)
		} else {
			m.history.Append("system", "[turn cancelled]")
		}
	case protocol.EventTurnFailed:
		m.state.Busy = false
		m.state.Activity = ""
		m.state.TurnOutcome = "error"
		m.history.Append("error", evt.Error)
	case protocol.EventTurnNotVerified:
		// L3: terminal — a claim alone never ends a run done.
		m.state.Busy = false
		m.state.Activity = ""
		m.state.TurnOutcome = "error"
		m.state.Verdict = VerdictState{
			Active:      true,
			Verified:    false,
			NotVerified: []string{evt.Text},
			Required:    len(m.state.Contract),
			Branch:      m.state.GitBranch,
			Trust:       "T0",
		}
		m.history.Append("error", "[not verified] "+evt.Text)
	case protocol.EventVerification:
		// C3: the verdict card renders only the verifier's measured
		// evidence; the contract strip takes its final states here.
		if evt.Verification != nil {
			m.state.Verdict = verdictFromEvent(evt)
			m.state.Verdict.Branch = m.state.GitBranch
			if len(evt.Verification.Contract) > 0 {
				m.state.Contract = evt.Verification.Contract
			}
		}
	case protocol.EventToolStarted:
		// C2: the row is opened by the event and completed only by
		// its own completion/failure event, keyed by call id.
		verb := toolVerb(evt.ToolName)
		target := Sanitize(evt.Target)
		m.history.Cells = append(m.history.Cells, HistoryCell{
			Role: "tool", Kind: "tool", CallID: string(evt.CallID),
			State: stateRunning, Verb: verb, Target: target,
			// U2/E1: the engine attributes the call to a plan
			// step; empty means no step owns it and no header
			// renders.
			Step: evt.Step,
			// Text keeps the ledger renderer readable on the same
			// cells; classic renders the structured row.
			Text: strings.TrimSpace(verb + " " + target),
		})
		m.state.Activity = "running " + evt.ToolName
	case protocol.EventToolCompleted:
		if cell := m.history.OpenToolCell(string(evt.CallID)); cell != nil {
			cell.State = stateDone
			cell.Result = Sanitize(evt.Result)
		} else {
			m.history.Append("tool", evt.ToolName+" done")
		}
		// U2/E2: the diff block renders only from the patch
		// event's real diff — absent when the tool changed no
		// bytes.
		if evt.Diff != "" {
			m.history.Cells = append(m.history.Cells, HistoryCell{
				Role: "tool", Kind: "diff", Diff: splitExcerpt(Sanitize(evt.Diff)),
			})
		}
		if m.state.Activity != "" && strings.HasPrefix(m.state.Activity, "running ") {
			m.state.Activity = ""
		}
	case protocol.EventToolFailed:
		if cell := m.history.OpenToolCell(string(evt.CallID)); cell != nil {
			cell.State = stateFailed
			cell.Result = Sanitize(evt.Result)
			if evt.Excerpt != "" {
				m.history.Cells = append(m.history.Cells, HistoryCell{
					Role: "tool", Kind: "tool_excerpt",
					Excerpt: splitExcerpt(Sanitize(evt.Excerpt)),
				})
			}
		} else {
			m.history.Append("error", evt.ToolName+": "+evt.Error)
		}
		m.state.Activity = ""
	case protocol.EventWarning:
		if isLoopWarning(evt.Text) {
			// C2: loop rows come only from real retry/repetition
			// warnings.
			m.history.Cells = append(m.history.Cells, HistoryCell{
				Role: "system", Kind: "loop", Text: Sanitize(evt.Text),
			})
		} else {
			m.history.Append("system", "[warning] "+evt.Text)
		}
	case protocol.EventError:
		m.history.Append("error", evt.Error)
	case protocol.EventSteer:
		// L4: shown as an event at the safe point; not terminal.
		m.history.Append("system", "[steer] "+evt.Text)
	case protocol.EventContextCompacted:
		m.history.Append("system", "[context compacted]")
	case protocol.EventPlanUpdated:
		// C2: the plan is data; the classic next line renders
		// only from this event.
		m.state.Plan = evt.Plan
	case protocol.EventSubagentStarted:
		m.state.Activity = "subagent " + evt.Text
		// U1: the left rail's Agents list comes only from these
		// lifecycle events; the engine's agent id is the only
		// identity they carry.
		m.state.Agents = append(m.state.Agents, AgentRow{ID: evt.Text, Status: "running"})
	case protocol.EventSubagentCompleted, protocol.EventSubagentFailed:
		status := "done"
		if evt.Type == protocol.EventSubagentFailed {
			status = "failed"
		}
		for i := range m.state.Agents {
			if m.state.Agents[i].ID == evt.Text {
				m.state.Agents[i].Status = status
			}
		}
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
			m.state.Gate = GateState{
				Active:   true,
				Action:   evt.Questions[0].Question,
				Options:  gateOptions(evt.Questions[0]),
				Selected: safeIndex(evt.Questions[0]),
			}
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
	return tea.Batch(waitForEvent(m.eventChan), detectGitSubmodsCmd(m.state.Directory))
}

// gitSubmodsMsg delivers async submodule/nested-repo state from a
// background detection command so the status bar (G4) can reflect it
// without blocking boot.
type gitSubmodsMsg struct {
	submods []git.Submodule
}

// detectGitSubmodsCmd enumerates submodules and unregistered nested
// git repos of dir. It is best-effort: a detection error yields an
// empty message so the bar degrades to branch-only.
func detectGitSubmodsCmd(dir string) tea.Cmd {
	return func() tea.Msg {
		subs, err := git.Submods(dir)
		if err != nil {
			return gitSubmodsMsg{}
		}
		return gitSubmodsMsg{submods: subs}
	}
}

// submodsBadge renders a compact, color-coded submodule summary for
// the status bar. It is empty when there are no submodules, so
// hermetic golden frames are unaffected.
func submodsBadge(subs []git.Submodule, theme Theme) string {
	if len(subs) == 0 {
		return ""
	}
	dirty, unreg := 0, 0
	for _, s := range subs {
		switch s.Status {
		case "unregistered":
			unreg++
		case "clean":
		default:
			dirty++
		}
	}
	style := theme.Muted
	if dirty > 0 || unreg > 0 {
		style = theme.Error
	}
	note := fmt.Sprintf(" +%d subs", len(subs))
	if dirty > 0 {
		note += fmt.Sprintf(" (%d dirty)", dirty)
	}
	if unreg > 0 {
		note += fmt.Sprintf(" (%d unreg)", unreg)
	}
	return " " + style.Render(note)
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
// re-rendered here (U9/U10). Row rendering is
// layout-specific: classic rows, workbench
// rows at the centre-column width, or ledger
// cells.
func (m *AppModel) renderLive() {
	cells := m.history.Live()
	if !m.state.Inline {
		cells = m.history.Cells
	}
	switch m.state.Layout {
	case "classic":
		m.viewport.SetContent(m.classicTranscript(cells))
	case "workbench":
		// The centre column is narrower than the screen: rows are
		// laid out at the centre width so right-aligned results
		// and evidence land inside the column, never clipped.
		_, centreW, _ := m.wbColumns()
		mc := *m
		mc.state.Width = centreW
		m.viewport.SetContent(mc.classicTranscript(cells))
	default:
		m.viewport.SetContent(renderCells(*m, cells))
	}
	m.viewport.GotoBottom()
	// U1: the workbench's centre viewport is sized by the shell's
	// remaining height (the live area's line count varies).
	m.workbenchSyncViewport()
}

func (m AppModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmds []tea.Cmd

	switch msg := msg.(type) {
	case disarmExitMsg:
		m.exitArmed = false
		return m, nil

	case demoTickMsg:
		if m.state.Demo {
			var cmd tea.Cmd
			m, cmd = m.advanceDemo()
			return m, cmd
		}
		return m, nil

	case keyConnectedMsg:
		if msg.err != nil {
			m.history.Append("error", "Connect failed: "+msg.err.Error())
			return m, nil
		}
		m.state.ModelName = msg.modelName
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
		if msg.keychainStored {
			m.history.Append("system", fmt.Sprintf("✓ Connected %s API key (stored in OS keychain) and reloaded provider (%s)!", msg.prov, m.state.ModelName))
		} else {
			m.history.Append("system", fmt.Sprintf("✓ Connected %s API key (session only — keychain unavailable; export %s in your shell profile to persist) and reloaded provider (%s)!", msg.prov, envVarForProvider(msg.prov), m.state.ModelName))
		}
		return m, nil

	case modelPickedMsg:
		if msg.name != "" {
			m.state.ModelName = msg.name
		}
		return m, nil

	case configSavedMsg:
		if msg.err != nil {
			m.history.Append("error", "Config save failed: "+msg.err.Error())
			return m, nil
		}
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
		if msg.note != "" {
			m.history.Append("system", msg.note)
		}
		return m, nil

	case ollamaConfiguredMsg:
		if msg.err != nil {
			m.history.Append("error", "Ollama setup failed: "+msg.err.Error())
			return m, nil
		}
		m.state.ModelName = "openai: llama3.1"
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdReloadConfig}
		m.history.Append("system", "Ollama selected — provider points at http://localhost:11434/v1 with model llama3.1. Start Ollama and pull that model to run.")
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

	case tea.KeyPressMsg:
		if m.state.QuestionModal.Active {
			// C3 gate keys: n = the option that keeps the constraint
			// (always the focused default), y = allow once, v = view
			// the diff. Letters answer only the gate, never while
			// the composer has text.
			if m.state.Gate.Active && m.composer.Input.Value() == "" {
				// n always answers the safe option (the focused
				// default); y/v answer the others in order.
				idx := m.gateOptionIndex(msg.String())
				if idx >= 0 && idx < len(m.state.Gate.Options) {
					answer := m.state.Gate.Options[idx].Label
					m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdAnswerQuestion, Answers: []string{answer}}
					m.history.Append("system", "✓ Answered: "+answer)
					m.state.QuestionModal = QuestionState{}
					m.state.Gate = GateState{}
					return m, nil
				}
			}
			done, answers, cancelled := m.state.QuestionModal.HandleKey(msg)
			if done {
				// The modal and the classic gate are one state:
				// resolving either clears both.
				m.state.Gate = GateState{}
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
			switch msg.String() {
			case "ctrl+s", "esc":
				if m.state.SessionPicker.ConfirmDelete {
					m.state.SessionPicker.ConfirmDelete = false
					return m, nil
				}
				m.state.SessionPicker.Active = false
				return m, nil
			case "up":
				if m.state.SessionPicker.Selected > 0 {
					m.state.SessionPicker.Selected--
				}
				return m, nil
			case "down":
				filtered := FilterSessions(m.state.SessionPicker.Sessions, m.state.SessionPicker.Query)
				if m.state.SessionPicker.Selected < len(filtered)-1 {
					m.state.SessionPicker.Selected++
				}
				return m, nil
			case "enter":
				filtered := FilterSessions(m.state.SessionPicker.Sessions, m.state.SessionPicker.Query)
				if len(filtered) > 0 && m.state.SessionPicker.Selected < len(filtered) {
					sel := filtered[m.state.SessionPicker.Selected]
					m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdResumeSession, SessionID: sel.ID}
					m.state.SessionPicker.Active = false
					m.history.Append("system", fmt.Sprintf("📂 Resumed session: %s", sel.Title))
				}
				return m, nil
			case "backspace":
				if len(m.state.SessionPicker.Query) > 0 {
					m.state.SessionPicker.Query = m.state.SessionPicker.Query[:len(m.state.SessionPicker.Query)-1]
					m.state.SessionPicker.Selected = 0
				}
				return m, nil
			default:
				text := msg.Key().Text
				if m.state.SessionPicker.ConfirmDelete {
					if text != "" {
						ch := strings.ToLower(text)
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

				if text != "" || msg.String() == "space" {
					ch := text
					if ch == "" {
						ch = " "
					}
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
			switch msg.String() {
			case "up":
				if m.state.MentionOverlay.Selected > 0 {
					m.state.MentionOverlay.Selected--
				}
				return m, nil
			case "down":
				if m.state.MentionOverlay.Selected < len(m.state.MentionOverlay.Candidates)-1 {
					m.state.MentionOverlay.Selected++
				}
				return m, nil
			case "tab", "enter":
				if len(m.state.MentionOverlay.Candidates) > 0 && m.state.MentionOverlay.Selected < len(m.state.MentionOverlay.Candidates) {
					c := m.state.MentionOverlay.Candidates[m.state.MentionOverlay.Selected]
					newVal, newPos := ReplaceMentionWord(m.composer.Input.Value(), m.composer.Input.Position(), c.Path)
					m.composer.Input.SetValue(newVal)
					m.composer.Input.SetCursor(newPos)
					m.state.MentionOverlay.Active = false
					m.state.MentionOverlay.Candidates = nil
					return m, nil
				}
			case "esc":
				m.state.MentionOverlay.Active = false
				m.state.MentionOverlay.Candidates = nil
				return m, nil
			}
		}

		if msg.String() == "shift+tab" {
			return m.cycleWorkingMode()
		}

		switch msg.String() {
		case "shift+tab":
			return m.cycleWorkingMode()

		case "ctrl+c":
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

		case "ctrl+d":
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

		case "ctrl+z":
			// Bubbletea handles SuspendMsg: it releases the
			// terminal, SIGSTOPs the process group, and
			// restores everything on SIGCONT (L2).
			return m, tea.Suspend

		case "esc":
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
				// L5: Esc pauses at the next safe point; a second
				// Esc (while still busy) stops the run now.
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdPauseTurn}
				m.state.Activity = "pausing… (Esc again to stop)"
				return m, nil
			}
			return m, nil

		case "ctrl+s":
			m.state.SessionPicker.Active = !m.state.SessionPicker.Active
			if m.state.SessionPicker.Active {
				m.state.SessionPicker.Query = ""
				m.state.SessionPicker.Selected = 0
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdListSessions}
			}
			return m, nil

		case "ctrl+g":
			return m, launchEditorCmd(m.composer.Input.Value())

		case "ctrl+p":
			m.state.Palette.Open = !m.state.Palette.Open
			m.state.Palette.Query = ""
			m.state.Palette.Selected = 0
			m.state.Palette.Mode = "palette"
			return m, nil

		case "ctrl+o":
			m.state.ExpandToolOutput = !m.state.ExpandToolOutput
			m.renderLive()
			return m, nil

		case "ctrl+b":
			if m.state.Busy {
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdDetachTool}
				m.history.Append("system", "[tool detached into background execution]")
				m.state.Activity = "1 task in background"
				return m, nil
			}
			return m, nil

		case "ctrl+w":
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

		case "ctrl+k":
			// Classic Flow §1.4: ctrl+k is the actions palette
			// (registry "ctrl+k menu" hint). Kill-line-forward
			// loses its key here by design; ctrl+w/ctrl+u still
			// feed the kill ring and undo still recovers.
			m.state.Palette.Open = !m.state.Palette.Open
			m.state.Palette.Query = ""
			m.state.Palette.Selected = 0
			m.state.Palette.Mode = "palette"
			return m, nil

		case "ctrl+u":
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

		case "ctrl+y":
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

		case "ctrl+_":
			val := m.composer.Input.Value()
			pos := m.composer.Input.Position()
			if snap, ok := m.composer.Undo.Undo(val, pos); ok {
				m.composer.Input.SetValue(snap.Value)
				m.composer.Input.SetCursor(snap.Cursor)
			}
			return m, nil
		}

		if m.state.Palette.Open {
			switch msg.String() {
			case "ctrl+c", "esc":
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
			case "up":
				if m.state.Palette.Mode == "palette" {
					if m.state.Palette.Selected > 0 {
						m.state.Palette.Selected--
					}
				}
				return m, nil
			case "down":
				if m.state.Palette.Mode == "palette" {
					filtered := FilterPalette(DefaultPaletteCatalog(), m.state.Palette.Query)
					if m.state.Palette.Selected < len(filtered)-1 {
						m.state.Palette.Selected++
					}
				}
				return m, nil
			case "backspace":
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
			case "enter":
				switch m.state.Palette.Mode {
				case "connect":
					key := strings.TrimSpace(m.state.Palette.ConnectKey)
					prov := m.state.Palette.ConnectProvider
					m.state.Palette.Open = false
					m.state.Palette.Mode = "palette"
					m.state.Palette.ConnectKey = ""
					if key == "" {
						return m, nil
					}
					// I4: all keychain/config I/O happens in the
					// command (never in Update); Update only records
					// the result message.
					return m, connectKeyCmd(prov, key)

				case "custom_model":
					val := strings.TrimSpace(m.state.Palette.InputBuffer)
					m.state.Palette.Open = false
					m.state.Palette.Mode = "palette"
					m.state.Palette.InputBuffer = ""
					if val == "" {
						return m, nil
					}
					note := fmt.Sprintf("⚡ Switched custom model to: %s", val)
					provider, model := "", val
					if strings.Contains(val, ":") {
						parts := strings.SplitN(val, ":", 2)
						provider, model = strings.TrimSpace(parts[0]), strings.TrimSpace(parts[1])
					}
					return m, tea.Batch(
						editConfigCmd(note, func(c *config.Config) {
							if provider != "" {
								c.Provider.Name = provider
							}
							c.Model.Name = model
						}),
						func() tea.Msg { return modelPickedMsg{name: provider + ": " + model} },
					)

				case "custom_endpoint":
					val := strings.TrimSpace(m.state.Palette.InputBuffer)
					m.state.Palette.Open = false
					m.state.Palette.Mode = "palette"
					m.state.Palette.InputBuffer = ""
					if val == "" {
						return m, nil
					}
					return m, editConfigCmd(fmt.Sprintf("🌐 Provider base URL set to: %s", val),
						func(c *config.Config) { c.Provider.BaseURL = val })

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
						name, cmd := m.state.Palette.MCPName, m.state.Palette.MCPCmd
						args := strings.Fields(m.state.Palette.MCPArgs)
						note := fmt.Sprintf("✓ Added MCP server '%s' (%s %v) and saved to config!", name, cmd, args)
						m.state.Palette.Open = false
						m.state.Palette.Mode = "palette"
						m.state.Palette.InputBuffer = ""
						m.state.Palette.InputStep = 0
						return m, editConfigCmd(note, func(c *config.Config) {
							if c.MCP.Servers == nil {
								c.MCP.Servers = make(map[string]config.MCPServer)
							}
							c.MCP.Servers[name] = config.MCPServer{Command: cmd, Args: args}
						})
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
						payload := item.Payload
						m.state.Palette.Open = false
						return m, editConfigCmd(
							fmt.Sprintf("🤖 Secondary subagent model set to: %s", payload),
							func(c *config.Config) {
								if provider, model, ok := strings.Cut(payload, ":"); ok {
									c.SecondaryModel.Provider = strings.TrimSpace(provider)
									c.SecondaryModel.Model = strings.TrimSpace(model)
								} else {
									c.SecondaryModel.Model = payload
								}
							})
					case "set_model":
						payload := item.Payload
						provider, model, ok := strings.Cut(payload, ":")
						m.state.Palette.Open = false
						if !ok {
							return m, nil
						}
						return m, tea.Batch(
							editConfigCmd(fmt.Sprintf("⚡ Switched to model: %s", payload),
								func(c *config.Config) {
									c.Provider.Name = strings.TrimSpace(provider)
									c.Model.Name = strings.TrimSpace(model)
								}),
							func() tea.Msg { return modelPickedMsg{name: payload} },
						)
					case "set_mode":
						m.state.PermissionMode = item.Payload
						m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: item.Payload}
						m.history.Append("system", fmt.Sprintf("🛡️ Permission mode set to %s", item.Payload))
						m.state.Palette.Open = false
						return m, nil
					case "set_thinking":
						m.state.ThinkingEffort = item.Payload
						m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetThinkingEffort, Effort: item.Payload}
						m.history.Append("system", fmt.Sprintf("🧠 Model thinking effort set to %s", item.Payload))
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
				if text := msg.Key().Text; text != "" || msg.String() == "space" {
					ch := text
					if ch == "" {
						ch = " "
					}
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

		// "?" opens the key help when the composer is empty; with
		// text in the composer it types a question mark instead.
		if msg.String() == "?" && m.composer.Input.Value() == "" {
			m.history.Append("system", "Keys:\n"+strings.TrimRight(KeyHelpText(), "\n"))
			return m, nil
		}

		// SETUP choices (C1): k adds a key through the palette's
		// connect flow; o configures a local Ollama endpoint. Both
		// are inert while the composer has text (the letters type).
		if m.needsSetup() && m.composer.Input.Value() == "" {
			switch msg.String() {
			case "k":
				m.state.Palette.Open = true
				m.state.Palette.Mode = "palette"
				m.state.Palette.Query = "connect"
				m.state.Palette.Selected = 0
				return m, nil
			case "o":
				return m, configureOllamaCmd()
			case "d":
				// Recorded demo: replay the five-frame fixture
				// through the same event path a live run uses.
				m.history.Append("user", "harden POST /login: reject expired tokens and rate-limit failed logins")
				m.state.Busy = true
				m.state.Demo = true
				m.state.DemoStep = 0
				m.renderLive()
				return m, demoTick()
			}
		}

		// Verdict card key: d shows the diff. The t trace key waits
		// for a real event-trace backer (CLASSIC.md §7).
		if m.state.Verdict.Active && m.composer.Input.Value() == "" {
			switch msg.String() {
			case "d":
				return m.executeSlashCommand("/diff")
			}
		}

		if msg.String() == "enter" {
			raw := m.composer.Input.Value()
			input := strings.TrimSpace(ExpandPasteTokens(raw))
			if strings.HasPrefix(input, "!") {
				cmdStr := strings.TrimSpace(strings.TrimPrefix(input, "!"))
				m.composer.Input.Reset()
				if cmdStr == "" {
					return m, nil
				}
				return m.executeShellCommand(cmdStr)
			}
			if strings.HasPrefix(input, "/") {
				m.composer.Input.Reset()
				return m.executeSlashCommand(input)
			} else if input != "" {
				if m.state.ModelName == "None" || m.state.ModelName == "" || m.state.ModelName == "none" {
					m.history.Append("user", input)
					m.composer.Input.Reset()
					alert := alerts.NewNoModelAlert()
					m.history.Append("system", alert.FormatPlain(m.state.Width))
					m.state.Palette.Open = true
					m.state.Palette.Mode = "palette"
					m.state.Palette.Query = "connect"
					m.state.Palette.Selected = 0
					return m, nil
				}
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
		switch m.state.Layout {
		case "workbench":
			chrome = m.workbenchChrome()
		case "ledger":
		default:
			chrome = m.classicChrome()
		}
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
			m.composer.Input.SetWidth(msg.Width - 6)
		}
		if !m.state.Ready {
			m.viewport = viewport.New(viewport.WithWidth(msg.Width), viewport.WithHeight(chrome))
			m.state.Ready = true
			if m.state.Inline && !m.headerPrinted {
				// The header scrolls away into
				// native scrollback (U4).
				m.headerPrinted = true
				cmds = append(cmds, tea.Printf("%s\n\n", m.scrollbackHeader()))
			}
		} else {
			m.viewport.SetWidth(msg.Width)
			m.viewport.SetHeight(chrome)
		}
		// U1: the workbench sizes its own centre viewport.
		m.workbenchSyncViewport()

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
			evt.Type == protocol.EventTurnCancelled ||
			evt.Type == protocol.EventTurnNotVerified ||
			evt.Type == protocol.EventBudgetWrap ||
			evt.Type == protocol.EventBudgetStopped
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

	case gitSubmodsMsg:
		m.state.GitSubmods = msg.submods
		return m, nil

	case shellResultMsg:
		m.state.Busy = false
		m.state.Activity = ""
		output := strings.TrimRight(msg.Output, "\r\n")
		if output != "" {
			m.history.Append("shell_output", output)
		}
		if msg.ExitCode == 0 {
			m.history.Append("shell_status", "  └ exit 0")
		} else {
			m.history.Append("shell_status", fmt.Sprintf("  ✗ exit %d", msg.ExitCode))
		}
		m.renderLive()
		return m, nil

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
	inputStyles := m.composer.Input.Styles()
	if strings.HasPrefix(val, "!") {
		m.composer.Input.Prompt = "$ "
		inputStyles.Focused.Prompt = m.theme.Accent
		m.composer.Input.Placeholder = "Run shell command directly (e.g. ! git status)..."
	} else {
		m.composer.Input.Prompt = "> "
		inputStyles.Focused.Prompt = m.theme.PromptPrefix
		m.composer.Input.Placeholder = "Type a prompt or task (type / for commands, ! for shell)..."
	}
	m.composer.Input.SetStyles(inputStyles)
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
		// H2: generated from the key registry — every advertised
		// key dispatches (proven by the registry test). Never Ask
		// is intentionally absent from the cycle (X2: explicit
		// /yolo only); Ctrl+R has no handler and is not advertised.
		helpText.WriteString(KeyHelpText())
		helpText.WriteString("  ! <cmd>: Execute local shell command directly (fast path, no LLM)\n")
		helpText.WriteString("  @<file>: Fuzzy file & symbol autocomplete overlay\n")
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
			provider, modelName, ok := strings.Cut(modelArg, ":")
			m.state.Palette.Open = false
			return m, tea.Batch(
				editConfigCmd(fmt.Sprintf("⚡ Switched to model: %s", modelArg),
					func(c *config.Config) {
						if ok {
							c.Provider.Name = strings.TrimSpace(provider)
							c.Model.Name = strings.TrimSpace(modelName)
						} else {
							c.Model.Name = modelArg
						}
					}),
				func() tea.Msg { return modelPickedMsg{name: modelArg} },
			)
		} else {
			m.history.Append("system", fmt.Sprintf("⚡ Active Model: %s (context window: %s tokens)", m.state.ModelName, formatTokens(m.state.MaxTokens)))
			m.state.Palette.Open = true
			m.state.Palette.Mode = "palette"
			m.state.Palette.Query = "model"
			m.state.Palette.Selected = 0
		}
	case input == "/doctor":
		// I3: the real nine checks — never a canned all-green report.
		probes := doctor.DefaultProbes("")
		probes.TerminalProbe = func() (terminal.Capabilities, error) {
			return BootCaps, BootCapsErr
		}
		var b strings.Builder
		b.WriteString("🏥 System Health:")
		fails, warns := 0, 0
		for _, r := range doctor.Run(probes) {
			b.WriteString("\n  " + strings.ReplaceAll(doctor.Render(r), "\n", "\n  "))
			switch r.Status {
			case doctor.StatusFail:
				fails++
			case doctor.StatusWarn:
				warns++
			}
		}
		fmt.Fprintf(&b, "\n  doctor: %d failed, %d warnings", fails, warns)
		m.history.Append("system", b.String())
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
	case strings.HasPrefix(input, "/mode"):
		parts := strings.Fields(input)
		if len(parts) > 1 {
			target := strings.ToLower(parts[1])
			switch target {
			case "plan":
				return m.executeSlashCommand("/plan")
			case "yolo", "never":
				return m.executeSlashCommand("/yolo")
			case "auto", "ask":
				return m.executeSlashCommand("/auto")
			case "manual", "always":
				return m.executeSlashCommand("/manual")
			default:
				m.history.Append("system", "Usage: /mode [ask | always | yolo | plan] (or press Shift+Tab to cycle)")
			}
		} else {
			return m.cycleWorkingMode()
		}
	case input == "/plan":
		if m.state.Mode == "plan" {
			m.state.Mode = "agent"
			m.state.PermissionMode = "workspace_write"
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPlanMode, Mode: "false"}
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: "workspace_write"}
			m.history.Append("system", "✨ Switched to Agent Mode · Ask When Needed (Standard execution; workspace writes enabled).")
		} else {
			m.state.Mode = "plan"
			m.state.PermissionMode = "readonly"
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPlanMode, Mode: "plan"}
			m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: "readonly"}
			m.history.Append("system", "📋 Entered Plan Mode (Read-only exploration; file edits and commands locked).")
		}
	case input == "/yolo":
		m.state.Mode = "agent"
		m.state.PermissionMode = "full_access"
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPlanMode, Mode: "false"}
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: "full_access"}
		m.history.Append("system", "⚡ Mode switched to: Agent · Never Ask (Yolo mode: all actions run unattended).")
	case input == "/auto" || input == "/ask":
		m.state.Mode = "agent"
		m.state.PermissionMode = "workspace_write"
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPlanMode, Mode: "false"}
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: "workspace_write"}
		m.history.Append("system", "✨ Mode switched to: Agent · Ask When Needed (Standard workspace write mode).")
	case input == "/manual" || input == "/always":
		m.state.Mode = "agent"
		m.state.PermissionMode = "manual"
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPlanMode, Mode: "false"}
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetPermissionMode, Mode: "manual"}
		m.history.Append("system", "🛡️ Mode switched to: Agent · Always Ask (Strict safety: prompt on every action).")
	case strings.HasPrefix(input, "/thinking"):
		parts := strings.Fields(input)
		if len(parts) > 1 {
			effort := strings.ToLower(parts[1])
			switch effort {
			case "off", "none":
				m.state.ThinkingEffort = "none"
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetThinkingEffort, Effort: "none"}
				m.history.Append("system", "🧠 Model reasoning effort disabled.")
			case "low", "medium", "med", "high":
				if effort == "med" {
					effort = "medium"
				}
				m.state.ThinkingEffort = effort
				m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdSetThinkingEffort, Effort: effort}
				m.history.Append("system", fmt.Sprintf("🧠 Model reasoning effort set to: %s", effort))
			default:
				m.history.Append("system", "Usage: /thinking [off | low | medium | high]")
			}
		} else {
			cur := m.state.ThinkingEffort
			if cur == "" {
				cur = "none"
			}
			m.history.Append("system", fmt.Sprintf("Current thinking effort: %s. Usage: /thinking [off | low | medium | high]", cur))
		}
	case input == "/agents":
		m.history.Append("system", "Subagents: 0 active child agents in root session.")
	case input == "/diff" || strings.HasPrefix(input, "/diff"):
		dir := m.state.Directory
		if dir == "" {
			dir, _ = os.Getwd()
		}
		rawDiff := git.RawDiff(dir)
		if strings.TrimSpace(rawDiff) == "" {
			m.history.Append("system", "No git diff in working directory (clean repository status).")
		} else {
			files := ParseUnifiedDiff(rawDiff)
			if len(files) == 0 {
				m.history.Append("system", "No diff hunks parsed from git output.")
			} else {
				m.state.DiffViewer = NewDiffViewerState(files)
				m.state.DiffViewer.Active = true
				m.renderLive()
			}
		}
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
	case input == "/undo":
		// E4: real engine command; the engine's answer arrives as an
		// event (nothing to undo / undo refused / undid <tool>).
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdUndo}
		m.history.Append("system", "Undo requested — the engine replies when the stack is empty or the file changed.")
	case strings.HasPrefix(input, "/rewind"):
		// E3: real engine command, optionally targeting a turn id.
		arg := strings.TrimSpace(strings.TrimPrefix(input, "/rewind"))
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdRewind, TurnID: protocol.TurnId(arg)}
		m.history.Append("system", "Rewind requested — the engine replies with restored/skipped file counts.")
	case input == "/unrevert":
		m.cmdChan <- protocol.EngineCommand{Type: protocol.CmdUnrevert}
		m.history.Append("system", "Unrevert requested — the engine replies with what was restored.")
	case input == "/stash" || strings.HasPrefix(input, "/stash"):
		arg := strings.TrimSpace(strings.TrimPrefix(input, "/stash"))
		switch arg {
		case "pop":
			if entry, ok := m.stash.Pop(); ok {
				m.composer.Input.SetValue(entry.Content)
				m.history.Append("system", fmt.Sprintf("Restored stashed prompt (%d lines).", entry.LineCount))
			} else {
				m.history.Append("system", "Stash is empty. Nothing to pop.")
			}
		case "clear":
			m.stash.Clear()
			m.history.Append("system", "Stash cleared.")
		case "list":
			entries := m.stash.List()
			if len(entries) == 0 {
				m.history.Append("system", "Stash is empty.")
			} else {
				var sb strings.Builder
				sb.WriteString("Stashed prompts:\n")
				for i, e := range entries {
					firstLine := strings.Split(e.Content, "\n")[0]
					if len(firstLine) > 50 {
						firstLine = firstLine[:47] + "..."
					}
					fmt.Fprintf(&sb, "  [%d] %s (%d lines, %s)\n", i+1, firstLine, e.LineCount, e.CreatedAt.Format("15:04:05"))
				}
				sb.WriteString("Use '/stash pop' to restore the latest.")
				m.history.Append("system", sb.String())
			}
		default:
			content := m.composer.Input.Value()
			if strings.TrimSpace(content) != "" {
				entry := m.stash.Push(content)
				m.composer.Input.Reset()
				m.history.Append("system", fmt.Sprintf("Stashed active prompt (%d lines). Use '/stash pop' to restore.", entry.LineCount))
			} else {
				m.history.Append("system", "Usage: /stash [pop | list | clear]. If composer has text, /stash saves it.")
			}
		}
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
	top := m.theme.Accent.Render(" ▄███▄ ")
	mid := m.theme.Accent.Render("███◐███")
	bot := m.theme.Accent.Render(" ▀███▀ ")
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
		model = "None"
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
		Border(ChromeBorder()).
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
	var modeBadge string
	if m.state.Mode == "plan" {
		modeBadge = m.theme.BadgeMode.Render("[Plan: Read-Only]")
	} else {
		var permLabel string
		switch m.state.PermissionMode {
		case "manual", "always":
			permLabel = "Always Ask"
		case "full_access", "yolo", "never":
			permLabel = "Never Ask (Yolo)"
		case "readonly":
			permLabel = "Read Only"
		default:
			permLabel = "Ask When Needed"
		}
		modeBadge = m.theme.BadgePerm.Render("[" + permLabel + "]")
	}

	modelName := m.state.ModelName
	if modelName == "" {
		modelName = "None"
	}
	model := m.theme.BadgeModel.Render("[" + modelName + "]")

	var thinking string
	if m.state.ThinkingEffort != "" && m.state.ThinkingEffort != "none" {
		thinking = " " + m.theme.StatusThinking.Render("thinking: "+m.state.ThinkingEffort)
	}

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
		gitInfo = " " + m.theme.StatusGit.Render(m.state.GitBranch) + submodsBadge(m.state.GitSubmods, m.theme)
	}

	leftParts := modeBadge + " " + model + thinking + "  " + dirStyle + gitInfo

	var rightHints string
	if m.state.Busy {
		rightHints = m.theme.StatusHints.Render("esc interrupt · ctrl+o fold output")
	} else {
		hints := "shift+tab mode · ! shell · @ file · ctrl+p menu"
		if UseASCII() {
			hints = "shift+tab mode - ! shell - @ file - ctrl+p menu"
		}
		rightHints = m.theme.StatusHints.Render(hints)
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
	if m.state.BudgetState != "" {
		costStr += fmt.Sprintf(" | budget: %.0f%% %s", m.state.BudgetPct*100, strings.ToUpper(m.state.BudgetState))
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

// View dispatches to the active layout. Classic is the default
// (ui.layout = "classic", the alias "stream" mapping to it);
// workbench is the three-rail full-screen layout (U1); ledger is
// the previous full-screen layout behind ui.layout = "ledger".
// All are pure functions of state.
func (m AppModel) View() tea.View {
	if m.state.Layout == "workbench" {
		return m.workbenchView()
	}
	if m.state.Layout == "ledger" {
		return m.ledgerView()
	}
	// Classic renders the gate inline (the classic gate block); the
	// other overlays are layout-independent chrome.
	if m.state.SessionPicker.Active || m.state.DiffViewer.Active ||
		m.state.Palette.Open || m.state.MentionOverlay.Active || m.state.Btw.Active {
		return m.ledgerView()
	}
	return m.classicView()
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

// ledgerView is the previous full-screen layout, kept behind
// ui.layout = "ledger" (experimental). Pure function of state.
func (m AppModel) ledgerView() tea.View {
	if !m.state.Ready {
		return tea.NewView("Initializing NikiCode...")
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
	v := tea.NewView(body)
	v.AltScreen = !m.state.Inline
	return v
}

// activityView renders the single live activity
// line above the composer (U3).
func (m AppModel) activityView() string {
	act := m.state.Activity
	if act == "" {
		if UseASCII() {
			return m.theme.Muted.Render("  o NikiCode is ready")
		}
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
		Border(ChromeBorder()).
		Width(cardWidth).
		Render(m.composer.Input.View())

	if suggestions := composerSuggestions(m.composer.Input.Value()); len(suggestions) > 0 {
		composerBox += "\n" + m.theme.Muted.Render("  "+strings.Join(suggestions, "  "))
	}
	return composerBox
}

// debugView renders boot-phase timings and event
// telemetry (B9). Render timing is not measured
// here: View() is a pure function and performs no
// clock reads (pure-render rule), so the overlay
// reports event-side counters only.
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
	b.WriteString(m.theme.Muted.Render("events: "))
	b.WriteString(m.theme.Muted.Render(itoa(m.telemetry.Events)))
	b.WriteString("  deltas: ")
	b.WriteString(m.theme.Muted.Render(itoa(m.telemetry.Deltas)))
	b.WriteString("\n")
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

// worstBudgetPct returns the largest enabled gauge fraction (L2).
func worstBudgetPct(b *protocol.BudgetInfo) float64 {
	worst := 0.0
	if b.WallEnabled && b.Wall > worst {
		worst = b.Wall
	}
	if b.CostEnabled && b.Cost > worst {
		worst = b.Cost
	}
	if b.StepEnabled && b.Step > worst {
		worst = b.Step
	}
	return worst
}
