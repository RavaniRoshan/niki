package selfextend

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// Declared is the permission frontmatter of a skill (T5.4).
type Declared struct {
	Tools   []string `json:"tools"`
	Paths   []string `json:"paths"`
	Network string   `json:"network"` // "none" (default) or host allowlist entry
	Secrets string   `json:"secrets"` // "none" (default) or names
}

// ParseFrontmatter extracts the --- fenced frontmatter of SKILL.md.
// Missing keys default closed (no tools beyond the sandbox, no
// network, no secrets).
func ParseFrontmatter(skillMD string) (Declared, error) {
	d := Declared{Network: "none", Secrets: "none"}
	rest := skillMD
	if strings.HasPrefix(rest, "---") {
		parts := strings.SplitN(rest, "---", 3)
		if len(parts) < 3 {
			return d, fmt.Errorf("unterminated frontmatter")
		}
		for _, ln := range strings.Split(parts[1], "\n") {
			ln = strings.TrimSpace(ln)
			if ln == "" || strings.HasPrefix(ln, "#") {
				continue
			}
			kv := strings.SplitN(ln, ":", 2)
			if len(kv) != 2 {
				continue
			}
			k, v := strings.TrimSpace(kv[0]), strings.TrimSpace(kv[1])
			switch k {
			case "tools":
				d.Tools = splitList(v)
			case "paths":
				d.Paths = splitList(v)
			case "network":
				d.Network = v
			case "secrets":
				d.Secrets = v
			}
		}
	}
	return d, nil
}

func splitList(v string) []string {
	v = strings.Trim(v, "[]")
	var out []string
	for _, p := range strings.Split(v, ",") {
		if p = strings.TrimSpace(strings.Trim(p, `"'`)); p != "" {
			out = append(out, p)
		}
	}
	return out
}

// LoadDeclared reads and parses a staged skill's frontmatter.
func LoadDeclared(stagingDir string) (Declared, error) {
	data, err := os.ReadFile(filepath.Join(stagingDir, "SKILL.md"))
	if err != nil {
		return Declared{}, err
	}
	return ParseFrontmatter(string(data))
}

// NeedsSecondApproval reports whether the declaration needs the
// second, explicit approval at gate time (T5.6): any network, any
// write outside the workspace, or any secret.
func (d Declared) NeedsSecondApproval() bool {
	if d.Network != "" && d.Network != "none" {
		return true
	}
	if d.Secrets != "" && d.Secrets != "none" {
		return true
	}
	for _, p := range d.Paths {
		if p != "" && p != "." && !strings.HasPrefix(p, "./") && !strings.HasPrefix(p, "/") {
			return true
		}
		if strings.HasPrefix(p, "/") && !strings.HasPrefix(p, "/tmp/") {
			return true
		}
	}
	return false
}
