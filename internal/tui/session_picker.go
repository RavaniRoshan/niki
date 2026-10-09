package tui

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// SessionPickerState holds state for the interactive session explorer.
type SessionPickerState struct {
	Active        bool
	Query         string
	Selected      int
	Sessions      []protocol.SessionMetadata
	Preview       map[string][]string
	ConfirmDelete bool
}

// NewSessionPickerState creates an initialized session picker state.
func NewSessionPickerState() SessionPickerState {
	return SessionPickerState{
		Preview: make(map[string][]string),
	}
}

// FilterSessions filters session metadata based on query.
func FilterSessions(sessions []protocol.SessionMetadata, query string) []protocol.SessionMetadata {
	q := strings.TrimSpace(strings.ToLower(query))
	if q == "" {
		return sessions
	}
	var out []protocol.SessionMetadata
	for _, s := range sessions {
		if strings.Contains(strings.ToLower(s.Title), q) ||
			strings.Contains(strings.ToLower(string(s.ID)), q) {
			out = append(out, s)
		}
	}
	return out
}

func formatRelativeTime(t time.Time) string {
	if t.IsZero() {
		return "unknown"
	}
	d := time.Since(t)
	if d < time.Minute {
		return "just now"
	} else if d < time.Hour {
		return fmt.Sprintf("%dm ago", int(d.Minutes()))
	} else if d < 24*time.Hour {
		return fmt.Sprintf("%dh ago", int(d.Hours()))
	}
	return fmt.Sprintf("%dd ago", int(d.Hours()/24))
}

// RenderSessionPicker renders the interactive session browser modal overlay.
func RenderSessionPicker(state SessionPickerState, th Theme, width, height int) string {
	modalWidth := width - 6
	if modalWidth > 110 {
		modalWidth = 110
	}
	if modalWidth < 45 {
		modalWidth = 45
	}

	title := th.CardTitle.Render("📂 Session Browser & Rollouts")
	hintClose := th.Muted.Render("(Ctrl+S / Esc to close)")
	headerLine := fmt.Sprintf("%s  %s", title, hintClose)

	queryDisplay := state.Query
	if queryDisplay == "" {
		queryDisplay = th.Placeholder.Render("Type to search sessions by title or ID…")
	} else {
		queryDisplay = th.UserText.Render(state.Query)
	}
	searchBox := th.PaletteSearch.
		Border(lipgloss.RoundedBorder()).
		Padding(0, 1).
		Width(modalWidth - 6).
		Render("❯ " + queryDisplay + "█")

	filtered := FilterSessions(state.Sessions, state.Query)
	cursor := state.Selected
	if cursor >= len(filtered) {
		cursor = len(filtered) - 1
	}
	if cursor < 0 {
		cursor = 0
	}

	if state.ConfirmDelete && len(filtered) > 0 {
		cur := filtered[cursor]
		warnBox := th.Error.
			Border(lipgloss.DoubleBorder()).
			Padding(1, 2).
			Width(modalWidth - 6).
			Render(fmt.Sprintf("⚠️ Are you sure you want to delete session '%s'?\n\nPress 'y' to confirm deletion · 'n' to cancel", cur.Title))
		return th.PaletteBorder.
			Border(lipgloss.DoubleBorder()).
			Padding(1, 2).
			Width(modalWidth).
			Render(lipgloss.JoinVertical(lipgloss.Left, headerLine, "", searchBox, "", warnBox))
	}

	// Determine layout: dual-pane if wide enough, single-pane otherwise
	if modalWidth >= 80 {
		leftWidth := (modalWidth * 45) / 100
		rightWidth := modalWidth - leftWidth - 6

		const maxRows = 7
		startIdx := 0
		if cursor >= maxRows {
			startIdx = cursor - maxRows + 1
		}
		endIdx := startIdx + maxRows
		if endIdx > len(filtered) {
			endIdx = len(filtered)
		}

		var rows []string
		if len(filtered) == 0 {
			rows = append(rows, th.Muted.Render("  No saved sessions found."))
		} else {
			for i := startIdx; i < endIdx; i++ {
				s := filtered[i]
				isSel := (i == cursor)

				timeAgo := formatRelativeTime(s.CreatedAt)
				badge := th.PaletteTag.Padding(0, 1).Render(fmt.Sprintf("%d turns · %s", s.TurnCount, timeAgo))

				tStyle := th.Assistant
				if isSel {
					tStyle = th.PaletteSelected
				}

				line := ""
				displayTitle := s.Title
				if len(displayTitle) > leftWidth-8 {
					displayTitle = displayTitle[:leftWidth-11] + "…"
				}
				if isSel {
					line = th.PaletteSelected.Render("▶ ") + tStyle.Render(displayTitle) + "\n   " + badge
				} else {
					line = "  " + tStyle.Render(displayTitle) + "\n   " + badge
				}
				rows = append(rows, line)
			}
		}

		leftPane := lipgloss.JoinVertical(lipgloss.Left,
			th.CardSubtitle.Render("Recent Sessions"),
			"",
			strings.Join(rows, "\n\n"),
		)

		var previewLines []string
		if len(filtered) > 0 {
			selSession := filtered[cursor]
			previewLines = append(previewLines, th.CardSubtitle.Render("Preview: "+selSession.Title))
			previewLines = append(previewLines, th.Muted.Render(string(selSession.ID)))
			previewLines = append(previewLines, "")

			history, hasPreview := state.Preview[string(selSession.ID)]
			if hasPreview && len(history) > 0 {
				for _, h := range history {
					if strings.HasPrefix(h, ">") {
						previewLines = append(previewLines, th.UserText.Render(h))
					} else if strings.HasPrefix(h, "●") {
						previewLines = append(previewLines, th.ToolName.Render(h))
					} else {
						previewLines = append(previewLines, th.Assistant.Render(h))
					}
				}
			} else {
				previewLines = append(previewLines, th.Muted.Render("(No preview available for this session)"))
			}
		} else {
			previewLines = append(previewLines, th.Muted.Render("No session selected."))
		}

		rightPane := th.CardBorder.
			Border(lipgloss.RoundedBorder()).
			Padding(0, 1).
			Width(rightWidth).
			Height(11).
			Render(lipgloss.JoinVertical(lipgloss.Left, previewLines...))

		columns := lipgloss.JoinHorizontal(lipgloss.Top,
			lipgloss.NewStyle().Width(leftWidth).Render(leftPane),
			"  ",
			rightPane,
		)

		footer := th.Muted.Render("↑/↓ select · enter resume · f fork · d delete · esc close")
		content := lipgloss.JoinVertical(lipgloss.Left,
			headerLine,
			"",
			searchBox,
			"",
			columns,
			"",
			footer,
		)

		return th.PaletteBorder.
			Border(lipgloss.DoubleBorder()).
			Padding(1, 2).
			Width(modalWidth).
			Render(content)
	}

	// Narrow single-column layout
	const maxRows = 5
	startIdx := 0
	if cursor >= maxRows {
		startIdx = cursor - maxRows + 1
	}
	endIdx := startIdx + maxRows
	if endIdx > len(filtered) {
		endIdx = len(filtered)
	}

	var rows []string
	if len(filtered) == 0 {
		rows = append(rows, th.Muted.Render("  No saved sessions found."))
	} else {
		for i := startIdx; i < endIdx; i++ {
			s := filtered[i]
			isSel := (i == cursor)
			tStyle := th.Assistant
			if isSel {
				tStyle = th.PaletteSelected
			}
			badge := th.PaletteTag.Padding(0, 1).Render(fmt.Sprintf("%d turns", s.TurnCount))
			line := ""
			if isSel {
				line = th.PaletteSelected.Render("▶ ") + tStyle.Render(s.Title) + " " + badge
			} else {
				line = "  " + tStyle.Render(s.Title) + " " + badge
			}
			rows = append(rows, line)
		}
	}

	footer := th.Muted.Render("↑/↓ select · enter resume · f fork · d delete · esc close")
	content := lipgloss.JoinVertical(lipgloss.Left,
		headerLine,
		"",
		searchBox,
		"",
		strings.Join(rows, "\n"),
		"",
		footer,
	)

	return th.PaletteBorder.
		Border(lipgloss.DoubleBorder()).
		Padding(1, 2).
		Width(modalWidth).
		Render(content)
}
