package tui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"

	"github.com/RavaniRoshan/niki/internal/mention"
)

// MentionOverlayState tracks active @-mention candidate completion state.
type MentionOverlayState struct {
	Active     bool
	Query      string
	Selected   int
	Candidates []mention.Candidate
}

// RenderMentionOverlay renders the floating candidate card directly above the composer.
func RenderMentionOverlay(candidates []mention.Candidate, selected int, th Theme, width int) string {
	if len(candidates) == 0 {
		return ""
	}

	boxWidth := width - 8
	if boxWidth > 70 {
		boxWidth = 70
	}
	if boxWidth < 30 {
		boxWidth = 30
	}

	title := th.CardSubtitle.Render("🔍 Files & Symbols (@)")
	const maxItems = 5
	startIdx := 0
	if selected >= maxItems {
		startIdx = selected - maxItems + 1
	}
	endIdx := startIdx + maxItems
	if endIdx > len(candidates) {
		endIdx = len(candidates)
	}

	var rows []string
	for i := startIdx; i < endIdx; i++ {
		cand := candidates[i]
		isSel := (i == selected)

		p := cand.Path
		if len(p) > boxWidth-12 {
			p = "…" + p[len(p)-(boxWidth-15):]
		}

		if isSel {
			line := th.PaletteSelected.Render("▶ ") + th.UserText.Render(p)
			rows = append(rows, line)
		} else {
			line := "  " + th.Muted.Render(p)
			rows = append(rows, line)
		}
	}

	hint := th.Muted.Render("tab/enter insert · esc close")
	content := lipgloss.JoinVertical(lipgloss.Left,
		title,
		"",
		strings.Join(rows, "\n"),
		"",
		hint,
	)

	return th.CardBorder.
		Border(lipgloss.RoundedBorder()).
		Padding(0, 1).
		Width(boxWidth).
		Render(content)
}

// ComputeMentionQuery extracts the word starting with '@' under or preceding cursor.
func ComputeMentionQuery(val string, cursor int) (string, bool) {
	if cursor > len(val) {
		cursor = len(val)
	}
	runes := []rune(val[:cursor])
	if len(runes) == 0 {
		return "", false
	}

	start := len(runes) - 1
	for start >= 0 && runes[start] != ' ' && runes[start] != '\n' {
		start--
	}
	word := string(runes[start+1:])
	if strings.HasPrefix(word, "@") {
		return strings.TrimPrefix(word, "@"), true
	}
	return "", false
}

// ReplaceMentionWord replaces the active @word in input with the chosen candidate path.
func ReplaceMentionWord(val string, cursor int, replacement string) (string, int) {
	if cursor > len(val) {
		cursor = len(val)
	}
	runes := []rune(val[:cursor])
	start := len(runes) - 1
	for start >= 0 && runes[start] != ' ' && runes[start] != '\n' {
		start--
	}
	prefix := string(runes[:start+1])
	suffix := val[cursor:]

	newVal := prefix + "@" + replacement + " " + suffix
	newPos := len([]rune(prefix + "@" + replacement + " "))
	return newVal, newPos
}
