package tools

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
)

// G3: no force push, no history rewrite, no push at all from
// autonomous runs — no such command is reachable via shell.
func TestGitPushUnreachableViaShell(t *testing.T) {
	ctx := context.Background()
	run := func(cmd string) ToolResult {
		args, _ := json.Marshal(map[string]any{"command": cmd})
		res, err := NewShellTool().Run(ctx, args)
		if err != nil {
			t.Fatalf("push refusal must be a typed result, not an error: %v", err)
		}
		return res
	}
	for _, cmd := range []string{
		"git push origin main",
		"git push --force origin main",
		"git -C /tmp push",
		"git push --tags",
	} {
		res := run(cmd)
		if !res.IsError || !strings.Contains(res.Output, "never run by autonomous runs") {
			t.Fatalf("[G3] push reachable: %q -> %+v", cmd, res)
		}
	}
	t.Logf("[G3] 4 push forms refused (plain, --force, -C, --tags)")

	// Non-push git still works.
	res := run("git status --porcelain")
	if res.IsError && strings.Contains(res.Output, "never run by autonomous runs") {
		t.Fatalf("[G3] false positive on git status: %+v", res)
	}
	t.Logf("[G3] non-push commands unaffected")
}
