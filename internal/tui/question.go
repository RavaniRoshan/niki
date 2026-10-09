package tui

import (
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// QuestionState tracks user interactions with an active ask_user_question prompt.
type QuestionState struct {
	Active       bool
	Questions    []protocol.UserQuestion
	CurrentIdx   int
	SelectedOpts []int
	MultiChecked [][]bool
	CustomInputs []string
	InCustom     bool
}

// NewQuestionState initializes the interactive question state.
func NewQuestionState(questions []protocol.UserQuestion) QuestionState {
	selected := make([]int, len(questions))
	checked := make([][]bool, len(questions))
	customs := make([]string, len(questions))

	for i, q := range questions {
		checked[i] = make([]bool, len(q.Options))
		selected[i] = 0
		if len(checked[i]) > 0 {
			checked[i][0] = true
		}
	}

	return QuestionState{
		Active:       len(questions) > 0,
		Questions:    questions,
		CurrentIdx:   0,
		SelectedOpts: selected,
		MultiChecked: checked,
		CustomInputs: customs,
		InCustom:     false,
	}
}

// HandleKey processes keyboard navigation for the question modal.
func (q *QuestionState) HandleKey(msg tea.KeyMsg) (done bool, answers []string, cancelled bool) {
	if !q.Active || len(q.Questions) == 0 {
		return false, nil, false
	}

	curQ := q.Questions[q.CurrentIdx]
	totalOpts := len(curQ.Options)
	if curQ.AllowCustom {
		totalOpts++
	}

	switch msg.Type {
	case tea.KeyEsc:
		q.Active = false
		return true, nil, true

	case tea.KeyUp, tea.KeyCtrlP:
		if q.InCustom {
			q.InCustom = false
		} else if q.SelectedOpts[q.CurrentIdx] > 0 {
			q.SelectedOpts[q.CurrentIdx]--
		}
		return false, nil, false

	case tea.KeyDown, tea.KeyCtrlN:
		if q.SelectedOpts[q.CurrentIdx] < totalOpts-1 {
			q.SelectedOpts[q.CurrentIdx]++
			if curQ.AllowCustom && q.SelectedOpts[q.CurrentIdx] == len(curQ.Options) {
				q.InCustom = true
			}
		}
		return false, nil, false

	case tea.KeyTab:
		if q.CurrentIdx < len(q.Questions)-1 {
			q.CurrentIdx++
			q.InCustom = false
		} else {
			q.CurrentIdx = 0
			q.InCustom = false
		}
		return false, nil, false

	case tea.KeySpace:
		sel := q.SelectedOpts[q.CurrentIdx]
		if sel < len(curQ.Options) {
			q.MultiChecked[q.CurrentIdx][sel] = !q.MultiChecked[q.CurrentIdx][sel]
		}
		return false, nil, false

	case tea.KeyBackspace:
		if q.InCustom && len(q.CustomInputs[q.CurrentIdx]) > 0 {
			txt := q.CustomInputs[q.CurrentIdx]
			q.CustomInputs[q.CurrentIdx] = txt[:len(txt)-1]
		}
		return false, nil, false

	case tea.KeyEnter:
		// If there are more questions, advance to next question
		if q.CurrentIdx < len(q.Questions)-1 {
			q.CurrentIdx++
			q.InCustom = false
			return false, nil, false
		}

		// Collect answers
		ans := make([]string, len(q.Questions))
		for i, item := range q.Questions {
			sel := q.SelectedOpts[i]
			if item.AllowCustom && sel == len(item.Options) {
				ans[i] = q.CustomInputs[i]
				if ans[i] == "" {
					ans[i] = "custom write-in"
				}
			} else if sel < len(item.Options) {
				ans[i] = item.Options[sel]
			}
		}
		q.Active = false
		return true, ans, false

	default:
		if q.InCustom && (msg.Type == tea.KeyRunes || msg.Type == tea.KeySpace) {
			q.CustomInputs[q.CurrentIdx] += string(msg.Runes)
			return false, nil, false
		}
	}

	return false, nil, false
}

// Render renders the question modal overlay box.
func (q *QuestionState) Render(th Theme, width int) string {
	if !q.Active || len(q.Questions) == 0 {
		return ""
	}

	boxWidth := width - 4
	if boxWidth > 76 {
		boxWidth = 76
	}
	if boxWidth < 40 {
		boxWidth = 40
	}

	curQ := q.Questions[q.CurrentIdx]
	header := curQ.Header
	if header == "" {
		header = fmt.Sprintf("Question %d/%d", q.CurrentIdx+1, len(q.Questions))
	} else {
		header = fmt.Sprintf("[%s] (%d/%d)", header, q.CurrentIdx+1, len(q.Questions))
	}

	var sb strings.Builder
	title := th.CardTitle.Render("❓ " + header)
	sb.WriteString(title + "\n")
	sb.WriteString(th.UserText.Render(curQ.Question) + "\n\n")

	for i, opt := range curQ.Options {
		cursor := "  "
		if q.SelectedOpts[q.CurrentIdx] == i {
			cursor = "❯ "
		}

		check := "( ) "
		if q.SelectedOpts[q.CurrentIdx] == i {
			check = "(•) "
		}

		var optText string
		if q.SelectedOpts[q.CurrentIdx] == i {
			optText = th.Accent.Render(cursor + check + opt)
		} else {
			optText = th.Muted.Render(cursor + check + opt)
		}
		sb.WriteString(optText + "\n")
	}

	if curQ.AllowCustom {
		cursor := "  "
		idx := len(curQ.Options)
		if q.SelectedOpts[q.CurrentIdx] == idx {
			cursor = "❯ "
		}
		custVal := q.CustomInputs[q.CurrentIdx]
		if custVal == "" {
			custVal = "type your custom answer…"
		}
		line := fmt.Sprintf("%s[Other] %s", cursor, custVal)
		if q.SelectedOpts[q.CurrentIdx] == idx {
			sb.WriteString(th.Accent.Render(line) + "\n")
		} else {
			sb.WriteString(th.Muted.Render(line) + "\n")
		}
	}

	sb.WriteString("\n" + th.Muted.Render("↑/↓ select · Enter submit · Tab switch question · Esc skip"))

	content := sb.String()
	card := th.CardBorder.Width(boxWidth).Render(content)
	return lipgloss.PlaceHorizontal(width, lipgloss.Center, card)
}
