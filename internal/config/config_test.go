package config

import (
	"os"
	"path/filepath"
	"testing"
)

func TestDefaults(t *testing.T) {
	c := Default()
	if c.Provider.Name != "mock" || c.Permissions.Mode != "workspace_write" {
		t.Fatalf("bad defaults: %+v", c)
	}
}

func TestLoadLayered(t *testing.T) {
	dir := t.TempDir()
	proj := filepath.Join(dir, "niki.toml")
	os.WriteFile(proj, []byte(`
[model]
name = "gpt-4o"
[permissions]
mode = "full_access"
`), 0o644)
	c, err := Load(proj)
	if err != nil {
		t.Fatal(err)
	}
	if c.Model.Name != "gpt-4o" || c.Permissions.Mode != "full_access" {
		t.Fatalf("overlay failed: %+v", c)
	}
	if c.Provider.Name != "mock" {
		t.Fatalf("defaults lost: %+v", c)
	}
}

func TestResolveAPIKey(t *testing.T) {
	t.Setenv("OPENAI_API_KEY", "sk-test")
	c := Default()
	if c.ResolveAPIKey() != "sk-test" {
		t.Fatal("env key resolution failed")
	}
}
