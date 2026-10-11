package permissions

import (
	"strings"

	"mvdan.cc/sh/v3/syntax"
)

// ShellVerdict is the fail-closed classification of a shell command.
type ShellVerdict string

const (
	ShellSimple      ShellVerdict = "simple"
	ShellTooComplex  ShellVerdict = "too-complex"
	ShellUnparseable ShellVerdict = "parse-unavailable"
)

// AnalyzeShell parses cmd and returns a fail-closed verdict. Anything beyond a
// single simple command (pipes, subshells, command substitution, control
// flow, coprocs, functions, etc.) forces a prompt.
func AnalyzeShell(cmd string) ShellVerdict {
	parser := syntax.NewParser()
	f, err := parser.Parse(strings.NewReader(cmd), "")
	if err != nil {
		return ShellUnparseable
	}
	tooComplex := false
	syntax.Walk(f, func(n syntax.Node) bool {
		if n == nil {
			return false
		}
		// A backgrounded statement (cmd &) runs detached — treat as
		// too complex (approval required).
		if stmt, ok := n.(*syntax.Stmt); ok && stmt.Background {
			tooComplex = true
			return false
		}
		switch n.(type) {
		case *syntax.File, *syntax.Stmt, *syntax.CallExpr, *syntax.Word,
			*syntax.Lit, *syntax.SglQuoted, *syntax.DblQuoted, *syntax.Redirect,
			*syntax.Assign, *syntax.ParamExp:
			return true
		default:
			tooComplex = true
			return false
		}
	})
	if tooComplex {
		return ShellTooComplex
	}
	return ShellSimple
}

// IsPushCommand reports whether cmd invokes `git push` in any form
// (G3: no push at all from autonomous runs — no force push, no
// history rewrite via push, no push, period).
func IsPushCommand(cmd string) bool {
	parser := syntax.NewParser()
	f, err := parser.Parse(strings.NewReader(cmd), "")
	if err != nil {
		return false
	}
	push := false
	syntax.Walk(f, func(n syntax.Node) bool {
		call, ok := n.(*syntax.CallExpr)
		if !ok || len(call.Args) == 0 {
			return true
		}
		var words []string
		for _, a := range call.Args {
			if lit, ok := a.Parts[0].(*syntax.Lit); ok {
				words = append(words, lit.Value)
			} else {
				words = append(words, "")
			}
		}
		if len(words) >= 2 && words[0] == "git" {
			rest := words[1:]
			for len(rest) > 0 {
				w := rest[0]
				rest = rest[1:]
				if w == "push" {
					push = true
					return false
				}
				if strings.HasPrefix(w, "-") {
					// Flags (and -C/--git-dir/--work-tree
					// values) precede the subcommand.
					if (w == "-C" || w == "--git-dir" || w == "--work-tree") && len(rest) > 0 {
						rest = rest[1:]
					}
					continue
				}
				break // first non-flag word is not push
			}
		}
		return true
	})
	return push
}
