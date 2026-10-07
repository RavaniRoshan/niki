package skills

import (
	"strings"
	"os"
	"path/filepath"
	"testing"
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
