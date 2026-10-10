package doctor

import (
	"context"
	"os/exec"

	"github.com/RavaniRoshan/niki/internal/sandbox"
)

func execLookPath(name string) (string, error) { return exec.LookPath(name) }

func ctxBackground() context.Context { return context.Background() }

// runGitRevParse reports whether dir is inside a git work tree.
func runGitRevParse(ctx context.Context, dir string) error {
	cmd := exec.CommandContext(ctx, "git", "-C", dir, "rev-parse", "--is-inside-work-tree")
	return cmd.Run()
}

func sandboxDetect() (string, bool) { return sandbox.Detect() }
