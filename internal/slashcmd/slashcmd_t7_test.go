package slashcmd

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// T7: custom slash commands from Markdown, with argument substitution.
func TestCustomSlashCommandArgumentSubstitution(t *testing.T) {
	dir := t.TempDir()
	cmds := filepath.Join(dir, ".nikicode", "commands")
	if err := os.MkdirAll(cmds, 0o755); err != nil {
		t.Fatal(err)
	}
	tmpl := "---\nname: review\ndescription: review a PR\n---\n" +
		"Review PR #$1 focusing on $2. Full context: $ARGUMENTS"
	if err := os.WriteFile(filepath.Join(cmds, "review.md"), []byte(tmpl), 0o644); err != nil {
		t.Fatal(err)
	}

	found, err := Discover(StandardDirs(dir)...)
	if err != nil {
		t.Fatal(err)
	}
	if len(found) != 1 || found[0].Name != "review" {
		t.Fatalf("[T7] command not discovered: %+v", found)
	}
	c := found[0]
	if c.Description != "review a PR" {
		t.Fatalf("[T7] description not parsed: %q", c.Description)
	}
	t.Logf("[T7] discovered /%s from project commands dir", c.Name)

	// Argument substitution: positional + $ARGUMENTS.
	got := c.Render("42 security")
	wantContains := []string{"#42", "security", "42 security"}
	for _, w := range wantContains {
		if !strings.Contains(got, w) {
			t.Fatalf("[T7] substitution missing %q:\n%s", w, got)
		}
	}
	t.Logf("[T7] /review 42 security -> %q", got)

	// Missing positional renders empty (no panic, no literal $2).
	got2 := c.Render("onlyone")
	if strings.Contains(got2, "$2") || strings.Contains(got2, "$1") || strings.Contains(got2, "$ARGUMENTS") {
		t.Fatalf("[T7] unsubstituted placeholder left: %q", got2)
	}
	t.Logf("[T7] missing positional renders empty: %q", got2)

	// No re-expansion: an argument whose value is itself "$1" is
	// inserted verbatim, not expanded again (single left-to-right pass).
	c2 := Command{Name: "echo", Template: "say $1 and $2"}
	if got := c2.Render("$1 done"); got != "say $1 and done" {
		t.Fatalf("[T7] substituted value was re-expanded: %q", got)
	}
	t.Logf("[T7] single-pass substitution (no re-expansion): %q", c2.Render("$1 done"))

	// Braced form ${1} works too.
	c3 := Command{Name: "b", Template: "hi ${1}"}
	if got := c3.Render("there"); got != "hi there" {
		t.Fatalf("[T7] braced positional failed: %q", got)
	}
	t.Logf("[T7] braced ${1} substitution works")
}
