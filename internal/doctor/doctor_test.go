package doctor

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/config"
	"github.com/RavaniRoshan/niki/internal/terminal"
)

// I3 acceptance: each of the nine doctor checks has a failing-fixture test.
// A fixture is a pair of injected dependencies that reproduce the failure;
// every non-OK result must carry an exact, actionable fix.

func printResult(t *testing.T, r Result) {
	t.Helper()
	mark := "✓"
	if r.Status != StatusOK {
		mark = "✗"
	}
	line := mark + " " + r.ID + ": " + r.Detail
	if r.Status != StatusOK {
		line += "\n    Fix: " + r.Fix
	}
	t.Logf("%s", line)
}

func requireFailWithFix(t *testing.T, r Result, wantID string) {
	t.Helper()
	printResult(t, r)
	if r.ID != wantID {
		t.Fatalf("expected check %q, got %q", wantID, r.ID)
	}
	if r.Status != StatusFail {
		t.Fatalf("expected failing fixture to produce status fail, got %q (%s)", r.Status, r.Detail)
	}
	if strings.TrimSpace(r.Fix) == "" {
		t.Fatalf("failure without a fix: %+v", r)
	}
}

// Fixture 1: install integrity — the running binary vanished from disk or
// is not executable.
func TestCheckInstallIntegrityFailingFixture(t *testing.T) {
	r := CheckInstallIntegrity(
		func() (string, error) { return "/opt/niki/nikicode", nil },
		func(name string) (string, error) { return "", errors.New("not found") },
		func(p string) error { return errors.New("permission denied") },
	)
	requireFailWithFix(t, r, "install integrity")
	if !strings.Contains(r.Fix, "install.sh") && !strings.Contains(r.Fix, "PATH") {
		t.Fatalf("fix must name the reinstall path: %q", r.Fix)
	}
}

// Fixture 2: PATH shadowing — a different nikicode/nc/niki resolves first.
func TestCheckPATHShadowingFailingFixture(t *testing.T) {
	ours := "/home/u/.local/bin/nikicode"
	r := CheckPATHShadowing(
		ours,
		func(name string) (string, error) {
			if name == "nikicode" {
				return "/usr/local/bin/nikicode", nil // shadowing install
			}
			return "", errors.New("not found")
		},
		"/usr/local/bin:/home/u/.local/bin:/usr/bin",
	)
	requireFailWithFix(t, r, "PATH shadowing")
	if !strings.Contains(r.Fix, "/usr/local/bin") {
		t.Fatalf("fix must name the shadowing path: %q", r.Fix)
	}
}

// Fixture 3: config validity — the config file fails to parse.
func TestCheckConfigValidityFailingFixture(t *testing.T) {
	path := "/home/u/.config/nikicode/nikicode.toml"
	r := CheckConfigValidity(path, func(p string) (config.Config, error) {
		return config.Config{}, errors.New("toml: expected '=' after a key, but got garbage")
	})
	requireFailWithFix(t, r, "config validity")
	if !strings.Contains(r.Fix, path) {
		t.Fatalf("fix must name the config file: %q", r.Fix)
	}
	if !strings.Contains(r.Fix, "toml") && !strings.Contains(r.Fix, "delete") {
		t.Fatalf("fix must say how to repair (edit or delete): %q", r.Fix)
	}
}

// Fixture 3b: config loads but carries an invalid permissions mode.
func TestCheckConfigValidityBadPermissionsMode(t *testing.T) {
	cfg := config.Default()
	cfg.Permissions.Mode = "root_everything"
	r := CheckConfigValidity("nikicode.toml", func(p string) (config.Config, error) {
		return cfg, nil
	})
	requireFailWithFix(t, r, "config validity")
	if !strings.Contains(r.Fix, "workspace_write") {
		t.Fatalf("fix must list valid modes: %q", r.Fix)
	}
}

// Fixture 4: provider reachability — configured provider endpoint fails.
func TestCheckProviderReachabilityFailingFixture(t *testing.T) {
	cfg := config.Default()
	cfg.Provider.Name = "openai"
	cfg.Provider.BaseURL = "https://api.openai.com/v1"
	cfg.Provider.APIKey = "sk-fixture-test-key" // configured state requires a key
	cfg.Model.Name = "gpt-4o-mini"
	r := CheckProviderReachability(cfg, func(ctx context.Context, url string) error {
		return errors.New("dial tcp: lookup api.openai.com: no such host")
	})
	requireFailWithFix(t, r, "provider reachability")
	if !strings.Contains(r.Fix, "api.openai.com") {
		t.Fatalf("fix must name the endpoint: %q", r.Fix)
	}
	if !strings.Contains(r.Fix, "proxy") && !strings.Contains(r.Fix, "network") {
		t.Fatalf("fix must mention network/proxy diagnosis: %q", r.Fix)
	}
}

// Fixture 5: git — git is not installed.
func TestCheckGitFailingFixture(t *testing.T) {
	r := CheckGit(
		func(name string) (string, error) { return "", errors.New("executable file not found in $PATH") },
		func(dir string) error { return nil },
		"/home/u/project",
	)
	requireFailWithFix(t, r, "git")
	if !strings.Contains(r.Fix, "apt install git") && !strings.Contains(r.Fix, "git-scm.com") {
		t.Fatalf("fix must give exact install command: %q", r.Fix)
	}
}

// Fixture 6: sandbox backend — no backend available.
func TestCheckSandboxBackendFailingFixture(t *testing.T) {
	r := CheckSandboxBackend(func() (string, bool) { return "", false })
	requireFailWithFix(t, r, "sandbox backend")
	if !strings.Contains(r.Fix, "bubblewrap") {
		t.Fatalf("fix must name the concrete backend to install: %q", r.Fix)
	}
}

// Fixture 7: terminal capabilities — terminal negotiation failed.
func TestCheckTerminalCapabilitiesFailingFixture(t *testing.T) {
	r := CheckTerminalCapabilities(func() (terminal.Capabilities, error) {
		return terminal.Capabilities{}, errors.New("timeout waiting for terminal reply")
	})
	requireFailWithFix(t, r, "terminal capabilities")
	if !strings.Contains(r.Fix, "TERM") {
		t.Fatalf("fix must mention TERM: %q", r.Fix)
	}
}

// Fixture 8a: proxy settings — proxy URL without scheme is unusable.
func TestCheckProxyCAFailingFixtureProxyNoScheme(t *testing.T) {
	getenv := func(k string) string {
		if k == "HTTPS_PROXY" {
			return "proxy.internal:3128" // missing scheme
		}
		return ""
	}
	r := CheckProxyCA(getenv, os.Stat)
	requireFailWithFix(t, r, "proxy and CA settings")
	if !strings.Contains(r.Fix, "HTTPS_PROXY") || !strings.Contains(r.Fix, "http") {
		t.Fatalf("fix must show the corrected variable form: %q", r.Fix)
	}
}

// Fixture 8b: CA settings — SSL_CERT_FILE points at a missing file.
func TestCheckProxyCAFailingFixtureMissingCA(t *testing.T) {
	getenv := func(k string) string {
		if k == "SSL_CERT_FILE" {
			return "/etc/ssl/certs/company-ca.pem"
		}
		return ""
	}
	r := CheckProxyCA(getenv, func(p string) (os.FileInfo, error) {
		return nil, errors.New("no such file or directory")
	})
	requireFailWithFix(t, r, "proxy and CA settings")
	if !strings.Contains(r.Fix, "/etc/ssl/certs/company-ca.pem") {
		t.Fatalf("fix must name the missing CA path: %q", r.Fix)
	}
	if !strings.Contains(r.Fix, "SSL_CERT_FILE") {
		t.Fatalf("fix must name the variable: %q", r.Fix)
	}
}

// Fixture 9: disk space — free bytes below the 500 MiB floor.
func TestCheckDiskSpaceFailingFixture(t *testing.T) {
	r := CheckDiskSpace(
		func(p string) (uint64, error) { return 100 << 20, nil }, // 100 MiB free
		"/home/u",
	)
	requireFailWithFix(t, r, "disk space")
	if !strings.Contains(r.Fix, "500") {
		t.Fatalf("fix must state the required headroom: %q", r.Fix)
	}
	if !strings.Contains(r.Fix, "df -h") {
		t.Fatalf("fix must give an inspection command: %q", r.Fix)
	}
}

// Guard: a full Run over healthy fixtures must produce all nine checks,
// every non-OK result must carry a fix, and IDs must be unique.
func TestRunAllNineChecksAndFixCoverage(t *testing.T) {
	tmp := t.TempDir()
	cfg := config.Default()
	cfg.Provider.Name = "mock"
	cfg.Model.Name = "gpt-4o-mini"

	p := Probes{
		Executable:    func() (string, error) { return filepath.Join(tmp, "nikicode"), nil },
		LookPath:      func(name string) (string, error) { return filepath.Join(tmp, name), nil },
		PathEnv:       tmp,
		LoadConfig:    func(string) (config.Config, error) { return cfg, nil },
		ConfigPath:    filepath.Join(tmp, "nikicode.toml"),
		ProbeProvider: func(ctx context.Context, url string) error { return nil },
		GitInRepo:     func(dir string) error { return nil },
		Cwd:           func() (string, error) { return tmp, nil },
		SandboxProbe:  func() (string, bool) { return "bubblewrap", true },
		TerminalProbe: func() (terminal.Capabilities, error) {
			return terminal.Capabilities{KittyKeyboard: true}, nil
		},
		Getenv: func(string) string { return "" },
		Stat:   func(string) (os.FileInfo, error) { return os.Stat(tmp) },
		Statfs: func(string) (uint64, error) { return 8 << 30, nil }, // 8 GiB free
	}

	// Healthy install fixture: an executable file at the reported path.
	binPath := filepath.Join(tmp, "nikicode")
	if err := os.WriteFile(binPath, []byte("#!/bin/sh\n"), 0o755); err != nil {
		t.Fatal(err)
	}

	results := Run(p)
	ids := map[string]bool{}
	for _, r := range results {
		if ids[r.ID] {
			t.Fatalf("duplicate check id %q", r.ID)
		}
		ids[r.ID] = true
		printResult(t, r)
		if r.Status != StatusOK && strings.TrimSpace(r.Fix) == "" {
			t.Fatalf("non-OK result without fix: %+v", r)
		}
	}
	want := []string{
		"install integrity", "PATH shadowing", "config validity",
		"provider reachability", "git", "sandbox backend",
		"terminal capabilities", "proxy and CA settings", "disk space",
	}
	if len(results) != len(want) {
		t.Fatalf("expected %d checks, got %d", len(want), len(results))
	}
	for i, id := range want {
		if results[i].ID != id {
			t.Errorf("check %d: want %q got %q", i, id, results[i].ID)
		}
	}
}

// Fixture: a legacy config file still contains api_key — config validity
// must warn with the exact migration fix (I4: keys never in config files).
func TestConfigValidityLegacyAPIKeyWarns(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "nikicode.toml")
	legacy := "[provider]\nname = \"anthropic\"\napi_key = \"sk-legacy-file-key-1234567890\"\n"
	if err := os.WriteFile(path, []byte(legacy), 0o644); err != nil {
		t.Fatal(err)
	}
	r := CheckConfigValidity(path, func(p string) (config.Config, error) {
		cfg := config.Default()
		cfg.Provider.Name = "anthropic"
		cfg.Provider.APIKey = "sk-legacy-file-key-1234567890"
		cfg.Model.Name = "claude-3-5-sonnet"
		return cfg, nil
	})
	printResult(t, r)
	if r.Status == StatusOK {
		t.Fatal("legacy api_key on disk must not be reported as OK")
	}
	if r.Status != StatusWarn {
		t.Fatalf("expected warn (file parses), got %q", r.Status)
	}
	if !strings.Contains(r.Fix, "api_key") || !strings.Contains(r.Fix, "keychain") {
		t.Fatalf("fix must explain the keychain migration: %q", r.Fix)
	}
}
