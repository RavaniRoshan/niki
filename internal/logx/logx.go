// Package logx provides structured file logging for
// background diagnostics (P5). Logs go to a file,
// never to stdout or stderr while the TUI owns the
// terminal.
package logx

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
	"sync"
	"time"

	"github.com/RavaniRoshan/niki/internal/tools"
)

// Rotation defaults (O5): logs are local, redacted, rotated, and
// bounded in size.
const (
	// DefaultMaxBytes caps one log file before rotation.
	DefaultMaxBytes = 1 << 20 // 1 MiB
	// DefaultBackups keeps this many rotated files beside the live one.
	DefaultBackups = 3
)

// Logger appends JSON lines to a file. It is safe
// for concurrent use.
type Logger struct {
	mu       sync.Mutex
	f        *os.File
	path     string
	maxBytes int64
	backups  int
	size     int64
}

// Open opens (or creates) the log file at path,
// creating parent directories as needed. Rotation uses
// the defaults (1 MiB, 3 backups).
func Open(path string) (*Logger, error) {
	return OpenRotated(path, DefaultMaxBytes, DefaultBackups)
}

// OpenRotated opens a size-bounded log: when the live file would
// exceed maxBytes, it rotates (path -> path.1 -> ... -> path.N,
// oldest dropped). Non-positive maxBytes disables rotation;
// negative backups keeps none.
func OpenRotated(path string, maxBytes int64, backups int) (*Logger, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return nil, err
	}
	var size int64
	if fi, serr := f.Stat(); serr == nil {
		size = fi.Size()
	}
	return &Logger{f: f, path: path, maxBytes: maxBytes, backups: backups, size: size}, nil
}

// rotate moves path -> path.1 -> ... (oldest dropped). Caller holds mu.
func (l *Logger) rotate() {
	_ = l.f.Close()
	for i := l.backups; i >= 1; i-- {
		src := l.path
		if i > 1 {
			src = l.rotated(i - 1)
		}
		dst := l.rotated(i)
		if i == l.backups {
			_ = os.Remove(dst)
		}
		_ = os.Rename(src, dst)
	}
	f, err := os.OpenFile(l.path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		l.f = nil
		return
	}
	l.f = f
	l.size = 0
}

func (l *Logger) rotated(i int) string {
	return l.path + "." + itoa(i)
}

func itoa(i int) string {
	return strconv.Itoa(i)
}

// Entry is one structured log record.
type Entry struct {
	Time   time.Time         `json:"time"`
	Level  string            `json:"level"`
	Event  string            `json:"event"`
	Error  string            `json:"error,omitempty"`
	Fields map[string]string `json:"fields,omitempty"`
}

// Info records an informational event.
func (l *Logger) Info(event string, fields map[string]string) {
	l.write("info", event, "", fields)
}

// Error records a failed event with its error.
func (l *Logger) Error(event string, err error, fields map[string]string) {
	msg := ""
	if err != nil {
		msg = err.Error()
	}
	l.write("error", event, msg, fields)
}

func (l *Logger) write(level, event, errMsg string, fields map[string]string) {
	if l == nil || l.f == nil {
		return
	}
	// I4/X3: secrets are redacted before bytes hit the log file.
	e := Entry{
		Time:   time.Now().UTC(),
		Level:  level,
		Event:  tools.Redact(event),
		Error:  tools.Redact(errMsg),
		Fields: redactFields(fields),
	}
	line, merr := json.Marshal(e)
	if merr != nil {
		return
	}
	line = append(line, '\n')
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.f == nil {
		return
	}
	// O5: bound the live file — rotate before an append that would
	// exceed the cap, so no file ever grows past it.
	if l.maxBytes > 0 && l.size+int64(len(line)) > l.maxBytes {
		l.rotate()
		if l.f == nil {
			return
		}
	}
	n, _ := l.f.Write(line)
	l.size += int64(n)
}

// redactFields scrubs secret-like values from structured fields.
func redactFields(fields map[string]string) map[string]string {
	if fields == nil {
		return nil
	}
	out := make(map[string]string, len(fields))
	for k, v := range fields {
		out[k] = tools.Redact(v)
	}
	return out
}

// Close flushes and closes the log file.
func (l *Logger) Close() {
	if l == nil || l.f == nil {
		return
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	_ = l.f.Sync()
	_ = l.f.Close()
	l.f = nil
}
