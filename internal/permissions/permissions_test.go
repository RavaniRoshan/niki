package permissions

import "testing"

func TestAllowByMode(t *testing.T) {
	g := NewGuard(ModeReadOnly)
	if g.Allow("write_file") || g.Allow("shell") {
		t.Fatal("readonly must block write_file and shell")
	}
	if !g.Allow("read_file") {
		t.Fatal("readonly must allow read_file")
	}
	g = NewGuard(ModeFullAccess)
	if !g.Allow("shell") {
		t.Fatal("full_access must allow shell")
	}
}

func TestClassify(t *testing.T) {
	if ClassifyCommand("rm -rf /") != "dangerous" {
		t.Fatal("rm -rf")
	}
	if ClassifyCommand("ls -la") != "safe" {
		t.Fatal("ls")
	}
}
