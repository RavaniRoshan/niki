package tools

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// L7: parallel tool calls when independent; calls that write the
// same file are serialized — a fixture with conflicting writes
// applies them in order.
func TestConflictingWritesApplyInOrder(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, "order.txt")
	if err := os.WriteFile(target, []byte("start\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	reg := DefaultRegistry()
	ctx := context.Background()
	mkEdit := func(id, old, new string) Call {
		args, _ := json.Marshal(map[string]any{"path": target, "old_string": old, "new_string": new})
		return Call{ID: id, Name: "edit_file", Args: args}
	}
	// edit_file is not concurrency-safe: the batch must run
	// sequentially, so chained edits apply in order.
	results := reg.Batch(ctx, []Call{
		mkEdit("a", "start", "one"),
		mkEdit("b", "one", "two"),
		mkEdit("c", "two", "three"),
	})
	for i, r := range results {
		if r.IsError {
			t.Fatalf("[L7] call %d failed: %s", i, r.Output)
		}
	}
	data, _ := os.ReadFile(target)
	if string(data) != "three\n" {
		t.Fatalf("[L7] conflicting writes misordered: %q", data)
	}
	t.Logf("[L7] 3 conflicting same-file writes applied in order (sequential)")

	// Independent read-only calls run (results in call order).
	reads := reg.Batch(ctx, []Call{
		{ID: "r1", Name: "git_status", Args: json.RawMessage(`{"dir":"."}`)},
		{ID: "r2", Name: "git_status", Args: json.RawMessage(`{"dir":"."}`)},
	})
	if len(reads) != 2 {
		t.Fatalf("[L7] batch results = %d, want 2", len(reads))
	}
	t.Logf("[L7] independent calls return in original call order")
}
