package mention

import (
	"os"
	"path/filepath"
	"testing"
)

// C7: @file and @symbol mentions resolve — including gitignored
// paths (the picker walks the worktree, not the index).
func TestMentionsResolveGitignoredPaths(t *testing.T) {
	dir := t.TempDir()
	write := func(name, content string) {
		p := filepath.Join(dir, name)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write(".gitignore", "secret.env\nlocal/\n")
	write("secret.env", "TOKEN=x\n")
	write("local/notes.txt", "wip\n")
	write("main.go", "package main\n")

	found := Extract("see @secret.env and @local/notes.txt and @main")
	if len(found) != 3 {
		t.Fatalf("[C7] extracted = %v, want 3 mentions", found)
	}
	for _, m := range []string{"secret.env", "local/notes.txt", "main"} {
		c, err := Resolve(dir, m)
		if err != nil {
			t.Fatalf("[C7] gitignored-or-normal @%s unresolved: %v", m, err)
		}
		t.Logf("[C7] @%s -> %s", m, c.Path)
	}
	// @symbol shape resolves to the defining file too.
	if _, err := Resolve(dir, "secret"); err != nil {
		t.Fatalf("[C7] fuzzy @secret unresolved: %v", err)
	}
	t.Logf("[C7] mentions resolve incl. gitignored paths")
}
