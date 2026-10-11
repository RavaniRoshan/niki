package tui

import (
	"charm.land/bubbles/v2/textinput"
)

type Composer struct {
	Input    textinput.Model
	KillRing *KillRing
	Undo     *UndoStack
}

func NewComposer(theme Theme) Composer {
	ti := textinput.New()
	ti.Prompt = "> "
	ti.Placeholder = "Type a prompt or task (type / for commands)..."
	ti.Focus()
	ti.CharLimit = 8192
	ti.SetWidth(80)
	styles := textinput.DefaultStyles(true)
	state := textinput.StyleState{
		Text:        theme.InputText,
		Prompt:      theme.PromptPrefix,
		Placeholder: theme.Placeholder,
	}
	styles.Focused = state
	styles.Blurred = state
	ti.SetStyles(styles)
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
