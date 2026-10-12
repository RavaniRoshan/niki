package tui

import (
	"fmt"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/RavaniRoshan/niki/internal/routing"
)

// Workbench layout (docs/tui/WORKBENCH.md): full-screen, three-rail,
// task-first. Header, left rail (plan, agents), centre (run trace +
// live area), right rail (context, cost), composer, status bar.
// Every element is a pure function of AppState and size, and every
// element is driven by a real Op/Event — an element whose event has
// not fired is absent (P20/P21), never placeholder chrome.

// nibArt is the one art module for the mascot: a constant-width
// 3-row orb whose eye and body color come from the caller's state
// mapping. Width never changes between states (no jitter).
func nibArt(eye, asciiEye string, style lipgloss.Style) string {
	if UseASCII() {
		return lipgloss.JoinVertical(lipgloss.Left,
			style.Render("  _____ "),
			style.Render(" /  "+asciiEye+"  \\"),
			style.Render(" \\_____/ "))
	}
	return lipgloss.JoinVertical(lipgloss.Left,
		style.Render(" ▄███▄ "),
		style.Render("███"+eye+"███"),
		style.Render(" ▀███▀ "))
}

// nibState maps the run state machine to the word, the eye, its
// ASCII fallback, and the body style (WORKBENCH.md §4). Color is
// never the only signal: the word and the eye shape both change.
func (m AppModel) nibState() (word, eye, asciiEye string, style lipgloss.Style) {
	switch {
	case m.state.TurnOutcome == "error":
		return "error", "○", "x", m.theme.Error
	case m.state.TurnOutcome == "done":
		return "done", "●", "*", m.theme.Success
	case m.state.TurnOutcome == "interrupted":
		return "interrupted", "◐", "o", m.theme.Muted
	case m.state.Busy:
		return "working", "◑", "o", m.theme.ActivityGlyph
	default:
		return "idle", "◐", "o", m.theme.Accent
	}
}

// wbOrbOneRow is the compact mascot form for the status bar.
func (m AppModel) wbOrbOneRow() string {
	_, eye, asciiEye, style := m.nibState()
	if UseASCII() {
		eye = asciiEye
	}
	return style.Render(eye)
}

// wbOrb is the full 3-row mascot for the header.
func (m AppModel) wbOrb() string {
	_, eye, asciiEye, style := m.nibState()
	return nibArt(eye, asciiEye, style)
}

// wbMode names the workbench mode: plan | act | yolo (from the real
// mode and permission state, never invented).
func (m AppModel) wbMode() string {
	switch {
	case m.state.Mode == "plan" || m.state.PermissionMode == "readonly":
		return "plan"
	case m.state.PermissionMode == "full_access" || m.state.PermissionMode == "yolo" || m.state.PermissionMode == "never":
		return "yolo"
	default:
		return "act"
	}
}

// wbModel names the active model, or "—" when none is configured
// (P22: an unknown is never a fake zero).
func (m AppModel) wbModel() string {
	if m.needsSetup() {
		return dash()
	}
	return m.state.ModelName
}

// wbHeaderLines is the header's row count at the current width.
func (m AppModel) wbHeaderLines() int {
	if m.state.Width >= 80 {
		return 3
	}
	return 1
}

// wbFullHeader renders the wide header: orb beside the identity
// rows (product/version, model/mode, cwd/branch).
func (m AppModel) wbFullHeader(width int) string {
	ver := m.state.Version
	if ver == "" {
		ver = "0.11.0"
	}
	identity := lipgloss.JoinVertical(lipgloss.Left,
		m.theme.CardTitle.Render("NikiCode")+"  "+m.theme.Muted.Render("v"+ver),
		m.theme.Assistant.Render(m.wbModel()+" "+sep()+" mode "+m.wbMode()),
		m.theme.Muted.Render(homeDir(m.state.Directory)+" "+sep()+" "+m.wbBranch()),
	)
	head := lipgloss.JoinHorizontal(lipgloss.Top, m.wbOrb(), "  ", identity)
	return truncLines(head, width)
}

// wbCompactHeader renders the mid-width header: one row.
func (m AppModel) wbCompactHeader(width int) string {
	ver := m.state.Version
	if ver == "" {
		ver = "0.11.0"
	}
	parts := []string{
		"[" + m.wbOrbOneRow() + "] NikiCode v" + ver,
		m.wbModel() + " " + sep() + " " + m.wbMode(),
		homeDir(m.state.Directory) + " " + sep() + " " + m.wbBranch(),
	}
	return truncLines(m.theme.Muted.Render(strings.Join(parts, "  ")), width)
}

func (m AppModel) wbBranch() string {
	if m.state.GitBranch == "" {
		return dash()
	}
	return m.state.GitBranch
}

// wbLeftRail renders the PLAN and AGENTS sections. A section with
// no event behind it is absent (P21): no "PLAN" label before a plan
// event, no "AGENTS" label before a subagent event.
func (m AppModel) wbLeftRail(width int) string {
	var sections []string
	if plan := m.wbPlanSection(width); plan != "" {
		sections = append(sections, plan)
	}
	if agents := m.wbAgentsSection(width); agents != "" {
		sections = append(sections, agents)
	}
	if len(sections) == 0 {
		return ""
	}
	return truncLines(strings.Join(sections, "\n\n"), width)
}

// wbPlanSection renders the live plan checklist from the real plan
// event: each step pending | in_progress | completed.
func (m AppModel) wbPlanSection(width int) string {
	if len(m.state.Plan) == 0 {
		return ""
	}
	var b strings.Builder
	b.WriteString(m.theme.Muted.Render("PLAN") + "\n")
	for i, step := range m.state.Plan {
		glyph := m.renderStateGlyph(planStepState(step.Status))
		num := m.theme.ToolTree.Render(fmt.Sprintf("%d.", i+1))
		text := m.theme.Assistant.Render(truncLines(step.Description, width-4))
		b.WriteString(glyph + " " + num + " " + text + "\n")
	}
	return strings.TrimRight(b.String(), "\n")
}

// planStepState maps a plan step status to the shared row-state
// vocabulary (the glyph/word/ASCII table).
func planStepState(status string) string {
	switch status {
	case "completed":
		return stateDone
	case "active":
		return stateRunning
	case "failed":
		return stateFailed
	default:
		return "queued"
	}
}

// wbAgentsSection renders the live subagent list from the real
// lifecycle events: running | done | failed, keyed by agent id.
func (m AppModel) wbAgentsSection(width int) string {
	if len(m.state.Agents) == 0 {
		return ""
	}
	var b strings.Builder
	b.WriteString(m.theme.Muted.Render("AGENTS") + "\n")
	for _, ag := range m.state.Agents {
		glyph := m.renderStateGlyph(agentRowState(ag.Status))
		id := m.theme.Assistant.Render(truncLines(ag.ID, width-6))
		b.WriteString(glyph + " " + id + "\n")
	}
	return strings.TrimRight(b.String(), "\n")
}

func agentRowState(status string) string {
	switch status {
	case "done":
		return stateDone
	case "failed":
		return stateFailed
	default:
		return stateRunning
	}
}

// wbRightRail renders CONTEXT and COST. CHECKPOINTS is absent: no
// checkpoint event exists yet, so the section is never drawn
// (WORKBENCH.md §7). Unknown instruments read "—" (P22).
func (m AppModel) wbRightRail(width int) string {
	var b strings.Builder
	b.WriteString(m.theme.Muted.Render("CONTEXT") + "\n")
	b.WriteString("  " + m.wbContextText() + "\n\n")
	b.WriteString(m.theme.Muted.Render("COST") + "\n")
	b.WriteString("  " + m.theme.Assistant.Render("turn "+m.wbTurnCost()) + "\n")
	b.WriteString("  " + m.theme.Assistant.Render("session "+m.wbSessionCost()))
	return truncLines(b.String(), width)
}

// wbContextText is the real used/window pair, or "—" before the
// first usage event.
func (m AppModel) wbContextText() string {
	if m.state.UsedTokens <= 0 || m.state.MaxTokens <= 0 {
		return m.theme.Muted.Render(dash())
	}
	return m.theme.Assistant.Render(fmt.Sprintf("%s / %s", formatTokens(m.state.UsedTokens), formatTokens(m.state.MaxTokens)))
}

// wbTurnCost and wbSessionCost render real accumulators or "—"
// (no provider usage yet is not a zero).
func (m AppModel) wbTurnCost() string {
	if m.state.TurnCost <= 0 {
		return dash()
	}
	return routing.FormatCost(m.state.TurnCost)
}

func (m AppModel) wbSessionCost() string {
	if m.state.TotalCost <= 0 {
		return dash()
	}
	return routing.FormatCost(m.state.TotalCost)
}

// wbLiveBlock is the centre's live area: the gate replaces the
// activity line while a decision is pending; the verdict card ends
// a run; the contract strip rides above the composer during a run.
// The activity line is ABSENT when idle (P21) — the status bar's
// state word carries idle. The block renders at the centre-column
// width (a width-adjusted copy: View stays a pure function of the
// model).
func (m AppModel) wbLiveBlock(width int) string {
	mc := m
	mc.state.Width = width
	var parts []string
	switch {
	case mc.state.Gate.Active:
		parts = append(parts, mc.classicGate())
	case mc.state.Verdict.Active && !mc.state.HideContractUI:
		parts = append(parts, mc.classicVerdictCard())
	default:
		if phrase := mc.activityPhrase(); phrase != "" {
			parts = append(parts, mc.wbActivityLine(phrase))
		}
	}
	if contract := mc.classicContractLine(); contract != "" && !mc.state.Verdict.Active && !mc.state.HideContractUI {
		parts = append(parts, contract)
	}
	return truncLines(strings.Join(parts, "\n"), width)
}

// wbActivityLine is the live action line: sweep glyph, the phrase
// built from the real current action, and the interrupt hint. It
// renders only while something is in flight.
func (m AppModel) wbActivityLine(phrase string) string {
	glyph := m.theme.ActivityGlyph.Render(SpinGlyphWithStyle(SpinnerSweep, m.state.SpinFrame, m.state.ReducedMotion, UseASCII()) + " ")
	actText := m.theme.ActivityText.Render(localizeSep(phrase))
	hint := m.theme.ActivityHint.Render(" " + sep() + " esc pause")
	return "  " + glyph + actText + hint
}

// wbCentreOverlay renders the active panel over the centre column.
// The gate is NOT an overlay: the inline gate block in the live
// area is the workbench's decision UI (one state, one rendering).
func (m AppModel) wbCentreOverlay(width int) string {
	switch {
	case m.state.SessionPicker.Active:
		return truncLines(RenderSessionPicker(m.state.SessionPicker, m.theme, width, m.state.Height), width)
	case m.state.DiffViewer.Active:
		return truncLines(m.state.DiffViewer.Render(m.theme, width, m.state.Height), width)
	case m.state.Palette.Open:
		return truncLines(RenderPaletteView(m.state.Palette, m.theme, width), width)
	case m.state.QuestionModal.Active && !m.state.Gate.Active:
		return truncLines(m.state.QuestionModal.Render(m.theme, width), width)
	case m.state.Btw.Active:
		return truncLines(m.state.Btw.Render(m.theme, width), width)
	}
	if m.state.MentionOverlay.Active && len(m.state.MentionOverlay.Candidates) > 0 {
		return truncLines(RenderMentionOverlay(m.state.MentionOverlay.Candidates, m.state.MentionOverlay.Selected, m.theme, width), width)
	}
	return ""
}

// wbCentre renders the centre column: live area at the foot (per
// the mockup), the scrolling run trace above it.
func (m AppModel) wbCentre(width, height int) string {
	if overlay := m.wbCentreOverlay(width); overlay != "" {
		return overlay
	}
	live := m.wbLiveBlock(width)
	var parts []string
	if vp := m.viewport.View(); strings.TrimSpace(vp) != "" {
		parts = append(parts, vp)
	}
	parts = append(parts, live)
	return truncLines(strings.Join(parts, "\n"), width)
}

// wbComposer renders the input row with the palette hint.
func (m AppModel) wbComposer(width int) string {
	m.composer.Input.Placeholder = m.classicPlaceholder()
	input := m.composer.Input.View()
	prompt := m.theme.PromptPrefix.Render("> ")
	hint := m.theme.StatusHints.Render("mode " + m.wbMode() + " " + sep() + " ctrl+k")
	inW := lipgloss.Width(input)
	hintW := lipgloss.Width(hint)
	if width >= inW+hintW+4 {
		pad := width - inW - hintW - 3
		if pad < 1 {
			pad = 1
		}
		return truncLines(prompt+input+strings.Repeat(" ", pad)+hint, width)
	}
	return truncLines(prompt+input, width)
}

// wbStatusBar renders the mascot + run state, the live counts, the
// interrupt hint (running only), and the registry hints. Counts
// are absent at zero (P21). At 80+ the instruments live in the
// right rail; below 80 the status bar carries them and drops the
// registry hints (the composer row already shows the palette
// hint, and "?" lists every key) — nothing real is ever dropped
// for decoration.
func (m AppModel) wbStatusBar(width int) string {
	word, _, _, _ := m.nibState()
	stateWord := m.theme.Assistant.Render(word)
	left := "[" + m.wbOrbOneRow() + "] " + stateWord + " " + sep() + " " + m.wbModel()
	if n := len(m.state.Agents); n > 0 {
		left += " " + sep() + " " + m.theme.Muted.Render(fmt.Sprintf("agents %d", n))
	}
	if m.state.Busy {
		left += " " + sep() + " " + m.theme.Muted.Render("esc interrupt")
	}
	var right []string
	if m.state.Width < 80 {
		// The right rail is gone: the instruments move here.
		if m.state.UsedTokens > 0 && m.state.MaxTokens > 0 {
			pct := float64(m.state.UsedTokens) / float64(m.state.MaxTokens) * 100
			right = append(right, fmt.Sprintf("ctx %.0f%%", pct))
		}
		if m.state.TotalCost > 0 {
			right = append(right, routing.FormatCost(m.state.TotalCost))
		}
	} else {
		for _, k := range FooterHints(m.hintState()) {
			right = append(right, k.Hint)
		}
	}
	leftRendered := left
	rightRendered := m.theme.StatusHints.Render(strings.Join(right, " "+sep()+" "))
	leftW := lipgloss.Width(leftRendered)
	rightW := lipgloss.Width(rightRendered)
	if leftW+rightW+2 <= width {
		pad := width - leftW - rightW - 1
		if pad < 1 {
			pad = 1
		}
		return truncLines(leftRendered+strings.Repeat(" ", pad)+rightRendered, width)
	}
	return truncLines(leftRendered, width)
}

// wbColumns are the rail widths at the current size. The right rail
// exists only at 80+ columns; below that its instruments move to
// the status bar (WORKBENCH.md §5).
func (m AppModel) wbColumns() (leftW, centreW, rightW int) {
	w := m.state.Width
	switch {
	case w >= 100:
		return 26, w - 26 - 24 - 2, 24
	case w >= 80:
		return 22, w - 22 - 22 - 2, 22
	default:
		return 18, w - 18 - 1, 0
	}
}

// wbMiddleHeight is the height available to the three columns.
func (m AppModel) wbMiddleHeight() int {
	h := m.state.Height - m.wbHeaderLines() - 3 // rule, composer, status
	if h < 1 {
		h = 1
	}
	return h
}

// workbenchChrome is the fixed chrome around the scrolling centre
// (the live area's line count is subtracted per frame).
func (m AppModel) workbenchChrome() int {
	return m.wbHeaderLines() + 3
}

// workbenchSyncViewport sizes the centre viewport and the composer
// to the workbench's geometry. Called from renderLive (the reflow
// seam); View itself stays pure.
func (m *AppModel) workbenchSyncViewport() {
	if m.state.Layout != "workbench" || !m.state.Ready {
		return
	}
	_, centreW, _ := m.wbColumns()
	m.viewport.SetWidth(centreW)
	live := m.wbLiveBlock(centreW)
	liveLines := 0
	if live != "" {
		liveLines = strings.Count(live, "\n") + 1
	}
	h := m.wbMiddleHeight() - liveLines
	if h < 1 {
		h = 1
	}
	m.viewport.SetHeight(h)
	// Leave room on the composer row for the palette hint.
	if m.state.Width > 30 {
		m.composer.Input.SetWidth(m.state.Width - 22)
	} else {
		m.composer.Input.SetWidth(m.state.Width - 6)
	}
}

// workbenchNarrow is the under-50 (or too-short) notice: one line,
// state preserved; a pending gate stays visible so a decision is
// never hidden by the resize.
func (m AppModel) workbenchNarrow() string {
	notice := m.theme.Muted.Render(TruncateTo("resize to 50 or wider. Your run continues.", m.state.Width))
	if m.state.Gate.Active {
		return notice + "\n" + m.classicGate()
	}
	return notice
}

// workbenchView is the Workbench entry point: a full-screen
// (alt-screen) shell. The Workbench never renders inline — a
// three-rail layout cannot live in the scrollback.
func (m AppModel) workbenchView() tea.View {
	if !m.state.Ready {
		return tea.NewView("Initializing NikiCode...")
	}
	if m.state.Width < 50 || m.state.Height < 6 {
		v := tea.NewView(m.workbenchNarrow())
		v.AltScreen = true
		return v
	}
	leftW, centreW, rightW := m.wbColumns()

	var header string
	if m.state.Width >= 80 {
		header = m.wbFullHeader(m.state.Width)
	} else {
		header = m.wbCompactHeader(m.state.Width)
	}
	body := header + "\n" + classicRule(m.state.Width)

	middleH := m.wbMiddleHeight()
	left := padColumn(m.wbLeftRail(leftW), leftW, middleH)
	centre := padColumn(m.wbCentre(centreW, middleH), centreW, middleH)
	var cols []string
	cols = append(cols, left, " ", centre)
	if rightW > 0 {
		cols = append(cols, " ", padColumn(m.wbRightRail(rightW), rightW, middleH))
	}
	body += "\n" + truncLines(lipgloss.JoinHorizontal(lipgloss.Top, cols...), m.state.Width)
	body += "\n" + m.wbComposer(m.state.Width)
	body += "\n" + m.wbStatusBar(m.state.Width)
	if m.state.Debug {
		body += "\n" + m.debugView()
	}
	v := tea.NewView(body)
	v.AltScreen = true
	return v
}

// padColumn pads a column to exactly h lines (blank lines, not
// filler) so the three columns align to one height.
func padColumn(s string, width, h int) string {
	lines := strings.Split(s, "\n")
	for len(lines) < h {
		lines = append(lines, "")
	}
	if len(lines) > h {
		lines = lines[:h]
	}
	for i, l := range lines {
		lines[i] = truncLines(l, width)
	}
	return strings.Join(lines, "\n")
}

// truncLines truncates every line of s to width display cells,
// preserving ANSI escapes (railing content is cut, never wrapped —
// the shell never overflows).
func truncLines(s string, width int) string {
	if width < 1 {
		width = 1
	}
	lines := strings.Split(s, "\n")
	for i, l := range lines {
		if ansiWidth(l) > width {
			lines[i] = TruncateStyledTo(l, width)
		}
	}
	return strings.Join(lines, "\n")
}

// dash is the unknown-value marker: the em dash normally, "--" in
// ASCII/plain mode (every glyph has an ASCII fallback).
func dash() string {
	if UseASCII() {
		return "--"
	}
	return "\u2014"
}
