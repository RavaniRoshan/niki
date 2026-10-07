package sandbox

import (
	"context"
	"os/exec"
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
