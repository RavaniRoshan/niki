package skills

import (
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
