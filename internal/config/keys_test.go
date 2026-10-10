package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/secrets"
)

// plantedKey is the secret the acceptance test hunts for across every
// file the save/load flow writes.
const plantedKey = "sk-planted-I4-canary-0123456789abcdef"

type mapBackend struct{ m map[string]string }

func (b *mapBackend) Set(service, account, key string) error {
	b.m[service+"/"+account] = key
	return nil
}
func (b *mapBackend) Get(service, account string) (string, error) {
	v, ok := b.m[service+"/"+account]
	if !ok {
		return "", os.ErrNotExist
	}
	return v, nil
}
func (b *mapBackend) Delete(service, account string) error {
	delete(b.m, service+"/"+account)
	return nil
}

// scanTree fails the test if any file under root contains the planted key.
func scanTree(t *testing.T, root, needle string) int {
	t.Helper()
	files := 0
	err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil || info.IsDir() {
			return nil
		}
		files++
		data, rerr := os.ReadFile(path)
		if rerr != nil {
			return nil
		}
		if strings.Contains(string(data), needle) {
			t.Errorf("planted key leaked into %s", path)
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	return files
}

// I4 acceptance: a test scans written files for a planted key. The scan
// covers every file written under the fresh HOME — including a legacy
// config that already contains api_key (must be stripped on save) and the
// log file written while the key is live.
func TestPlantedKeyScanOfWrittenFiles(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	secrets.UseBackendForTest(t, &mapBackend{m: map[string]string{}})

	// Legacy config carrying a key from an older version.
	cfgDir := filepath.Join(home, ".config", "nikicode")
	if err := os.MkdirAll(cfgDir, 0o755); err != nil {
		t.Fatal(err)
	}
	legacy := "[provider]\nname = \"anthropic\"\napi_key = \"" + plantedKey + "\"\n"
	if err := os.WriteFile(filepath.Join(cfgDir, "nikicode.toml"), []byte(legacy), 0o644); err != nil {
		t.Fatal(err)
	}

	// Load the legacy key (must remain readable for the session), migrate
	// it to the keychain, and save — the file must come out key-free.
	cfg, err := Load("")
	if err != nil {
		t.Fatalf("Load legacy: %v", err)
	}
	if cfg.Provider.APIKey != plantedKey {
		t.Fatalf("legacy api_key not readable: %q", cfg.Provider.APIKey)
	}
	if _, err := secrets.Set(accountFor(cfg.Provider), cfg.Provider.APIKey); err != nil {
		t.Fatalf("secrets.Set: %v", err)
	}
	if err := SaveUserConfig(cfg); err != nil {
		t.Fatalf("SaveUserConfig: %v", err)
	}

	// Key still resolves (keychain), not from the file.
	cfg2, err := Load("")
	if err != nil {
		t.Fatalf("Load after save: %v", err)
	}
	if cfg2.Provider.APIKey != "" {
		t.Fatalf("in-memory config still carries the key after save: %q", cfg2.Provider.APIKey)
	}
	if got := cfg2.ResolveAPIKey(); got != plantedKey {
		t.Fatalf("key must resolve from keychain after save, got %q", got)
	}

	// The scan: no file under HOME may contain the planted key.
	files := scanTree(t, home, plantedKey)
	if files == 0 {
		t.Fatal("scan found no files — fixture broken")
	}
	t.Logf("planted-key scan: %d files under fresh HOME, 0 hits (keychain round-trip preserved)", files)
}

// ResolveAPIKey order: session/in-memory -> keychain -> environment
// (environment is the fallback, per mission I4).
func TestResolveAPIKeyOrder(t *testing.T) {
	fb := &mapBackend{m: map[string]string{}}
	secrets.UseBackendForTest(t, fb)

	// 1. keychain value present -> wins over env.
	fb.Set(secrets.Service, "anthropic", "from-keychain")
	t.Setenv("ANTHROPIC_API_KEY", "from-env")
	cfg := Default()
	cfg.Provider.Name = "anthropic"
	cfg.Model.Name = "claude-3-5-sonnet"
	if got := cfg.ResolveAPIKey(); got != "from-keychain" {
		t.Fatalf("keychain must beat env, got %q", got)
	}

	// 2. keychain empty -> env fallback.
	delete(fb.m, secrets.Service+"/anthropic")
	if got := cfg.ResolveAPIKey(); got != "from-env" {
		t.Fatalf("env must be the fallback, got %q", got)
	}

	// 3. in-memory value (set by /connect this session) wins over both.
	cfg.Provider.APIKey = "from-session"
	if got := cfg.ResolveAPIKey(); got != "from-session" {
		t.Fatalf("session value must win, got %q", got)
	}
	t.Logf("resolve order verified: session > keychain > env (got %q, %q, %q)",
		"from-session", "from-keychain", "from-env")
}

// accountFor must map provider configs to stable keychain accounts,
// distinguishing openrouter/deepseek from plain openai.
func TestAccountForStable(t *testing.T) {
	cases := []struct {
		p    ProviderConfig
		want string
	}{
		{ProviderConfig{Name: "anthropic"}, "anthropic"},
		{ProviderConfig{Name: "openai"}, "openai"},
		{ProviderConfig{Name: "openai", BaseURL: "https://openrouter.ai/api/v1"}, "openrouter"},
		{ProviderConfig{Name: "openai", BaseURL: "https://api.deepseek.com"}, "deepseek"},
		{ProviderConfig{Name: "ollama"}, "ollama"},
		{ProviderConfig{}, ""},
	}
	for _, c := range cases {
		if got := accountFor(c.p); got != c.want {
			t.Errorf("accountFor(%+v) = %q, want %q", c.p, got, c.want)
		}
	}
}
