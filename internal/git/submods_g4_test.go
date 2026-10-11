package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

// gitRun returns a closure that runs git in dir, fataling on error.
// GIT_CONFIG_GLOBAL is blanked so the test host's security defaults
// do not block the local file:// transport used for the submodule fixture.
func gitRun(t *testing.T, dir string) func(args ...string) string {
	t.Helper()
	return func(args ...string) string {
		t.Helper()
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		cmd.Env = append(os.Environ(),
			"GIT_TERMINAL_PROMPT=0",
			"GIT_CONFIG_GLOBAL=/dev/null",
			"GIT_CONFIG_COUNT=1",
			"GIT_CONFIG_KEY_0=protocol.file.allow",
			"GIT_CONFIG_VALUE_0=always",
		)
		out, err := cmd.CombinedOutput()
		if err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
		return string(out)
	}
}

// G4: submodule + nested-repo detection. Two fixtures: a registered
// submodule (reported with its URL and clean status), and an unregistered
// nested git repo sitting in-tree (reported with empty URL / "unregistered").
// Both must appear; silently skipping is the failure mode this guards.
func TestSubmodsDetectsRegisteredAndUnregistered(t *testing.T) {
	parent := initRepo(t)
	run := gitRun(t, parent)

	// A real local repo to act as the submodule's upstream, since
	// `git submodule add` clones from the URL we give it.
	seed := initRepo(t)
	runInSeed := gitRun(t, seed)
	if err := os.WriteFile(filepath.Join(seed, "dep.go"), []byte("package dep\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	runInSeed("add", "dep.go")
	runInSeed("commit", "-m", "dep seed")

	// Registered submodule using the seed repo's path as the URL.
	run("submodule", "add", "--quiet", seed, "vendor/dep")
	run("commit", "-m", "add dep submodule")

	subs, err := Submods(parent)
	if err != nil {
		t.Fatalf("Submods: %v", err)
	}

	var haveDep, haveUnreg bool
	var depStatus, depURL string
	for _, s := range subs {
		t.Logf("[G4] submodule: path=%s url=%q status=%s", s.Path, s.URL, s.Status)
		if s.Path == "vendor/dep" {
			haveDep = true
			depStatus = s.Status
			depURL = s.URL
		}
	}
	if !haveDep {
		t.Fatalf("[G4] registered submodule not reported: %+v", subs)
	}
	if depStatus != "clean" {
		t.Fatalf("[G4] dep status = %q, want clean", depStatus)
	}
	if depURL != seed {
		t.Fatalf("[G4] dep url = %q, want %q", depURL, seed)
	}

	// Unregistered nested repo: a fresh git work tree sitting in a subdir.
	nested := filepath.Join(parent, "nested-repo")
	if err := os.MkdirAll(nested, 0o755); err != nil {
		t.Fatal(err)
	}
	nrun := gitRun(t, nested)
	nrun("init", "-b", "main")
	nrun("config", "user.email", "test@example.com")
	nrun("config", "user.name", "Test")
	if err := os.WriteFile(filepath.Join(nested, "README.md"), []byte("nested\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	nrun("add", "README.md")
	nrun("commit", "-m", "nested init")

	subs, err = Submods(parent)
	if err != nil {
		t.Fatalf("Submods after nested: %v", err)
	}
	haveUnreg = false
	for _, s := range subs {
		t.Logf("[G4] submodule: path=%s url=%q status=%s", s.Path, s.URL, s.Status)
		if s.Status == "unregistered" && s.Path == "nested-repo" {
			haveUnreg = true
		}
	}
	if !haveUnreg {
		t.Fatalf("[G4] unregistered nested repo not reported: %+v", subs)
	}
	t.Logf("[G4] reported: registered submodule (vendor/dep, clean, URL) + unregistered nested-repo")
}
