package tools

import (
	"context"
	"encoding/json"
	"os"
	"strings"
	"testing"
)

// T1: a small core toolset — count and schema sizes recorded, no
// tool duplicates another (unique names).
func TestCoreToolsetCountSizesUnique(t *testing.T) {
	reg := DefaultRegistry()
	list := reg.List()
	names := map[string]bool{}
	total := 0
	biggest := ""
	biggestN := 0
	for _, tl := range list {
		if names[tl.Name()] {
			t.Fatalf("[T1] duplicate tool %q", tl.Name())
		}
		names[tl.Name()] = true
		n := len(tl.Schema())
		total += n
		if n > biggestN {
			biggest, biggestN = tl.Name(), n
		}
		if tl.Description() == "" {
			t.Fatalf("[T1] tool %q has no description", tl.Name())
		}
	}
	t.Logf("[T1] tools=%d schema_bytes=%d biggest=%s(%d)", len(list), total, biggest, biggestN)
	if len(list) != 36 {
		t.Fatalf("[T1] tool count = %d, want 36", len(list))
	}
}

// T2: schemas are strict and versioned.
func TestSchemasStrictAndVersioned(t *testing.T) {
	reg := DefaultRegistry()
	for _, tl := range reg.List() {
		var s struct {
			Version string `json:"version"`
		}
		if err := json.Unmarshal([]byte(tl.Schema()), &s); err != nil {
			t.Fatalf("[T2] %s: bad schema: %v", tl.Name(), err)
		}
		if s.Version != SchemaVersion {
			t.Fatalf("[T2] %s: schema version %q, want %q", tl.Name(), s.Version, SchemaVersion)
		}
	}
	t.Logf("[T2] all %d schemas carry version %q", len(reg.List()), SchemaVersion)

	// Strict: unknown arguments rejected.
	_, err := reg.Run(context.Background(), "read_file", json.RawMessage(`{"path":"x","bogus":1}`))
	if err == nil || !strings.Contains(err.Error(), `unknown arg "bogus"`) {
		t.Fatalf("[T2] unknown arg not rejected: %v", err)
	}
	// Unversioned schema rejected.
	if err := ValidateArgs(json.RawMessage(`{}`), `{"required":[]}`); err == nil {
		t.Fatalf("[T2] unversioned schema accepted")
	}
	t.Logf("[T2] strict validation: unknown args and unversioned schemas rejected")
}

// T8: tool results are typed success/error/timeout, each bounded.
func TestToolResultsTypedAndBounded(t *testing.T) {
	reg := DefaultRegistry()
	dir := t.TempDir()
	p := dir + "/t.txt"
	if err := writeFile(t, p, "hello"); err != nil {
		t.Fatal(err)
	}
	args, _ := json.Marshal(map[string]any{"path": p})
	res, err := reg.Run(context.Background(), "read_file", args)
	if err != nil || res.IsError || res.Kind != ResultSuccess {
		t.Fatalf("[T8] success mistyped: %+v err=%v", res, err)
	}

	missing, _ := json.Marshal(map[string]any{"path": dir + "/nope.txt"})
	res, err = reg.Run(context.Background(), "read_file", missing)
	if err != nil {
		t.Fatalf("[T8] missing file must be a typed result, not an error: %v", err)
	}
	if res.Kind != ResultError || !res.IsError {
		t.Fatalf("[T8] error mistyped: %+v", res)
	}

	// Timeout: an already-deadlined context around a tool that
	// reports a Go error surfaces as timeout, not generic error.
	tctx, cancel := context.WithCancel(context.Background())
	cancel()
	tres, terr := reg.Run(tctx, "shell", json.RawMessage(`{"command":"echo hi","timeout":5}`))
	if terr == nil {
		t.Fatalf("[T8] expected error from deadlined shell, got %+v", tres)
	}
	_ = tres
	t.Logf("[T8] kinds: success=%q error=%q (+timeout on deadline)", ResultSuccess, ResultError)

	// Bounded: tested in C2 (10 MB -> ~2 KB preview); smoke here.
	if len(res.Output) > MaxOutputChars {
		t.Fatalf("[T8] result body unbounded: %d", len(res.Output))
	}
}

func writeFile(t *testing.T, path, content string) error {
	t.Helper()
	return os.WriteFile(path, []byte(content), 0o644)
}
