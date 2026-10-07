package sandbox

import (
	"context"
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
