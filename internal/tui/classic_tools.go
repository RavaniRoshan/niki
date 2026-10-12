package tui

import (
	"os"
	"strings"

	"charm.land/lipgloss/v2"
)

// Classic tool rows (C2): one line per tool call with the state
// glyph in the gutter, the verb, the target, and the result
// right-aligned. Every field comes from real engine events
// (protocol Target/Result/Excerpt); nothing is invented here.

// Row states and the fixed verb list (docs/ui/CLASSIC.md §1.2).
const (
	stateRunning = "running"
	stateDone    = "done"
	stateFailed  = "failed"
)

// toolVerb maps a tool name to the fixed verb list; unmapped tools
// keep their own name (an mcp tool becomes mcp:<server>).
func toolVerb(name string) string {
	switch name {
	case "read_file", "view_image":
		return "read"
	case "grep", "glob":
		return "search"
	case "edit_file", "write_file", "apply_patch":
		return "edit"
	case "shell", "exec_command", "bash":
		return "shell"
	case "update_plan", "todo_write":
		return "plan"
	case "web_fetch":
		return "fetch"
	}
	if rest, ok := strings.CutPrefix(name, "mcp__"); ok {
		parts := strings.SplitN(rest, "__", 2)
		if len(parts) == 2 && parts[0] != "" {
			return "mcp:" + parts[0]
		}
	}
	return name
}

// stateGlyph is the row glyph with its ASCII fallback and plain-mode
// word (every glyph has a word in plain mode, design §7).
func stateGlyph(state string) (glyph, ascii, word string) {
	switch state {
	case stateDone:
		return "✓", "+", "done"
	case stateFailed:
		return "✗", "x", "failed"
	case stateRunning:
		return "◐", "*", "running"
	default:
		return "○", "o", "queued"
	}
}

// plainGlyphWords reports whether glyphs render as words (plain
// mode): every state must be readable without the glyph set.
func plainGlyphWords() bool {
	return UseASCII() && os.Getenv("NIKI_PLAIN") == "1"
}

// localizeSep transliterates the common non-ASCII separators in
// engine-provided strings (results, excerpts, evidence, warnings) so
// ASCII/plain mode stays pure ASCII. Plain mode is a documented lossy
// accessibility mode (P1); live rendering is untouched.
func localizeSep(s string) string {
	if !UseASCII() {
		return s
	}
	s = strings.ReplaceAll(s, "\u00b7", "-")
	s = strings.ReplaceAll(s, "\u2014", "--")
	s = strings.ReplaceAll(s, "\u2013", "-")
	s = strings.ReplaceAll(s, "\u2212", "-")
	s = strings.ReplaceAll(s, "\u2026", "...")
	// Glyphs in authored system text degrade to their words.
	s = strings.ReplaceAll(s, "\u2713", "done ")
	s = strings.ReplaceAll(s, "\u2717", "failed ")
	s = strings.ReplaceAll(s, "\u25a0", "needs you ")
	s = strings.ReplaceAll(s, "\u21ba", "loop ")
	s = strings.ReplaceAll(s, "\u203a", ">")
	return s
}

func (m AppModel) renderStateGlyph(state string) string {
	glyph, ascii, word := stateGlyph(state)
	if UseASCII() {
		glyph = ascii
	}
	style := m.theme.Muted
	switch state {
	case stateDone:
		style = m.theme.Success
	case stateFailed:
		style = m.theme.Error
	case stateRunning:
		style = m.theme.ActivityGlyph
	}
	if plainGlyphWords() {
		return style.Render(word)
	}
	return style.Render(glyph)
}

// middleTruncate shortens s from the middle so both ends stay
// visible (design §5: 80-99 column behavior).
func middleTruncate(s string, max int) string {
	if max < 1 {
		max = 1
	}
	if CellWidth(s) <= max {
		return s
	}
	ell, ellW := "…", 1
	if UseASCII() {
		ell, ellW = "...", 3
	}
	if max <= ellW+1 {
		return TruncateTo(s, max)
	}
	leftLen := (max - ellW) / 2
	left := TruncateTo(s, leftLen)
	tail := s
	for CellWidth(tail) > max-leftLen-ellW && len(tail) > 0 {
		tail = tail[1:]
	}
	return left + ell + tail
}

// classicToolRow renders one tool row: glyph, verb, target, and the
// right-aligned result (a second indented line under 80 columns).
func (m AppModel) classicToolRow(c HistoryCell) string {
	width := m.state.Width
	if width < 20 {
		width = 20
	}
	verb := c.Verb
	if verb == "" {
		verb = toolVerb(c.Text)
	}
	result := m.theme.ToolDetail.Render(localizeSep(c.Result))
	// Prefix width in cells: glyph(1) + space + verb + two spaces.
	prefixWidth := 1 + 1 + CellWidth(verb) + 2
	resultWidth := lipgloss.Width(result)
	// Under 80 the result moves to its own line, so the target gets
	// the full row width (it is not squeezed by a result that is
	// not on its line).
	maxTarget := width - prefixWidth - 2
	if width >= 80 {
		maxTarget = width - prefixWidth - resultWidth - 2
	}
	if CellWidth(c.Target) > maxTarget {
		c.Target = middleTruncate(c.Target, maxTarget)
	}
	left := m.renderStateGlyph(c.State) + " " + m.theme.ToolName.Render(verb) + "  " + m.theme.ToolDetail.Render(c.Target)
	if width >= 80 {
		pad := width - lipgloss.Width(left) - resultWidth
		if pad < 1 {
			pad = 1
		}
		return left + strings.Repeat(" ", pad) + result
	}
	// 50-79: the result moves to a second, indented line (design §5).
	return left + "\n" + "    " + result
}

// splitExcerpt splits an excerpt payload into its (at most two)
// lines.
func splitExcerpt(s string) []string {
	if s == "" {
		return nil
	}
	return strings.Split(s, "\n")
}

// classicExcerpt renders the failure excerpt: up to two lines
// indented under the verb.
func (m AppModel) classicExcerpt(c HistoryCell) string {
	var lines []string
	for _, line := range c.Excerpt {
		lines = append(lines, "    "+m.theme.Error.Render(localizeSep(line)))
	}
	return strings.Join(lines, "\n")
}

// classicLoopRow renders a retry/repetition row in amber.
func (m AppModel) classicLoopRow(c HistoryCell) string {
	glyph := "↺"
	if UseASCII() {
		glyph = "!"
	}
	return m.theme.BadgePerm.Render(glyph + " " + localizeSep(c.Text))
}

// isLoopWarning reports whether a warning text describes a retry or
// repetition (the loop row's only source).
func isLoopWarning(text string) bool {
	t := strings.ToLower(text)
	return strings.Contains(t, "retrying") ||
		strings.Contains(t, "twice in a row") ||
		strings.Contains(t, "doom loop")
}

// classicPlanLine renders the optional next step from a real plan
// event (design §1.3: the next line appears only when the plan
// supplies it). It names the next pending step — the active step is
// what the activity line already reports.
func (m AppModel) classicPlanLine() string {
	for _, step := range m.state.Plan {
		if step.Status == "pending" {
			return m.theme.Muted.Render("next  " + step.Description)
		}
	}
	return ""
}

// activityPhrase builds the plain activity phrase from the real
// current action (design §7): the running tool's verb and target,
// or the engine's activity text when no tool is in flight. No
// elapsed counter — no event reports one.
func (m AppModel) activityPhrase() string {
	for i := len(m.history.Cells) - 1; i >= 0; i-- {
		c := m.history.Cells[i]
		if c.Kind == "tool" && c.State == stateRunning {
			switch c.Verb {
			case "read":
				return "reading " + c.Target
			case "search":
				return "searching " + c.Target
			case "edit":
				return "writing " + c.Target
			case "shell":
				return "running " + c.Target
			case "fetch":
				return "fetching " + c.Target
			case "plan":
				return "planning"
			}
			if v, ok := strings.CutPrefix(c.Verb, "mcp:"); ok {
				return "calling " + v
			}
			return c.Verb + " " + c.Target
		}
		if c.Kind == "tool" {
			break
		}
	}
	return m.state.Activity
}

// classicActivity renders the live activity line and the optional
// plan line beneath it.
func (m AppModel) classicActivity() string {
	var line string
	phrase := m.activityPhrase()
	if phrase == "" {
		glyph := "○"
		if UseASCII() {
			glyph = "o"
		}
		line = m.theme.Muted.Render("  " + glyph + " ready")
	} else {
		glyph := m.theme.ActivityGlyph.Render(SpinGlyphWithStyle(SpinnerSweep, m.state.SpinFrame, m.state.ReducedMotion, UseASCII()) + " ")
		actText := m.theme.ActivityText.Render(phrase)
		hint := m.theme.ActivityHint.Render(" " + sep() + " esc pause")
		line = "  " + glyph + actText + hint
	}
	if next := m.classicPlanLine(); next != "" {
		return line + "\n    " + next
	}
	return line
}
