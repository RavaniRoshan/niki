// Package doctor implements `nikicode doctor`: nine environment checks
// (install integrity, PATH shadowing, config validity, provider
// reachability, git, sandbox backend, terminal capabilities, proxy and CA
// settings, disk space). Every non-OK result carries an exact fix, and
// every check is exercised by a failing-fixture test.
package doctor

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/RavaniRoshan/niki/internal/config"
	"github.com/RavaniRoshan/niki/internal/terminal"
)

// Status is the outcome of one check.
type Status string

const (
	StatusOK   Status = "ok"
	StatusWarn Status = "warn"
	StatusFail Status = "fail"
)

// Result is one doctor check: what was observed and, when not OK, the
// exact fix the user should run.
type Result struct {
	ID     string
	Status Status
	Detail string
	Fix    string // mandatory whenever Status != StatusOK
}

// MinFreeBytes is the disk headroom floor (500 MiB).
const MinFreeBytes = 500 << 20

// CheckInstallIntegrity verifies the running binary exists, is executable,
// and is reachable by name.
func CheckInstallIntegrity(executable func() (string, error), lookPath func(string) (string, error), canExec func(string) error) Result {
	const id = "install integrity"
	bin, err := executable()
	if err != nil {
		return Result{ID: id, Status: StatusFail,
			Detail: "cannot locate the running executable",
			Fix:    "Re-run ./install.sh from a GitHub release (it verifies checksums.txt), then open a new shell."}
	}
	if err := canExec(bin); err != nil {
		return Result{ID: id, Status: StatusFail,
			Detail: fmt.Sprintf("%s is not executable: %v", bin, err),
			Fix:    fmt.Sprintf("Restore the exec bit and reinstall: `chmod +x %s` or re-run ./install.sh (checksum-verified).", bin)}
	}
	if _, err := lookPath("nikicode"); err != nil {
		dir := filepath.Dir(bin)
		return Result{ID: id, Status: StatusFail,
			Detail: fmt.Sprintf("%s exists but is not reachable as `nikicode` in PATH", bin),
			Fix:    fmt.Sprintf("Re-run ./install.sh if the binary is missing, and add %s to PATH: export PATH=\"$PATH:%s\"", dir, dir)}
	}
	return Result{ID: id, Status: StatusOK, Detail: fmt.Sprintf("%s present and executable", bin)}
}

// CheckPATHShadowing reports a different nikicode resolving first in PATH.
func CheckPATHShadowing(ourBinary string, lookPath func(string) (string, error), pathEnv string) Result {
	const id = "PATH shadowing"
	resolved, err := lookPath("nikicode")
	if err != nil {
		ourDir := filepath.Dir(ourBinary)
		return Result{ID: id, Status: StatusFail,
			Detail: "`nikicode` not found in PATH",
			Fix:    fmt.Sprintf("Add the install directory to PATH: export PATH=\"$PATH:%s\" (PATH=%s)", ourDir, pathEnv)}
	}
	if filepath.Clean(resolved) == filepath.Clean(ourBinary) {
		return Result{ID: id, Status: StatusOK, Detail: fmt.Sprintf("resolves to this install (%s)", resolved)}
	}
	shadowDir := filepath.Dir(resolved)
	ourDir := filepath.Dir(ourBinary)
	return Result{ID: id, Status: StatusFail,
		Detail: fmt.Sprintf("%s shadows the installed %s", resolved, ourBinary),
		Fix:    fmt.Sprintf("Remove or rename %s, or put %s earlier in PATH (shadowing also breaks the `nc`/`niki` aliases).", shadowDir, ourDir)}
}

// CheckConfigValidity parses the config file and validates its values.
func CheckConfigValidity(configPath string, load func(string) (config.Config, error)) Result {
	const id = "config validity"
	cfg, err := load(configPath)
	if err != nil {
		return Result{ID: id, Status: StatusFail,
			Detail: fmt.Sprintf("%s does not parse: %v", configPath, err),
			Fix:    fmt.Sprintf("Fix the TOML in %s (parser error above), or move it aside: `mv %s %s.bak` to fall back to defaults.", configPath, configPath, configPath)}
	}
	switch cfg.Permissions.Mode {
	case "readonly", "workspace_write", "full_access":
	default:
		return Result{ID: id, Status: StatusFail,
			Detail: fmt.Sprintf("invalid permissions.mode %q in %s", cfg.Permissions.Mode, configPath),
			Fix:    fmt.Sprintf("Set [permissions] mode to one of: readonly, workspace_write, full_access (in %s).", configPath)}
	}
	known := map[string]bool{"": true, "openai": true, "anthropic": true, "responses": true, "ollama": true, "mock": true}
	if !known[cfg.Provider.Name] {
		return Result{ID: id, Status: StatusFail,
			Detail: fmt.Sprintf("unknown provider %q in %s", cfg.Provider.Name, configPath),
			Fix:    fmt.Sprintf("Set [provider] name to one of: openai, anthropic, responses, ollama, mock (in %s).", configPath)}
	}
	if cfg.Provider.Name != "" && cfg.Provider.Name != "mock" && cfg.Model.Name == "" {
		return Result{ID: id, Status: StatusFail,
			Detail: fmt.Sprintf("provider %q configured without a model in %s", cfg.Provider.Name, configPath),
			Fix:    fmt.Sprintf("Set [model] name (e.g. name = \"gpt-4o-mini\") in %s, or clear [provider] to run first-run detection.", configPath)}
	}
	name := configPath
	if name == "" {
		name = "merged config (defaults + user + project)"
	} else {
		name = filepath.Base(configPath)
	}
	// I4: a legacy file may still carry api_key on disk (the current
	// save path strips it). Warn with the migration fix.
	if configPath != "" {
		if raw, rerr := os.ReadFile(configPath); rerr == nil && strings.Contains(string(raw), "api_key") {
			return Result{ID: id, Status: StatusWarn,
				Detail: fmt.Sprintf("%s parses but still contains a legacy api_key entry", name),
				Fix:    fmt.Sprintf("Keys must live in the OS keychain, not the file: open niki and re-run /connect (saves a key-free config), or delete the api_key line from %s. The resolved key keeps working via keychain/env.", configPath)}
		}
	}
	return Result{ID: id, Status: StatusOK,
		Detail: fmt.Sprintf("%s parses (provider=%s, model=%s)", name, cfg.Provider.Name, cfg.Model.Name)}
}

// CheckProviderReachability probes the configured endpoint; unconfigured is
// a warning with the first-run fix.
func CheckProviderReachability(cfg config.Config, probe func(context.Context, string) error) Result {
	const id = "provider reachability"
	if !cfg.IsConfigured() {
		return Result{ID: id, Status: StatusWarn,
			Detail: "no provider configured (first-run detection pending)",
			Fix:    "Set ANTHROPIC_API_KEY or OPENAI_API_KEY, set provider.name = \"ollama\" for a local server, or run `nikicode demo` for the recorded walkthrough."}
	}
	if cfg.Provider.Name == "mock" {
		return Result{ID: id, Status: StatusOK, Detail: "mock provider (offline, deterministic)"}
	}
	base := cfg.Provider.BaseURL
	if base == "" {
		switch cfg.Provider.Name {
		case "anthropic":
			base = "https://api.anthropic.com"
		case "ollama":
			base = "http://localhost:11434"
		default:
			base = "https://api.openai.com/v1"
		}
	}
	ctx, cancel := context.WithTimeout(context.Background(), 3*time.Second)
	defer cancel()
	if err := probe(ctx, base); err != nil {
		return Result{ID: id, Status: StatusFail,
			Detail: fmt.Sprintf("%s unreachable: %v", base, err),
			Fix:    fmt.Sprintf("Verify `%s` is up (`curl -I %s`); check base_url in your config, the API key, and proxy/CA settings (see proxy and CA settings check).", base, base)}
	}
	return Result{ID: id, Status: StatusOK, Detail: fmt.Sprintf("%s reachable", base)}
}

// CheckGit verifies git is installed and the workspace is a repository.
func CheckGit(lookPath func(string) (string, error), inRepo func(dir string) error, dir string) Result {
	const id = "git"
	gitPath, err := lookPath("git")
	if err != nil {
		return Result{ID: id, Status: StatusFail,
			Detail: "git not found in PATH",
			Fix:    "Install git: Debian/Ubuntu `sudo apt install git`; macOS `xcode-select --install` (or `brew install git`); then verify `git --version`."}
	}
	if err := inRepo(dir); err != nil {
		return Result{ID: id, Status: StatusWarn,
			Detail: fmt.Sprintf("%s available, but %s is not a git repository", gitPath, dir),
			Fix:    fmt.Sprintf("Checkpoint/rollback features need git — run `git init` in %s.", dir)}
	}
	return Result{ID: id, Status: StatusOK, Detail: fmt.Sprintf("%s available, workspace is a repository", gitPath)}
}

// CheckSandboxBackend verifies a sandbox backend (bubblewrap/seatbelt) exists.
func CheckSandboxBackend(detect func() (string, bool)) Result {
	const id = "sandbox backend"
	name, ok := detect()
	if !ok {
		return Result{ID: id, Status: StatusFail,
			Detail: "no sandbox backend available (bubblewrap/seatbelt)",
			Fix:    "Install a sandbox backend — Linux: `sudo apt install bubblewrap` (or your distro's package); macOS: seatbelt ships with the OS. Until then sessions run in env-scrubbed passthrough mode."}
	}
	return Result{ID: id, Status: StatusOK, Detail: fmt.Sprintf("%s available", name)}
}

// CheckTerminalCapabilities runs terminal capability negotiation.
func CheckTerminalCapabilities(detect func() (terminal.Capabilities, error)) Result {
	const id = "terminal capabilities"
	caps, err := detect()
	if err != nil {
		return Result{ID: id, Status: StatusFail,
			Detail: fmt.Sprintf("negotiation failed: %v", err),
			Fix:    "Run inside a real terminal emulator; if TERM is wrong run `export TERM=xterm-256color` (inside tmux/screen, set a modern TERM as well)."}
	}
	if !caps.KittyKeyboard && !caps.SyncOutput {
		return Result{ID: id, Status: StatusWarn,
			Detail: "basic terminal only (no synchronized output, no kitty keyboard protocol)",
			Fix:    "Cosmetic only: upgrade the terminal (kitty, iTerm2, WezTerm, foot) or set TERM to gain flicker-free redraws."}
	}
	return Result{ID: id, Status: StatusOK,
		Detail: fmt.Sprintf("kitty-keyboard=%v sync-output=%v", caps.KittyKeyboard, caps.SyncOutput)}
}

// CheckProxyCA validates proxy URLs and CA bundle paths from the environment.
func CheckProxyCA(getenv func(string) string, stat func(string) (os.FileInfo, error)) Result {
	const id = "proxy and CA settings"
	for _, v := range []string{"HTTPS_PROXY", "HTTP_PROXY", "ALL_PROXY"} {
		val := getenv(v)
		if val == "" {
			continue
		}
		u, err := url.Parse(val)
		if err != nil || u.Host == "" || (u.Scheme != "http" && u.Scheme != "https" && u.Scheme != "socks5") {
			return Result{ID: id, Status: StatusFail,
				Detail: fmt.Sprintf("%s=%q is not a usable proxy URL", v, val),
				Fix:    fmt.Sprintf("Correct it with a scheme and port — e.g. export %s=https://proxy.internal:3128 — or unset it: unset %s.", v, v)}
		}
	}
	for _, v := range []string{"SSL_CERT_FILE", "REQUESTS_CA_BUNDLE", "CURL_CA_BUNDLE"} {
		val := getenv(v)
		if val == "" {
			continue
		}
		if _, err := stat(val); err != nil {
			return Result{ID: id, Status: StatusFail,
				Detail: fmt.Sprintf("%s=%s: CA bundle not found", v, val),
				Fix:    fmt.Sprintf("Install the CA file at %s, or unset %s to fall back to the system trust store.", val, v)}
		}
	}
	return Result{ID: id, Status: StatusOK, Detail: "no proxy/CA overrides; using system trust store"}
}

// CheckDiskSpace reports free bytes on the workspace filesystem.
func CheckDiskSpace(statfs func(string) (uint64, error), path string) Result {
	const id = "disk space"
	free, err := statfs(path)
	if err != nil {
		return Result{ID: id, Status: StatusWarn,
			Detail: fmt.Sprintf("cannot determine free space for %s: %v", path, err),
			Fix:    fmt.Sprintf("Check manually: `df -h %s`.", path)}
	}
	if free < MinFreeBytes {
		return Result{ID: id, Status: StatusFail,
			Detail: fmt.Sprintf("only %d MB free on %s (floor is 500 MB)", free>>20, path),
			Fix:    fmt.Sprintf("Free at least 500 MB on %s: run `df -h %s`, then clear old session logs and transcripts under your niki home directory.", path, path)}
	}
	return Result{ID: id, Status: StatusOK, Detail: fmt.Sprintf("%.1f GB free on %s", float64(free)/(1<<30), path)}
}

// Probes are the injected dependencies for Run. Production values come
// from DefaultProbes; tests supply failing fixtures.
type Probes struct {
	Executable    func() (string, error)
	LookPath      func(string) (string, error)
	PathEnv       string
	LoadConfig    func(string) (config.Config, error)
	ConfigPath    string
	ProbeProvider func(context.Context, string) error
	GitInRepo     func(dir string) error
	Cwd           func() (string, error)
	SandboxProbe  func() (string, bool)
	TerminalProbe func() (terminal.Capabilities, error)
	Getenv        func(string) string
	Stat          func(string) (os.FileInfo, error)
	Statfs        func(string) (uint64, error)
}

// Run executes all nine checks in canonical order.
func Run(p Probes) []Result {
	dir := "."
	if p.Cwd != nil {
		if d, err := p.Cwd(); err == nil {
			dir = d
		}
	}
	cfg, cfgErr := p.LoadConfig(p.ConfigPath)
	ourBin := ""
	if p.Executable != nil {
		if b, err := p.Executable(); err == nil {
			ourBin = b
		}
	}
	canExec := func(path string) error {
		fi, err := os.Stat(path)
		if err != nil {
			return err
		}
		if fi.Mode()&0o111 == 0 {
			return fmt.Errorf("no execute bit (mode %v)", fi.Mode())
		}
		return nil
	}

	// Config validity first: a parse error must not cascade.
	cfgResult := Result{ID: "config validity", Status: StatusFail, Detail: "config not loaded", Fix: ""}
	if cfgErr != nil {
		cfgResult = CheckConfigValidity(p.ConfigPath, func(string) (config.Config, error) {
			return config.Config{}, cfgErr
		})
	} else {
		cfgResult = CheckConfigValidity(p.ConfigPath, func(string) (config.Config, error) { return cfg, nil })
	}

	return []Result{
		CheckInstallIntegrity(p.Executable, p.LookPath, canExec),
		CheckPATHShadowing(ourBin, p.LookPath, p.PathEnv),
		cfgResult,
		CheckProviderReachability(cfg, p.ProbeProvider),
		CheckGit(p.LookPath, p.GitInRepo, dir),
		CheckSandboxBackend(p.SandboxProbe),
		CheckTerminalCapabilities(p.TerminalProbe),
		CheckProxyCA(p.Getenv, p.Stat),
		CheckDiskSpace(p.Statfs, dir),
	}
}

// DefaultProbes builds production dependencies for Run.
func DefaultProbes(configPath string) Probes {
	return Probes{
		Executable: os.Executable,
		LookPath:   func(name string) (string, error) { return execLookPath(name) },
		PathEnv:    os.Getenv("PATH"),
		LoadConfig: func(p string) (config.Config, error) { return config.Load(p) },
		ConfigPath: configPath,
		ProbeProvider: func(ctx context.Context, rawurl string) error {
			req, err := http.NewRequestWithContext(ctx, http.MethodGet, rawurl, nil)
			if err != nil {
				return err
			}
			resp, err := http.DefaultClient.Do(req)
			if err != nil {
				return err
			}
			_ = resp.Body.Close()
			return nil // any HTTP response = reachable
		},
		GitInRepo: func(dir string) error {
			return runGitRevParse(ctxBackground(), dir)
		},
		Cwd:          os.Getwd,
		SandboxProbe: sandboxDetect,
		TerminalProbe: func() (terminal.Capabilities, error) {
			return terminal.Detect(25 * time.Millisecond)
		},
		Getenv: os.Getenv,
		Stat:   os.Stat,
		Statfs: statfsFree,
	}
}

// Render renders one result the way the doctor command prints it.
func Render(r Result) string {
	mark := "✓"
	if r.Status == StatusFail {
		mark = "✗"
	} else if r.Status == StatusWarn {
		mark = "!"
	}
	out := fmt.Sprintf("%s %s: %s", mark, r.ID, r.Detail)
	if r.Status != StatusOK {
		out += "\n  Fix: " + r.Fix
	}
	return out
}
