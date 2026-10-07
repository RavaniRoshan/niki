package skills

import (
	"os"
	"path/filepath"
	"strings"
)

// Skill is a discovered SKILL.md document.
type Skill struct {
	ID          string
	Path        string
	Name        string
	Description string
	Body        string // loaded lazily
	Loaded      bool
}

// Discover scans for SKILL.md files under roots (non-recursive-limited walk).
func Discover(roots ...string) ([]Skill, error) {
	var out []Skill
	for _, root := range roots {
		filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return nil
			}
			if d.IsDir() && (d.Name() == ".git" || d.Name() == "node_modules") {
				return filepath.SkipDir
			}
			if d.IsDir() {
				return nil
			}
			if strings.EqualFold(d.Name(), "SKILL.md") {
				s := parseSkill(path)
				out = append(out, s)
			}
			return nil
		})
	}
	return out, nil
}

func parseSkill(path string) Skill {
	s := Skill{Path: path, ID: filepath.Base(filepath.Dir(path))}
	data, err := os.ReadFile(path)
	if err != nil {
		return s
	}
	content := string(data)
	if strings.HasPrefix(content, "---") {
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
			}
			s.Body = strings.TrimSpace(parts[2])
		}
	}
	return s
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
