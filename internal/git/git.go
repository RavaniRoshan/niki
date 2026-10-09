// Package git implements NikiCode's git workflows (G2) by shelling out
// to the system git — no libgit2, no cgo — and parsing porcelain output.
// Every error says what happened and what to do. Write operations refuse
// to run outside a work tree and never force-push, never rebase --onto
// without an explicit target, and abort (rather than strand) a rebase
// that conflicts, unless the caller drives the conflict cycle.
package git

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"time"
)

// Available reports whether the system git exists.
func Available() bool {
	_, err := exec.LookPath("git")
	return err == nil
}

// GitError is a failed git invocation with its stderr and a hint.
type GitError struct {
	Args   []string
	Stderr string
	Hint   string
}

func (e *GitError) Error() string {
	msg := fmt.Sprintf("git %s failed: %s", strings.Join(e.Args, " "), e.Stderr)
	if e.Hint != "" {
		msg += ". " + e.Hint
	}
	return msg
}

func run(dir string, hint string, args ...string) (string, error) {
	cmd := exec.Command("git", args...)
	cmd.Dir = dir
	// Non-interactive: never prompt for credentials or editors.
	cmd.Env = append(os.Environ(),
		"GIT_TERMINAL_PROMPT=0",
		"GIT_EDITOR=true",
		"EDITOR=true",
	)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return "", &GitError{Args: args, Stderr: strings.TrimSpace(stderr.String()), Hint: hint}
	}
	return stdout.String(), nil
}

// InWorkTree reports whether dir is inside a git work tree.
func InWorkTree(dir string) bool {
	out, err := run(dir, "run this inside a git repository", "rev-parse", "--is-inside-work-tree")
	return err == nil && strings.TrimSpace(out) == "true"
}

// FileStatus is one porcelain v1 status entry.
type FileStatus struct {
	// XY are the porcelain status codes (index, worktree).
	X, Y   byte
	Path   string
	Staged bool
}

// Status returns `git status --porcelain=v1` entries.
func Status(dir string) ([]FileStatus, error) {
	out, err := run(dir, "run this inside a git repository", "status", "--porcelain=v1", "--untracked-files=normal")
	if err != nil {
		return nil, err
	}
	var files []FileStatus
	for _, line := range strings.Split(out, "\n") {
		if len(line) < 4 {
			continue
		}
		if line[2] != ' ' {
			continue
		}
		files = append(files, FileStatus{
			X: line[0], Y: line[1],
			Path:   strings.TrimSpace(line[3:]),
			Staged: line[0] != ' ' && line[0] != '?',
		})
	}
	return files, nil
}

// StagedFiles returns paths with staged changes.
func StagedFiles(dir string) ([]string, error) {
	files, err := Status(dir)
	if err != nil {
		return nil, err
	}
	var out []string
	for _, f := range files {
		if f.Staged {
			out = append(out, f.Path)
		}
	}
	return out, nil
}

// NumStat is per-file added/removed line counts from `git diff --numstat`.
type NumStat struct {
	Path       string
	Added, Del int
	Binary     bool
}

// StagedNumStat returns `git diff --cached --numstat` rows.
func StagedNumStat(dir string) ([]NumStat, error) {
	out, err := run(dir, "stage changes first with git add", "diff", "--cached", "--numstat")
	if err != nil {
		return nil, err
	}
	var rows []NumStat
	for _, line := range strings.Split(strings.TrimSpace(out), "\n") {
		if line == "" {
			continue
		}
		parts := strings.SplitN(line, "\t", 3)
		if len(parts) != 3 {
			continue
		}
		row := NumStat{Path: parts[2]}
		if parts[0] == "-" {
			row.Binary = true
		} else {
			row.Added, _ = strconv.Atoi(parts[0])
			row.Del, _ = strconv.Atoi(parts[1])
		}
		rows = append(rows, row)
	}
	return rows, nil
}

// ProposeCommitMessage derives a conventional-style commit message from
// the REAL staged diff (never invented): scope from the top directory,
// file count, and line totals.
func ProposeCommitMessage(dir string) (string, error) {
	rows, err := StagedNumStat(dir)
	if err != nil {
		return "", err
	}
	if len(rows) == 0 {
		return "", &GitError{Args: []string{"diff", "--cached"}, Hint: "nothing is staged: stage changes with git add first"}
	}
	scope := topDir(rows)
	var added, del int
	var names []string
	for _, r := range rows {
		added += r.Added
		del += r.Del
		names = append(names, r.Path)
	}
	subject := fmt.Sprintf("%s: update %d file%s (+%d/-%d)", scope, len(rows), plural(len(rows)), added, del)
	body := "Files:\n- " + strings.Join(names, "\n- ")
	return subject + "\n\n" + body, nil
}

func topDir(rows []NumStat) string {
	if len(rows) == 1 {
		if d := filepath.Dir(rows[0].Path); d != "." {
			return d
		}
		return strings.TrimSuffix(rows[0].Path, filepath.Ext(rows[0].Path))
	}
	// Common leading directory across all paths.
	split := func(p string) []string { return strings.Split(filepath.Dir(p), string(filepath.Separator)) }
	common := split(rows[0].Path)
	for _, r := range rows[1:] {
		parts := split(r.Path)
		i := 0
		for i < len(common) && i < len(parts) && common[i] == parts[i] {
			i++
		}
		common = common[:i]
	}
	if len(common) == 0 || (len(common) == 1 && common[0] == ".") {
		return "repo"
	}
	return strings.Join(common, "/")
}

func plural(n int) string {
	if n == 1 {
		return ""
	}
	return "s"
}

// Stage stages paths (git add). Empty paths is a no-op error.
func Stage(dir string, paths ...string) error {
	if len(paths) == 0 {
		return &GitError{Args: []string{"add"}, Hint: "no paths given to stage"}
	}
	args := append([]string{"add", "--"}, paths...)
	_, err := run(dir, "check the paths exist inside the work tree", args...)
	return err
}

// Commit creates a commit with msg. It refuses when nothing is staged.
func Commit(dir, msg string) error {
	staged, err := StagedFiles(dir)
	if err != nil {
		return err
	}
	if len(staged) == 0 {
		return &GitError{Args: []string{"commit"}, Hint: "nothing is staged: stage changes with git add first"}
	}
	_, err = run(dir, "resolve the failure (e.g. set user.name/user.email) and retry", "commit", "-m", msg)
	return err
}

// CurrentBranch returns the checked-out branch (or detached SHA).
func CurrentBranch(dir string) (string, error) {
	out, err := run(dir, "run this inside a git repository", "branch", "--show-current")
	if err != nil {
		return "", err
	}
	if b := strings.TrimSpace(out); b != "" {
		return b, nil
	}
	out, err = run(dir, "run this inside a git repository", "rev-parse", "--short", "HEAD")
	if err != nil {
		return "", err
	}
	return "detached:" + strings.TrimSpace(out), nil
}

// BranchCreate creates and checks out a new branch.
func BranchCreate(dir, name string) error {
	if strings.TrimSpace(name) == "" {
		return &GitError{Args: []string{"checkout", "-b"}, Hint: "give the new branch a name"}
	}
	_, err := run(dir, "pick a name that does not exist yet, or switch to it with git switch", "checkout", "-b", name)
	return err
}

// BranchSwitch checks out an existing branch.
func BranchSwitch(dir, name string) error {
	if strings.TrimSpace(name) == "" {
		return &GitError{Args: []string{"switch"}, Hint: "give a branch name to switch to"}
	}
	_, err := run(dir, "list branches with git branch and pick an existing one", "switch", name)
	return err
}

// ConflictError reports a rebase (or merge) stopped on conflicts.
// The repository is left in the conflicted state for inspection;
// call RebaseAbort to back out or resolve + RebaseContinue.
type ConflictError struct {
	Files []string
}

func (e *ConflictError) Error() string {
	return fmt.Sprintf("stopped on conflicts in %d file(s): %s. Resolve them, stage with git add, then continue — or abort to back out", len(e.Files), strings.Join(e.Files, ", "))
}

// UnmergedFiles lists paths with unresolved conflicts.
func UnmergedFiles(dir string) ([]string, error) {
	out, err := run(dir, "run this inside a git repository", "diff", "--name-only", "--diff-filter=U")
	if err != nil {
		return nil, err
	}
	var files []string
	for _, line := range strings.Split(strings.TrimSpace(out), "\n") {
		if line != "" {
			files = append(files, line)
		}
	}
	return files, nil
}

// Rebase rebases the current branch onto onto with autostash. On
// conflict it returns a *ConflictError with the repo mid-rebase;
// otherwise the rebase completes and nil is returned.
func Rebase(dir, onto string) error {
	if strings.TrimSpace(onto) == "" {
		return &GitError{Args: []string{"rebase"}, Hint: "give a branch or ref to rebase onto"}
	}
	_, err := run(dir, "fetch the upstream ref first, or fix the conflict set", "rebase", "--autostash", onto)
	if err == nil {
		return nil
	}
	files, uerr := UnmergedFiles(dir)
	if uerr != nil || len(files) == 0 {
		_, _ = run(dir, "", "rebase", "--abort")
		return err
	}
	return &ConflictError{Files: files}
}

// RebaseAbort backs out of a conflicted rebase.
func RebaseAbort(dir string) error {
	_, err := run(dir, "no rebase may be in progress; check git status", "rebase", "--abort")
	return err
}

// RebaseContinue continues a rebase after conflicts are resolved+staged.
func RebaseContinue(dir string) error {
	_, err := run(dir, "resolve all conflicts and stage them with git add first", "rebase", "--continue")
	if err == nil {
		return nil
	}
	if files, uerr := UnmergedFiles(dir); uerr == nil && len(files) > 0 {
		return &ConflictError{Files: files}
	}
	return err
}

// Head returns the current HEAD hash, or "" when unborn/missing.
func Head(dir string) string {
	out, err := run(dir, "", "rev-parse", "HEAD")
	if err != nil {
		return ""
	}
	return strings.TrimSpace(out)
}

// ResetSoft moves the branch tip to ref without touching index or tree.
func ResetSoft(dir, ref string) error {
	_, err := run(dir, "check git status and the ref, then retry", "reset", "--soft", ref)
	return err
}

// DeleteBranch removes a branch by name.
func DeleteBranch(dir, name string) error {
	_, err := run(dir, "switch off the branch first, then retry", "branch", "-D", name)
	return err
}

// LogEntry is one `git log` row.
type LogEntry struct {
	Hash, Author, Date, Subject string
}

// Log returns the last n commits (most recent first).
func Log(dir string, n int) ([]LogEntry, error) {
	if n <= 0 {
		n = 10
	}
	out, err := run(dir, "run this inside a git repository", "log", fmt.Sprintf("-n%d", n), "--pretty=format:%H%x1f%an%x1f%ad%x1f%s", "--date=short")
	if err != nil {
		return nil, err
	}
	var entries []LogEntry
	for _, line := range strings.Split(strings.TrimSpace(out), "\n") {
		if line == "" {
			continue
		}
		parts := strings.SplitN(line, "\x1f", 4)
		if len(parts) != 4 {
			continue
		}
		entries = append(entries, LogEntry{Hash: parts[0], Author: parts[1], Date: parts[2], Subject: parts[3]})
	}
	return entries, nil
}

// Changelog renders the last n commits as markdown.
func Changelog(dir string, n int) (string, error) {
	entries, err := Log(dir, n)
	if err != nil {
		return "", err
	}
	var b strings.Builder
	fmt.Fprintf(&b, "# Changelog (last %d)\n\n", len(entries))
	for _, e := range entries {
		short := e.Hash
		if len(short) > 7 {
			short = short[:7]
		}
		fmt.Fprintf(&b, "- %s %s (%s, %s)\n", short, e.Subject, e.Author, e.Date)
	}
	return b.String(), nil
}

// FileReview is a per-file staged-diff summary for review.
type FileReview struct {
	Path       string
	Added, Del int
	Hunks      int
	Sample     string
}

// ReviewStaged summarizes each staged file: line counts, hunk count,
// and the first hunk as a sample.
func ReviewStaged(dir string) ([]FileReview, error) {
	stats, err := StagedNumStat(dir)
	if err != nil {
		return nil, err
	}
	if len(stats) == 0 {
		return nil, &GitError{Args: []string{"diff", "--cached"}, Hint: "nothing is staged: stage changes with git add first"}
	}
	out, err := run(dir, "stage changes first with git add", "diff", "--cached", "--unified=1")
	if err != nil {
		return nil, err
	}
	// Split the combined diff per file on "diff --git " boundaries.
	var chunks []string
	for _, c := range strings.Split(out, "diff --git ") {
		if strings.TrimSpace(c) != "" {
			chunks = append(chunks, c)
		}
	}
	chunkByPath := map[string]string{}
	for _, c := range chunks {
		first, _, _ := strings.Cut(c, "\n")
		fields := strings.Fields(first)
		if len(fields) >= 2 {
			p := strings.TrimPrefix(fields[len(fields)-1], "b/")
			chunkByPath[p] = "diff --git " + c
		}
	}
	var reviews []FileReview
	for _, st := range stats {
		rev := FileReview{Path: st.Path, Added: st.Added, Del: st.Del}
		if chunk, ok := chunkByPath[st.Path]; ok {
			rev.Hunks = strings.Count(chunk, "\n@@")
			if i := strings.Index(chunk, "@@"); i >= 0 {
				if j := strings.Index(chunk[i:], "\n@@"); j >= 0 {
					rev.Sample = chunk[i : i+j]
				} else {
					rev.Sample = chunk[i:]
					if len(rev.Sample) > 1200 {
						rev.Sample = rev.Sample[:1200] + "\n…(truncated)"
					}
				}
			}
		}
		reviews = append(reviews, rev)
	}
	return reviews, nil
}

// BlameInfo answers "why does this exist" for one line.
type BlameInfo struct {
	Hash, Author, Date, Summary string
	Line                        string
}

// Blame runs `git blame -L <line>,<line>` and annotates with the commit.
func Blame(dir, file string, line int) (BlameInfo, error) {
	if line <= 0 {
		return BlameInfo{}, &GitError{Args: []string{"blame"}, Hint: "give a line number starting at 1"}
	}
	out, err := run(dir, "check the file is tracked and the line exists", "blame", "-L",
		fmt.Sprintf("%d,%d", line, line), "--porcelain", "--", file)
	if err != nil {
		return BlameInfo{}, err
	}
	info := BlameInfo{}
	for _, l := range strings.Split(out, "\n") {
		switch {
		case strings.HasPrefix(l, "author "):
			info.Author = strings.TrimPrefix(l, "author ")
		case strings.HasPrefix(l, "author-time "):
			if ts, err := strconv.ParseInt(strings.TrimPrefix(l, "author-time "), 10, 64); err == nil {
				info.Date = time.Unix(ts, 0).Format("2006-01-02")
			}
		case strings.HasPrefix(l, "summary "):
			info.Summary = strings.TrimPrefix(l, "summary ")
		case strings.HasPrefix(l, "\t"):
			info.Line = strings.TrimPrefix(l, "\t")
		default:
			if info.Hash == "" && len(l) >= 40 {
				info.Hash = l[:40]
			}
		}
	}
	if info.Hash == "" {
		return BlameInfo{}, &GitError{Args: []string{"blame"}, Hint: "no blame information found for that line"}
	}
	return info, nil
}

// PRDraft renders a pull-request draft as markdown: base..HEAD commits
// plus the diff stat. It writes nothing to the network; when out is
// non-empty it also saves the file.
func PRDraft(dir, base, out string) (string, error) {
	if base == "" {
		base = "main"
	}
	commits, err := run(dir, "fetch the base branch first, or name the right base", "log", base+"..HEAD", "--pretty=format:- %s (%an)")
	if err != nil {
		// Base may not exist (e.g. only master): fall back to recent log.
		commits, err = run(dir, "run this inside a git repository", "log", "-n5", "--pretty=format:- %s (%an)")
		if err != nil {
			return "", err
		}
	}
	stat, err := run(dir, "run this inside a git repository", "diff", base+"...HEAD", "--stat")
	if err != nil {
		stat, _ = run(dir, "", "diff", "--stat")
	}
	branch, _ := CurrentBranch(dir)
	var b strings.Builder
	fmt.Fprintf(&b, "# PR draft: %s → %s\n\n## Commits\n%s\n\n## Diff stat\n```\n%s```\n",
		branch, base, strings.TrimSpace(commits), strings.TrimSpace(stat))
	draft := b.String()
	if out != "" {
		if err := os.WriteFile(filepath.Join(dir, out), []byte(draft), 0o644); err != nil {
			return "", err
		}
	}
	return draft, nil
}
