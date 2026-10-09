package tui

import (
	"github.com/charmbracelet/bubbles/textinput"
)

type Composer struct {
	Input    textinput.Model
	KillRing *KillRing
	Undo     *UndoStack
}

func NewComposer(theme Theme) Composer {
	ti := textinput.New()
	ti.Prompt = "> "
	ti.PromptStyle = theme.PromptPrefix
	ti.TextStyle = theme.InputText
	ti.Placeholder = "Type a prompt or task (type / for commands)..."
	ti.PlaceholderStyle = theme.Placeholder
	ti.Focus()
	ti.CharLimit = 8192
	ti.Width = 80
	return Composer{
		Input:    ti,
		KillRing: NewKillRing(32),
		Undo:     NewUndoStack(),
	}
}

// Value returns the current input value with expanded paste tokens.
func (c *Composer) Value() string {
	return c.Input.Value()
}

// Reset clears the input value and resets cursor.
func (c *Composer) Reset() {
	c.Input.Reset()
}
