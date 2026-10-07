package permissions

import "testing"

// FuzzAnalyzeShell ensures hostile shell input never wedges the parser.
func FuzzAnalyzeShell(f *testing.F) {
	f.Add("ls -la")
	f.Add("cat a | grep b")
	f.Add("$(rm -rf /)")
	f.Add("echo 'unterminated")
	f.Add("if true; then ls; fi")
	f.Fuzz(func(t *testing.T, cmd string) {
		_ = AnalyzeShell(cmd)
	})
}
