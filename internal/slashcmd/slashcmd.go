// Package slashcmd loads custom slash commands from Markdown files
// (T7). A command is a Markdown file whose frontmatter names it and
// whose body is a prompt template; `$ARGUMENTS`, `$1`, `$2`, … are
// substituted from the invocation. Commands are discovered from user
// and project command directories and never overwrite built-ins.
package slashcmd

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// Command is one custom slash command loaded from Markdown.
type Command struct {
	Name        string // e.g. "review" (invoked as /review)
	Description string
	Template    string // body with $ARGUMENTS / $1.. placeholders
	Path        string
}

// StandardDirs returns the command directories to scan: the user
// config dir's commands folder, then the project's .nikicode/commands
// (T7: user and project locations).
func StandardDirs(projectDir string) []string {
	var dirs []string
	if home := os.Getenv("HOME"); home != "" {
		dirs = append(dirs, filepath.Join(home, ".nikicode", "commands"))
	}
	if projectDir != "" {
		dirs = append(dirs,
			filepath.Join(projectDir, ".nikicode", "commands"),
			filepath.Join(projectDir, ".niki", "commands"),
		)
	}
	return dirs
}

// Discover scans the given dirs for *.md command files, parsing
// frontmatter (name/description) and the body template.
func Discover(dirs ...string) ([]Command, error) {
	seen := map[string]bool{}
	var out []Command
	for _, dir := range dirs {
		entries, err := os.ReadDir(dir)
		if err != nil {
			continue
		}
		for _, e := range entries {
			if e.IsDir() || !strings.HasSuffix(strings.ToLower(e.Name()), ".md") {
				continue
			}
			path := filepath.Join(dir, e.Name())
			c, err := parse(path)
			if err != nil {
				continue
			}
			if seen[c.Name] {
				continue // first location wins (user before project)
			}
			seen[c.Name] = true
			out = append(out, c)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Name < out[j].Name })
	return out, nil
}

func parse(path string) (Command, error) {
	c := Command{Path: path, Name: strings.TrimSuffix(filepath.Base(path), ".md")}
	data, err := os.ReadFile(path)
	if err != nil {
		return c, err
	}
	content := string(data)
	if strings.HasPrefix(content, "---") {
		parts := strings.SplitN(content, "---", 3)
		if len(parts) == 3 {
			for _, line := range strings.Split(parts[1], "\n") {
				if strings.HasPrefix(line, "name:") {
					c.Name = strings.TrimSpace(strings.TrimPrefix(line, "name:"))
				}
				if strings.HasPrefix(line, "description:") {
					c.Description = strings.TrimSpace(strings.TrimPrefix(line, "description:"))
				}
			}
			c.Template = strings.TrimSpace(parts[2])
		} else {
			c.Template = strings.TrimSpace(content)
		}
	} else {
		c.Template = strings.TrimSpace(content)
	}
	return c, nil
}

// Render substitutes arguments into a command template (T7):
//   - $ARGUMENTS (or $@) -> the full argument string
//   - $1, $2, …          -> the nth whitespace-split argument
//   - ${1}, ${2}, …      -> same, braced form
//
// Unknown positional placeholders render empty. Substitution is a
// single left-to-right pass so a substituted value containing "$1"
// is not re-expanded.
func (c Command) Render(args string) string {
	fields := strings.Fields(args)
	replace := func(token string) (string, bool) {
		switch token {
		case "$ARGUMENTS", "$@", "${ARGUMENTS}", "${@}":
			return args, true
		}
		// Positional: $1 / ${1}
		num := token
		num = strings.TrimPrefix(num, "$")
		num = strings.TrimPrefix(num, "{")
		num = strings.TrimSuffix(num, "}")
		if n, err := parseInt(num); err == nil {
			if n >= 1 && n <= len(fields) {
				return fields[n-1], true
			}
			return "", true
		}
		return "", false
	}
	return substitute(c.Template, replace)
}

// substitute scans s for $TOKEN placeholders (TOKEN = word chars or a
// {…} group) and replaces each via replace; a false return leaves the
// token verbatim. The scan never re-scans replaced text.
func substitute(s string, replace func(string) (string, bool)) string {
	var b strings.Builder
	for i := 0; i < len(s); {
		if s[i] != '$' {
			b.WriteByte(s[i])
			i++
			continue
		}
		// Read the token.
		j := i + 1
		if j < len(s) && s[j] == '{' {
			k := strings.IndexByte(s[j:], '}')
			if k >= 0 {
				token := s[i : j+k+1]
				if v, ok := replace(token); ok {
					b.WriteString(v)
					i = j + k + 1
					continue
				}
			}
		}
		k := j
		for k < len(s) && isWordChar(s[k]) {
			k++
		}
		if k > j {
			token := s[i:k]
			if v, ok := replace(token); ok {
				b.WriteString(v)
				i = k
				continue
			}
		}
		b.WriteByte(s[i])
		i++
	}
	return b.String()
}

func isWordChar(b byte) bool {
	return b == '_' || (b >= 'a' && b <= 'z') || (b >= 'A' && b <= 'Z') || (b >= '0' && b <= '9') || b == '@'
}

func parseInt(s string) (int, error) {
	if s == "" {
		return 0, fmt.Errorf("empty")
	}
	n := 0
	for _, r := range s {
		if r < '0' || r > '9' {
			return 0, fmt.Errorf("not a number")
		}
		n = n*10 + int(r-'0')
	}
	return n, nil
}
