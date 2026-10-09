// Package logx provides structured file logging for
// background diagnostics (P5). Logs go to a file,
// never to stdout or stderr while the TUI owns the
// terminal.
package logx

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sync"
	"time"
)

// Logger appends JSON lines to a file. It is safe
// for concurrent use.
type Logger struct {
	mu sync.Mutex
	f  *os.File
}

// Open opens (or creates) the log file at path,
// creating parent directories as needed.
func Open(path string) (*Logger, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return nil, err
	}
	return &Logger{f: f}, nil
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
	e := Entry{
		Time:   time.Now().UTC(),
		Level:  level,
		Event:  event,
		Error:  errMsg,
		Fields: fields,
	}
	line, merr := json.Marshal(e)
	if merr != nil {
		return
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	_, _ = l.f.Write(append(line, '\n'))
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
