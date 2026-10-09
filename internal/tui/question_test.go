package tui

import (
	"strings"
	"testing"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

func TestQuestionModalNavigationAndSubmit(t *testing.T) {
	questions := []protocol.UserQuestion{
		{
			Header:   "DB Choice",
			Question: "Which database would you like to configure?",
			Options:  []string{"PostgreSQL", "SQLite", "MySQL"},
		},
		{
			Header:   "Auth Setup",
			Question: "Enable OAuth2 authentication?",
			Options:  []string{"Yes", "No"},
		},
	}

	state := NewQuestionState(questions)
	if !state.Active {
		t.Fatalf("expected state to be active")
	}

	// Navigate down to second option on Question 1
	downKey := tea.KeyMsg{Type: tea.KeyDown}
	done, _, _ := state.HandleKey(downKey)
	if done {
		t.Fatalf("down key should not complete modal")
	}
	if state.SelectedOpts[0] != 1 {
		t.Fatalf("expected selected option to be 1 (SQLite), got %d", state.SelectedOpts[0])
	}

	// Press Enter to move to Question 2
	enterKey := tea.KeyMsg{Type: tea.KeyEnter}
	done, _, _ = state.HandleKey(enterKey)
	if done {
		t.Fatalf("enter on first question should advance to question 2, not complete")
	}
	if state.CurrentIdx != 1 {
		t.Fatalf("expected current index to be 1, got %d", state.CurrentIdx)
	}

	// Test render while active
	th := NewDefaultTheme()
	rendered := state.Render(th, 80)
	if !strings.Contains(rendered, "Auth Setup") && !strings.Contains(rendered, "Question") {
		t.Fatalf("rendered output did not contain question header: %s", rendered)
	}

	// Press Enter on final question to submit
	done, answers, cancelled := state.HandleKey(enterKey)
	if !done {
		t.Fatalf("enter on final question should complete modal")
	}
	if cancelled {
		t.Fatalf("submission should not be cancelled")
	}
	if len(answers) != 2 {
		t.Fatalf("expected 2 answers, got %d", len(answers))
	}
	if answers[0] != "SQLite" || answers[1] != "Yes" {
		t.Fatalf("unexpected answers: %v", answers)
	}
}
