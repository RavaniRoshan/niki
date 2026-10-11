package skills

import (
	"os"
	"path/filepath"
	"testing"
)

// C1: instruction files load in a hierarchy — global, then
// repository root, then the directory of the file being worked
// on — and contents load lazily (paths resolve without reading).
func TestInstructionSetHierarchyAndLazy(t *testing.T) {
	base := t.TempDir()
	cfg := filepath.Join(base, "cfg")
	repo := filepath.Join(base, "repo")
	sub := filepath.Join(repo, "sub")
	for _, d := range []string{cfg, repo, sub} {
		if err := os.MkdirAll(d, 0o755); err != nil {
			t.Fatal(err)
		}
	}
	write := func(p, c string) {
		if err := os.WriteFile(p, []byte(c), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write(filepath.Join(cfg, "AGENTS.md"), "global rules")
	write(filepath.Join(repo, "AGENTS.md"), "repo rules")
	write(filepath.Join(sub, "AGENTS.md"), "dir rules")

	set := ResolveInstructionSet(sub, cfg, 1<<20)
	want := []string{
		filepath.Join(cfg, "AGENTS.md"),
		filepath.Join(repo, "AGENTS.md"),
		filepath.Join(sub, "AGENTS.md"),
	}
	if len(set.Paths) != len(want) {
		t.Fatalf("paths = %v, want %v", set.Paths, want)
	}
	for i := range want {
		if set.Paths[i] != want[i] {
			t.Fatalf("paths = %v, want %v", set.Paths, want)
		}
	}
	if set.Loaded {
		t.Fatalf("resolving must not read contents")
	}

	// Laziness proof: delete a file after resolving — Load must
	// simply skip it instead of having a stale copy.
	if err := os.Remove(filepath.Join(repo, "AGENTS.md")); err != nil {
		t.Fatal(err)
	}
	got := set.Load()
	if len(got) != 2 || got[0].Content != "global rules" || got[1].Content != "dir rules" {
		paths := []string{}
		for _, g := range got {
			paths = append(paths, g.Path)
		}
		t.Fatalf("loaded = %v", paths)
	}
	t.Logf("[C1] hierarchy global->root->dir, contents lazy (deleted file skipped at Load)")

	// The right set per directory: at the repo root the sub
	// directory's file is not included.
	write(filepath.Join(repo, "AGENTS.md"), "repo rules")
	rootSet := ResolveInstructionSet(repo, cfg, 1<<20)
	if len(rootSet.Paths) != 2 {
		t.Fatalf("root paths = %v, want global+root only", rootSet.Paths)
	}
	t.Logf("[C1] per-directory set: root sees %d files, sub sees %d", len(rootSet.Paths), len(set.Paths))
}
