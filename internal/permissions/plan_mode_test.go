package permissions

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/RavaniRoshan/niki/internal/tools"
)

func TestPlanModeWithholdingAndApproval(t *testing.T) {
	guard := NewGuard(ModeWorkspaceWrite)

	// Initially in workspace_write mode, write tools are allowed
	if !guard.Allow("write_file") || !guard.Allow("shell") {
		t.Fatalf("expected write tools to be allowed in workspace write mode")
	}

	// 1. Enter Plan Mode
	guard.EnterPlanMode()
	if !guard.InPlanMode() {
		t.Fatalf("expected to be in plan mode")
	}

	// 2. Read-only tools MUST be allowed in Plan Mode
	for _, tool := range []string{
		"read_file", "glob", "grep", "web_search", "web_fetch",
		"view_image", "tool_search", "bash_output", "ask_user_question",
		"git_status", "git_blame", "git_log", "git_review", "git_changelog",
	} {
		if !guard.Allow(tool) {
			t.Fatalf("tool %q should be allowed in plan mode", tool)
		}
	}

	// 3. Write & exec tools MUST be withheld / denied in Plan Mode
	for _, tool := range []string{
		"write_file", "edit_file", "apply_patch", "shell",
		"notebook_edit", "exec_command", "write_stdin", "kill_shell",
		"spawn_agent", "send_input", "close_agent", "resume_agent",
		"git_commit", "git_branch", "git_rebase",
	} {
		if guard.Allow(tool) {
			t.Fatalf("tool %q should be WITHHELD in plan mode", tool)
		}
	}

	// 4. UpdatePlan tool rejection test during Plan Mode
	up := tools.NewUpdatePlanTool()
	up.SetPlanMode(true)
	res, err := up.Run(context.Background(), json.RawMessage(`{
		"steps": [{"title": "Step 1", "status": "in_progress"}]
	}`))
	if err != nil {
		t.Fatal(err)
	}
	if !res.IsError {
		t.Fatalf("update_plan should be rejected in plan mode: %v", res)
	}

	// 5. Exiting Plan Mode requires explicit approval
	if err := guard.ExitPlanMode(false); err == nil {
		t.Fatalf("expected error when attempting to exit plan mode without approval")
	}
	if !guard.InPlanMode() {
		t.Fatalf("guard should remain in plan mode after unapproved exit")
	}

	if err := guard.ExitPlanMode(true); err != nil {
		t.Fatalf("failed exiting plan mode with approval: %v", err)
	}
	if guard.InPlanMode() {
		t.Fatalf("guard should have exited plan mode")
	}
	if !guard.Allow("write_file") {
		t.Fatalf("write tools should be restored after approved exit")
	}
}
