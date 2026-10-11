package verifier

import (
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// Standard end-to-end with an injected runner: the tier matrix
// through the real Verify path.
func TestStandardMatrix(t *testing.T) {
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "go.mod"), []byte("module mtest\n\ngo 1.21\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	pass := func(_ context.Context, c Check) CheckResult { return CheckResult{Check: c, Passed: true} }

	v, _ := Standard{Dir: dir, Runner: pass}.Verify(context.Background(), Request{Prompt: "ship it", Claim: "done"})
	if v.Result != Verified || !strings.Contains(v.Reason, "T3") {
		t.Fatalf("[V] all-pass must verify at T3: %+v", v)
	}
	t.Logf("[V] all-pass -> %s (%s)", v.Result, v.Reason)

	failOnce := func(_ context.Context, c Check) CheckResult {
		if c.Name == "go-test" {
			return CheckResult{Check: c, Output: "FAIL"}
		}
		return CheckResult{Check: c, Passed: true}
	}
	v, _ = Standard{Dir: dir, Runner: failOnce}.Verify(context.Background(), Request{Prompt: "ship it", Claim: "done"})
	if v.Result != NotVerified {
		t.Fatalf("[V] failing check must not verify: %+v", v)
	}
	t.Logf("[V] one failure -> %s (%s)", v.Result, v.Reason)

	v, _ = Standard{Dir: t.TempDir()}.Verify(context.Background(), Request{Prompt: "ship it", Claim: "done"})
	if v.Result != Inconclusive {
		t.Fatalf("[V] checkless repo must be inconclusive: %+v", v)
	}
	t.Logf("[V] no checks -> %s (%s)", v.Result, v.Reason)
}

// Standard against a REAL toolchain: a tiny module whose suite
// passes must verify (no injected runner).
func TestStandardRealToolchain(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain not available")
	}
	dir := t.TempDir()
	files := map[string]string{
		"go.mod":       "module tinyv\n\ngo 1.21\n",
		"tiny.go":      "package tinyv\n\nfunc Add(a, b int) int { return a + b }\n",
		"tiny_test.go": "package tinyv\n\nimport \"testing\"\n\nfunc TestAdd(t *testing.T) {\n\tif Add(1, 2) != 3 {\n\t\tt.Fatal(\"broken\")\n\t}\n}\n",
	}
	for name, content := range files {
		if err := os.WriteFile(filepath.Join(dir, name), []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	v, err := Standard{Dir: dir}.Verify(context.Background(), Request{Prompt: "Add works", Claim: "done"})
	if err != nil {
		t.Fatal(err)
	}
	if v.Result != Verified {
		t.Fatalf("[V] real passing suite must verify: %+v", v)
	}
	t.Logf("[V] real toolchain (go test + go vet on fixture) -> %s (%s)", v.Result, v.Reason)
}
