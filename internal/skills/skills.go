package skills

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
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

// StandardRoots returns standard and compat roots for skill discovery,
// including ~/.niki/skills, <project>/.niki/skills, and .agents/skills.
func StandardRoots(projectDir string) []string {
	var roots []string
	if home, err := os.UserHomeDir(); err == nil {
		roots = append(roots, filepath.Join(home, ".niki", "skills"))
	}
	if projectDir != "" {
		roots = append(roots,
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
			s.Body = strings.TrimSpace(parts[2])
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

// Instructions returns the AGENTS.md / NIKI.md hierarchy found walking upward from dir.
func Instructions(dir string) []string {
	var out []string
	for {
		for _, name := range []string{"AGENTS.md", "NIKI.md"} {
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
		for _, name := range []string{"AGENTS.md", "NIKI.md"} {
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
