package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// initRepo builds a throwaway repo with user config and one commit.
func initRepo(t *testing.T) string {
	t.Helper()
	if !Available() {
		t.Skip("system git not available")
	}
	dir := t.TempDir()
	run := func(args ...string) string {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0")
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
		return string(out)
	}
	run("init", "-b", "main")
	run("config", "user.email", "test@example.com")
	run("config", "user.name", "Test")
	if err := os.WriteFile(filepath.Join(dir, "hello.txt"), []byte("line one\nline two\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", "hello.txt")
	run("commit", "-m", "initial commit")
	return dir
}

func TestStatusAndCommitFromStagedDiff(t *testing.T) {
	dir := initRepo(t)
	if err := os.WriteFile(filepath.Join(dir, "hello.txt"), []byte("line one\nline two\nline three\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "new.txt"), []byte("brand new\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	files, err := Status(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(files) == 0 {
		t.Fatal("status shows no changes")
	}
	if err := Stage(dir, "hello.txt", "new.txt"); err != nil {
		t.Fatal(err)
	}
	// The message is derived from the REAL staged diff.
	msg, err := ProposeCommitMessage(dir)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(msg, "hello.txt") || !strings.Contains(msg, "new.txt") {
		t.Fatalf("message not derived from staged diff:\n%s", msg)
	}
	if err := Commit(dir, msg); err != nil {
		t.Fatal(err)
	}
	entries, err := Log(dir, 5)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 2 || !strings.Contains(entries[0].Subject, "2 files") {
		t.Fatalf("commit missing from log: %+v", entries)
	}
	// Committing with nothing staged refuses.
	if err := Commit(dir, "empty"); err == nil {
		t.Fatal("commit with empty stage should refuse")
	}
}

func TestBranchCreateAndSwitch(t *testing.T) {
	dir := initRepo(t)
	if err := BranchCreate(dir, "feature"); err != nil {
		t.Fatal(err)
	}
	branch, err := CurrentBranch(dir)
	if err != nil || branch != "feature" {
		t.Fatalf("branch = %q, %v", branch, err)
	}
	if err := BranchSwitch(dir, "main"); err != nil {
		t.Fatal(err)
	}
	if branch, _ := CurrentBranch(dir); branch != "main" {
		t.Fatalf("branch = %q", branch)
	}
	if err := BranchCreate(dir, ""); err == nil {
		t.Fatal("empty branch name should refuse")
	}
}

func TestRebaseClean(t *testing.T) {
	dir := initRepo(t)
	if err := BranchCreate(dir, "feature"); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "feat.txt"), []byte("work\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command("git", "add", "feat.txt")
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("%v %s", err, out)
	}
	cmd = exec.Command("git", "commit", "-m", "feature work")
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("%v %s", err, out)
	}
	if err := Rebase(dir, "main"); err != nil {
		t.Fatal(err)
	}
	entries, err := Log(dir, 3)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) != 2 {
		t.Fatalf("rebase lost commits: %+v", entries)
	}
}

func TestRebaseConflictResolveContinue(t *testing.T) {
	dir := initRepo(t)
	run := func(args ...string) {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	// main moves the line one way…
	if err := os.WriteFile(filepath.Join(dir, "hello.txt"), []byte("line one\nline two MAIN\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("commit", "-am", "main side")
	// …feature moves it another way.
	run("checkout", "-b", "feature", "HEAD~1")
	if err := os.WriteFile(filepath.Join(dir, "hello.txt"), []byte("line one\nline two FEATURE\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("commit", "-am", "feature side")
	// Rebase must stop with a ConflictError, not fail opaquely.
	err := Rebase(dir, "main")
	conf, ok := err.(*ConflictError)
	if !ok {
		t.Fatalf("want *ConflictError, got %v (%T)", err, err)
	}
	if len(conf.Files) != 1 || conf.Files[0] != "hello.txt" {
		t.Fatalf("conflict files = %v", conf.Files)
	}
	unmerged, err := UnmergedFiles(dir)
	if err != nil || len(unmerged) != 1 {
		t.Fatalf("unmerged = %v, %v", unmerged, err)
	}
	// Resolve in favor of the feature side, stage, continue.
	if err := os.WriteFile(filepath.Join(dir, "hello.txt"), []byte("line one\nline two FEATURE\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Stage(dir, "hello.txt"); err != nil {
		t.Fatal(err)
	}
	if err := RebaseContinue(dir); err != nil {
		t.Fatal(err)
	}
	entries, err := Log(dir, 3)
	if err != nil {
		t.Fatal(err)
	}
	if len(entries) == 0 || entries[0].Subject != "feature side" {
		t.Fatalf("rebased history wrong: %+v", entries)
	}
	if files, _ := UnmergedFiles(dir); len(files) != 0 {
		t.Fatalf("repo left conflicted: %v", files)
	}
}

func TestChangelogAndBlameAndReview(t *testing.T) {
	dir := initRepo(t)
	cl, err := Changelog(dir, 5)
	if err != nil || !strings.Contains(cl, "initial commit") {
		t.Fatalf("changelog missing commit: %v\n%s", err, cl)
	}
	bl, err := Blame(dir, "hello.txt", 1)
	if err != nil {
		t.Fatal(err)
	}
	if bl.Author != "Test" || !strings.Contains(bl.Summary, "initial") || !strings.Contains(bl.Line, "line one") {
		t.Fatalf("blame wrong: %+v", bl)
	}
	if _, err := Blame(dir, "hello.txt", 99); err == nil {
		t.Fatal("blame of missing line should refuse")
	}
	if err := os.WriteFile(filepath.Join(dir, "hello.txt"), []byte("line one\nline two\nline three\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := Stage(dir, "hello.txt"); err != nil {
		t.Fatal(err)
	}
	reviews, err := ReviewStaged(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(reviews) != 1 || reviews[0].Path != "hello.txt" || reviews[0].Added != 1 {
		t.Fatalf("review wrong: %+v", reviews)
	}
	if !strings.Contains(reviews[0].Sample, "@@") {
		t.Fatalf("review missing hunk sample: %q", reviews[0].Sample)
	}
}

func TestPRDraft(t *testing.T) {
	dir := initRepo(t)
	if err := BranchCreate(dir, "feature"); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "feat.txt"), []byte("work\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	cmd := exec.Command("git", "add", "feat.txt")
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("%v %s", err, out)
	}
	cmd = exec.Command("git", "commit", "-m", "feature work")
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("%v %s", err, out)
	}
	draft, err := PRDraft(dir, "main", "PR_DRAFT.md")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(draft, "feature work") || !strings.Contains(draft, "feature") {
		t.Fatalf("draft missing commits/branch:\n%s", draft)
	}
	saved, err := os.ReadFile(filepath.Join(dir, "PR_DRAFT.md"))
	if err != nil || string(saved) != draft {
		t.Fatalf("draft not saved: %v", err)
	}
}

func TestNotARepoRefuses(t *testing.T) {
	if !Available() {
		t.Skip("system git not available")
	}
	dir := t.TempDir()
	if InWorkTree(dir) {
		t.Fatal("empty dir reported as work tree")
	}
	if _, err := Status(dir); err == nil {
		t.Fatal("status outside a repo should refuse")
	}
	if err := Commit(dir, "x"); err == nil {
		t.Fatal("commit outside a repo should refuse")
	}
}
