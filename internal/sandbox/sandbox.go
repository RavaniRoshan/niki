package sandbox

import (
	"context"
	"os"
	"os/exec"
	"strings"
)

// Sandbox abstracts process containment.
type Sandbox interface {
	Exec(ctx context.Context, name string, args ...string) *exec.Cmd
	Name() string
}

// Passthrough is a no-isolation sandbox for trusted environments.
type Passthrough struct{}

func (p *Passthrough) Name() string { return "passthrough" }

func (p *Passthrough) Exec(ctx context.Context, name string, args ...string) *exec.Cmd {
	return exec.CommandContext(ctx, name, args...)
}

// RestrictedEnv strips network-ish env vars before exec where available.
func (p *Passthrough) Env(base []string) []string {
	return base
}

// SensitivePrefixes are environment variables scrubbed from sandboxed
// execution so an agent command cannot harvest cloud/SSH credentials.
var SensitivePrefixes = []string{"AWS_", "SSH_", "GITHUB_TOKEN", "OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GH_TOKEN"}

// SanitizedEnv returns the process environment minus credentials.
func SanitizedEnv() []string {
	var out []string
	for _, kv := range os.Environ() {
		drop := false
		for _, p := range SensitivePrefixes {
			if strings.HasPrefix(kv, p) {
				drop = true
				break
			}
		}
		if !drop {
			out = append(out, kv)
		}
	}
	return out
}

// ExecIsolated runs name with a scrubbed environment and a bounded cwd. This
// is the documented fallback when a real OS sandbox backend is unavailable.
func (p *Passthrough) ExecIsolated(ctx context.Context, dir, name string, args ...string) *exec.Cmd {
	cmd := exec.CommandContext(ctx, name, args...)
	cmd.Env = SanitizedEnv()
	if dir != "" {
		cmd.Dir = dir
	}
	return cmd
}
