package agent

import (
	"fmt"
	"strings"
)

// Role-scoped subagents (S1): each role exposes a fixed tool family.
//   - explorer:  read-only (no writes, no shell exec, no spawn)
//   - implementer: may write, gets its own worktree, may not spawn
//   - reviewer:  read-only + git review/status, may not write or spawn
//
// A role's tool set is a closed allowlist; anything not named is
// refused. Roles are pure data (a name + a tool allowlist + flags) so
// they can be unit-tested without spawning a process.

// Role identifies a subagent's capability class (S1).
type Role string

const (
	RoleExplorer    Role = "explorer"
	RoleImplementer Role = "implementer"
	RoleReviewer    Role = "reviewer"
)

// RoleSpec is the closed capability contract for a role.
type RoleSpec struct {
	Name Role
	// Tools is the exact allowlist of tool names the role may call.
	Tools []string
	// ReadOnly forbids every mutating tool regardless of the allowlist.
	ReadOnly bool
	// OwnWorktree provisions an isolated git worktree for the role.
	OwnWorktree bool
	// MaySpawn reports whether the role can create child subagents.
	MaySpawn bool
}

// BuiltinRoles returns the three shipped roles (S1).
func BuiltinRoles() map[Role]RoleSpec {
	return map[Role]RoleSpec{
		RoleExplorer: {
			Name:     RoleExplorer,
			Tools:    []string{"read_file", "grep", "glob", "list", "symbol_search", "git_status", "git_log", "git_blame", "git_review", "git_changelog", "explain", "web_search", "web_fetch"},
			ReadOnly: true,
			MaySpawn: false,
		},
		RoleImplementer: {
			Name:        RoleImplementer,
			Tools:       []string{"read_file", "grep", "glob", "list", "edit_file", "write_file", "apply_patch", "notebook_edit", "shell", "todo_write", "update_plan", "symbol_search"},
			OwnWorktree: true,
			MaySpawn:    false,
		},
		RoleReviewer: {
			Name:     RoleReviewer,
			Tools:    []string{"read_file", "grep", "glob", "list", "git_status", "git_diff", "git_log", "git_review", "git_changelog", "symbol_search"},
			ReadOnly: true,
			MaySpawn: false,
		},
	}
}

// RoleFor resolves a role by name; unknown roles are refused.
func RoleFor(name string) (RoleSpec, error) {
	roles := BuiltinRoles()
	r, ok := roles[Role(strings.ToLower(strings.TrimSpace(name)))]
	if !ok {
		return RoleSpec{}, fmt.Errorf("unknown subagent role %q (want explorer|implementer|reviewer)", name)
	}
	return r, nil
}

// AllowsTool reports whether the role may call the named tool (S1).
// A read-only role never allows a mutating tool even if it were
// (mistakenly) listed, so the read-only guarantee is absolute.
func (r RoleSpec) AllowsTool(tool string) bool {
	if r.ReadOnly && isMutatingTool(tool) {
		return false
	}
	for _, t := range r.Tools {
		if t == tool {
			return true
		}
	}
	return false
}

// isMutatingTool names tools that change state or dispatch work; a
// read-only role must never hold any of them.
func isMutatingTool(tool string) bool {
	switch tool {
	case "edit_file", "write_file", "apply_patch", "notebook_edit",
		"shell", "exec_command", "write_stdin", "kill_shell",
		"spawn_agent", "send_input", "close_agent", "resume_agent",
		"git_commit", "git_branch", "git_rebase", "git_push", "git_smart_commit":
		return true
	}
	return false
}

// BoundedSummary is a subagent's return value (S2): a short summary
// with evidence references (file:line), never the full transcript.
type BoundedSummary struct {
	AgentID   string
	Role      Role
	Text      string   // <= MaxSummaryRunes
	Evidence  []string // file:line references
	Truncated bool     // the raw transcript exceeded the bound
}

// MaxSummaryRunes caps a subagent summary so a caller never receives an
// unbounded transcript (S2).
const MaxSummaryRunes = 1200

// MaxEvidenceRefs caps the number of evidence references returned.
const MaxEvidenceRefs = 12

// Summarize builds a bounded summary from a role, an agent id, the
// raw output, and the evidence refs the run collected. The text is
// truncated to MaxSummaryRunes (a single trailing ellipsis marks it),
// and evidence is capped at MaxEvidenceRefs. The full transcript is
// never returned — only the bounded digest.
func Summarize(agentID string, role Role, rawOutput string, evidence []string) BoundedSummary {
	runes := []rune(rawOutput)
	truncated := false
	if len(runes) > MaxSummaryRunes {
		runes = runes[:MaxSummaryRunes]
		truncated = true
	}
	text := string(runes)
	if truncated {
		text += "…"
	}
	if len(evidence) > MaxEvidenceRefs {
		evidence = evidence[:MaxEvidenceRefs]
	}
	return BoundedSummary{
		AgentID:   agentID,
		Role:      role,
		Text:      text,
		Evidence:  evidence,
		Truncated: truncated,
	}
}

// Size reports the summary's total byte size — the S2 bound test
// asserts this stays under a fixed cap regardless of input length.
func (b BoundedSummary) Size() int {
	n := len(b.Text)
	for _, e := range b.Evidence {
		n += len(e) + 1
	}
	return n
}
