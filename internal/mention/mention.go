// Package mention resolves @-file mentions through a fuzzy picker
// (G3): "@main.go" in a natural-language instruction finds the real
// path, ranked, with an explicit refusal when nothing matches.
package mention

import (
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

var mentionPattern = regexp.MustCompile(`@([A-Za-z0-9_./-]+)`)

// Extract returns @-mentions in order, deduplicated.
func Extract(input string) []string {
	var out []string
	seen := map[string]bool{}
	for _, m := range mentionPattern.FindAllStringSubmatch(input, -1) {
		name := strings.Trim(m[1], "./")
		if name == "" || seen[name] {
			continue
		}
		seen[name] = true
		out = append(out, name)
	}
	return out
}

// Candidate is a ranked file match.
type Candidate struct {
	Path  string
	Score int
}

// Resolve fuzzy-matches one mention against files under root (skipping
// .git and the state dir). Exact basename match wins; otherwise the
// best subsequence score wins. Refuses with the searched scope when
// nothing matches.
func Resolve(root, mention string) (Candidate, error) {
	cands, err := Picker(root, mention)
	if err != nil {
		return Candidate{}, err
	}
	if len(cands) == 0 {
		return Candidate{}, &NoMatchError{Mention: mention, Root: root}
	}
	return cands[0], nil
}

// NoMatchError reports an unresolvable mention.
type NoMatchError struct {
	Mention, Root string
}

func (e *NoMatchError) Error() string {
	return "no file matches @" + e.Mention + " under " + e.Root
}

// Picker lists all ranked candidates for a mention (the fuzzy picker).
func Picker(root, mention string) ([]Candidate, error) {
	var files []string
	err := filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			switch d.Name() {
			case ".git", "node_modules", ".nikicode", ".niki", "bin":
				return filepath.SkipDir
			}
			return nil
		}
		if rel, err := filepath.Rel(root, path); err == nil {
			files = append(files, rel)
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	norm := strings.ToLower(mention)
	var cands []Candidate
	for _, f := range files {
		low := strings.ToLower(f)
		base := strings.ToLower(filepath.Base(f))
		switch {
		case base == norm || low == norm:
			cands = append(cands, Candidate{Path: f, Score: 1000})
		case base == norm+extGuess(norm, base):
			cands = append(cands, Candidate{Path: f, Score: 900})
		case strings.HasSuffix(low, "/"+norm):
			cands = append(cands, Candidate{Path: f, Score: 800})
		default:
			if s, ok := subseqScore(base, norm); ok {
				cands = append(cands, Candidate{Path: f, Score: s})
			} else if s, ok := subseqScore(strings.ReplaceAll(low, "/", ""), norm); ok {
				cands = append(cands, Candidate{Path: f, Score: s - 50})
			}
		}
	}
	sort.Slice(cands, func(i, j int) bool {
		if cands[i].Score == cands[j].Score {
			return cands[i].Path < cands[j].Path
		}
		return cands[i].Score > cands[j].Score
	})
	if len(cands) > 8 {
		cands = cands[:8]
	}
	return cands, nil
}

func extGuess(mention, base string) string {
	if !strings.Contains(base, ".") || strings.Contains(mention, ".") {
		return ""
	}
	return base[strings.Index(base, "."):]
}

// subseqScore scores a subsequence match: longer and earlier wins.
func subseqScore(hay, needle string) (int, bool) {
	pos := 0
	matched := 0
	first := -1
	for _, r := range needle {
		idx := strings.IndexRune(hay[pos:], r)
		if idx < 0 {
			return 0, false
		}
		if first < 0 {
			first = pos + idx
		}
		pos += idx + 1
		matched++
	}
	return matched*10 - first, true
}
