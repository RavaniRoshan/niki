package tui

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// U7: the wiring audit. This is the build check the mission asks
// for: every rendered element is registered with a real event, and
// the registry cannot drift from the doc's wiring table.

func TestWiringAudit(t *testing.T) {
	if len(WiringTable) == 0 {
		t.Fatal("the wiring table is empty")
	}
	// The proof functions must exist: read the package's test
	// files once and index every test function name.
	testSrc := map[string]bool{}
	files, err := filepath.Glob("*_test.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range files {
		data, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		for _, line := range strings.Split(string(data), "\n") {
			if name, ok := strings.CutPrefix(strings.TrimSpace(line), "func Test"); ok {
				if i := strings.Index(name, "("); i > 0 {
					testSrc["Test"+name[:i]] = true
				}
			}
		}
	}

	have, deferCount := 0, 0
	for _, el := range WiringTable {
		if el.Driver == "" {
			t.Fatalf("[wiring] element %q has no event: no element may render without a driver", el.ID)
		}
		switch el.Status {
		case wiringHave:
			have++
			if el.Absent != "" {
				if el.Proof == "" {
					t.Fatalf("[wiring] element %q has an absent state but no proof test", el.ID)
				}
				if !testSrc[el.Proof] {
					t.Fatalf("[wiring] element %q names proof %q, which is not a test in this package", el.ID, el.Proof)
				}
			}
		case wiringDefer:
			deferCount++
			if el.DeferWhy == "" {
				t.Fatalf("[wiring] DEFER element %q has no reason", el.ID)
			}
		default:
			t.Fatalf("[wiring] element %q has unknown status %q", el.ID, el.Status)
		}
		if el.DocElement == "" {
			t.Fatalf("[wiring] element %q has no doc label", el.ID)
		}
	}

	// Doc parity: the registry and WORKBENCH.md's wiring table must
	// name the same elements, so neither can drift.
	doc := filepath.Join("..", "..", "docs", "tui", "WORKBENCH.md")
	data, err := os.ReadFile(doc)
	if err != nil {
		t.Fatalf("[wiring] cannot read %s: %v", doc, err)
	}
	section := string(data)
	if i := strings.Index(section, "## 3."); i >= 0 {
		section = section[i:]
		if j := strings.Index(section, "## 4."); j >= 0 {
			section = section[:j]
		}
	}
	docElements := map[string]bool{}
	for _, line := range strings.Split(section, "\n") {
		if !strings.HasPrefix(line, "| ") {
			continue
		}
		cells := strings.Split(line, "|")
		if len(cells) < 3 {
			continue
		}
		label := strings.TrimSpace(cells[1])
		if label == "" || label == "Element" || strings.HasPrefix(label, "---") {
			continue
		}
		docElements[label] = true
	}
	if len(docElements) == 0 {
		t.Fatal("no wiring table rows parsed from the doc")
	}
	for _, el := range WiringTable {
		if !docElements[el.DocElement] {
			t.Fatalf("[wiring] element %q (doc label %q) is not in the WORKBENCH.md wiring table", el.ID, el.DocElement)
		}
	}
	for label := range docElements {
		found := false
		for _, el := range WiringTable {
			if el.DocElement == label {
				found = true
				break
			}
		}
		if !found {
			t.Fatalf("[wiring] the doc's wiring table has row %q with no registry entry", label)
		}
	}
	t.Logf("[wiring] %d elements registered (%d HAVE, %d DEFER); doc table in sync (%d rows)", len(WiringTable), have, deferCount, len(docElements))
}

// U7: the notice row renders from real notice events and is absent
// when none fired (completes the registry's coverage).
func TestTraceNoticeRow(t *testing.T) {
	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	if strings.Contains(traceText(m), "[warning]") || strings.Contains(traceText(m), "[mcp]") {
		t.Fatal("notice rows present before any notice event")
	}
	m = feedEvents(m,
		protocol.EngineEvent{Type: protocol.EventWarning, Text: "approaching the step limit"},
		protocol.EngineEvent{Type: protocol.EventMcpServerReady, ToolName: "fs"},
		protocol.EngineEvent{Type: protocol.EventContextCompacted},
	)
	got := traceText(m)
	for _, want := range []string{"approaching the step limit", "[mcp] fs ready", "context compacted"} {
		if !strings.Contains(got, want) {
			t.Fatalf("notice row %q missing after its event:\n%s", want, got)
		}
	}
}

// U7: the diff viewer opens on a real diff from the real working
// tree (/diff), and is absent until one exists.
func TestDiffViewerFromRealDiff(t *testing.T) {
	if _, err := exec.LookPath("git"); err != nil {
		t.Skip("git not installed")
	}
	dir := t.TempDir()
	run := func(args ...string) {
		cmd := exec.Command("git", args...)
		cmd.Dir = dir
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("git %v: %v\n%s", args, err, out)
		}
	}
	run("init", "-q")
	run("config", "user.email", "t@t")
	run("config", "user.name", "t")
	if err := os.WriteFile(filepath.Join(dir, "a.go"), []byte("package a\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	run("add", "a.go")
	run("commit", "-qm", "init")

	m := workbenchModel(t, 100, 27)
	m.state.ModelName = "claude-sonnet"
	m.state.Directory = dir
	if m.state.DiffViewer.Active {
		t.Fatal("diff viewer active with no diff")
	}
	// A real change in the real working tree.
	if err := os.WriteFile(filepath.Join(dir, "a.go"), []byte("package a\n\nfunc B() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	um, _ := m.executeSlashCommand("/diff")
	m = um.(AppModel)
	if !m.state.DiffViewer.Active {
		t.Fatal("/diff must open the viewer on a real working-tree diff")
	}
	if v := workbenchFrame(m); !strings.Contains(v, "a.go") {
		t.Fatalf("diff view missing the real file:\n%s", v)
	}
	// A clean tree renders no diff and says so (with the viewer
	// closed first, as the user would).
	m.state.DiffViewer = DiffViewerState{}
	run("checkout", "--", "a.go")
	before := len(m.history.Cells)
	um2, _ := m.executeSlashCommand("/diff")
	m = um2.(AppModel)
	if m.state.DiffViewer.Active {
		t.Fatal("a clean tree must not open the diff viewer")
	}
	if len(m.history.Cells) <= before {
		t.Fatal("a clean tree must report itself")
	}
	if !strings.Contains(m.history.Cells[len(m.history.Cells)-1].Text, "No git diff") {
		t.Fatalf("unexpected clean-tree row: %q", m.history.Cells[len(m.history.Cells)-1].Text)
	}
}

// U7: the removals record is printed with its reasons.
func TestWiringRemovalsPrinted(t *testing.T) {
	if len(WiringRemovals) == 0 {
		t.Fatal("the removals record is empty")
	}
	for _, r := range WiringRemovals {
		if r.Reason == "" {
			t.Fatalf("removal %q has no reason", r.Element)
		}
		t.Logf("[removed] %s — %s", r.Element, r.Reason)
	}
}
