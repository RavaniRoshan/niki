package tui

import (
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/mention"
)

func TestMentionQueryAndReplace(t *testing.T) {
	input := "Please inspect @main.go and fix it"
	cursor := len("Please inspect @main.go")

	q, ok := ComputeMentionQuery(input, cursor)
	if !ok || q != "main.go" {
		t.Fatalf("expected query 'main.go', got %q (ok=%v)", q, ok)
	}

	replaced, newPos := ReplaceMentionWord(input, cursor, "cmd/nikicode/main.go")
	if !strings.Contains(replaced, "@cmd/nikicode/main.go") {
		t.Fatalf("unexpected replaced string: %s", replaced)
	}
	if newPos <= 0 {
		t.Fatalf("invalid new cursor position: %d", newPos)
	}
}

func TestRenderMentionOverlay(t *testing.T) {
	th := NewDefaultTheme()
	cands := []mention.Candidate{
		{Path: "cmd/nikicode/main.go", Score: 100},
		{Path: "internal/tui/app.go", Score: 80},
	}
	out := RenderMentionOverlay(cands, 0, th, 80)
	if !strings.Contains(out, "cmd/nikicode/main.go") {
		t.Fatalf("expected candidate in overlay output, got: %s", out)
	}
}
