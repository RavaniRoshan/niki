package tui

import (
	"github.com/charmbracelet/bubbles/textinput"
)

type Composer struct {
	Input textinput.Model
}

func NewComposer(theme Theme) Composer {
	ti := textinput.New()
	ti.Prompt = "> "
	ti.PromptStyle = theme.PromptPrefix
	ti.TextStyle = theme.InputText
	ti.Placeholder = "Type a prompt or task (type / for commands)..."
	ti.PlaceholderStyle = theme.Placeholder
	ti.Focus()
	ti.CharLimit = 4096
	ti.Width = 80
	return Composer{Input: ti}
}
