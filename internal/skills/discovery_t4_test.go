package skills

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// T4: skills are SKILL.md folders with optional scripts, discovered
// from user and project locations, loaded on demand — the body is NOT
// read at discovery time, only when the skill is invoked.
func TestSkillFolderDiscoveryAndLazyBody(t *testing.T) {
	base := t.TempDir()
	userRoot := filepath.Join(base, "user", "skills")
	projectRoot := filepath.Join(base, "project")

	// A user-location skill with a body and an optional script.
	userSkill := filepath.Join(userRoot, "deploy")
	if err := os.MkdirAll(userSkill, 0o755); err != nil {
		t.Fatal(err)
	}
	body := "Deploy steps:\n1. build\n2. ship\n"
	if err := os.WriteFile(filepath.Join(userSkill, "SKILL.md"),
		[]byte("---\nname: deploy\ndescription: ship it\n---\n"+body), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(userSkill, "run.sh"),
		[]byte("#!/bin/sh\necho deploying\n"), 0o755); err != nil {
		t.Fatal(err)
	}

	// A project-location skill (overrides/shadows by location).
	projSkill := filepath.Join(projectRoot, ".nikicode", "skills", "lint")
	if err := os.MkdirAll(projSkill, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(projSkill, "SKILL.md"),
		[]byte("---\nname: lint\ndescription: lint the repo\n---\nRun the linter.\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	// Discover from both locations.
	roots := []string{userRoot, filepath.Join(projectRoot, ".nikicode", "skills")}
	found, err := Discover(roots...)
	if err != nil {
		t.Fatal(err)
	}
	byName := map[string]Skill{}
	for _, s := range found {
		byName[s.Name] = s
	}
	if _, ok := byName["deploy"]; !ok {
		t.Fatalf("[T4] user skill 'deploy' not discovered: %+v", found)
	}
	if _, ok := byName["lint"]; !ok {
		t.Fatalf("[T4] project skill 'lint' not discovered: %+v", found)
	}
	t.Logf("[T4] discovered from user + project roots: deploy, lint")

	// Laziness proof: discovery parsed frontmatter only; the body is
	// empty until the skill is invoked (LoadBody).
	deploy := byName["deploy"]
	if deploy.Loaded || strings.TrimSpace(deploy.Body) != "" {
		t.Fatalf("[T4] body must not be read at discovery: loaded=%v body=%q", deploy.Loaded, deploy.Body)
	}
	if err := deploy.LoadBody(); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(deploy.Body, "Deploy steps") {
		t.Fatalf("[T4] body not loaded on invoke: %q", deploy.Body)
	}
	t.Logf("[T4] body empty after discovery, loaded on invoke (%d bytes)", len(deploy.Body))

	// The optional script travels with the folder.
	if _, err := os.Stat(filepath.Join(userSkill, "run.sh")); err != nil {
		t.Fatalf("[T4] optional script missing from skill folder: %v", err)
	}
	t.Logf("[T4] skill folder carries its optional script (run.sh)")
}
