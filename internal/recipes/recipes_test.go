package recipes

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/tools"
)

func testRegistry(t *testing.T) *tools.Registry {
	t.Helper()
	return tools.DefaultRegistry()
}

func fullAccess() *permissions.Guard {
	return permissions.NewGuard(permissions.ModeFullAccess)
}

// seedModule writes a minimal Go module with one test.
func seedModule(t *testing.T, extra map[string]string) string {
	t.Helper()
	dir := t.TempDir()
	files := map[string]string{
		"go.mod":  "module fixture\n\ngo 1.24\n",
		"main.go": "package main\n\nfunc main() {}\n\nfunc Add(a, b int) int { return a + b }\n",
	}
	for k, v := range extra {
		files[k] = v
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
	return dir
}

func findRecipe(t *testing.T, name string) Recipe {
	t.Helper()
	all, err := Discover("")
	if err != nil {
		t.Fatal(err)
	}
	for _, r := range all {
		if r.Name == name {
			return r
		}
	}
	t.Fatalf("recipe %q not found", name)
	return Recipe{}
}

func TestDiscoverAllSeven(t *testing.T) {
	all, err := Discover("")
	if err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"test", "lint", "build", "commit", "scaffold", "refactor", "docs"} {
		found := false
		for _, r := range all {
			if r.Name == name {
				found = true
				if len(r.Steps) == 0 || len(r.Match) == 0 {
					t.Fatalf("recipe %q has no steps or match phrases", name)
				}
			}
		}
		if !found {
			t.Fatalf("recipe %q missing", name)
		}
	}
}

func TestMatchPhrases(t *testing.T) {
	all, err := Discover("")
	if err != nil {
		t.Fatal(err)
	}
	cases := map[string]string{
		"please run the tests":       "test",
		"lint this for me":           "lint",
		"build the project":          "build",
		"commit my changes":          "commit",
		"scaffold a worker":          "scaffold",
		"rename the OldSymbol":       "refactor",
		"generate docs for this":     "docs",
		"do something unrelated xyz": "",
	}
	for input, want := range cases {
		got, ok := Match(all, input)
		if want == "" {
			if ok {
				t.Fatalf("input %q matched %q, want none", input, got.Name)
			}
			continue
		}
		if !ok || got.Name != want {
			t.Fatalf("input %q matched %q, want %q", input, got.Name, want)
		}
	}
}

func TestRecipeTest(t *testing.T) {
	dir := seedModule(t, map[string]string{
		"main_test.go": "package main\n\nimport \"testing\"\n\nfunc TestAdd(t *testing.T) {\n\tif Add(1, 2) != 3 {\n\t\tt.Fatal(\"bad\")\n\t}\n}\n",
	})
	rep, err := Execute(context.Background(), testRegistry(t), fullAccess(), findRecipe(t, "test"), map[string]string{"dir": dir})
	if err != nil {
		t.Fatal(err)
	}
	if rep.Refused {
		t.Fatalf("refused: %s", rep.Reason)
	}
	if !strings.Contains(rep.Steps[0].Output, "ok") {
		t.Fatalf("no passing output:\n%s", rep.Steps[0].Output)
	}
}

func TestRecipeLint(t *testing.T) {
	dir := seedModule(t, map[string]string{
		"bad.go": "package main\n\nfunc  Bad(  ) int  {  return 1 }\n",
	})
	rep, err := Execute(context.Background(), testRegistry(t), fullAccess(), findRecipe(t, "lint"), map[string]string{"dir": dir})
	if err != nil {
		t.Fatal(err)
	}
	if rep.Refused {
		t.Fatalf("refused: %s", rep.Reason)
	}
	// gofmt must flag the badly formatted file.
	if !strings.Contains(rep.Steps[0].Output, "bad.go") {
		t.Fatalf("lint missed the unformatted file:\n%s", rep.Steps[0].Output)
	}
}

func TestRecipeBuild(t *testing.T) {
	dir := seedModule(t, nil)
	rep, err := Execute(context.Background(), testRegistry(t), fullAccess(), findRecipe(t, "build"), map[string]string{"dir": dir})
	if err != nil {
		t.Fatal(err)
	}
	if rep.Refused {
		t.Fatalf("refused: %s", rep.Reason)
	}
}

func TestRecipeCommit(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("system git not available")
	}
	dir := t.TempDir()
	run := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0")
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	run("init", "-b", "main")
	run("config", "user.email", "t@example.com")
	run("config", "user.name", "T")
	if err := os.WriteFile(filepath.Join(dir, "f.txt"), []byte("v1\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", "f.txt")
	// Nothing staged beyond the seed: refuse first.
	run("commit", "-m", "seed")
	rep, err := Execute(context.Background(), testRegistry(t), fullAccess(), findRecipe(t, "commit"), map[string]string{"dir": dir})
	if err != nil {
		t.Fatal(err)
	}
	if !rep.Refused {
		t.Fatal("commit recipe should refuse with nothing staged")
	}
	// Stage a change: the recipe commits with a derived message.
	if err := os.WriteFile(filepath.Join(dir, "f.txt"), []byte("v1\nv2\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", "f.txt")
	rep, err = Execute(context.Background(), testRegistry(t), fullAccess(), findRecipe(t, "commit"), map[string]string{"dir": dir})
	if err != nil {
		t.Fatal(err)
	}
	if rep.Refused {
		t.Fatalf("refused: %s", rep.Reason)
	}
	cmd := exec.Command("git", "log", "-1", "--pretty=%s")
	cmd.Dir = dir
	out, err := cmd.Output()
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(out), "f: update 1 file") {
		t.Fatalf("commit message not derived from diff: %s", out)
	}
}

func TestRecipeScaffold(t *testing.T) {
	dir := seedModule(t, nil)
	rep, err := Execute(context.Background(), testRegistry(t), fullAccess(), findRecipe(t, "scaffold"),
		map[string]string{"dir": dir, "name": "worker", "Name": "Worker"})
	if err != nil {
		t.Fatal(err)
	}
	if rep.Refused {
		t.Fatalf("refused: %s\n%+v", rep.Reason, rep)
	}
	for _, f := range []string{"worker.go", "worker_test.go"} {
		data, err := os.ReadFile(filepath.Join(dir, f))
		if err != nil {
			t.Fatalf("scaffolded file missing: %s", f)
		}
		if !strings.Contains(string(data), "Worker") {
			t.Fatalf("%s lacks the symbol:\n%s", f, data)
		}
	}
	// Missing vars refuse instead of writing broken files.
	rep, err = Execute(context.Background(), testRegistry(t), fullAccess(), findRecipe(t, "scaffold"), map[string]string{"dir": dir})
	if err != nil {
		t.Fatal(err)
	}
	if !rep.Refused {
		t.Fatal("scaffold without vars should refuse")
	}
}

func TestRecipeRefactor(t *testing.T) {
	dir := seedModule(t, map[string]string{
		"a.go": "package main\n\nfunc OldSymbol() int { return 1 }\n",
		"b.go": "package main\n\nfunc Use() int { return OldSymbol() }\n",
	})
	rep, err := Execute(context.Background(), testRegistry(t), fullAccess(), findRecipe(t, "refactor"),
		map[string]string{"dir": dir, "old": "OldSymbol", "new": "NewSymbol"})
	if err != nil {
		t.Fatal(err)
	}
	if rep.Refused {
		t.Fatalf("refused: %s\n%+v", rep.Reason, rep)
	}
	if !strings.Contains(rep.Steps[len(rep.Steps)-1].Output, "REFACTOR-OK") {
		t.Fatalf("refactor did not verify:\n%+v", rep.Steps)
	}
	for _, f := range []string{"a.go", "b.go"} {
		data, _ := os.ReadFile(filepath.Join(dir, f))
		if strings.Contains(string(data), "OldSymbol") || !strings.Contains(string(data), "NewSymbol") {
			t.Fatalf("%s not renamed:\n%s", f, data)
		}
	}
}

func TestRecipeDocs(t *testing.T) {
	dir := seedModule(t, map[string]string{
		"doc.go": "package main\n\n// Add adds two integers.\nfunc Add2(a, b int) int { return a + b }\n",
	})
	rep, err := Execute(context.Background(), testRegistry(t), fullAccess(), findRecipe(t, "docs"), map[string]string{"dir": dir})
	if err != nil {
		t.Fatal(err)
	}
	if rep.Refused {
		t.Fatalf("refused: %s", rep.Reason)
	}
	data, err := os.ReadFile(filepath.Join(dir, "GODOC.md"))
	if err != nil {
		t.Fatalf("GODOC.md missing: %v", err)
	}
	if !strings.Contains(string(data), "Add") {
		t.Fatalf("docs lack the exported symbol:\n%s", data)
	}
}

func TestExecutePermissionGate(t *testing.T) {
	dir := seedModule(t, nil)
	// Read-only guard: the test recipe's shell step must be denied.
	guard := permissions.NewGuard(permissions.ModeReadOnly)
	rep, err := Execute(context.Background(), testRegistry(t), guard, findRecipe(t, "test"), map[string]string{"dir": dir})
	if err != nil {
		t.Fatal(err)
	}
	if !rep.Refused || !strings.Contains(rep.Reason, "denied by permission gate") {
		t.Fatalf("read-only guard should deny shell: %+v", rep)
	}
}
