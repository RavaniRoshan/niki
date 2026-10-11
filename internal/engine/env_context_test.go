package engine

import (
	"os"
	"os/exec"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
)

// C9 (engine wiring): the environment block is collected once per
// session and refreshed on change — the model's context names the
// branch the turn actually runs on.
func TestEnvBlockReachesModelAndRefreshes(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not available")
	}
	dir := t.TempDir()
	run := func(args ...string) {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	run("init", "-q")
	run("config", "user.email", "c9e@test")
	run("config", "user.name", "c9e")
	if err := os.WriteFile(dir+"/f.txt", []byte("one\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", ".")
	run("commit", "-qm", "one")
	run("checkout", "-qb", "env-a")

	oldWd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	if err := os.Chdir(dir); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = os.Chdir(oldWd) })

	p := &scriptProvider{steps: [][]provider.ToolCall{{}, {}}}
	eng, cmdChan, eventChan := newScriptEngine(t, p)
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "first"}
	drainUntil(t, eventChan, protocol.EventTurnCompleted, 15*time.Second)
	sys := eng.runner.Context.Snapshot()[0].Content
	if !strings.Contains(sys, "git_branch: env-a") {
		t.Fatalf("[C9] turn-1 context missing branch env-a:\n%s", sys)
	}

	run("checkout", "-qb", "env-b")
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "second"}
	drainUntil(t, eventChan, protocol.EventTurnCompleted, 15*time.Second)
	sys = eng.runner.Context.Snapshot()[0].Content
	if !strings.Contains(sys, "git_branch: env-b") {
		t.Fatalf("[C9] turn-2 context not refreshed to env-b:\n%s", sys)
	}
	t.Logf("[C9] env block attached on turn 1 (env-a), refreshed on turn 2 (env-b)")
}
