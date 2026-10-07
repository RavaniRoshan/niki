package tui

// RenderHistory renders transcript cells into a single string.
func (m AppModel) RenderHistory() string {
	var out string
	for _, c := range m.history.Cells {
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
