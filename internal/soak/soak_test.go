package soak

import (
	"encoding/csv"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func TestSoakCompletes(t *testing.T) {
	work := t.TempDir()
	if err := os.WriteFile(filepath.Join(work, "soak.txt"), []byte("soak fixture\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	out := filepath.Join(t.TempDir(), "soak.csv")
	v, err := Run(Config{Turns: 5, WorkDir: work, OutCSV: out})
	if err != nil {
		t.Fatal(err)
	}
	if v.Turns != 5 || v.Crashes != 0 {
		t.Fatalf("verdict = %+v", v)
	}
	f, err := os.Open(out)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	rows, err := csv.NewReader(f).ReadAll()
	if err != nil {
		t.Fatal(err)
	}
	// header + 5 samples.
	if len(rows) != 6 {
		t.Fatalf("csv rows = %d", len(rows))
	}
	if v.Goroutines <= 0 || v.HeapMB <= 0 {
		t.Fatalf("verdict missing telemetry: %+v", v)
	}
}

func TestSoakWithHookAndMCP(t *testing.T) {
	work := t.TempDir()
	if err := os.WriteFile(filepath.Join(work, "soak.txt"), []byte("x\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	// Build the fake MCP server like the mcp tests do.
	bin := filepath.Join(t.TempDir(), "fakeserver")
	build := exec.Command("go", "build", "-o", bin, "../mcp/testdata/fakeserver/main.go")
	if out, err := build.CombinedOutput(); err != nil {
		t.Skipf("cannot build fake MCP server: %v\n%s", err, out)
	}
	out := filepath.Join(t.TempDir(), "soak.csv")
	v, err := Run(Config{Turns: 25, WorkDir: work, OutCSV: out, HookCmd: "/bin/true", MCPBin: bin})
	if err != nil {
		t.Fatal(err)
	}
	if v.Crashes != 0 {
		t.Fatalf("verdict = %+v", v)
	}
}
