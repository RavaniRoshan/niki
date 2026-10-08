package main

// Natural-language git dispatch shared by `nikicode do` and the
// `nikicode git` subcommands (G2). Parsing is deterministic; every
// behavior is covered by the internal/git tests plus the parser tests
// below. Errors are git.GitError text: what happened and what to do.

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"

	"github.com/RavaniRoshan/niki/internal/git"
	"github.com/RavaniRoshan/niki/internal/intent"
)

var fileLinePattern = regexp.MustCompile(`([A-Za-z0-9_./-]+\.[A-Za-z0-9]+):(\d+)`)

// parseBranchTarget extracts create/switch intent plus the branch name.
func parseBranchTarget(input string) (action, name string, err error) {
	lowered := strings.ToLower(input)
	tokens := strings.Fields(input)
	after := func(words ...string) string {
		low := make([]string, len(tokens))
		for i, t := range tokens {
			low[i] = strings.ToLower(t)
		}
		skip := map[string]bool{"a": true, "the": true, "to": true, "branch": true, "called": true, "named": true}
		for i, w := range low {
			matched := false
			for _, word := range words {
				if w == word {
					matched = true
					break
				}
			}
			if !matched {
				continue
			}
			for _, next := range tokens[i+1:] {
				if !skip[strings.ToLower(next)] {
					return next
				}
			}
			return ""
		}
		return ""
	}
	switch {
	case strings.Contains(lowered, "switch") || strings.Contains(lowered, "checkout") || strings.Contains(lowered, "go to"):
		name = after("to", "switch", "checkout")
		if name == "" {
			return "", "", fmt.Errorf("which branch? say e.g. 'switch to main'")
		}
		return "switch", name, nil
	default:
		name = after("branch", "create", "new")
		if name == "" {
			return "", "", fmt.Errorf("which branch? say e.g. 'create branch feature/login'")
		}
		return "create", name, nil
	}
}

// parseRebaseOnto extracts the ref after "onto".
func parseRebaseOnto(input string) (string, error) {
	tokens := strings.Fields(input)
	for i, t := range tokens {
		if strings.ToLower(t) == "onto" && i+1 < len(tokens) {
			return tokens[i+1], nil
		}
	}
	return "", fmt.Errorf("rebase onto what? say e.g. 'rebase onto main'")
}

// parseBlameTarget extracts file and line from "file.go:10",
// "line 10 of file.go", or "file.go line 10".
func parseBlameTarget(input string) (file string, line int, err error) {
	if m := fileLinePattern.FindStringSubmatch(input); m != nil {
		n, _ := strconv.Atoi(m[2])
		return m[1], n, nil
	}
	tokens := strings.Fields(input)
	for i, t := range tokens {
		if strings.ToLower(t) == "line" && i+1 < len(tokens) {
			n, aerr := strconv.Atoi(strings.Trim(tokens[i+1], ",:"))
			if aerr != nil {
				return "", 0, fmt.Errorf("bad line number %q", tokens[i+1])
			}
			// File is the nearest path-like token.
			for _, cand := range tokens {
				if strings.Contains(cand, ".") && !strings.HasPrefix(cand, "line") {
					return strings.Trim(cand, ",:"), n, nil
				}
			}
			return "", 0, fmt.Errorf("which file? say e.g. 'blame main.go:10'")
		}
	}
	return "", 0, fmt.Errorf("say e.g. 'blame main.go:10'")
}

// parseCount returns the first integer token, or def.
func parseCount(input string, def int) int {
	for _, t := range strings.Fields(input) {
		if n, err := strconv.Atoi(strings.Trim(t, ",:")); err == nil && n > 0 && n < 1000 {
			return n
		}
	}
	return def
}

// parsePRBase extracts the base after "against", defaulting to main.
func parsePRBase(input string) string {
	tokens := strings.Fields(input)
	for i, t := range tokens {
		if strings.ToLower(t) == "against" && i+1 < len(tokens) {
			return tokens[i+1]
		}
	}
	return "main"
}

// runDoGitOp executes one routed git action and returns printable output.
func runDoGitOp(dir string, a intent.Action) (string, error) {
	switch a.Op {
	case intent.GitStatus:
		files, err := git.Status(dir)
		if err != nil {
			return "", err
		}
		if len(files) == 0 {
			return "clean", nil
		}
		var b strings.Builder
		for _, f := range files {
			fmt.Fprintf(&b, "%c%c %s\n", f.X, f.Y, f.Path)
		}
		return strings.TrimRight(b.String(), "\n"), nil
	case intent.GitCommit:
		msg, err := git.ProposeCommitMessage(dir)
		if err != nil {
			return "", err
		}
		if err := git.Commit(dir, msg); err != nil {
			return "", err
		}
		return "committed: " + strings.SplitN(msg, "\n", 2)[0], nil
	case intent.GitBranch:
		action, name, err := parseBranchTarget(a.Text)
		if err != nil {
			return "", err
		}
		if action == "switch" {
			err = git.BranchSwitch(dir, name)
		} else {
			err = git.BranchCreate(dir, name)
		}
		if err != nil {
			return "", err
		}
		return action + "d branch " + name, nil
	case intent.GitRebase:
		onto, err := parseRebaseOnto(a.Text)
		if err != nil {
			return "", err
		}
		if err := git.Rebase(dir, onto); err != nil {
			return "", err
		}
		return "rebased onto " + onto, nil
	case intent.GitBlame:
		file, line, err := parseBlameTarget(a.Text)
		if err != nil {
			return "", err
		}
		info, err := git.Blame(dir, file, line)
		if err != nil {
			return "", err
		}
		return fmt.Sprintf("%s:%d: %s — %s (%s, %s)", file, line, info.Line, info.Summary, info.Author, info.Date), nil
	case intent.GitLog:
		entries, err := git.Log(dir, parseCount(a.Text, 10))
		if err != nil {
			return "", err
		}
		var b strings.Builder
		for _, e := range entries {
			short := e.Hash
			if len(short) > 7 {
				short = short[:7]
			}
			fmt.Fprintf(&b, "%s %s (%s, %s)\n", short, e.Subject, e.Author, e.Date)
		}
		return strings.TrimRight(b.String(), "\n"), nil
	case intent.GitReview:
		reviews, err := git.ReviewStaged(dir)
		if err != nil {
			return "", err
		}
		var b strings.Builder
		for _, r := range reviews {
			fmt.Fprintf(&b, "## %s (+%d/-%d, %d hunks)\n%s\n\n", r.Path, r.Added, r.Del, r.Hunks, r.Sample)
		}
		return strings.TrimRight(b.String(), "\n"), nil
	case intent.GitChangelog:
		return git.Changelog(dir, parseCount(a.Text, 10))
	case intent.GitPRDraft:
		return git.PRDraft(dir, parsePRBase(a.Text), "PR_DRAFT.md")
	default:
		return "", fmt.Errorf("unknown git operation")
	}
}
