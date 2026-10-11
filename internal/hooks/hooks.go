// Package hooks implements lifecycle hooks (T6): named shell commands
// the user configures to run at session start/end, before/after a tool
// use, on prompt submit, and on stop. A hook can block the action it
// guards (a non-zero exit with a message vetoes a tool call). Hooks
// run ONLY in trusted projects — an untrusted project's hooks are
// never executed.
package hooks

import (
	"context"
	"fmt"
	"os/exec"
	"strings"
	"time"
)

// Event is a lifecycle point a hook can attach to (T6).
type Event string

const (
	SessionStart Event = "session_start"
	SessionEnd   Event = "session_end"
	PreToolUse   Event = "pre_tool_use"
	PostToolUse  Event = "post_tool_use"
	PromptSubmit Event = "prompt_submit"
	Stop         Event = "stop"
)

// Events lists every hookable event in a stable order.
func Events() []Event {
	return []Event{SessionStart, SessionEnd, PreToolUse, PostToolUse, PromptSubmit, Stop}
}

// Hook is one configured command for one event.
type Hook struct {
	Event   Event  `json:"event"`
	Command string `json:"command"` // shell command line
	// Blocking hooks can veto the guarded action (pre_tool_use).
	Blocking bool `json:"blocking,omitempty"`
}

// Result is the outcome of running one hook.
type Result struct {
	Event    Event
	Command  string
	ExitCode int
	Output   string
	Blocked  bool  // the hook vetoed the action
	Err      error // execution error (not a clean non-zero)
	Skipped  bool  // not run (untrusted project, empty command)
}

// DefaultTimeout bounds one hook's execution so a hung hook cannot
// stall a turn.
const DefaultTimeout = 10 * time.Second

// Runner executes hooks for a set of events. Trusted gates execution:
// when the project is untrusted, no hook runs (T6).
type Runner struct {
	// Hooks is the configured hook set (from the project config).
	Hooks []Hook
	// Trusted reports whether the current project is trusted. When
	// false, every hook is skipped (T6: hooks run only in trusted
	// projects).
	Trusted bool
	// Dir is the working directory for hook commands.
	Dir string
	// Timeout bounds each hook; <=0 means DefaultTimeout.
	Timeout time.Duration
	// Run overrides command execution (tests inject a fake); nil execs.
	Run func(ctx context.Context, dir, command string) (int, string, error)
}

// runOne executes a single hook command.
func (r Runner) runOne(ctx context.Context, h Hook) Result {
	res := Result{Event: h.Event, Command: h.Command}
	if strings.TrimSpace(h.Command) == "" {
		res.Skipped = true
		return res
	}
	if !r.Trusted {
		// T6: untrusted projects never run their own hooks.
		res.Skipped = true
		res.Output = "skipped: untrusted project"
		return res
	}
	timeout := r.Timeout
	if timeout <= 0 {
		timeout = DefaultTimeout
	}
	cctx, cancel := context.WithTimeout(ctx, timeout)
	defer cancel()

	if r.Run != nil {
		code, out, err := r.Run(cctx, r.Dir, h.Command)
		res.ExitCode, res.Output, res.Err = code, out, err
	} else {
		cmd := exec.CommandContext(cctx, "sh", "-c", h.Command)
		cmd.Dir = r.Dir
		out, err := cmd.CombinedOutput()
		res.Output = string(out)
		if err != nil {
			if ee, ok := err.(*exec.ExitError); ok {
				res.ExitCode = ee.ExitCode()
			} else {
				res.Err = err
			}
		}
	}
	// A non-zero exit from a blocking hook vetoes the action.
	if h.Blocking && res.ExitCode != 0 {
		res.Blocked = true
	}
	return res
}

// Fire runs every hook registered for the event, in order, and returns
// their results. Blocked is true if any blocking hook vetoed the
// action. In an untrusted project all hooks are skipped and Blocked is
// false (nothing from the project's config executes).
func (r Runner) Fire(ctx context.Context, event Event) (results []Result, blocked bool) {
	for _, h := range r.Hooks {
		if h.Event != event {
			continue
		}
		res := r.runOne(ctx, h)
		results = append(results, res)
		if res.Blocked {
			blocked = true
		}
	}
	return results, blocked
}

// FirePreTool is the guarded path: it fires pre_tool_use hooks and
// reports whether the tool call is blocked. A blocking hook that exits
// non-zero vetoes the call; its output is the reason shown to the user.
func (r Runner) FirePreTool(ctx context.Context) ([]Result, bool, string) {
	results, blocked := r.Fire(ctx, PreToolUse)
	if !blocked {
		return results, false, ""
	}
	// The first blocking hook's output is the veto reason.
	for _, res := range results {
		if res.Blocked {
			reason := strings.TrimSpace(res.Output)
			if reason == "" {
				reason = fmt.Sprintf("blocked by pre_tool_use hook (exit %d)", res.ExitCode)
			}
			return results, true, reason
		}
	}
	return results, true, "blocked by pre_tool_use hook"
}
