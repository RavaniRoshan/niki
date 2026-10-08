package provider

import (
	"context"
	"testing"
	"time"
)

func TestMockToolScriptsEmitFirst(t *testing.T) {
	m := NewMockProvider()
	m.ToolScripts = map[string][]ToolCall{
		"read the main": {{Tool: "read_file", Args: `{"path":"main.go"}`}},
	}
	ctx := context.Background()
	deltas, errs := m.Stream(ctx, []Message{{Role: "user", Content: "read the main file please"}})
	var kinds []DeltaKind
	var toolName string
	timeout := time.After(5 * time.Second)
loop:
	for {
		select {
		case d, ok := <-deltas:
			if !ok {
				break loop
			}
			kinds = append(kinds, d.Kind)
			if d.Kind == DeltaToolCall {
				toolName = d.ToolName
			}
		case err := <-errs:
			if err != nil {
				t.Fatal(err)
			}
		case <-timeout:
			t.Fatal("stream timed out")
		}
	}
	if len(kinds) == 0 || kinds[0] != DeltaToolCall || toolName != "read_file" {
		t.Fatalf("tool call must lead: kinds=%v tool=%q", kinds, toolName)
	}
}
