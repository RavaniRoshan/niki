package sandbox

import (
	"context"
	"strings"
	"testing"
)

func TestPassthroughExec(t *testing.T) {
	s := &Passthrough{}
	if s.Name() != "passthrough" {
		t.Fatal("name")
	}
	cmd := s.Exec(context.Background(), "echo", "ok")
	out, err := cmd.Output()
	if err != nil || string(out) != "ok\n" {
		t.Fatalf("out=%q err=%v", out, err)
	}
}

func TestSanitizedEnvStripsSecrets(t *testing.T) {
	t.Setenv("AWS_SECRET_ACCESS_KEY", "x")
	t.Setenv("SSH_AUTH_SOCK", "/tmp/sock")
	t.Setenv("OPENAI_API_KEY", "sk-test")
	t.Setenv("SAFE_VAR", "ok")
	env := SanitizedEnv()
	for _, kv := range env {
		if strings.HasPrefix(kv, "AWS_") || strings.HasPrefix(kv, "SSH_") || strings.HasPrefix(kv, "OPENAI_API_KEY") {
			t.Fatalf("secret leaked: %s", kv)
		}
	}
	found := false
	for _, kv := range env {
		if kv == "SAFE_VAR=ok" {
			found = true
		}
	}
	if !found {
		t.Fatal("safe var stripped")
	}
}

func TestExecIsolatedEnv(t *testing.T) {
	t.Setenv("AWS_SECRET_ACCESS_KEY", "x")
	s := &Passthrough{}
	cmd := s.ExecIsolated(context.Background(), t.TempDir(), "sh", "-c", "env")
	out, err := cmd.Output()
	if err != nil {
		t.Fatal(err)
	}
	if strings.Contains(string(out), "AWS_SECRET_ACCESS_KEY") {
		t.Fatal("sandboxed env leaked AWS secret")
	}
}
