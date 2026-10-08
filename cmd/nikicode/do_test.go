package main

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/paths"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/recipes"
	"github.com/RavaniRoshan/niki/internal/tools"
)

func doFixture(t *testing.T, files map[string]string) (string, *tools.Registry, *permissions.Guard, []recipes.Recipe) {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	paths.Reset()
	dir := t.TempDir()
	base := map[string]string{
		"go.mod":  "module fixture\n\ngo 1.24\n",
		"main.go": "package main\n\nfunc main() {}\n",
	}
	for k, v := range files {
		base[k] = v
	}
	for rel, content := range base {
		p := filepath.Join(dir, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	all, err := recipes.Discover("")
	if err != nil {
		t.Fatal(err)
	}
	return dir, tools.DefaultRegistry(), permissions.NewGuard(permissions.ModeFullAccess), all
}

func TestDoPlanPreview(t *testing.T) {
	dir, reg, guard, all := doFixture(t, nil)
	out, err := doRun(reg, guard, all, dir, "run the tests", true)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(out, "Plan for test") || !strings.Contains(out, "shell") || !strings.Contains(out, "dry run") {
		t.Fatalf("plan preview missing:\n%s", out)
	}
}

func TestDoMultistepOrder(t *testing.T) {
	dir, reg, guard, all := doFixture(t, map[string]string{
		"tracked.txt": "v1\n",
	})
	init := exec.Command("git", "init", "-b", "main")
	init.Dir = dir
	if out, err := init.CombinedOutput(); err != nil {
		t.Fatalf("%v %s", err, out)
	}
	for _, args := range [][]string{
		{"config", "user.email", "t@example.com"},
		{"config", "user.name", "T"},
		{"add", "tracked.txt"},
		{"commit", "-m", "seed"},
	} {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("%v %s", err, out)
		}
	}
	// Dirty the tree so status has something to show.
	if err := os.WriteFile(dir+"/tracked.txt", []byte("v1\nv2\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	out, err := doRun(reg, guard, all, dir, "show status then show recent commits", false)
	if err != nil {
		t.Fatalf("chain stopped: %v\n%s", err, out)
	}
	i1 := strings.Index(out, "step 1")
	i2 := strings.Index(out, "step 2")
	if i1 < 0 || i2 < 0 || i1 > i2 {
		t.Fatalf("steps out of order:\n%s", out)
	}
	if !strings.Contains(out, "tracked.txt") || !strings.Contains(out, "seed") {
		t.Fatalf("step outputs missing:\n%s", out)
	}
}

func TestDoCorrectionReroutes(t *testing.T) {
	dir, reg, guard, all := doFixture(t, nil)
	if _, err := doRun(reg, guard, all, dir, "scaffold name=alpha Name=Alpha", false); err != nil {
		t.Fatal(err)
	}
	out, err := doRun(reg, guard, all, dir, "no, name=beta Name=Beta", false)
	if err != nil {
		t.Fatalf("correction refused: %v\n%s", err, out)
	}
	for _, f := range []string{"beta.go", "beta_test.go"} {
		if _, err := os.Stat(filepath.Join(dir, f)); err != nil {
			t.Fatalf("correction did not re-run with new vars: %v\n%s", err, out)
		}
	}
}

func TestDoUndoRedo(t *testing.T) {
	dir, reg, guard, all := doFixture(t, nil)
	if _, err := doRun(reg, guard, all, dir, "scaffold name=gadget Name=Gadget", false); err != nil {
		t.Fatal(err)
	}
	target := filepath.Join(dir, "gadget.go")
	if _, err := os.Stat(target); err != nil {
		t.Fatal("scaffold did not create the file")
	}
	msg, err := doRun(reg, guard, all, dir, "undo", false)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(msg, "undid") {
		t.Fatalf("undo message = %q", msg)
	}
	if _, err := os.Stat(target); !os.IsNotExist(err) {
		t.Fatal("undo did not remove the created file")
	}
	msg, err = doRun(reg, guard, all, dir, "redo", false)
	if err != nil {
		t.Fatalf("redo failed: %v", err)
	}
	if !strings.Contains(msg, "Plan for scaffold") {
		t.Fatalf("redo did not re-execute: %q", msg)
	}
	if _, err := os.Stat(target); err != nil {
		t.Fatalf("redo did not recreate the file: %v", err)
	}
}

func TestDoPronounFollowUp(t *testing.T) {
	dir, reg, guard, all := doFixture(t, map[string]string{
		"calc.go": "package main\n\n// Total sums inputs.\nfunc Total(xs []int) int {\n\tsum := 0\n\tfor _, x := range xs {\n\t\tsum += x\n\t}\n\treturn sum\n}\n",
	})
	out, err := doRun(reg, guard, all, dir, "explain `Total`", false)
	if err != nil || !strings.Contains(out, "calc.go:") {
		t.Fatalf("explain = %q %v", out, err)
	}
	out, err = doRun(reg, guard, all, dir, "explain it", false)
	if err != nil || !strings.Contains(out, "calc.go:") {
		t.Fatalf("pronoun follow-up lost context: %q %v", out, err)
	}
}

func TestDoMention(t *testing.T) {
	dir, reg, guard, all := doFixture(t, map[string]string{
		"worker.go": "package main\n\n// Work does work.\nfunc Work() {}\n",
	})
	out, err := doRun(reg, guard, all, dir, "explain @worker.go", false)
	if err != nil || !strings.Contains(out, "worker.go") {
		t.Fatalf("mention not resolved: %q %v", out, err)
	}
}

func TestDoScriptedSet(t *testing.T) {
	// Plain-language tasks complete without naming any tool.
	dir, reg, guard, all := doFixture(t, map[string]string{
		"calc.go": "package main\n\nfunc Total() int { return 1 }\n",
	})
	tasks := []string{
		"run the tests",
		"check the formatting",
		"build the project",
		"write the docs",
		"explain `Total`",
		"what does Total do",
		"show status",
	}
	// Make it a repo for the status task.
	cmd := exec.Command("git", "init", "-b", "main")
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("%v %s", err, out)
	}
	for _, task := range tasks {
		out, err := doRun(reg, guard, all, dir, task, false)
		if err != nil {
			t.Fatalf("task %q failed: %v\n%s", task, err, out)
		}
		if out == "" {
			t.Fatalf("task %q produced no output", task)
		}
	}
}

func TestDoCorrectionPicksRecipeByVars(t *testing.T) {
	dir, reg, guard, all := doFixture(t, nil)
	// Run two recipes: scaffold, then build. A bare-var correction must
	// re-run the recipe that uses those vars (scaffold), not the last one.
	if _, err := doRun(reg, guard, all, dir, "scaffold name=one Name=One", false); err != nil {
		t.Fatal(err)
	}
	if _, err := doRun(reg, guard, all, dir, "build the project", false); err != nil {
		t.Fatal(err)
	}
	out, err := doRun(reg, guard, all, dir, "no, name=two Name=Two", false)
	if err != nil {
		t.Fatalf("correction refused: %v\n%s", err, out)
	}
	if !strings.Contains(out, "Plan for scaffold") {
		t.Fatalf("correction re-ran the wrong recipe:\n%s", out)
	}
	if _, err := os.Stat(filepath.Join(dir, "two.go")); err != nil {
		t.Fatalf("corrected vars not applied: %v", err)
	}
}
