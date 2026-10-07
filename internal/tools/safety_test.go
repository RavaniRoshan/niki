package tools

import "testing"

func TestRedact(t *testing.T) {
	out := Redact("key sk-abcdefghijklmnop and Bearer xyz123")
	if out == "key sk-abcdefghijklmnop and Bearer xyz123" {
		t.Fatal("redaction failed")
	}
}

func TestSafeJoin(t *testing.T) {
	if _, ok := SafeJoin("/tmp/root", "sub/file.txt"); !ok {
		t.Fatal("valid join rejected")
	}
	if _, ok := SafeJoin("/tmp/root", "../etc/passwd"); ok {
		t.Fatal("escape accepted")
	}
}
