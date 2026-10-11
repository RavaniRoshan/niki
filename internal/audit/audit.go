// Package audit records a local, rotated log of approvals and sandbox
// denials (X7). Every permission decision (allow/deny) and every
// sandbox refusal is appended as one JSON line; the log rotates at a
// size cap so it stays bounded. Nothing is transmitted — the log is
// local-only and append-only.
package audit

import (
	"bufio"
	"encoding/json"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// Kind labels what an audit entry records (X7).
type Kind string

const (
	KindApproval Kind = "approval" // a permission decision
	KindSandbox  Kind = "sandbox"  // a sandbox denial / containment event
)

// Entry is one audit record.
type Entry struct {
	Time    time.Time `json:"time"`
	Kind    Kind      `json:"kind"`
	Tool    string    `json:"tool,omitempty"`
	Command string    `json:"command,omitempty"`
	// Decision is the approval outcome ("allow" / "deny") or the
	// sandbox outcome ("denied" / "contained").
	Decision string `json:"decision"`
	Reason   string `json:"reason,omitempty"`
}

// DefaultMaxBytes is the rotation threshold: once the live log reaches
// this size, it is rotated to a .1 backup (shifting older backups up)
// and a fresh log is started.
const DefaultMaxBytes = 1 << 20 // 1 MiB

// DefaultBackups is how many rotated files are kept beside the live
// log (matching logx): path -> path.1 -> ... -> path.N, oldest dropped.
const DefaultBackups = 3

// Log is an append-only, size-rotated audit log (X7). Safe for
// concurrent use.
type Log struct {
	mu       sync.Mutex
	path     string
	maxBytes int64
	backups  int
}

// Open opens (creating if needed) the audit log at path.
func Open(path string) (*Log, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	return &Log{path: path, maxBytes: DefaultMaxBytes, backups: DefaultBackups}, nil
}

// SetMaxBytes overrides the rotation threshold (<=0 keeps the default).
func (l *Log) SetMaxBytes(n int64) {
	if n > 0 {
		l.maxBytes = n
	}
}

// Append writes one entry, rotating first if the live log is at/over
// the cap so no file grows unbounded (X7: rotated).
func (l *Log) Append(e Entry) error {
	if e.Time.IsZero() {
		e.Time = time.Now()
	}
	data, err := json.Marshal(e)
	if err != nil {
		return err
	}
	data = append(data, '\n')

	l.mu.Lock()
	defer l.mu.Unlock()

	if err := l.rotateIfNeeded(); err != nil {
		return err
	}
	f, err := os.OpenFile(l.path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	defer func() { _ = f.Close() }()
	_, err = f.Write(data)
	return err
}

// RecordApproval logs a permission decision (X7).
func (l *Log) RecordApproval(tool, cmd, decision, reason string) error {
	return l.Append(Entry{Kind: KindApproval, Tool: tool, Command: cmd, Decision: decision, Reason: reason})
}

// RecordSandbox logs a sandbox denial/containment event (X7).
func (l *Log) RecordSandbox(tool, cmd, decision, reason string) error {
	return l.Append(Entry{Kind: KindSandbox, Tool: tool, Command: cmd, Decision: decision, Reason: reason})
}

// rotateIfNeeded rotates the live log when it has reached the cap:
// path -> path.1 -> ... -> path.N (oldest dropped), the same scheme
// logx uses. The caller holds l.mu.
func (l *Log) rotateIfNeeded() error {
	fi, err := os.Stat(l.path)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	if fi.Size() < l.maxBytes {
		return nil
	}
	// Shift backups upward: .N is dropped, .N-1 -> .N, ..., .1 -> .2,
	// live -> .1.
	_ = os.Remove(l.rotated(l.backups))
	for i := l.backups - 1; i >= 1; i-- {
		if _, err := os.Stat(l.rotated(i)); err == nil {
			_ = os.Rename(l.rotated(i), l.rotated(i+1))
		}
	}
	return os.Rename(l.path, l.rotated(1))
}

// rotated returns the i-th backup path (i >= 1).
func (l *Log) rotated(i int) string {
	return l.path + "." + itoa(i)
}

func itoa(i int) string {
	if i == 0 {
		return "0"
	}
	var b [10]byte
	pos := len(b)
	for i > 0 {
		pos--
		b[pos] = byte('0' + i%10)
		i /= 10
	}
	return string(b[pos:])
}

// Read returns every entry currently in the live log and its backups
// (oldest first). Malformed lines are skipped.
func (l *Log) Read() ([]Entry, error) {
	l.mu.Lock()
	defer l.mu.Unlock()
	var out []Entry
	// Oldest backup first ... newest backup, then the live log.
	paths := []string{}
	for i := l.backups; i >= 1; i-- {
		paths = append(paths, l.rotated(i))
	}
	paths = append(paths, l.path)
	for _, p := range paths {
		entries, err := readFile(p)
		if err != nil {
			if os.IsNotExist(err) {
				continue
			}
			return out, err
		}
		out = append(out, entries...)
	}
	return out, nil
}

func readFile(path string) ([]Entry, error) {
	f, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer func() { _ = f.Close() }()
	var out []Entry
	sc := bufio.NewScanner(f)
	sc.Buffer(make([]byte, 0, 64*1024), 1<<20)
	for sc.Scan() {
		var e Entry
		if json.Unmarshal(sc.Bytes(), &e) == nil {
			out = append(out, e)
		}
	}
	return out, sc.Err()
}
