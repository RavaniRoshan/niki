package tui

import (
	"github.com/charmbracelet/bubbles/textinput"
)

type Composer struct {
	Input textinput.Model
}

func NewComposer() Composer {
	ti := textinput.New()
	ti.Placeholder = "Type a prompt or task (type / for commands)..."
	ti.Focus()
	ti.CharLimit = 4096
	ti.Width = 80
	return Composer{Input: ti}
}
