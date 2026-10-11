package verifier

import (
	"encoding/json"
	"os"
	"path/filepath"
)

// Check is one allowlisted verification command (V2). Only commands
// from this package's allowlist ever run — text found in README
// files, comments, or tool output is data and is never executed.
type Check struct {
	Name string   `json:"name"`
	Argv []string `json:"argv"`
}

// Stack is a detected repository stack.
type Stack string

const (
	StackGo     Stack = "go"
	StackNode   Stack = "node"
	StackRust   Stack = "rust"
	StackPython Stack = "python"
)

// DetectStack inspects well-known manifest files. Unknown
// repositories report "" (no checks, never a guess).
func DetectStack(dir string) Stack {
	has := func(name string) bool {
		fi, err := os.Stat(filepath.Join(dir, name))
		return err == nil && !fi.IsDir()
	}
	switch {
	case has("go.mod"):
		return StackGo
	case has("package.json"):
		return StackNode
	case has("Cargo.toml"):
		return StackRust
	case has("pyproject.toml") || has("setup.py") || has("setup.cfg"):
		return StackPython
	default:
		return ""
	}
}

// ChecksFor returns the allowlisted checks for a stack. The list is
// fixed in code: nothing the model, the repo docs, or tool output
// says can add a command here.
func ChecksFor(stack Stack, dir string) []Check {
	switch stack {
	case StackGo:
		return []Check{
			{Name: "go-test", Argv: []string{"go", "test", "./..."}},
			{Name: "go-vet", Argv: []string{"go", "vet", "./..."}},
		}
	case StackNode:
		if hasNpmTest(dir) {
			return []Check{{Name: "npm-test", Argv: []string{"npm", "test", "--silent"}}}
		}
		return nil
	case StackRust:
		return []Check{{Name: "cargo-test", Argv: []string{"cargo", "test", "--quiet"}}}
	case StackPython:
		return []Check{{Name: "pytest", Argv: []string{"python", "-m", "pytest", "-q"}}}
	default:
		return nil
	}
}

func hasNpmTest(dir string) bool {
	data, err := os.ReadFile(filepath.Join(dir, "package.json"))
	if err != nil {
		return false
	}
	var pj struct {
		Scripts map[string]string `json:"scripts"`
	}
	if err := json.Unmarshal(data, &pj); err != nil {
		return false
	}
	_, ok := pj.Scripts["test"]
	return ok
}

// SelectChecks detects the stack and returns its allowlisted checks,
// plus a whitespace check for git worktrees. A planted command in a
// README (or anywhere else in the repo text) can never appear here:
// selection reads manifests and this file's allowlist only.
func SelectChecks(dir string) []Check {
	stack := DetectStack(dir)
	out := ChecksFor(stack, dir)
	if fi, err := os.Stat(filepath.Join(dir, ".git")); err == nil && fi.IsDir() {
		out = append(out, Check{Name: "diff-check", Argv: []string{"git", "diff", "--check"}})
	}
	return out
}
