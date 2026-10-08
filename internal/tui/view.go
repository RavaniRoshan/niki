package tui

// renderCells renders transcript cells into a
// single string. Text is sanitized before it
// reaches the terminal (L6).
func renderCells(m AppModel, cells []HistoryCell) string {
	var out string
	for _, c := range cells {
		c.Text = Sanitize(c.Text)
		switch c.Role {
		case "user":
			out += m.theme.User.Render("You: ") + c.Text + "\n\n"
		case "assistant":
			out += m.theme.Assistant.Render("Niki: ") + c.Text + "\n\n"
		case "tool":
			out += m.theme.Muted.Render("[tool] " + c.Text + "\n\n")
		case "error":
			out += m.theme.Error.Render("[error] " + c.Text + "\n\n")
		default:
			out += m.theme.Muted.Render(c.Text) + "\n\n"
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
