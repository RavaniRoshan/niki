// Package intent routes plain-language instructions to NikiCode
// capabilities (G2): recipes, explain, and git workflows. Matching is
// deterministic phrase/verb routing — no model, no spend, fully tested.
// G3 adds conversation context, corrections, and multi-step on top.
package intent

import (
	"regexp"
	"strings"
	"sync"

	"github.com/RavaniRoshan/niki/internal/recipes"
)

// Kind is what the input asks for.
type Kind int

const (
	Unknown Kind = iota
	Recipe
	Explain
	Git
)

// GitOp names the git workflow.
type GitOp string

const (
	GitStatus    GitOp = "status"
	GitCommit    GitOp = "commit"
	GitBranch    GitOp = "branch"
	GitRebase    GitOp = "rebase"
	GitBlame     GitOp = "blame"
	GitLog       GitOp = "log"
	GitReview    GitOp = "review"
	GitChangelog GitOp = "changelog"
	GitPRDraft   GitOp = "prdraft"
)

// Action is one routed instruction.
type Action struct {
	Kind   Kind
	Recipe recipes.Recipe
	// Text carries the explain question or the raw input for git parsing.
	Text string
	Op   GitOp
	// Vars carry key=value pairs from the input (e.g. name=worker).
	Vars map[string]string
}

// VarsFromInput extracts key=value tokens (values without spaces).
// Keys keep their case: recipes use {{name}} and {{Name}} distinctly.
func VarsFromInput(input string) map[string]string {
	vars := map[string]string{}
	for _, tok := range strings.Fields(input) {
		if k, v, ok := strings.Cut(tok, "="); ok && k != "" && v != "" && !strings.ContainsAny(k, "\"'`") {
			vars[k] = v
		}
	}
	return vars
}

var backticked = sync.OnceValue(func() *regexp.Regexp {
	return regexp.MustCompile("`([^`]+)`")
})

// Route maps input to an action. Recipes win on phrase match; explain
// verbs and git verbs dispatch the other two promises.
func Route(all []recipes.Recipe, input string) Action {
	norm := strings.ToLower(strings.TrimSpace(input))
	vars := VarsFromInput(input)
	if r, ok := recipes.Match(all, input); ok {
		return Action{Kind: Recipe, Recipe: r, Text: input, Vars: vars}
	}
	if op, ok := matchGit(norm); ok {
		return Action{Kind: Git, Op: op, Text: input, Vars: vars}
	}
	if matchExplain(norm) {
		return Action{Kind: Explain, Text: input, Vars: vars}
	}
	return Action{Kind: Unknown, Text: input, Vars: vars}
}

func matchExplain(norm string) bool {
	if backticked().MatchString(norm) {
		return true
	}
	for _, v := range []string{"explain", "what is", "what does", "what are", "why does", "why is", "how does", "where is", "show me", "what's"} {
		if strings.Contains(norm, v) {
			return true
		}
	}
	return false
}

func matchGit(norm string) (GitOp, bool) {
	switch {
	case containsAny(norm, "rebase"):
		return GitRebase, true
	case containsAny(norm, "merge conflict", "conflict"):
		return GitRebase, true
	case containsAny(norm, "changelog"):
		return GitChangelog, true
	case containsAny(norm, "blame"):
		return GitBlame, true
	case containsAny(norm, "review", "staged diff", "look at my changes"):
		return GitReview, true
	case containsAny(norm, "pr draft", "draft.*pr", "pull request"):
		return GitPRDraft, true
	case containsAny(norm, "commit"):
		return GitCommit, true
	case containsAny(norm, "branch", "switch to"):
		return GitBranch, true
	case containsAny(norm, "log", "history", "recent commits"):
		return GitLog, true
	case containsAny(norm, "status", "what changed", "working tree"):
		return GitStatus, true
	}
	return "", false
}

// SplitSteps splits "do X then Y" into ordered steps. Separators:
// " then ", " and then ", ";", and " after that ". A separator inside
// backticks or matched quotes does not split.
func SplitSteps(input string) []string {
	var parts []string
	var cur strings.Builder
	runes := []rune(input)
	i := 0
	lowered := strings.ToLower(input)
	for i < len(runes) {
		rest := lowered[i:]
		sep := ""
		for _, s := range []string{" and then ", " then ", ";", " after that "} {
			if strings.HasPrefix(rest, s) {
				sep = s
				break
			}
		}
		if sep != "" {
			if p := strings.TrimSpace(cur.String()); p != "" {
				parts = append(parts, p)
			}
			cur.Reset()
			i += len([]rune(sep))
			continue
		}
		cur.WriteRune(runes[i])
		i++
	}
	if p := strings.TrimSpace(cur.String()); p != "" {
		parts = append(parts, p)
	}
	return parts
}

// correctionPrefixes mark a follow-up that re-routes the last action.
var correctionPrefixes = []string{
	"no,", "no ", "actually,", "actually ", "i meant", "rather,",
	"rather ", "instead,", "instead ", "correction:", "wrong,",
}

// IsCorrection reports whether the input corrects the previous turn.
func IsCorrection(input string) bool {
	norm := strings.ToLower(strings.TrimSpace(input))
	for _, p := range correctionPrefixes {
		if strings.HasPrefix(norm, p) {
			return true
		}
	}
	return false
}

// StripCorrection removes the correction prefix, leaving the new direction.
func StripCorrection(input string) string {
	norm := strings.ToLower(strings.TrimSpace(input))
	for _, p := range correctionPrefixes {
		if strings.HasPrefix(norm, p) {
			return strings.TrimSpace(input[len(p):])
		}
	}
	return strings.TrimSpace(input)
}

// SubstituteIt replaces a standalone "it" with the last subject
// ("explain it" after asking about RunTurn means RunTurn).
func SubstituteIt(input, subject string) string {
	if subject == "" {
		return input
	}
	words := strings.Fields(input)
	changed := false
	for i, w := range words {
		core := strings.Trim(w, "?.!,;:'\"")
		if strings.EqualFold(core, "it") {
			words[i] = strings.Replace(w, core, subject, 1)
			changed = true
		}
	}
	if !changed {
		return input
	}
	return strings.Join(words, " ")
}

func containsAny(s string, subs ...string) bool {
	for _, sub := range subs {
		if containsWord(s, sub) {
			return true
		}
	}
	return false
}

func isWordChar(c byte) bool {
	return c == '_' || 'a' <= c && c <= 'z' || '0' <= c && c <= '9'
}

// containsWord reports whether phrase occurs with word boundaries, so
// "commits" never matches the "commit" verb.
func containsWord(s, phrase string) bool {
	for i := 0; i+len(phrase) <= len(s); i++ {
		if s[i:i+len(phrase)] != phrase {
			continue
		}
		if i > 0 && isWordChar(s[i-1]) {
			continue
		}
		if j := i + len(phrase); j < len(s) && isWordChar(s[j]) {
			continue
		}
		return true
	}
	return false
}
