// Package recipes is NikiCode's routine-task library (G2): small,
// reviewable, permission-gated flows invoked by name or by natural
// language. Each recipe is Markdown with frontmatter (same style as
// skills); steps run through the real tool registry behind the same
// permission gate the engine uses.
package recipes

import (
	"context"
	"embed"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/RavaniRoshan/niki/internal/paths"
	"github.com/RavaniRoshan/niki/internal/permissions"
	"github.com/RavaniRoshan/niki/internal/tools"
)

//go:embed *.md
var embedded embed.FS

// Step is one tool call in a recipe.
type Step struct {
	Tool string
	Args json.RawMessage
	Raw  string
}

// Recipe is a parsed routine-task flow.
type Recipe struct {
	Name        string
	Description string
	Match       []string
	Steps       []Step
	Notes       string
	Path        string
}

// Result is the outcome of one executed step or a refusal.
type StepResult struct {
	Tool   string
	Output string
	IsErr  bool
}

// FileEffect records one file write for undo: the pre-image (Before,
// nil when the file is new), what was written (After), and the mode.
type FileEffect struct {
	Path         string
	Existed      bool
	Before, After []byte
	Mode         os.FileMode
}

// Report is a full recipe execution.
type Report struct {
	Recipe  string
	Steps   []StepResult
	Effects []FileEffect
	Refused bool
	Reason  string
}

// parseFile parses one recipe file.
func parseFile(path string, data []byte) (Recipe, error) {
	r := Recipe{Path: path}
	content := string(data)
	if !strings.HasPrefix(content, "---") {
		return r, fmt.Errorf("%s: missing frontmatter", path)
	}
	parts := strings.SplitN(content, "---", 3)
	if len(parts) != 3 {
		return r, fmt.Errorf("%s: bad frontmatter", path)
	}
	inSteps := false
	for _, line := range strings.Split(parts[1], "\n") {
		trimmed := strings.TrimSpace(line)
		switch {
		case strings.HasPrefix(trimmed, "name:"):
			r.Name = strings.TrimSpace(strings.TrimPrefix(trimmed, "name:"))
		case strings.HasPrefix(trimmed, "description:"):
			r.Description = strings.TrimSpace(strings.TrimPrefix(trimmed, "description:"))
		case strings.HasPrefix(trimmed, "match:"):
			for _, m := range strings.Split(strings.TrimPrefix(trimmed, "match:"), "|") {
				if m = strings.ToLower(strings.TrimSpace(m)); m != "" {
					r.Match = append(r.Match, m)
				}
			}
		case trimmed == "steps:":
			inSteps = true
		case inSteps && strings.HasPrefix(trimmed, "- "):
			rest := strings.TrimPrefix(trimmed, "- ")
			tool, argstr, _ := strings.Cut(rest, ":")
			tool = strings.TrimSpace(tool)
			argstr = strings.TrimSpace(argstr)
			if tool == "" || argstr == "" {
				return r, fmt.Errorf("%s: bad step %q", path, line)
			}
			r.Steps = append(r.Steps, Step{Tool: tool, Raw: argstr})
		case inSteps && trimmed != "":
			return r, fmt.Errorf("%s: bad step line %q", path, line)
		}
	}
	r.Notes = strings.TrimSpace(parts[2])
	if r.Name == "" {
		return r, fmt.Errorf("%s: missing name", path)
	}
	if len(r.Steps) == 0 {
		return r, fmt.Errorf("%s: no steps", path)
	}
	return r, nil
}

// Embedded returns the recipes shipped in the binary.
func Embedded() ([]Recipe, error) {
	byName := map[string]Recipe{}
	entries, err := embedded.ReadDir(".")
	if err != nil {
		return nil, err
	}
	for _, e := range entries {
		if filepath.Ext(e.Name()) != ".md" {
			continue
		}
		data, err := embedded.ReadFile(e.Name())
		if err != nil {
			return nil, err
		}
		r, err := parseFile("embedded:"+e.Name(), data)
		if err != nil {
			return nil, err
		}
		byName[r.Name] = r
	}
	var out []Recipe
	for _, r := range byName {
		out = append(out, r)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out, nil
}

// Discover loads recipes: embedded defaults, then user
// (~/.nikicode/recipes), then project (.nikicode/recipes); later
// sources override by name.
func Discover(projectDir string) ([]Recipe, error) {
	byName := map[string]Recipe{}
	add := func(path string, data []byte) error {
		r, err := parseFile(path, data)
		if err != nil {
			return err
		}
		byName[r.Name] = r
		return nil
	}
	entries, err := embedded.ReadDir(".")
	if err != nil {
		return nil, err
	}
	for _, e := range entries {
		if filepath.Ext(e.Name()) != ".md" {
			continue
		}
		data, err := embedded.ReadFile(e.Name())
		if err != nil {
			return nil, err
		}
		if err := add("embedded:"+e.Name(), data); err != nil {
			return nil, err
		}
	}
	dirs := []string{filepath.Join(paths.Dir(), "recipes")}
	if projectDir != "" {
		dirs = append(dirs, filepath.Join(projectDir, ".nikicode", "recipes"))
	}
	for _, dir := range dirs {
		matches, _ := filepath.Glob(filepath.Join(dir, "*.md"))
		for _, m := range matches {
			data, err := os.ReadFile(m)
			if err != nil {
				return nil, err
			}
			if err := add(m, data); err != nil {
				return nil, err
			}
		}
	}
	var out []Recipe
	for _, r := range byName {
		out = append(out, r)
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out, nil
}

// Match finds the recipe whose match phrase best fits the input:
// longest matching phrase wins. Matching is word-boundary aware, so
// "commits" does not match the "commit" phrase, and articles (the/a/an)
// are ignored, so "check the formatting" matches "check formatting".
// Empty when nothing matches.
func Match(all []Recipe, input string) (Recipe, bool) {
	norm := dropArticles(strings.ToLower(strings.TrimSpace(input)))
	var best Recipe
	bestLen := 0
	found := false
	for _, r := range all {
		for _, m := range r.Match {
			if len(m) > bestLen && containsWord(norm, m) {
				best, bestLen, found = r, len(m), true
			}
		}
	}
	return best, found
}

func isWordChar(c byte) bool {
	return c == '_' || 'a' <= c && c <= 'z' || '0' <= c && c <= '9'
}

// dropArticles removes standalone the/a/an tokens for matching.
func dropArticles(s string) string {
	var kept []string
	for _, w := range strings.Fields(s) {
		switch w {
		case "the", "a", "an":
		default:
			kept = append(kept, w)
		}
	}
	return strings.Join(kept, " ")
}

// containsWord reports whether phrase occurs with word boundaries.
func containsWord(s, phrase string) bool {
	for i := 0; i+len(phrase) <= len(s); i++ {
		if !strings.HasPrefix(s[i:], phrase) {
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

// Execute runs a recipe's steps in order through the registry. Every
// step passes the SAME permission gate the engine uses
// (guard.Allow); a denied or failed step stops the run with the
// reason. Vars substitute {{key}} placeholders (including {{dir}});
// unsubstituted placeholders refuse the run.
func Execute(ctx context.Context, reg *tools.Registry, guard *permissions.Guard, r Recipe, vars map[string]string) (Report, error) {
	rep := Report{Recipe: r.Name}
	dir := vars["dir"]
	if dir == "" {
		dir = "."
	}
	for i, s := range r.Steps {
		raw := s.Raw
		for k, v := range vars {
			raw = strings.ReplaceAll(raw, "{{"+k+"}}", v)
		}
		raw = strings.ReplaceAll(raw, "{{dir}}", dir)
		if strings.Contains(raw, "{{") {
			rep.Refused = true
			rep.Reason = fmt.Sprintf("step %d (%s) has unsubstituted variables: %s", i+1, s.Tool, raw)
			return rep, nil
		}
		if !guard.Allow(s.Tool) {
			rep.Refused = true
			rep.Reason = fmt.Sprintf("step %d (%s) denied by permission gate (mode %s)", i+1, s.Tool, guard.Mode)
			return rep, nil
		}
		args, err := stepArgs(s.Tool, raw)
		if err != nil {
			rep.Refused = true
			rep.Reason = fmt.Sprintf("step %d (%s): %v", i+1, s.Tool, err)
			return rep, nil
		}
		// Snapshot the pre-image so undo can restore or remove.
		effect := captureEffect(s.Tool, args)
		res, err := reg.Run(ctx, s.Tool, args)
		if err != nil {
			rep.Steps = append(rep.Steps, StepResult{Tool: s.Tool, Output: err.Error(), IsErr: true})
			rep.Refused = true
			rep.Reason = fmt.Sprintf("step %d (%s) errored: %v", i+1, s.Tool, err)
			return rep, nil
		}
		rep.Steps = append(rep.Steps, StepResult{Tool: s.Tool, Output: res.Output, IsErr: res.IsError})
		if res.IsError {
			rep.Refused = true
			rep.Reason = fmt.Sprintf("step %d (%s) failed: %s", i+1, s.Tool, res.Output)
			return rep, nil
		}
		if effect != nil {
			if after, err := os.ReadFile(effect.Path); err == nil {
				effect.After = after
				rep.Effects = append(rep.Effects, *effect)
			}
		}
	}
	return rep, nil
}

// captureEffect snapshots a file before a write/edit step. Nil when the
// step does not write a known path.
func captureEffect(tool string, args json.RawMessage) *FileEffect {
	if tool != "write_file" && tool != "edit_file" {
		return nil
	}
	var a struct {
		Path string `json:"path"`
	}
	if err := json.Unmarshal(args, &a); err != nil || a.Path == "" {
		return nil
	}
	e := &FileEffect{Path: a.Path}
	if st, err := os.Stat(a.Path); err == nil {
		e.Existed = true
		e.Mode = st.Mode()
		if data, err := os.ReadFile(a.Path); err == nil {
			e.Before = data
		}
	}
	return e
}

// stepArgs decodes a step's args: JSON objects pass through; bare
// strings are sugar for the single-main-argument tools.
func stepArgs(tool, raw string) (json.RawMessage, error) {
	if strings.HasPrefix(raw, "{") {
		var v map[string]any
		if err := json.Unmarshal([]byte(raw), &v); err != nil {
			return nil, fmt.Errorf("bad JSON args: %w", err)
		}
		return json.RawMessage(raw), nil
	}
	var key string
	switch tool {
	case "shell":
		key = "command"
	case "read_file":
		key = "path"
	case "glob":
		key = "pattern"
	case "grep":
		key = "pattern"
	default:
		return nil, fmt.Errorf("tool %s needs JSON object args, got a bare string", tool)
	}
	enc, _ := json.Marshal(map[string]string{key: raw})
	return json.RawMessage(enc), nil
}
