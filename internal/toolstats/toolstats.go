// Package toolstats records local per-tool usage statistics (T9):
// call counts and failure counts per tool, persisted to a local JSONL
// file under the canonical home. Nothing is ever transmitted — the
// store is append-only and read only by `niki stats`.
package toolstats

import (
	"bufio"
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Record is one tool invocation logged locally (T9).
type Record struct {
	Time time.Time `json:"time"`
	Tool string    `json:"tool"`
	OK   bool      `json:"ok"`
}

// Stats is the aggregated view for one tool.
type Stats struct {
	Tool     string `json:"tool"`
	Calls    int    `json:"calls"`
	Failures int    `json:"failures"`
}

// Store is an append-only local stats log. It is safe for concurrent
// use and never performs any network I/O.
type Store struct {
	mu   sync.Mutex
	path string
}

// Open opens (creating if needed) the stats log at path.
func Open(path string) (*Store, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return nil, err
	}
	_ = f.Close()
	return &Store{path: path}, nil
}

// DefaultPath is the canonical local stats file.
func DefaultPath() string {
	if home := os.Getenv("HOME"); home != "" {
		return filepath.Join(home, ".nikicode", "toolstats.jsonl")
	}
	return filepath.Join(os.TempDir(), "nikicode-toolstats.jsonl")
}

// Record appends one tool invocation (T9). It never blocks on network
// and never fails the caller's turn — a write error is swallowed so
// stats collection can never break a run.
func (s *Store) Record(tool string, ok bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	rec := Record{Time: time.Now(), Tool: tool, OK: ok}
	data, err := json.Marshal(rec)
	if err != nil {
		return
	}
	f, err := os.OpenFile(s.path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return
	}
	defer func() { _ = f.Close() }()
	_, _ = f.Write(append(data, '\n'))
}

// Aggregate reads the log and returns per-tool stats sorted by call
// count descending, then name. Malformed lines are skipped.
func (s *Store) Aggregate() ([]Stats, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	f, err := os.Open(s.path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	defer func() { _ = f.Close() }()

	byTool := map[string]*Stats{}
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 0, 64*1024), 1<<20)
	for sc.Scan() {
		var rec Record
		if err := json.Unmarshal(sc.Bytes(), &rec); err != nil || rec.Tool == "" {
			continue
		}
		st, ok := byTool[rec.Tool]
		if !ok {
			st = &Stats{Tool: rec.Tool}
			byTool[rec.Tool] = st
		}
		st.Calls++
		if !rec.OK {
			st.Failures++
		}
	}
	out := make([]Stats, 0, len(byTool))
	for _, st := range byTool {
		out = append(out, *st)
	}
	sort.Slice(out, func(i, j int) bool {
		if out[i].Calls != out[j].Calls {
			return out[i].Calls > out[j].Calls
		}
		return out[i].Tool < out[j].Tool
	})
	return out, nil
}

// Render formats the aggregate as a plain-text table for `niki stats`.
func Render(stats []Stats) string {
	if len(stats) == 0 {
		return "no tool usage recorded yet\n"
	}
	var b strings.Builder
	b.WriteString("tool                 calls  failures\n")
	b.WriteString("------------------------------------\n")
	totalCalls, totalFail := 0, 0
	for _, s := range stats {
		b.WriteString(pad(s.Tool, 20) + " " + pad(strconv.Itoa(s.Calls), 6) + " " + pad(strconv.Itoa(s.Failures), 8) + "\n")
		totalCalls += s.Calls
		totalFail += s.Failures
	}
	b.WriteString("------------------------------------\n")
	b.WriteString(pad("TOTAL", 20) + " " + pad(strconv.Itoa(totalCalls), 6) + " " + pad(strconv.Itoa(totalFail), 8) + "\n")
	return b.String()
}

func pad(s string, n int) string {
	if len(s) >= n {
		return s
	}
	return s + spaces(n-len(s))
}

func spaces(n int) string {
	if n <= 0 {
		return ""
	}
	return strings.Repeat(" ", n)
}
