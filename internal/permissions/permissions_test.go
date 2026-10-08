package permissions

import "testing"

func TestAllowByMode(t *testing.T) {
	g := NewGuard(ModeReadOnly)
	if g.Allow("write_file") || g.Allow("shell") {
		t.Fatal("readonly must block write_file and shell")
	}
	if !g.Allow("read_file") {
		t.Fatal("readonly must allow read_file")
	}
	g = NewGuard(ModeFullAccess)
	if !g.Allow("shell") {
		t.Fatal("full_access must allow shell")
	}
}

// TestSandboxedShellAutoAllow (S1): in
// read-only mode the shell tool is allowed
// only when a real sandbox backend enforces
// the read-only boundary at the OS level.
func TestSandboxedShellAutoAllow(t *testing.T) {
	g := NewGuard(ModeReadOnly)
	if g.Allow("shell") {
		t.Fatal("readonly must block shell without a sandbox")
	}
	g.SandboxedShell = true
	if !g.Allow("shell") {
		t.Fatal("readonly must allow sandboxed shell")
	}
	if g.Allow("write_file") {
		t.Fatal("auto-allow must not extend to write_file")
	}
}

func TestClassify(t *testing.T) {
	if ClassifyCommand("rm -rf /") != "dangerous" {
		t.Fatal("rm -rf")
	}
	if ClassifyCommand("ls -la") != "safe" {
		t.Fatal("ls")
	}
}

func TestApprovalSafestFocusedDefault(t *testing.T) {
	p := NewApprovalPrompt("shell", "rm -rf /tmp/test")
	if p.Focused != OptionDeny {
		t.Fatalf("safest option (OptionDeny=0) should be focused by default, got %v", p.Focused)
	}
	// Enter on default focus returns Deny
	if p.HandleKey("enter") != OptionDeny {
		t.Fatal("default Enter did not return Deny")
	}
}

func TestApprovalEscDenies(t *testing.T) {
	p := NewApprovalPrompt("shell", "rm -rf /")
	p.Focused = OptionAllowAlways // User selected allow
	// Esc unconditionally denies regardless of current focus
	if p.HandleKey("esc") != OptionDeny {
		t.Fatal("Esc did not deny approval request")
	}
	if p.HandleKey("ESC") != OptionDeny {
		t.Fatal("ESC did not deny approval request")
	}
}

func TestApprovalDecisionLogging(t *testing.T) {
	g := NewGuard(ModeWorkspaceWrite)
	g.LogDecision("shell", "git status", OptionAllowOnce, "user approved")
	g.LogDecision("shell", "rm -rf /", OptionDeny, "dangerous command denied via Esc")

	if len(g.AuditLog) != 2 {
		t.Fatalf("expected 2 logged decisions, got %d", len(g.AuditLog))
	}
	if g.AuditLog[0].Decision != OptionAllowOnce || g.AuditLog[0].Command != "git status" {
		t.Fatalf("unexpected log entry 0: %+v", g.AuditLog[0])
	}
	if g.AuditLog[1].Decision != OptionDeny || g.AuditLog[1].Reason != "dangerous command denied via Esc" {
		t.Fatalf("unexpected log entry 1: %+v", g.AuditLog[1])
	}
}
