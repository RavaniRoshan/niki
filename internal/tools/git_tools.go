package tools

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/RavaniRoshan/niki/internal/git"
)

// Git workflow tools (G2): thin, fail-closed wrappers over internal/git,
// which shells out to the system git. Read-only tools are marked safe;
// write tools are withheld in plan mode and need an allowing guard.
func gitDir(d string) string {
	if d == "" {
		return "."
	}
	return d
}

func gitErr(err error) (ToolResult, error) {
	if ce, ok := err.(*git.ConflictError); ok {
		return ToolResult{Output: ce.Error(), IsError: true}, nil
	}
	if ge, ok := err.(*git.GitError); ok {
		return ToolResult{Output: ge.Error(), IsError: true}, nil
	}
	return ToolResult{}, err
}

// --- git_status ---

type GitStatusTool struct{ Base }

func NewGitStatusTool() *GitStatusTool {
	return &GitStatusTool{Base: Base{SchemaStr: `{"required":[],"fields":{"dir":"string"}}`}}
}

func (t *GitStatusTool) Name() string        { return "git_status" }
func (t *GitStatusTool) Description() string { return "Show git status (porcelain) for a work tree" }
func (t *GitStatusTool) IsConcurrencySafe() bool { return true }
func (t *GitStatusTool) IsReadOnly() bool        { return true }

func (t *GitStatusTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a struct {
		Dir string `json:"dir"`
	}
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	files, err := git.Status(gitDir(a.Dir))
	if err != nil {
		return gitErr(err)
	}
	if len(files) == 0 {
		return ToolResult{Output: "clean"}, nil
	}
	var b strings.Builder
	for _, f := range files {
		fmt.Fprintf(&b, "%c%c %s\n", f.X, f.Y, f.Path)
	}
	return ToolResult{Output: strings.TrimRight(b.String(), "\n")}, nil
}

// --- git_commit ---

type GitCommitTool struct{ Base }

func NewGitCommitTool() *GitCommitTool {
	return &GitCommitTool{Base: Base{SchemaStr: `{"required":[],"fields":{"dir":"string","message":"string","stage":"string"}}`}}
}

func (t *GitCommitTool) Name() string        { return "git_commit" }
func (t *GitCommitTool) Description() string { return "Commit staged changes (message derived from the staged diff when empty); optional stage paths first" }

func (t *GitCommitTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a struct {
		Dir     string `json:"dir"`
		Message string `json:"message"`
		Stage   string `json:"stage"`
	}
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	dir := gitDir(a.Dir)
	if a.Stage != "" {
		if err := git.Stage(dir, strings.Fields(a.Stage)...); err != nil {
			return gitErr(err)
		}
	}
	msg := a.Message
	if msg == "" {
		var err error
		msg, err = git.ProposeCommitMessage(dir)
		if err != nil {
			return gitErr(err)
		}
	}
	if err := git.Commit(dir, msg); err != nil {
		return gitErr(err)
	}
	return ToolResult{Output: "committed: " + strings.SplitN(msg, "\n", 2)[0]}, nil
}

// --- git_branch ---

type GitBranchTool struct{ Base }

func NewGitBranchTool() *GitBranchTool {
	return &GitBranchTool{Base: Base{SchemaStr: `{"required":["action","name"],"fields":{"dir":"string","action":"string","name":"string"}}`}}
}

func (t *GitBranchTool) Name() string        { return "git_branch" }
func (t *GitBranchTool) Description() string { return "Create (action=create) or switch to (action=switch) a branch" }

func (t *GitBranchTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a struct {
		Dir    string `json:"dir"`
		Action string `json:"action"`
		Name   string `json:"name"`
	}
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	dir := gitDir(a.Dir)
	var err error
	switch a.Action {
	case "create":
		err = git.BranchCreate(dir, a.Name)
	case "switch":
		err = git.BranchSwitch(dir, a.Name)
	default:
		return ToolResult{Output: fmt.Sprintf("unknown action %q: use create or switch", a.Action), IsError: true}, nil
	}
	if err != nil {
		return gitErr(err)
	}
	return ToolResult{Output: a.Action + "d branch " + a.Name}, nil
}

// --- git_rebase ---

type GitRebaseTool struct{ Base }

func NewGitRebaseTool() *GitRebaseTool {
	return &GitRebaseTool{Base: Base{SchemaStr: `{"required":["onto"],"fields":{"dir":"string","onto":"string","abort":"boolean","continue":"boolean"}}`}}
}

func (t *GitRebaseTool) Name() string        { return "git_rebase" }
func (t *GitRebaseTool) Description() string { return "Rebase onto a ref (autostash); conflicts stop with a report; abort/continue drive the cycle" }

func (t *GitRebaseTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a struct {
		Dir      string `json:"dir"`
		Onto     string `json:"onto"`
		Abort    bool   `json:"abort"`
		Continue bool   `json:"continue"`
	}
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	dir := gitDir(a.Dir)
	switch {
	case a.Abort:
		if err := git.RebaseAbort(dir); err != nil {
			return gitErr(err)
		}
		return ToolResult{Output: "rebase aborted"}, nil
	case a.Continue:
		if err := git.RebaseContinue(dir); err != nil {
			return gitErr(err)
		}
		return ToolResult{Output: "rebase continued"}, nil
	default:
		if err := git.Rebase(dir, a.Onto); err != nil {
			return gitErr(err)
		}
		return ToolResult{Output: "rebased onto " + a.Onto}, nil
	}
}

// --- git_blame ---

type GitBlameTool struct{ Base }

func NewGitBlameTool() *GitBlameTool {
	return &GitBlameTool{Base: Base{SchemaStr: `{"required":["file","line"],"fields":{"dir":"string","file":"string","line":"number"}}`}}
}

func (t *GitBlameTool) Name() string        { return "git_blame" }
func (t *GitBlameTool) Description() string { return "Blame one line: who, when, and the commit summary" }
func (t *GitBlameTool) IsConcurrencySafe() bool { return true }
func (t *GitBlameTool) IsReadOnly() bool        { return true }

func (t *GitBlameTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a struct {
		Dir  string `json:"dir"`
		File string `json:"file"`
		Line int    `json:"line"`
	}
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	info, err := git.Blame(gitDir(a.Dir), a.File, a.Line)
	if err != nil {
		return gitErr(err)
	}
	return ToolResult{Output: fmt.Sprintf("%s:%d: %s — %s (%s, %s)", a.File, a.Line, info.Line, info.Summary, info.Author, info.Date)}, nil
}

// --- git_log ---

type GitLogTool struct{ Base }

func NewGitLogTool() *GitLogTool {
	return &GitLogTool{Base: Base{SchemaStr: `{"required":[],"fields":{"dir":"string","n":"number"}}`}}
}

func (t *GitLogTool) Name() string        { return "git_log" }
func (t *GitLogTool) Description() string { return "Recent commit history" }
func (t *GitLogTool) IsConcurrencySafe() bool { return true }
func (t *GitLogTool) IsReadOnly() bool        { return true }

func (t *GitLogTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a struct {
		Dir string `json:"dir"`
		N   int    `json:"n"`
	}
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	entries, err := git.Log(gitDir(a.Dir), a.N)
	if err != nil {
		return gitErr(err)
	}
	var b strings.Builder
	for _, e := range entries {
		short := e.Hash
		if len(short) > 7 {
			short = short[:7]
		}
		fmt.Fprintf(&b, "%s %s (%s, %s)\n", short, e.Subject, e.Author, e.Date)
	}
	return ToolResult{Output: strings.TrimRight(b.String(), "\n")}, nil
}

// --- git_review ---

type GitReviewTool struct{ Base }

func NewGitReviewTool() *GitReviewTool {
	return &GitReviewTool{Base: Base{SchemaStr: `{"required":[],"fields":{"dir":"string"}}`}}
}

func (t *GitReviewTool) Name() string        { return "git_review" }
func (t *GitReviewTool) Description() string { return "Review staged changes: per-file counts plus the first hunk" }
func (t *GitReviewTool) IsConcurrencySafe() bool { return true }
func (t *GitReviewTool) IsReadOnly() bool        { return true }

func (t *GitReviewTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a struct {
		Dir string `json:"dir"`
	}
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	reviews, err := git.ReviewStaged(gitDir(a.Dir))
	if err != nil {
		return gitErr(err)
	}
	var b strings.Builder
	for _, r := range reviews {
		fmt.Fprintf(&b, "## %s (+%d/-%d, %d hunks)\n%s\n\n", r.Path, r.Added, r.Del, r.Hunks, r.Sample)
	}
	return ToolResult{Output: strings.TrimRight(b.String(), "\n")}, nil
}

// --- git_changelog ---

type GitChangelogTool struct{ Base }

func NewGitChangelogTool() *GitChangelogTool {
	return &GitChangelogTool{Base: Base{SchemaStr: `{"required":[],"fields":{"dir":"string","n":"number"}}`}}
}

func (t *GitChangelogTool) Name() string        { return "git_changelog" }
func (t *GitChangelogTool) Description() string { return "Render recent history as markdown changelog" }
func (t *GitChangelogTool) IsConcurrencySafe() bool { return true }
func (t *GitChangelogTool) IsReadOnly() bool        { return true }

func (t *GitChangelogTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a struct {
		Dir string `json:"dir"`
		N   int    `json:"n"`
	}
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	out, err := git.Changelog(gitDir(a.Dir), a.N)
	if err != nil {
		return gitErr(err)
	}
	return ToolResult{Output: out}, nil
}

// --- git_diff_summary ---

type GitDiffSummaryTool struct{ Base }

func NewGitDiffSummaryTool() *GitDiffSummaryTool {
	return &GitDiffSummaryTool{Base: Base{SchemaStr: `{"required":[],"fields":{"dir":"string"}}`}}
}

func (t *GitDiffSummaryTool) Name() string              { return "git_diff_summary" }
func (t *GitDiffSummaryTool) Description() string       { return "Show line additions, deletions, and file change statistics" }
func (t *GitDiffSummaryTool) IsConcurrencySafe() bool   { return true }
func (t *GitDiffSummaryTool) IsReadOnly() bool          { return true }

func (t *GitDiffSummaryTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a struct {
		Dir string `json:"dir"`
	}
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	out, err := git.DiffSummary(gitDir(a.Dir))
	if err != nil {
		return gitErr(err)
	}
	return ToolResult{Output: out}, nil
}

// --- git_smart_commit ---

type GitSmartCommitTool struct{ Base }

func NewGitSmartCommitTool() *GitSmartCommitTool {
	return &GitSmartCommitTool{Base: Base{SchemaStr: `{"required":[],"fields":{"dir":"string"}}`}}
}

func (t *GitSmartCommitTool) Name() string        { return "git_smart_commit" }
func (t *GitSmartCommitTool) Description() string { return "Draft an automated conventional commit message from diff and commit" }
func (t *GitSmartCommitTool) IsConcurrencySafe() bool { return false }
func (t *GitSmartCommitTool) IsReadOnly() bool        { return false }

func (t *GitSmartCommitTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a struct {
		Dir string `json:"dir"`
	}
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	dir := gitDir(a.Dir)
	msg, err := git.SmartCommitMessage(dir)
	if err != nil {
		return gitErr(err)
	}
	if err := git.Commit(dir, msg); err != nil {
		return gitErr(err)
	}
	return ToolResult{Output: fmt.Sprintf("Committed with message:\n%s", msg)}, nil
}

// --- git_pr_summary ---

type GitPRSummaryTool struct{ Base }

func NewGitPRSummaryTool() *GitPRSummaryTool {
	return &GitPRSummaryTool{Base: Base{SchemaStr: `{"required":[],"fields":{"dir":"string","base":"string"}}`}}
}

func (t *GitPRSummaryTool) Name() string              { return "git_pr_summary" }
func (t *GitPRSummaryTool) Description() string       { return "Draft a GitHub PR description with commits and change stats" }
func (t *GitPRSummaryTool) IsConcurrencySafe() bool   { return true }
func (t *GitPRSummaryTool) IsReadOnly() bool          { return true }

func (t *GitPRSummaryTool) Run(ctx context.Context, args json.RawMessage) (ToolResult, error) {
	var a struct {
		Dir  string `json:"dir"`
		Base string `json:"base"`
	}
	if err := json.Unmarshal(args, &a); err != nil {
		return ToolResult{}, fmt.Errorf("bad args: %w", err)
	}
	out, err := git.PRSummary(gitDir(a.Dir), a.Base)
	if err != nil {
		return gitErr(err)
	}
	return ToolResult{Output: out}, nil
}
