package skills

import (
	"os"
	"path/filepath"
	"testing"
)

// FuzzParseSkill ensures hostile SKILL.md never wedges parsing (L6-adjacent).
func FuzzParseSkill(f *testing.F) {
	f.Add("---\nname: x\n---\nbody\n")
	f.Add("no frontmatter")
	f.Add("---\n---\n")
	f.Add(string(make([]byte, 1024)))
	f.Fuzz(func(t *testing.T, content string) {
		dir := t.TempDir()
		p := filepath.Join(dir, "SKILL.md")
		os.WriteFile(p, []byte(content), 0o644)
		_, _ = parseSkill(p)
	})
}
