package permissions

import "testing"

func TestAnalyzeShellSimple(t *testing.T) {
	if got := AnalyzeShell("ls -la"); got != ShellSimple {
		t.Fatalf("ls -la = %s", got)
	}
	if got := AnalyzeShell("echo hello > out.txt"); got != ShellSimple {
		t.Fatalf("redirect = %s", got)
	}
}

func TestAnalyzeShellComplex(t *testing.T) {
	for _, cmd := range []string{
		"cat file | grep x",
		"echo $(whoami)",
		"(cd /tmp && ls)",
		"if true; then ls; fi",
		"for i in 1 2; do echo $i; done",
		"ls && rm -rf /",
	} {
		if got := AnalyzeShell(cmd); got != ShellTooComplex {
			t.Errorf("%q = %s, want too-complex", cmd, got)
		}
	}
}

func TestAnalyzeShellSequenceAllowed(t *testing.T) {
	if got := AnalyzeShell("ls; echo hi"); got != ShellSimple {
		t.Fatalf("sequence = %s", got)
	}
}

func TestAnalyzeShellUnparseable(t *testing.T) {
	if got := AnalyzeShell("echo 'unterminated"); got == ShellSimple {
		t.Fatalf("unterminated quote = %s", got)
	}
}
