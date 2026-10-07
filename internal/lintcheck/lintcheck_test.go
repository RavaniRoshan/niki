package lintcheck

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// repoRoot walks up to find go.mod.
func repoRoot(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatal("go.mod not found")
		}
		dir = parent
	}
}

func readTree(t *testing.T, root, suffix string) map[string]string {
	t.Helper()
	out := map[string]string{}
	filepath.WalkDir(root, func(path string, d os.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			return nil
		}
		if strings.HasSuffix(path, suffix) {
			b, _ := os.ReadFile(path)
			out[path] = string(b)
		}
		return nil
	})
	return out
}

var forbiddenInTUI = []*regexp.Regexp{
	regexp.MustCompile(`os\.Stdout`),
	regexp.MustCompile(`os\.Stderr`),
	regexp.MustCompile(`fmt\.Print`),
	regexp.MustCompile(`http\.`),
	regexp.MustCompile(`exec\.Command`),
	regexp.MustCompile(`os\.Open`),
	regexp.MustCompile(`os\.ReadFile`),
	regexp.MustCompile(`net\.Dial`),
}

// TestTUIUpdateHasNoIO asserts the render path performs no I/O (B8, L5).
func TestTUIUpdateHasNoIO(t *testing.T) {
	root := repoRoot(t)
	for path, src := range readTree(t, filepath.Join(root, "internal", "tui"), ".go") {
		if strings.HasSuffix(path, "_test.go") {
			continue
		}
		for _, re := range forbiddenInTUI {
			if re.MatchString(src) {
				t.Errorf("%s matches forbidden pattern %s (no I/O on render path)", path, re)
			}
		}
	}
}

var colorLiteral = regexp.MustCompile(`lipgloss\.Color\("[0-9A-Fa-fx#]+"\)`)

// TestNoColorLiteralsOutsideTheme (U7).
func TestNoColorLiteralsOutsideTheme(t *testing.T) {
	root := repoRoot(t)
	for path, src := range readTree(t, filepath.Join(root, "internal"), ".go") {
		if strings.HasSuffix(path, "_test.go") || strings.HasSuffix(path, "theme.go") {
			continue
		}
		if colorLiteral.MatchString(src) {
			t.Errorf("%s contains a color literal outside theme.go", path)
		}
	}
}
