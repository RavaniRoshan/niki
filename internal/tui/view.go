package tui

import (
	"strings"
)

// renderCells renders transcript cells into a
// single string. Text is sanitized before it
// reaches the terminal (L6).
func renderCells(m AppModel, cells []HistoryCell) string {
	var out string
	for _, c := range cells {
		c.Text = Sanitize(c.Text)
		switch c.Role {
		case "user":
			out += m.theme.UserPrompt.Render("✨ ") + m.theme.UserText.Render(c.Text) + "\n\n"
		case "assistant":
			out += m.theme.Accent.Render("● ") + m.theme.Assistant.Render(c.Text) + "\n\n"
		case "tool":
			bullet := m.theme.ToolBullet.Render("● ")
			tree := m.theme.ToolTree.Render("  └ ")
			if idx := strings.Index(c.Text, ": "); idx != -1 {
				toolName := c.Text[:idx]
				detail := c.Text[idx+2:]
				if !m.state.ExpandToolOutput && (len(detail) > 120 || strings.Contains(detail, "\n")) {
					lines := strings.Split(detail, "\n")
					firstLine := strings.TrimSpace(lines[0])
					if len(firstLine) > 80 {
						firstLine = firstLine[:80] + "…"
					}
					out += bullet + m.theme.ToolName.Render(toolName) + "\n" +
						tree + m.theme.ToolDetail.Render(firstLine) + m.theme.Muted.Render(" · ctrl+o to expand") + "\n\n"
				} else {
					out += bullet + m.theme.ToolName.Render(toolName) + "\n" +
						tree + m.theme.ToolDetail.Render(detail) + "\n\n"
				}
			} else {
				out += bullet + m.theme.ToolName.Render(c.Text) + "\n\n"
			}
		case "error":
			bullet := m.theme.Error.Render("● ")
			out += bullet + m.theme.Error.Render("[error] "+c.Text) + "\n\n"
		default:
			bullet := m.theme.Accent.Render("✦ ")
			out += bullet + m.theme.Muted.Render(c.Text) + "\n\n"
		}
	}
	return out
}

// RenderHistory renders the full transcript
// (committed + live). The inline view uses
// renderCells over the live region only so
// committed history is never re-rendered (U9).
func (m AppModel) RenderHistory() string {
	return renderCells(m, m.history.Cells)
}
