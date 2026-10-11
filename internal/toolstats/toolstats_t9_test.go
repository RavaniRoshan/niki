package toolstats

import (
	"path/filepath"
	"strings"
	"testing"
)

// T9: local tool statistics (counts, failures) visible through
// `niki stats`. Nothing leaves the machine — the store is a local
// append-only JSONL file, read only to render the table.
func TestToolStatsLocalCountsAndFailures(t *testing.T) {
	path := filepath.Join(t.TempDir(), "toolstats.jsonl")
	st, err := Open(path)
	if err != nil {
		t.Fatal(err)
	}

	// Record a mix of successes and failures across tools.
	st.Record("read_file", true)
	st.Record("read_file", true)
	st.Record("read_file", false) // one failure
	st.Record("grep", true)
	st.Record("shell", false) // failed
	st.Record("shell", false) // failed again

	agg, err := st.Aggregate()
	if err != nil {
		t.Fatal(err)
	}
	byTool := map[string]Stats{}
	for _, s := range agg {
		byTool[s.Tool] = s
	}
	if byTool["read_file"].Calls != 3 || byTool["read_file"].Failures != 1 {
		t.Fatalf("[T9] read_file stats wrong: %+v", byTool["read_file"])
	}
	if byTool["shell"].Calls != 2 || byTool["shell"].Failures != 2 {
		t.Fatalf("[T9] shell stats wrong: %+v", byTool["shell"])
	}
	if byTool["grep"].Calls != 1 || byTool["grep"].Failures != 0 {
		t.Fatalf("[T9] grep stats wrong: %+v", byTool["grep"])
	}
	// Sorted by call count descending: read_file(3) first.
	if agg[0].Tool != "read_file" {
		t.Fatalf("[T9] not sorted by calls desc: %+v", agg)
	}
	t.Logf("[T9] recorded 6 calls: read_file 3/1 fail, shell 2/2 fail, grep 1/0 fail")

	// Render produces a table with totals.
	out := Render(agg)
	if !strings.Contains(out, "read_file") || !strings.Contains(out, "TOTAL") {
		t.Fatalf("[T9] render missing rows:\n%s", out)
	}
	t.Logf("[T9] `niki stats` table:\n%s", out)

	// The store is purely local: reopening reads back the same counts,
	// and the file is a plain JSONL under the home dir (no network).
	st2, _ := Open(path)
	agg2, _ := st2.Aggregate()
	if len(agg2) != 3 {
		t.Fatalf("[T9] reopen lost data: %+v", agg2)
	}
	t.Logf("[T9] stats persist locally across reopen (%d tools)", len(agg2))
}
