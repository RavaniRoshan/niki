// Package explain answers codebase questions with citations to real
// file:line locations (G2). It never invents symbols: every cited
// location is read from disk during the search, and an unknown symbol
// is refused with the searched scope instead of guessed at.
package explain

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"sync"
)

// Location is one verified hit: the file, 1-based line, and the line's
// exact text as read from disk.
type Location struct {
	Path string
	Line int
	Text string
}

// Answer is an evidence bundle for a question.
type Answer struct {
	Query     string
	Locations []Location
	Summary   string
	Refused   bool
	Reason    string
	Searched  []string
}

const (
	maxFileBytes = 1 << 20 // 1 MB: larger files are skipped, never truncated silently
	maxHits      = 50
)

var skipDirs = map[string]bool{
	".git": true, "node_modules": true, "vendor": true,
	".nikicode": true, ".niki": true, "bin": true,
}

// symbolBoundary matches symbol as a code token, not a substring.
func symbolPattern(symbol string) *regexp.Regexp {
	return regexp.MustCompile(`(?:^|[^A-Za-z0-9_])` + regexp.QuoteMeta(symbol) + `(?:[^A-Za-z0-9_]|$)`)
}

func isBinary(path string) bool {
	f, err := os.Open(path)
	if err != nil {
		return true
	}
	defer f.Close()
	buf := make([]byte, 8000)
	n, _ := f.Read(buf)
	for _, b := range buf[:n] {
		if b == 0 {
			return true
		}
	}
	return false
}

// ExplainSymbol searches repoRoot for symbol and returns verified hits.
func ExplainSymbol(repoRoot, symbol string) Answer {
	ans := Answer{Query: symbol, Searched: []string{repoRoot}}
	symbol = strings.TrimSpace(symbol)
	if symbol == "" {
		ans.Refused = true
		ans.Reason = "no symbol given: ask about a named function, type, file, or `quoted` symbol"
		return ans
	}
	pat := symbolPattern(symbol)
	_ = filepath.WalkDir(repoRoot, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			if skipDirs[d.Name()] {
				return filepath.SkipDir
			}
			return nil
		}
		if info, err := d.Info(); err != nil || info.Size() > maxFileBytes || isBinary(path) {
			return nil
		}
		hits := searchFile(path, pat, repoRoot)
		ans.Locations = append(ans.Locations, hits...)
		if len(ans.Locations) >= maxHits {
			return filepath.SkipAll
		}
		return nil
	})
	sort.Slice(ans.Locations, func(i, j int) bool {
		if ans.Locations[i].Path == ans.Locations[j].Path {
			return ans.Locations[i].Line < ans.Locations[j].Line
		}
		return ans.Locations[i].Path < ans.Locations[j].Path
	})
	if len(ans.Locations) == 0 {
		ans.Refused = true
		ans.Reason = fmt.Sprintf("no definition or use of %q found under %s (skipped .git, binaries, files over 1MB)", symbol, repoRoot)
		return ans
	}
	ans.Summary = summarizeSymbol(symbol, ans.Locations)
	return ans
}

func searchFile(path string, pat *regexp.Regexp, root string) []Location {
	f, err := os.Open(path)
	if err != nil {
		return nil
	}
	defer f.Close()
	rel := path
	if r, err := filepath.Rel(root, path); err == nil {
		rel = r
	}
	var hits []Location
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 64*1024), 2*1024*1024)
	line := 0
	for sc.Scan() {
		line++
		text := sc.Text()
		if pat.MatchString(text) {
			hits = append(hits, Location{Path: rel, Line: line, Text: strings.TrimSpace(text)})
			if len(hits) >= 8 {
				break
			}
		}
	}
	return hits
}

var defPattern = sync.OnceValue(func() *regexp.Regexp {
	return regexp.MustCompile(`^\s*(func\b|type\b|class\b|def\b|const\b|var\b|interface\b).*$`)
})

func summarizeSymbol(symbol string, locs []Location) string {
	files := map[string]int{}
	for _, l := range locs {
		files[l.Path]++
	}
	var names []string
	for f := range files {
		names = append(names, f)
	}
	sort.Strings(names)
	var b strings.Builder
	fmt.Fprintf(&b, "%q appears %d time(s) in %d file(s): %s.", symbol, len(locs), len(names), strings.Join(names, ", "))
	var defs []Location
	for _, l := range locs {
		if defPattern().MatchString(l.Text) {
			defs = append(defs, l)
		}
	}
	if len(defs) > 0 {
		b.WriteString(" Likely definition(s):")
		for _, d := range defs {
			fmt.Fprintf(&b, "\n- %s:%d: %s", d.Path, d.Line, d.Text)
		}
	}
	return b.String()
}

// defLine recognizes a definition-ish line for outlining.
var outlinePattern = sync.OnceValue(func() *regexp.Regexp {
	return regexp.MustCompile(`^\s*(func\b.+|type\s+\w+.+|class\s+\w+.*|def\s+\w+.*|package\s+\w+|#[^!].*|//.*)$`)
})

// ExplainFile outlines a real file: line count, definition-ish lines,
// and the head comment. Refuses when the file does not exist.
func ExplainFile(repoRoot, path string) Answer {
	ans := Answer{Query: path, Searched: []string{repoRoot}}
	abs := path
	if !filepath.IsAbs(path) {
		abs = filepath.Join(repoRoot, path)
	}
	data, err := os.ReadFile(abs)
	if err != nil {
		ans.Refused = true
		ans.Reason = fmt.Sprintf("cannot read %q: %v", path, err)
		return ans
	}
	rel := path
	if r, err := filepath.Rel(repoRoot, abs); err == nil && !strings.HasPrefix(r, "..") {
		rel = r
	}
	lines := strings.Split(string(data), "\n")
	var b strings.Builder
	fmt.Fprintf(&b, "%s has %d lines. Outline:", rel, len(lines))
	shown := 0
	for i, l := range lines {
		if outlinePattern().MatchString(l) && strings.TrimSpace(l) != "" {
			fmt.Fprintf(&b, "\n- %s:%d: %s", rel, i+1, strings.TrimSpace(l))
			shown++
			if shown >= 20 {
				b.WriteString("\n- …(outline capped at 20)")
				break
			}
		}
	}
	if shown == 0 {
		b.WriteString(" (no definition-like lines found)")
	}
	ans.Summary = b.String()
	ans.Locations = []Location{{Path: rel, Line: 1, Text: firstLine(lines)}}
	return ans
}

func firstLine(lines []string) string {
	for _, l := range lines {
		if strings.TrimSpace(l) != "" {
			return strings.TrimSpace(l)
		}
	}
	return "(empty file)"
}

// AnswerQuestion routes a natural question: a `quoted` token or a path
// ending in a known extension goes to file/symbol search; leading verbs
// (explain, what, why, how, where) are stripped to find the subject.
func AnswerQuestion(repoRoot, question string) Answer {
	q := strings.TrimSpace(question)
	if m := regexp.MustCompile("`([^`]+)`").FindStringSubmatch(q); m != nil {
		subj := strings.TrimSpace(m[1])
		if looksLikePath(subj) {
			return ExplainFile(repoRoot, subj)
		}
		return ExplainSymbol(repoRoot, subj)
	}
	if subj := SubjectOf(q); subj != "" {
		if looksLikePath(subj) || strings.ContainsAny(subj, "./") && existingPath(repoRoot, subj) {
			return ExplainFile(repoRoot, subj)
		}
		return ExplainSymbol(repoRoot, subj)
	}
	return Answer{Query: q, Refused: true, Reason: "ask about a named symbol or file, e.g. /explain `RunTurn`", Searched: []string{repoRoot}}
}

// SubjectOf extracts the probable subject of a question for follow-up
// context ("explain it" resolves to the last subject).
func SubjectOf(question string) string {
	q := strings.TrimSpace(question)
	if m := regexp.MustCompile("`([^`]+)`").FindStringSubmatch(q); m != nil {
		return strings.TrimSpace(m[1])
	}
	lowered := strings.ToLower(q)
	for _, verb := range []string{"explain ", "what is ", "what does ", "why does ", "how does ", "where is ", "show me "} {
		if strings.HasPrefix(lowered, verb) {
			return subjectToken(strings.TrimSpace(q[len(verb):]))
		}
	}
	subj := strings.Trim(q, "?.!\"'` ")
	if looksLikePath(subj) {
		return subj
	}
	return subjectToken(subj)
}

// stopwords are skipped when hunting the subject token.
var stopwords = map[string]bool{
	"the": true, "a": true, "an": true, "is": true, "does": true, "do": true,
	"did": true, "are": true, "was": true, "were": true, "it": true, "this": true,
	"that": true, "these": true, "those": true, "here": true, "there": true,
	"function": true, "method": true, "code": true, "symbol": true, "file": true,
	"mean": true, "means": true, "work": true, "works": true, "why": true, "how": true,
	"what": true, "where": true, "explain": true, "show": true, "tell": true,
	"me": true, "about": true, "of": true, "in": true, "on": true, "for": true, "to": true,
}

// subjectToken returns the first non-stopword code token: the probable
// subject of "what does RunTurn do?" is RunTurn, not "do".
func subjectToken(rest string) string {
	for _, tok := range regexp.MustCompile(`[A-Za-z0-9_./-]+`).FindAllString(rest, -1) {
		clean := strings.Trim(tok, "\"'`?.!()")
		if clean == "" || stopwords[strings.ToLower(clean)] {
			continue
		}
		return clean
	}
	return ""
}

func looksLikePath(s string) bool {
	ext := strings.ToLower(filepath.Ext(s))
	switch ext {
	case ".go", ".md", ".toml", ".json", ".js", ".ts", ".py", ".rs", ".sh", ".yaml", ".yml":
		return true
	}
	return false
}

func existingPath(root, p string) bool {
	abs := p
	if !filepath.IsAbs(p) {
		abs = filepath.Join(root, p)
	}
	_, err := os.Stat(abs)
	return err == nil
}

// Format renders an answer as cited text.
func Format(ans Answer) string {
	if ans.Refused {
		return "Cannot answer: " + ans.Reason
	}
	var b strings.Builder
	b.WriteString(ans.Summary)
	b.WriteString("\nCitations:")
	shown := 0
	for _, l := range ans.Locations {
		fmt.Fprintf(&b, "\n- %s:%d: %s", l.Path, l.Line, l.Text)
		shown++
		if shown >= 10 {
			fmt.Fprintf(&b, "\n- …(%d more, capped at 10)", len(ans.Locations)-shown)
			break
		}
	}
	return b.String()
}
