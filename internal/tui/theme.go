package tui

import "github.com/charmbracelet/lipgloss"

type Theme struct {
	Header    lipgloss.Style
	Assistant lipgloss.Style
	Success   lipgloss.Style
	Error     lipgloss.Style
	Muted     lipgloss.Style
	User      lipgloss.Style
}

func NewDefaultTheme() Theme {
	return Theme{
		Header:    lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("205")),
		Assistant: lipgloss.NewStyle().Foreground(lipgloss.Color("75")),
		Success:   lipgloss.NewStyle().Foreground(lipgloss.Color("42")),
		Error:     lipgloss.NewStyle().Foreground(lipgloss.Color("196")),
		Muted:     lipgloss.NewStyle().Foreground(lipgloss.Color("240")),
		User:      lipgloss.NewStyle().Foreground(lipgloss.Color("214")),
	}
}
