package tui

import (
	"fmt"
	"os"
	"strings"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

// Classic layout (docs/ui/CLASSIC.md): orb header, raised user rows,
// one-line tool rows, live area, single-rule composer, registry-driven
// footer. Every element is a pure function of AppState and width.

// SetLayout selects the layout renderer. Unknown values fall back to
// classic (config.NormalizeLayout already guarantees this, but the TUI
// never trusts an unvalidated string). "stream" is the mission name
// for the classic inline flow.
func (m *AppModel) SetLayout(layout string) {
	switch layout {
	case "ledger":
		m.state.Layout = "ledger"
	case "workbench":
		m.state.Layout = "workbench"
	default:
		m.state.Layout = "classic"
	}
}

// sep is the middle dot separator, ASCII "-" under plain/fallback
// (every non-ASCII glyph has an ASCII fallback).
func sep() string {
	if UseASCII() {
		return "-"
	}
	return "\u00b7"
}

// promptGlyph is the user-row prefix, ASCII ">" under plain/fallback.
func promptGlyph() string {
	if UseASCII() {
		return ">"
	}
	return "\u203a"
}

// ruleChar is the horizontal rule glyph, ASCII under plain/fallback.
func ruleChar() string {
	if UseASCII() {
		return "-"
	}
	return "─"
}

// classicRule renders the single rule above the composer.
func classicRule(width int) string {
	if width < 1 {
		width = 1
	}
	return strings.Repeat(ruleChar(), width)
}

// classicOrb renders the one-eyed orb for the current run state. The
// eye is driven only by real events: working, done, error,
// interrupted, idle (C0). ASCII fallback art under plain/ASCII mode.
// The art itself lives in nibArt (one module, WORKBENCH.md §4); the
// classic eye mapping stays here so the classic goldens hold.
func (m AppModel) classicOrb() string {
	eye, asciiEye := "◐", "o"
	style := m.theme.Accent
	switch m.state.TurnOutcome {
	case "done":
		eye, asciiEye = "●", "o"
		style = m.theme.Success
	case "error":
		eye, asciiEye = "○", "x"
		style = m.theme.Error
	}
	if m.state.Busy {
		eye, asciiEye = "◑", "*"
		style = m.theme.ActivityGlyph
	}
	return nibArt(eye, asciiEye, style)
}

// permissionShort maps a permission mode to its footer label.
func permissionShort(mode string) string {
	switch mode {
	case "manual", "always":
		return "manual"
	case "full_access", "yolo", "never":
		return "yolo"
	case "readonly":
		return "read-only"
	case "plan":
		return "plan"
	default:
		return "ask"
	}
}

// homeDir shortens dir to a ~-relative path for the header/footer.
func homeDir(dir string) string {
	if dir == "" {
		return "~"
	}
	if home, err := os.UserHomeDir(); err == nil && strings.HasPrefix(dir, home) {
		return "~" + strings.TrimPrefix(dir, home)
	}
	return dir
}

// classicHeader renders the orb beside the identity rows and the tip
// line (docs/ui/CLASSIC.md §1.1). Every field comes from real state;
// segments without an event source are omitted (see §7 deferrals).
func (m AppModel) classicHeader() string {
	orb := m.classicOrb()

	ver := m.state.Version
	if ver == "" {
		ver = "0.11.0"
	}
	model := m.state.ModelName
	if model == "" {
		model = "none"
	}
	dir := homeDir(m.state.Directory)
	branch := m.state.GitBranch

	identity := lipgloss.JoinVertical(lipgloss.Left,
		m.theme.CardTitle.Render("niki ")+m.theme.Muted.Render(ver),
		m.theme.Assistant.Render(model+" "+sep()+" approvals "+permissionShort(m.state.PermissionMode)),
		m.theme.Muted.Render(dir+" "+sep()+" "+branch),
	)
	head := lipgloss.JoinHorizontal(lipgloss.Top, orb, "  ", identity)

	tipText := "Type a request " + sep() + " / for commands " + sep() + " ctrl+k for everything " + sep() + " ? for help"
	if m.state.Width > 0 && CellWidth(tipText) > m.state.Width {
		tipText = TruncateTo(tipText, m.state.Width)
	}
	tip := m.theme.Placeholder.Render(tipText)
	return head + "\n" + tip
}

// needsSetup reports whether the first-launch SETUP block shows: no
// model configured means nothing can run (C1).
func (m AppModel) needsSetup() bool {
	return m.state.ModelName == "" || m.state.ModelName == "None" || m.state.ModelName == "none"
}

// classicSetup renders the SETUP block. Choices are shown only when
// they dispatch a real action; [d] recorded demo ships with the
// five-frame fixture replay (C3).
func (m AppModel) classicSetup() string {
	var b strings.Builder
	b.WriteString(m.theme.BadgePerm.Render("SETUP") + "\n")
	b.WriteString(m.theme.Muted.Render("  No model is configured, so nothing can run yet.") + "\n")
	b.WriteString(m.theme.Muted.Render("  [k] add a key    [o] use Ollama    [d] recorded demo"))
	return b.String()
}

// classicUserRow renders the raised user row: `› text`, full-width
// raised background, never truncated (docs/ui/CLASSIC.md §1.2).
func (m AppModel) classicUserRow(text string) string {
	line := promptGlyph() + " " + Sanitize(text)
	style := m.theme.RaisedUser
	if m.state.Width > 0 {
		style = style.Width(m.state.Width)
	}
	return style.Render(line)
}

// classicSay renders say text: indented two columns, at most two
// lines — the statement's first two lines, each truncated with an
// ellipsis when it does not fit (docs/ui/CLASSIC.md §1.2).
func (m AppModel) classicSay(text string) string {
	text = Sanitize(text)
	const indent = "  "
	width := m.state.Width - 2
	if width < 10 {
		width = 10
	}
	logical := strings.Split(text, "\n")
	more := len(logical) > 2
	if len(logical) > 2 {
		logical = logical[:2]
	}
	var lines []string
	for i, l := range logical {
		if CellWidth(l) > width {
			// TruncateTo appends the ellipsis itself.
			l = TruncateTo(l, width)
		}
		if more && i == len(logical)-1 {
			l = strings.TrimRight(l, " ") + " " + Ellipsis()
		}
		lines = append(lines, indent+m.theme.Assistant.Render(l))
	}
	return strings.Join(lines, "\n")
}

// classicTranscript renders the classic transcript rows from cells.
// Tool rows, excerpts, and loop rows render only from their events
// (C2/C3); step headers group tool rows under the plan step the
// engine attributed them to (U2/E1); system notes stay dim.
func (m AppModel) classicTranscript(cells []HistoryCell) string {
	var out []string
	// Step ordinals are derived from trace order and stay stable
	// within a frame: the first distinct step label is step 1, the
	// next new one step 2, and a recurring label keeps its number.
	ordinals := map[string]int{}
	lastStep := ""
	for _, c := range cells {
		if c.Kind == "tool" && c.Step != "" && c.Step != lastStep {
			n, ok := ordinals[c.Step]
			if !ok {
				n = len(ordinals) + 1
				ordinals[c.Step] = n
			}
			lastStep = c.Step
			out = append(out, m.classicStepHeader(n, c.Step))
		}
		switch c.Kind {
		case "tool":
			out = append(out, m.classicToolRow(c))
		case "tool_excerpt":
			out = append(out, m.classicExcerpt(c))
		case "diff":
			out = append(out, m.classicDiffBlock(c))
		case "reasoning":
			out = append(out, m.classicReasoning(c))
		case "loop":
			out = append(out, m.classicLoopRow(c))
		default:
			c.Text = Sanitize(c.Text)
			switch c.Role {
			case "user":
				out = append(out, m.classicUserRow(c.Text))
			case "assistant":
				out = append(out, m.classicSay(c.Text))
			case "error":
				out = append(out, "  "+m.theme.Error.Render(localizeSep(c.Text)))
			default:
				out = append(out, "  "+m.theme.Muted.Render(localizeSep(c.Text)))
			}
		}
	}
	return strings.Join(out, "\n")
}

// classicStepHeader renders the step rule that opens a step's tool
// rows: `── step 1 · <label> ────`, the label from the engine's
// plan event and the ordinal from trace order (U2/E1). ASCII/plain
// degrade through the same rule and separator glyphs.
func (m AppModel) classicStepHeader(n int, label string) string {
	width := m.state.Width
	if width < 20 {
		width = 20
	}
	prefix := ruleChar() + ruleChar() + " step " + itoa(n) + " " + sep() + " "
	budget := width - CellWidth(prefix) - 1
	if budget < 4 {
		budget = 4
	}
	if CellWidth(label) > budget {
		label = TruncateTo(label, budget)
	}
	head := prefix + m.theme.ToolName.Render(label)
	pad := width - CellWidth(prefix) - CellWidth(label) - 2
	if pad > 0 {
		head += " " + m.theme.ToolTree.Render(strings.Repeat(ruleChar(), pad))
	}
	return head
}

// diffBlockMaxLines caps a rendered diff block; the remainder is a
// labeled count, never silently dropped (the full diff stays on the
// event and in the diff viewer).
const diffBlockMaxLines = 40

// classicDiffBlock renders the real unified diff under the patching
// tool's row (U2/E2): + lines success, - lines error, headers muted.
func (m AppModel) classicDiffBlock(c HistoryCell) string {
	var b strings.Builder
	lines := c.Diff
	dropped := 0
	if len(lines) > diffBlockMaxLines {
		dropped = len(lines) - diffBlockMaxLines
		lines = lines[:diffBlockMaxLines]
	}
	for _, l := range lines {
		style := m.theme.ToolDetail
		switch {
		case strings.HasPrefix(l, "+++"), strings.HasPrefix(l, "---"):
			style = m.theme.Muted
		case strings.HasPrefix(l, "@@"):
			style = m.theme.Muted
		case strings.HasPrefix(l, "+"):
			style = m.theme.Success
		case strings.HasPrefix(l, "-"):
			style = m.theme.Error
		}
		// Lip Gloss expands tabs to four spaces when rendering; do
		// it here so the block's alignment is owned by the renderer,
		// not an implicit library detail.
		b.WriteString("  " + style.Render(localizeSep(strings.ReplaceAll(l, "\t", "    "))) + "\n")
	}
	if dropped > 0 {
		b.WriteString("  " + m.theme.Muted.Render(fmt.Sprintf("+%d more lines", dropped)))
	}
	return strings.TrimRight(b.String(), "\n")
}

// reasoningCollapsedLines and reasoningMaxLines cap the expanded
// reasoning text; the collapsed line is always one row.
const (
	reasoningMaxLines = 6
)

// classicReasoning renders the provider reasoning line: collapsed to
// one row by default (ctrl+o expands), absent entirely when no
// reasoning event ever fired (U2). No glyph — a plain word, per the
// originality rules.
func (m AppModel) classicReasoning(c HistoryCell) string {
	hint := "thought " + sep() + " ctrl+o expand"
	if !m.state.ExpandToolOutput {
		return m.theme.Thinking.Render(localizeSep(hint))
	}
	width := m.state.Width - 4
	if width < 10 {
		width = 10
	}
	logical := strings.Split(strings.TrimRight(c.Text, "\n"), "\n")
	dropped := 0
	if len(logical) > reasoningMaxLines {
		dropped = len(logical) - reasoningMaxLines
		logical = logical[:reasoningMaxLines]
	}
	var lines []string
	for _, l := range logical {
		if CellWidth(l) > width {
			l = TruncateTo(l, width)
		}
		lines = append(lines, "  "+m.theme.Thinking.Render(localizeSep(l)))
	}
	if dropped > 0 {
		lines = append(lines, "  "+m.theme.Muted.Render(fmt.Sprintf("+%d more lines", dropped)))
	}
	return strings.Join(lines, "\n")
}

// classicPlaceholder is the composer placeholder for the current state
// (docs/ui/CLASSIC.md §1.4).
func (m AppModel) classicPlaceholder() string {
	switch {
	case m.state.QuestionModal.Active || m.state.Gate.Active:
		return "answer the question above, or steer"
	case m.state.Busy:
		return "steer with enter"
	default:
		return "Describe a change, or ask a question"
	}
}

// classicComposer renders the single-rule composer: one rule above,
// the prompt line, no bottom rule.
func (m AppModel) classicComposer() string {
	width := m.state.Width
	if width < 20 {
		width = 20
	}
	m.composer.Input.Placeholder = m.classicPlaceholder()
	return classicRule(width) + "\n" + m.composer.Input.View()
}

// hintState names the footer hint state for the current model.
func (m AppModel) hintState() string {
	switch {
	case m.state.Gate.Active:
		return hintGate
	case m.state.Verdict.Active:
		return hintVerdict
	case m.state.Busy:
		return hintRunning
	default:
		return hintIdle
	}
}

// classicFooter renders ambient facts left and registry hints right
// (docs/ui/CLASSIC.md §1.4). ctx N% shows only when the provider
// reported usage. Under 80 columns the facts drop and the footer
// shows hints only (design §5: 50-79 behavior).
func (m AppModel) classicFooter() string {
	dir := homeDir(m.state.Directory)
	left := permissionShort(m.state.PermissionMode)
	if m.state.GitBranch != "" {
		left += " " + sep() + " " + m.state.GitBranch
	}
	left += " " + sep() + " " + dir
	leftRendered := m.theme.StatusDir.Render(left)

	var rightParts []string
	for _, k := range FooterHints(m.hintState()) {
		rightParts = append(rightParts, k.Hint)
	}
	if m.state.UsedTokens > 0 && m.state.MaxTokens > 0 {
		pct := float64(m.state.UsedTokens) / float64(m.state.MaxTokens) * 100
		rightParts = append(rightParts, fmt.Sprintf("ctx %.0f%%", pct))
	}
	right := m.theme.StatusHints.Render(strings.Join(rightParts, " "+sep()+" "))

	w := m.state.Width
	if w < 40 {
		w = 80
	}
	leftLen := lipgloss.Width(leftRendered)
	rightLen := lipgloss.Width(right)
	if w < 80 {
		// 50-79: hints only; the header and verdict card carry the
		// ambient facts (design §5).
		return right
	}
	if leftLen+rightLen+2 <= w {
		pad := w - leftLen - rightLen - 1
		if pad < 1 {
			pad = 1
		}
		return leftRendered + strings.Repeat(" ", pad) + right
	}
	return leftRendered + "\n" + right
}

// classicChrome is the number of lines the classic layout spends on
// chrome around the scroll region: orb header (3) + tip + blank +
// SETUP block (3 + blank, when unconfigured) + blank + activity +
// rule + composer + footer.
func (m AppModel) classicChrome() int {
	chrome := 3 + 1 + 1 + 1 + 1 + 1 + 1 + 1
	if m.needsSetup() {
		chrome += 4
	}
	return chrome
}

// classicBody renders the full classic screen (non-inline form: the
// header stays visible above the live area). The transcript scrolls
// through the viewport so per-frame cost stays flat as history grows.
func (m AppModel) classicBody() string {
	var parts []string
	parts = append(parts, m.classicHeader())
	if m.needsSetup() {
		parts = append(parts, "", m.classicSetup())
	}
	parts = append(parts, "", m.viewport.View())
	// C3: the gate block replaces the activity line while a decision
	// is pending; the verdict card ends the run above the composer.
	switch {
	case m.state.Gate.Active:
		parts = append(parts, "", m.classicGate())
	case m.state.Verdict.Active && !m.state.HideContractUI:
		parts = append(parts, "", m.classicVerdictCard())
	default:
		parts = append(parts, "", m.classicActivity())
	}
	if contract := m.classicContractLine(); contract != "" && !m.state.Verdict.Active && !m.state.HideContractUI {
		parts = append(parts, contract)
	}
	parts = append(parts, m.classicComposer())
	parts = append(parts, m.classicFooter())
	body := strings.Join(parts, "\n")
	if m.state.Debug {
		body += "\n" + m.debugView()
	}
	return body
}

// classicInlineBody renders the live area only: in inline mode the
// header has scrolled away into native scrollback (U4). The live
// transcript scrolls through the viewport.
func (m AppModel) classicInlineBody() string {
	var parts []string
	switch {
	case m.state.Gate.Active:
		parts = append(parts, m.classicGate())
	case m.state.Verdict.Active && !m.state.HideContractUI:
		parts = append(parts, m.classicVerdictCard())
	default:
		parts = append(parts, m.classicActivity())
	}
	if contract := m.classicContractLine(); contract != "" && !m.state.Verdict.Active && !m.state.HideContractUI {
		parts = append(parts, contract)
	}
	if vp := m.viewport.View(); strings.TrimSpace(vp) != "" {
		parts = append(parts, vp)
	}
	parts = append(parts, m.classicComposer(), m.classicFooter())
	return strings.Join(parts, "\n")
}

// scrollbackHeader is the header printed once to native scrollback
// in inline mode (U4): the classic orb header (with the SETUP block
// when unconfigured), or the ledger welcome card on that layout.
func (m AppModel) scrollbackHeader() string {
	if m.state.Layout == "ledger" {
		return m.welcomeCardView()
	}
	header := m.classicHeader()
	if m.needsSetup() {
		header += "\n\n" + m.classicSetup()
	}
	return header
}

// classicNarrow is the under-50-column notice: one line, the run
// continues (design §3). A pending gate stays visible so a decision
// is never hidden by the resize.
func (m AppModel) classicNarrow() string {
	notice := m.theme.Muted.Render(TruncateTo("resize to 50 or wider. Your run continues.", m.state.Width))
	if m.state.Gate.Active {
		return notice + "\n" + m.classicGate()
	}
	return notice
}

// classicView is the classic layout entry point.
func (m AppModel) classicView() tea.View {
	if !m.state.Ready {
		return tea.NewView("Initializing NikiCode...")
	}
	var body string
	switch {
	case m.state.Width > 0 && m.state.Width < 50:
		body = m.classicNarrow()
	case m.state.Inline:
		body = m.classicInlineBody()
	default:
		body = m.classicBody()
	}
	v := tea.NewView(body)
	v.AltScreen = !m.state.Inline
	return v
}
