package journal

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/git"
	"github.com/RavaniRoshan/niki/internal/paths"
	"github.com/RavaniRoshan/niki/internal/recipes"
)

func testHome(t *testing.T) {
	t.Helper()
	home := t.TempDir()
	t.Setenv("HOME", home)
	paths.Reset()
}

func initRepo(t *testing.T) string {
	t.Helper()
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
	if err := os.WriteFile(filepath.Join(dir, "a.txt"), []byte("one\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", "a.txt")
	run("commit", "-m", "seed")
	return dir
}

func TestUndoRestoresFile(t *testing.T) {
	testHome(t)
	dir := t.TempDir()
	target := filepath.Join(dir, "new.txt")
	if err := os.WriteFile(target, []byte("created"), 0o644); err != nil {
		t.Fatal(err)
	}
	before := git.Head(dir)
	if err := Append(Entry{Kind: "recipe", Name: "scaffold", Dir: dir,
		Effects:    []recipes.FileEffect{{Path: target, After: []byte("created")}},
		HeadBefore: before, HeadAfter: before}); err != nil {
		t.Fatal(err)
	}
	msg, err := Undo()
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(msg, "removed") {
		t.Fatalf("undo message = %q", msg)
	}
	if _, err := os.Stat(target); !os.IsNotExist(err) {
		t.Fatal("created file not removed by undo")
	}
	// Nothing left to undo.
	if _, err := Undo(); err == nil {
		t.Fatal("second undo should refuse")
	}
}

func TestUndoRestoresEditedFile(t *testing.T) {
	testHome(t)
	dir := t.TempDir()
	target := filepath.Join(dir, "f.txt")
	if err := os.WriteFile(target, []byte("v1"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Append(Entry{Kind: "recipe", Name: "refactor", Dir: dir,
		Effects: []recipes.FileEffect{{Path: target, Existed: true, Before: []byte("v1"), After: []byte("v2"), Mode: 0o644}}}); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(target, []byte("v2"), 0o644); err != nil {
		t.Fatal(err)
	}
	msg, err := Undo()
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(msg, "restored") {
		t.Fatalf("undo message = %q", msg)
	}
	got, _ := os.ReadFile(target)
	if string(got) != "v1" {
		t.Fatalf("content = %q", got)
	}
}

func TestUndoRefusesChangedFile(t *testing.T) {
	testHome(t)
	dir := t.TempDir()
	target := filepath.Join(dir, "f.txt")
	if err := os.WriteFile(target, []byte("v1"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Append(Entry{Kind: "recipe", Name: "refactor", Dir: dir,
		Effects: []recipes.FileEffect{{Path: target, Existed: true, Before: []byte("v1"), After: []byte("v2"), Mode: 0o644}}}); err != nil {
		t.Fatal(err)
	}
	// User edits after the action: undo must not clobber.
	if err := os.WriteFile(target, []byte("v2-user-edit"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := Undo(); err == nil {
		t.Fatal("undo over user edits should refuse")
	}
	got, _ := os.ReadFile(target)
	if string(got) != "v2-user-edit" {
		t.Fatalf("user edit clobbered: %q", got)
	}
}

func TestUndoCommitResetsTip(t *testing.T) {
	testHome(t)
	dir := initRepo(t)
	before := git.Head(dir)
	if err := os.WriteFile(filepath.Join(dir, "a.txt"), []byte("one\ntwo\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command("git", "add", "a.txt")
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("%v %s", err, out)
	}
	cmd = exec.Command("git", "commit", "-m", "second")
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("%v %s", err, out)
	}
	after := git.Head(dir)
	if before == after {
		t.Fatal("fixture commit failed")
	}
	if err := Append(Entry{Kind: "git", Name: "commit", Dir: dir, HeadBefore: before, HeadAfter: after}); err != nil {
		t.Fatal(err)
	}
	msg, err := Undo()
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(msg, "tip restored") {
		t.Fatalf("undo message = %q", msg)
	}
	if got := git.Head(dir); got != before {
		t.Fatalf("tip not restored: %s", got)
	}
	// Changes stay staged: redo (recommit) is possible.
	staged, err := git.StagedFiles(dir)
	if err != nil || len(staged) != 1 {
		t.Fatalf("staged = %v %v", staged, err)
	}
}

func TestUndoRefusesMovedHistory(t *testing.T) {
	testHome(t)
	dir := initRepo(t)
	before := git.Head(dir)
	if err := Append(Entry{Kind: "git", Name: "commit", Dir: dir, HeadBefore: before, HeadAfter: "deadbee"}); err != nil {
		t.Fatal(err)
	}
	if _, err := Undo(); err == nil {
		t.Fatal("undo with moved history should refuse")
	}
}

func TestRedoEntry(t *testing.T) {
	testHome(t)
	if _, err := RedoEntry(); err == nil {
		t.Fatal("redo with no history should refuse")
	}
	if err := Append(Entry{Kind: "recipe", Name: "scaffold", Dir: "/tmp/x"}); err != nil {
		t.Fatal(err)
	}
	if _, err := Undo(); err != nil {
		t.Fatal(err)
	}
	got, err := RedoEntry()
	if err != nil {
		t.Fatal(err)
	}
	if got.Name != "scaffold" || got.Undone {
		t.Fatalf("redo entry = %+v", got)
	}
}

func TestLastContext(t *testing.T) {
	testHome(t)
	if _, err := Last(); err != nil {
		t.Fatal(err)
	}
	if last, _ := Last(); last != nil {
		t.Fatal("empty journal should have no last entry")
	}
	if err := Append(Entry{Kind: "explain", Name: "explain", Dir: "/tmp/x", Extra: map[string]string{"subject": "RunTurn"}}); err != nil {
		t.Fatal(err)
	}
	last, err := Last()
	if err != nil || last.Extra["subject"] != "RunTurn" {
		t.Fatalf("last = %+v %v", last, err)
	}
}
