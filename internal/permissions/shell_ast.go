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
