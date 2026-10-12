package tui

import (
	"strings"
)

// renderCells renders transcript cells into a
// single string. Text is sanitized before it
// reaches the terminal (L6).
// markers are the transcript glyphs, ASCII under ASCII/plain (P1).
func markers() (prompt, bullet, tree, errBullet, star string) {
	if UseASCII() {
		return "* ", "* ", "  `- ", "* ", "* "
	}
	return "✨ ", "● ", "  └ ", "● ", "✦ "
}

func renderCells(m AppModel, cells []HistoryCell) string {
	promptMark, bulletMark, treeMark, errMark, starMark := markers()
	var out string
	for _, c := range cells {
		// U2 cells first: the diff and reasoning rows exist only in
		// the classic/workbench trace shapes; the ledger fossil
		// renders their real content, never a blank role row.
		switch c.Kind {
		case "diff":
			for _, l := range c.Diff {
				style := m.theme.ToolDetail
				switch {
				case strings.HasPrefix(l, "+"):
					style = m.theme.Success
				case strings.HasPrefix(l, "-"):
					style = m.theme.Error
				}
				out += "  " + style.Render(localizeSep(l)) + "\n"
			}
			out += "\n"
			continue
		case "reasoning":
			out += m.theme.Thinking.Render("thought") + "\n" +
				"  " + m.theme.Thinking.Render(localizeSep(c.Text)) + "\n\n"
			continue
		}
		c.Text = Sanitize(c.Text)
		switch c.Role {
		case "user":
			out += m.theme.UserPrompt.Render(promptMark) + m.theme.UserText.Render(c.Text) + "\n\n"
		case "assistant":
			out += m.theme.Accent.Render(bulletMark) + m.theme.Assistant.Render(c.Text) + "\n\n"
		case "tool":
			bullet := m.theme.ToolBullet.Render(bulletMark)
			tree := m.theme.ToolTree.Render(treeMark)
			if idx := strings.Index(c.Text, ": "); idx != -1 {
				toolName := c.Text[:idx]
				detail := c.Text[idx+2:]
				if !m.state.ExpandToolOutput && (len(detail) > 120 || strings.Contains(detail, "\n")) {
					lines := strings.Split(detail, "\n")
					firstLine := strings.TrimSpace(lines[0])
					if len(firstLine) > 80 {
						firstLine = firstLine[:80] + Ellipsis()
					}
					out += bullet + m.theme.ToolName.Render(toolName) + "\n" +
						tree + m.theme.ToolDetail.Render(firstLine) + m.theme.Muted.Render(" - ctrl+o to expand") + "\n\n"
				} else {
					out += bullet + m.theme.ToolName.Render(toolName) + "\n" +
						tree + m.theme.ToolDetail.Render(detail) + "\n\n"
				}
			} else {
				out += bullet + m.theme.ToolName.Render(c.Text) + "\n\n"
			}
		case "error":
			bullet := m.theme.Error.Render(errMark)
			out += bullet + m.theme.Error.Render("[error] "+c.Text) + "\n\n"
		case "shell_input":
			cmdText := strings.TrimPrefix(c.Text, "$ ")
			out += m.theme.StatusDir.Render("$ ") + m.theme.UserText.Render(cmdText) + "\n\n"
		case "shell_output":
			lines := strings.Split(c.Text, "\n")
			for _, line := range lines {
				out += "  " + m.theme.ToolDetail.Render(line) + "\n"
			}
			out += "\n"
		case "shell_status":
			if strings.Contains(c.Text, "exit 0") {
				out += m.theme.ToolTree.Render(treeMark) + m.theme.Success.Render("exit 0") + "\n\n"
			} else {
				out += m.theme.ToolTree.Render(treeMark) + m.theme.Error.Render(strings.TrimSpace(c.Text)) + "\n\n"
			}
		default:
			bullet := m.theme.Accent.Render(starMark)
			out += bullet + m.theme.Muted.Render(c.Text) + "\n\n"
		}
	}
	return out
}

// RenderHistory renders the full transcript
// (committed + live), layout-specific. The inline view uses
// renderCells over the live region only so
// committed history is never re-rendered (U9).
func (m AppModel) RenderHistory() string {
	if m.state.Layout == "classic" || m.state.Layout == "workbench" {
		return m.classicTranscript(m.history.Cells)
	}
	return renderCells(m, m.history.Cells)
}
