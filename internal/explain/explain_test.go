package explain

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// seedFixture builds a small repo with known symbols.
func seedFixture(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	files := map[string]string{
		"engine/turn.go": "package engine\n\n// RunTurn executes one agent turn.\nfunc RunTurn(prompt string) string {\n\treturn prompt\n}\n",
		"engine/loop.go": "package engine\n\nfunc Loop() {\n\tRunTurn(\"hi\")\n}\n",
		"README.md":      "# Fixture\n\nRunTurn is the entry point.\n",
	}
	for rel, content := range files {
		p := filepath.Join(dir, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	if err := os.MkdirAll(filepath.Join(dir, ".git"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, ".git", " decoy"), []byte("RunTurn decoy"), 0o644); err != nil {
		t.Fatal(err)
	}
	return dir
}

func TestExplainSymbolCitesRealLines(t *testing.T) {
	dir := seedFixture(t)
	ans := ExplainSymbol(dir, "RunTurn")
	if ans.Refused {
		t.Fatalf("refused a present symbol: %s", ans.Reason)
	}
	if len(ans.Locations) == 0 {
		t.Fatal("no locations for a present symbol")
	}
	// Every citation must exist on disk with matching content.
	for _, l := range ans.Locations {
		data, err := os.ReadFile(filepath.Join(dir, l.Path))
		if err != nil {
			t.Fatalf("cited file missing: %s", l.Path)
		}
		lines := strings.Split(string(data), "\n")
		if l.Line < 1 || l.Line > len(lines) {
			t.Fatalf("cited line out of range: %s:%d", l.Path, l.Line)
		}
		if strings.TrimSpace(lines[l.Line-1]) != l.Text {
			t.Fatalf("cited text mismatch at %s:%d: %q vs %q", l.Path, l.Line, l.Text, lines[l.Line-1])
		}
	}
	// The definition is identified.
	if !strings.Contains(ans.Summary, "engine/turn.go") || !strings.Contains(ans.Summary, "func RunTurn") {
		t.Fatalf("summary misses the definition:\n%s", ans.Summary)
	}
	formatted := Format(ans)
	if !strings.Contains(formatted, "engine/turn.go:") {
		t.Fatalf("formatted answer lacks file:line citations:\n%s", formatted)
	}
}

func TestExplainUnknownSymbolRefused(t *testing.T) {
	dir := seedFixture(t)
	ans := ExplainSymbol(dir, "NoSuchSymbolAnywhere")
	if !ans.Refused {
		t.Fatalf("invented an answer for a missing symbol: %+v", ans)
	}
	if ans.Reason == "" || len(ans.Searched) == 0 {
		t.Fatalf("refusal lacks reason/scope: %+v", ans)
	}
	if len(ans.Locations) != 0 {
		t.Fatalf("refusal carries locations: %+v", ans)
	}
}

func TestExplainFileOutline(t *testing.T) {
	dir := seedFixture(t)
	ans := ExplainFile(dir, "engine/turn.go")
	if ans.Refused {
		t.Fatalf("refused a present file: %s", ans.Reason)
	}
	if !strings.Contains(ans.Summary, "func RunTurn") {
		t.Fatalf("outline misses the function:\n%s", ans.Summary)
	}
	if ans.Locations[0].Line != 1 {
		t.Fatalf("file anchor wrong: %+v", ans.Locations[0])
	}
	if ans := ExplainFile(dir, "nope/missing.go"); !ans.Refused {
		t.Fatal("missing file should refuse")
	}
}

func TestAnswerQuestionRouting(t *testing.T) {
	dir := seedFixture(t)
	for _, q := range []string{"explain `RunTurn`", "what does RunTurn do?", "where is RunTurn?"} {
		ans := AnswerQuestion(dir, q)
		if ans.Refused || len(ans.Locations) == 0 {
			t.Fatalf("question %q refused: %s", q, ans.Reason)
		}
	}
	if ans := AnswerQuestion(dir, "explain `NoSuchThing`"); !ans.Refused {
		t.Fatal("unknown backticked symbol should refuse")
	}
	if ans := AnswerQuestion(dir, ""); !ans.Refused {
		t.Fatal("empty question should refuse")
	}
}
