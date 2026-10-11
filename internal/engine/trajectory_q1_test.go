package engine

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
	"time"

	"github.com/RavaniRoshan/niki/internal/protocol"
	"github.com/RavaniRoshan/niki/internal/provider"
)

var trajIDRe = regexp.MustCompile(`[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}|[0-9a-f]{8,}`)
var trajTmpRe = regexp.MustCompile(`/tmp/Test[A-Za-z0-9_]+[0-9]+/[^ "]*`)

// normalizeTrajectory renders one event per line (type, tool, short
// text) with volatile IDs and tmp paths scrubbed, so the golden is
// deterministic across runs and machines.
func normalizeTrajectory(events []protocol.EngineEvent) string {
	var b strings.Builder
	for _, e := range events {
		line := string(e.Type)
		if e.ToolName != "" {
			line += "|" + e.ToolName
		}
		text := e.Text
		if e.Error != "" {
			text = "ERR:" + e.Error
		}
		text = trajIDRe.ReplaceAllString(text, "<id>")
		text = trajTmpRe.ReplaceAllString(text, "<tmp>")
		text = strings.ReplaceAll(text, "\n", "\\n")
		if len(text) > 120 {
			text = text[:120] + "…"
		}
		if text != "" {
			line += "|" + text
		}
		b.WriteString(line + "\n")
	}
	return b.String()
}

func diffLines(want, got string) string {
	w, g := strings.Split(want, "\n"), strings.Split(got, "\n")
	var b strings.Builder
	n := len(w)
	if len(g) > n {
		n = len(g)
	}
	for i := 0; i < n; i++ {
		var wl, gl string
		if i < len(w) {
			wl = w[i]
		}
		if i < len(g) {
			gl = g[i]
		}
		if wl != gl {
			b.WriteString("-" + wl + "\n+" + gl + "\n")
		}
	}
	return b.String()
}

// Q1: fixture provider with recorded trajectories (golden). Every
// behavior change shows up as a trajectory diff, printed in review.
func TestGoldenTrajectoryReadAndVerify(t *testing.T) {
	dir := t.TempDir()
	target := filepath.Join(dir, "gold.txt")
	if err := os.WriteFile(target, []byte("golden-bytes\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	p := &scriptProvider{steps: [][]provider.ToolCall{
		{{ID: "c1", Tool: "read_file", Args: `{"path":"` + target + `"}`}},
	}}
	eng, cmdChan, eventChan := newScriptEngine(t, p)
	cmdChan <- protocol.EngineCommand{Type: protocol.CmdSubmitPrompt, Prompt: "read the gold file"}
	drainUntil(t, eventChan, protocol.EventTurnCompleted, 15*time.Second)
	got := normalizeTrajectory(eng.session.Snapshot())

	goldenPath := filepath.Join("testdata", "trajectory_read_verify.golden")
	want, err := os.ReadFile(goldenPath)
	if err != nil {
		if os.IsNotExist(err) && os.Getenv("NIKI_WRITE_GOLDEN") == "1" {
			if err := os.MkdirAll("testdata", 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(goldenPath, []byte(got), 0o644); err != nil {
				t.Fatal(err)
			}
			t.Logf("[Q1] golden written; re-run to verify")
			return
		}
		t.Fatal(err)
	}
	if string(want) != got {
		t.Fatalf("[Q1] trajectory diff (behavior change — review):\n%s", diffLines(string(want), got))
	}
	t.Logf("[Q1] trajectory matches golden (%d events)", strings.Count(got, "\n"))
}
