// Package changelog renders release notes from conventional commits (U3).
// A conventional commit looks like:
//
//	type(scope)!: subject
//
// with an optional trailer body containing "BREAKING CHANGE: <text>".
// This package parses that shape into structured notes grouped by
// Added / Changed / Fixed / Removed and renders a Markdown segment.
package changelog

import (
	"bufio"
	"fmt"
	"io"
	"os"
	"regexp"
	"sort"
	"strings"
)

// Group is one section of the rendered notes.
type Group string

const (
	Added    Group = "Added"
	Changed  Group = "Changed"
	Removed  Group = "Removed"
	Fixed    Group = "Fixed"
	Security Group = "Security"
	Other    Group = "Other"
)

// Entry is one parsed commit note.
type Entry struct {
	Type     string // feat, fix, perf, etc.
	Scope    string // parenthesized scope, if any
	Breaking bool   // true when `!` present or a BREAKING CHANGE trailer exists
	Subject  string // human text, trailing period trimmed
}

// Changelog is a versioned collection of parsed entries grouped by section.
type Changelog struct {
	Version string
	Date    string
	Entries []Entry
}

// FromCommits parses conventional-commit subject lines into Entry values.
func FromCommits(lines []string) Changelog {
	return Changelog{Entries: parseLines(lines)}
}

var ccRe = regexp.MustCompile(`^([a-z]+)(?:\(([^)]+)\))?!?:\s*(.*)$`)

func parseLines(lines []string) []Entry {
	var out []Entry
	for _, raw := range lines {
		subject := strings.TrimSpace(raw)
		if subject == "" {
			continue
		}
		m := ccRe.FindStringSubmatch(subject)
		if m == nil {
			// Not conventional: keep the first line as an uncategorized note.
			out = append(out, Entry{Type: "other", Subject: subject})
			continue
		}
		e := Entry{Type: m[1], Scope: m[2], Subject: strings.TrimRight(m[3], ".")}
		e.Breaking = strings.Contains(subject, "!")
		out = append(out, e)
	}
	return out
}

// groupOf maps a commit type into a release-notes section.
func groupOf(e Entry) Group {
	switch e.Type {
	case "feat":
		return Added
	case "fix":
		return Fixed
	case "perf", "refactor", "build", "ci", "chore", "style", "test", "docs", "revert":
		if e.Breaking {
			return Changed
		}
		return Other
	default:
		return Other
	}
}

// RenderMarkdown writes the changelog as a Markdown segment.
func (c Changelog) RenderMarkdown(w io.Writer) {
	fmt.Fprintf(w, "## %s — %s\n\n", c.Version, c.Date)
	byGroup := map[Group][]Entry{}
	var order []Group
	for _, e := range c.Entries {
		g := groupOf(e)
		if _, ok := byGroup[g]; !ok {
			order = append(order, g)
		}
		byGroup[g] = append(byGroup[g], e)
	}
	priority := []Group{Added, Changed, Removed, Fixed, Security, Other}
	sort.Slice(order, func(i, j int) bool {
		return groupRank(order[i], priority) < groupRank(order[j], priority)
	})
	for _, g := range order {
		fmt.Fprintf(w, "### %s\n", g)
		for _, e := range byGroup[g] {
			if e.Scope != "" {
				fmt.Fprintf(w, "- %s: %s\n", e.Scope, e.Subject)
			} else {
				fmt.Fprintf(w, "- %s\n", e.Subject)
			}
		}
		fmt.Fprintln(w)
	}
}

func groupRank(g Group, priority []Group) int {
	for i, p := range priority {
		if p == g {
			return i
		}
	}
	return len(priority)
}

// ScanLog reads conventional-commit subject lines from a log, one per line,
// ignoring the "hash subject" prefix that `git log --oneline` emits.
func ScanLog(r io.Reader) []Entry {
	var lines []string
	sc := bufio.NewScanner(r)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" {
			continue
		}
		if rest := stripHash(line); rest != "" {
			line = rest
		}
		lines = append(lines, line)
	}
	return parseLines(lines)
}

var hashRe = regexp.MustCompile(`^[0-9a-f]{7,40}\s`)

func stripHash(line string) string {
	return hashRe.ReplaceAllString(line, "")
}

// readFileBytes is a tiny helper for golden comparison (tests only).
func readFileBytes(path string) ([]byte, error) {
	return os.ReadFile(path)
}
