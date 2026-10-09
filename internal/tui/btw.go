package tui

import (
	"fmt"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

// BtwState tracks a docked mini-agent lookup session.
type BtwState struct {
	Active   bool
	Query    string
	Response string
	Busy     bool
}

// HandleKey handles keyboard interaction for the docked /btw card.
func (b *BtwState) HandleKey(msg tea.KeyMsg) (closed bool) {
	if !b.Active {
		return false
	}
	switch msg.Type {
	case tea.KeyEsc, tea.KeyEnter:
		b.Active = false
		b.Busy = false
		return true
	}
	return false
}

// Render renders the docked /btw side-agent card above the composer.
func (b *BtwState) Render(th Theme, width int) string {
	if !b.Active {
		return ""
	}

	cardWidth := width - 4
	if cardWidth > 80 {
		cardWidth = 80
	}
	if cardWidth < 40 {
		cardWidth = 40
	}

	header := th.CardTitle.Render("💡 /btw side-query: ") + th.UserText.Render(b.Query)
	var body string
	if b.Busy {
		body = th.ActivityText.Render("● Searching codebase without polluting main context…")
	} else if b.Response != "" {
		body = th.Assistant.Render(b.Response)
	} else {
		body = th.Muted.Render("No information found.")
	}

	footer := th.Muted.Render("esc or enter to dismiss")
	content := fmt.Sprintf("%s\n\n%s\n\n%s", header, body, footer)
	card := th.CardBorder.Width(cardWidth).Render(content)
	return lipgloss.PlaceHorizontal(width, lipgloss.Center, card)
}
