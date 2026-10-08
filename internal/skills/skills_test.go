package skills

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func TestDiscover(t *testing.T) {
	dir := t.TempDir()
	sk := filepath.Join(dir, "myskill")
	os.MkdirAll(sk, 0o755)
	os.WriteFile(filepath.Join(sk, "SKILL.md"), []byte("---\nname: test\ndescription: d\n---\nbody here\n"), 0o644)
	found, err := Discover(dir)
	if err != nil || len(found) != 1 {
		t.Fatalf("found=%v err=%v", found, err)
	}
	if found[0].Name != "test" || found[0].Description != "d" {
		t.Fatalf("bad parse: %+v", found[0])
	}
	if err := found[0].LoadBody(); err != nil || found[0].Body != "body here" {
		t.Fatalf("body=%q err=%v", found[0].Body, err)
	}
}

func TestInstructions(t *testing.T) {
	dir := t.TempDir()
	os.WriteFile(filepath.Join(dir, "AGENTS.md"), []byte("x"), 0o644)
	sub := filepath.Join(dir, "sub")
	os.MkdirAll(sub, 0o755)
	found := Instructions(sub)
	if len(found) == 0 {
		t.Fatal("expected AGENTS.md discovered upward")
	}
}

func TestMalformedSkillSkipped(t *testing.T) {
	dir := t.TempDir()
	os.MkdirAll(filepath.Join(dir, "bad"), 0o755)
	os.WriteFile(filepath.Join(dir, "bad", "SKILL.md"), []byte("no frontmatter at all\n"), 0o644)
	os.MkdirAll(filepath.Join(dir, "good"), 0o755)
	os.WriteFile(filepath.Join(dir, "good", "SKILL.md"), []byte("---\nname: ok\n---\nbody\n"), 0o644)
	found, err := Discover(dir)
	if err != nil {
		t.Fatal(err)
	}
	if len(found) != 1 || found[0].Name != "ok" {
		t.Fatalf("found=%v", found)
	}
}

func TestInstructionsBounded(t *testing.T) {
	dir := t.TempDir()
	os.WriteFile(filepath.Join(dir, "AGENTS.md"), []byte(strings.Repeat("x", 100)), 0o644)
	got := InstructionsBounded(dir, 50)
	if len(got) != 1 || len(got[0].Content) > 50 {
		t.Fatalf("got=%v len=%d", got, len(got))
	}
}

func TestInstructionsRootToCwd(t *testing.T) {
	root := t.TempDir()
	sub := filepath.Join(root, "pkg", "core")
	os.MkdirAll(sub, 0o755)

	os.WriteFile(filepath.Join(root, "AGENTS.md"), []byte("root instructions\n"), 0o644)
	os.WriteFile(filepath.Join(sub, "AGENTS.md"), []byte("sub instructions\n"), 0o644)

	chain := InstructionsRootToCwd(sub, 0)
	if len(chain) != 2 {
		t.Fatalf("expected 2 instruction files, got %d", len(chain))
	}
	if !strings.Contains(chain[0].Content, "root instructions") {
		t.Fatalf("first instruction must be root, got %s", chain[0].Content)
	}
	if !strings.Contains(chain[1].Content, "sub instructions") {
		t.Fatalf("second instruction must be sub, got %s", chain[1].Content)
	}
}

func TestCompatAgentsSkillsDiscovery(t *testing.T) {
	dir := t.TempDir()
	agentsSkills := filepath.Join(dir, ".agents", "skills")
	_ = os.MkdirAll(agentsSkills, 0o755)
	_ = os.WriteFile(filepath.Join(agentsSkills, "tool_helper.md"), []byte("---\nname: tool_helper\ndescription: helper\n---\nbody"), 0o644)

	roots := StandardRoots(dir)
	skills, err := Discover(roots...)
	if err != nil {
		t.Fatal(err)
	}
	found := false
	for _, s := range skills {
		if s.Name == "tool_helper" {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("failed to discover skill in .agents/skills compat path: %+v", skills)
	}
}

func TestCachedDiscoverColdVsWarm(t *testing.T) {
	dir := t.TempDir()
	for i := 0; i < 20; i++ {
		skDir := filepath.Join(dir, fmt.Sprintf("skill_%d", i))
		_ = os.MkdirAll(skDir, 0o755)
		_ = os.WriteFile(filepath.Join(skDir, "SKILL.md"), []byte(fmt.Sprintf("---\nname: sk_%d\ndescription: desc\n---\nbody", i)), 0o644)
	}

	startCold := time.Now()
	coldSkills, err := CachedDiscover(dir)
	coldDuration := time.Since(startCold)
	if err != nil || len(coldSkills) != 20 {
		t.Fatalf("cold discover failed: len=%d err=%v", len(coldSkills), err)
	}

	startWarm := time.Now()
	warmSkills, err := CachedDiscover(dir)
	warmDuration := time.Since(startWarm)
	if err != nil || len(warmSkills) != 20 {
		t.Fatalf("warm discover failed: len=%d err=%v", len(warmSkills), err)
	}

	t.Logf("skills_cold_duration=%v skills_warm_duration=%v (warm_speedup=%.1fx)",
		coldDuration, warmDuration, float64(coldDuration)/float64(max(warmDuration, 1*time.Nanosecond)))
}
