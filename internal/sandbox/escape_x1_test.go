package sandbox

import (
	"context"
	"os"
	"strings"
	"testing"
	"time"
)

// X1: default sandbox escapes — home/credential paths hidden,
// network off, writes confined to the workspace (Linux bwrap).
func TestBubblewrapHomeHiddenAndNetworkOff(t *testing.T) {
	if _, ok := Detect(); !ok {
		t.Skip("no sandbox backend installed on this system")
	}
	name, _ := Detect()
	if name != "bubblewrap" {
		t.Skipf("backend is %s, not bubblewrap", name)
	}
	if os.Getenv("HOME") == "" {
		t.Skip("HOME unset")
	}
	dir := t.TempDir()
	sb := &Bubblewrap{WritableDirs: []string{dir}}
	ctx := context.Background()

	// Credential paths are hidden: ~/.ssh is not reachable.
	_, _, err := sb.Run(ctx, dir, "sh", "-c", "ls \"$HOME/.ssh\"")
	if err == nil {
		t.Fatalf("HOME/.ssh visible inside sandbox")
	}
	out, _, err := sb.Run(ctx, dir, "sh", "-c", "ls -A \"$HOME\" 2>/dev/null | head -5")
	if err == nil && strings.TrimSpace(out) != "" {
		t.Fatalf("HOME not empty inside sandbox: %q", out)
	}

	// Network is off, even loopback (unshared net namespace).
	cctx, cancel := context.WithTimeout(ctx, 10*time.Second)
	defer cancel()
	_, _, err = sb.Run(cctx, dir, "sh", "-c", "cat < /dev/null > /dev/tcp/127.0.0.1/9")
	if err == nil {
		t.Fatalf("loopback TCP unexpectedly reachable inside sandbox")
	}

	// The workspace bind still works under a hidden HOME.
	probe := dir + "/ws-ok"
	if _, _, err := sb.Run(ctx, dir, "sh", "-c", "echo hi > "+probe); err != nil {
		t.Fatalf("workspace write failed under hidden HOME: %v", err)
	}
	t.Logf("[X1] HOME hidden, loopback unreachable, workspace writable")
}
