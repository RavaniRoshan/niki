package skills

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/RavaniRoshan/niki/internal/paths"
)

// Skill is a discovered SKILL.md document.
type Skill struct {
	ID          string
	Path        string
	Name        string
	Description string
	Context     string // "fork" | "inline"
	Body        string // loaded lazily
	Loaded      bool
}

// StandardRoots returns standard and compat roots for skill discovery:
// ~/.nikicode/skills, then the legacy ~/.niki/skills, then project
// .nikicode/skills, .niki/skills, skills, and .agents/skills.
func StandardRoots(projectDir string) []string {
	var roots []string
	roots = append(roots, filepath.Join(paths.Dir(), "skills"))
	if home := paths.Home(); home != "" {
		roots = append(roots, filepath.Join(paths.LegacyDir(), "skills"))
	}
	if projectDir != "" {
		roots = append(roots,
			filepath.Join(projectDir, ".nikicode", "skills"),
			filepath.Join(projectDir, ".niki", "skills"),
			filepath.Join(projectDir, "skills"),
			filepath.Join(projectDir, ".agents", "skills"),
		)
	}
	return roots
}

var (
	indexMu     sync.RWMutex
	cachedRoots = make(map[string]time.Time)
	cachedIndex []Skill
)

// CachedDiscover returns skills from cache if directory mtimes are unchanged,
// loading frontmatter only.
func CachedDiscover(roots ...string) ([]Skill, error) {
	indexMu.Lock()
	defer indexMu.Unlock()

	changed := false
	for _, root := range roots {
		fi, err := os.Stat(root)
		if err != nil {
			continue
		}
		prev, ok := cachedRoots[root]
		if !ok || !prev.Equal(fi.ModTime()) {
			changed = true
			cachedRoots[root] = fi.ModTime()
		}
	}

	if !changed && len(cachedIndex) > 0 {
		return cachedIndex, nil
	}

	skills, err := Discover(roots...)
	if err == nil {
		cachedIndex = skills
	}
	return skills, err
}

// InvalidateCache clears the cached roots and skill index to force a hot reload.
func InvalidateCache() {
	indexMu.Lock()
	defer indexMu.Unlock()
	cachedRoots = make(map[string]time.Time)
	cachedIndex = nil
}

// Discover scans for SKILL.md or *.md files under roots (frontmatter only).
func Discover(roots ...string) ([]Skill, error) {
	var out []Skill
	for _, root := range roots {
		_ = filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return nil
			}
			if d.IsDir() && (d.Name() == ".git" || d.Name() == "node_modules") {
				return filepath.SkipDir
			}
			if d.IsDir() {
				return nil
			}
			if strings.EqualFold(d.Name(), "SKILL.md") || strings.HasSuffix(strings.ToLower(d.Name()), ".md") {
				s, err := parseSkill(path)
				if err == nil {
					out = append(out, s)
				}
			}
			return nil
		})
	}
	return out, nil
}

func parseSkill(path string) (Skill, error) {
	s := Skill{Path: path, ID: filepath.Base(filepath.Dir(path))}
	data, err := os.ReadFile(path)
	if err != nil {
		return s, err
	}
	content := string(data)
	if !strings.HasPrefix(content, "---") {
		return s, fmt.Errorf("missing frontmatter")
	}
	{
		parts := strings.SplitN(content, "---", 3)
		if len(parts) == 3 {
			fm := parts[1]
			for _, line := range strings.Split(fm, "\n") {
				if strings.HasPrefix(line, "name:") {
					s.Name = strings.TrimSpace(strings.TrimPrefix(line, "name:"))
				}
				if strings.HasPrefix(line, "description:") {
					s.Description = strings.TrimSpace(strings.TrimPrefix(line, "description:"))
				}
				if strings.HasPrefix(line, "context:") {
					s.Context = strings.TrimSpace(strings.TrimPrefix(line, "context:"))
				}
			}
			// T4: discovery parses frontmatter only. The body is NOT
			// read here — it loads lazily on the first LoadBody (when
			// the skill is actually invoked), so listing skills never
			// touches every file's contents.
		} else {
			return s, fmt.Errorf("bad frontmatter")
		}
	}
	if s.Name == "" {
		return s, fmt.Errorf("missing name")
	}
	return s, nil
}

// LoadBody lazily reads the body.
func (s *Skill) LoadBody() error {
	if s.Loaded {
		return nil
	}
	data, err := os.ReadFile(s.Path)
	if err != nil {
		return err
	}
	if strings.HasPrefix(string(data), "---") {
		parts := strings.SplitN(string(data), "---", 3)
		if len(parts) == 3 {
			s.Body = strings.TrimSpace(parts[2])
		}
	} else {
		s.Body = string(data)
	}
	s.Loaded = true
	return nil
}

// Instructions returns the AGENTS.md / NIKICODE.md hierarchy found walking
// upward from dir (NIKI.md is the legacy spelling, still honored).
func Instructions(dir string) []string {
	var out []string
	for {
		for _, name := range []string{"AGENTS.md", "NIKICODE.md", "NIKI.md"} {
			p := filepath.Join(dir, name)
			if _, err := os.Stat(p); err == nil {
				out = append(out, p)
			}
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	return out
}

// Instruction is a loaded instruction file.
type Instruction struct {
	Path    string
	Content string
}

// InstructionsBounded loads instruction files walking upward, bounding each file's content (C2).
func InstructionsBounded(dir string, maxBytes int) []Instruction {
	var out []Instruction
	for {
		for _, name := range []string{"AGENTS.md", "NIKICODE.md", "NIKI.md"} {
			p := filepath.Join(dir, name)
			if data, err := os.ReadFile(p); err == nil {
				content := string(data)
				if maxBytes > 0 && len(content) > maxBytes {
					content = content[:maxBytes]
				}
				out = append(out, Instruction{Path: p, Content: content})
			}
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	return out
}

// InstructionsRootToCwd returns the AGENTS.md instruction chain ordered from repo root down to cwd.
func InstructionsRootToCwd(dir string, maxBytes int) []Instruction {
	upward := InstructionsBounded(dir, maxBytes)
	// Reverse upward slice [cwd...root] to root-to-cwd [root...cwd]
	n := len(upward)
	out := make([]Instruction, n)
	for i, inst := range upward {
		out[n-1-i] = inst
	}
	return out
}

// InstructionSet is the resolved instruction hierarchy for one
// working directory: global (user config dir), then repository
// root down to the directory itself (C1). Paths are resolved up
// front; file contents load lazily on the first Load call, so
// merely resolving instructions never touches the disk beyond
// cheap stats.
type InstructionSet struct {
	Paths    []string
	Loaded   bool
	contents []Instruction
	maxBytes int
}

// ResolveInstructionSet builds the lazy hierarchy for dir: one
// global file set from configDir, then InstructionsRootToCwd paths
// (root first, the file's own directory last).
func ResolveInstructionSet(dir, configDir string, maxBytes int) *InstructionSet {
	set := &InstructionSet{maxBytes: maxBytes}
	seen := map[string]bool{}
	add := func(p string) {
		if seen[p] || p == "" {
			return
		}
		seen[p] = true
		if fi, err := os.Stat(p); err == nil && !fi.IsDir() {
			set.Paths = append(set.Paths, p)
		}
	}
	for _, name := range []string{"AGENTS.md", "NIKICODE.md", "NIKI.md"} {
		add(filepath.Join(configDir, name))
	}
	// Instructions walks upward (file's directory first); reverse it
	// so the root comes first. Paths only — no file is read here.
	upward := Instructions(dir)
	for i := len(upward) - 1; i >= 0; i-- {
		add(upward[i])
	}
	return set
}

// Load reads every resolved file (skipping files that vanished
// since resolution — proof the read happens here, not at resolve
// time) and bounds each to maxBytes. Later calls are no-ops.
func (s *InstructionSet) Load() []Instruction {
	if s.Loaded {
		return s.contents
	}
	s.Loaded = true
	for _, p := range s.Paths {
		data, err := os.ReadFile(p)
		if err != nil {
			continue
		}
		content := string(data)
		if s.maxBytes > 0 && len(content) > s.maxBytes {
			content = content[:s.maxBytes]
		}
		s.contents = append(s.contents, Instruction{Path: p, Content: content})
	}
	return s.contents
}
