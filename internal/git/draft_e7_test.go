package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// E7: commit drafting and PR-description drafting from the run's
// evidence. Nothing is pushed (the fixture has no remote at all,
// and both drafts succeed using log/diff only).
func TestDraftCommitAndPRFromEvidence(t *testing.T) {
	if !Available() {
		t.Skip("git not available")
	}
	dir := t.TempDir()
	run := func(args ...string) string {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
		return strings.TrimSpace(string(out))
	}
	run("init", "-q", "-b", "main")
	run("config", "user.email", "e7@test")
	run("config", "user.name", "e7")
	if run("remote", "-v") != "" {
		t.Fatalf("[E7] fixture must have no remote")
	}
	sub := filepath.Join(dir, "auth")
	if err := os.MkdirAll(sub, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(sub, "login.go"), []byte("package auth\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", ".")
	run("commit", "-qm", "auth: add login handler")

	// Draft a commit message from staged evidence.
	if err := os.WriteFile(filepath.Join(sub, "login.go"), []byte("package auth\n\nfunc Login() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", ".")
	msg, err := ProposeCommitMessage(dir)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(msg, "auth") || !strings.Contains(msg, "login.go") {
		t.Fatalf("[E7] commit draft not from evidence:\n%s", msg)
	}
	t.Logf("[E7] commit draft from staged evidence: %q", strings.Split(msg, "\n")[0])

	// Draft a PR description from branch evidence.
	run("commit", "-qm", "auth: wire login handler")
	run("checkout", "-qb", "feature")
	if err := os.WriteFile(filepath.Join(sub, "login.go"), []byte("package auth\n\nfunc Login() {}\n\nfunc Logout() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", ".")
	run("commit", "-qm", "auth: add logout")
	pr, err := PRSummary(dir, "main")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(pr, "auth: add logout") || !strings.Contains(pr, "login.go") {
		t.Fatalf("[E7] PR draft not from evidence:\n%s", pr)
	}
	if got := run("log", "--oneline", "main..feature"); !strings.Contains(got, "logout") {
		t.Fatalf("[E7] branch evidence missing")
	}
	t.Logf("[E7] PR draft lists branch commits + diffstat; no remote exists, nothing pushed")
}
