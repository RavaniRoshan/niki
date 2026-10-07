package session

import (
	"database/sql"
	"encoding/json"
	"os"
	"path/filepath"
	"time"

	_ "modernc.org/sqlite"

	"github.com/RavaniRoshan/niki/internal/protocol"
)

// Store persists sessions and events to SQLite + a JSONL event log.
type Store struct {
	db   *sql.DB
	logPath string
	log *os.File
}

func Open(path string) (*Store, error) {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return nil, err
	}
	db, err := sql.Open("sqlite", path)
	if err != nil {
		return nil, err
	}
	if _, err := db.Exec(`CREATE TABLE IF NOT EXISTS sessions (id TEXT PRIMARY KEY, created_at INTEGER, title TEXT)`); err != nil {
		return nil, err
	}
	if _, err := db.Exec(`CREATE TABLE IF NOT EXISTS events (id INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT, type TEXT, payload TEXT, ts INTEGER)`); err != nil {
		return nil, err
	}
	logPath := path + ".jsonl"
	f, err := os.OpenFile(logPath, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return nil, err
	}
	return &Store{db: db, logPath: logPath, log: f}, nil
}

func (s *Store) Close() error {
	_ = s.log.Close()
	return s.db.Close()
}

func (s *Store) CreateSession(id protocol.SessionId, title string) error {
	_, err := s.db.Exec(`INSERT INTO sessions (id, created_at, title) VALUES (?, ?, ?)`, string(id), time.Now().Unix(), title)
	return err
}

func (s *Store) AppendEvent(sessionID protocol.SessionId, evt protocol.EngineEvent) error {
	payload, _ := json.Marshal(evt)
	_, err := s.db.Exec(`INSERT INTO events (session_id, type, payload, ts) VALUES (?, ?, ?, ?)`, string(sessionID), string(evt.Type), string(payload), evt.Timestamp.Unix())
	if err != nil {
		return err
	}
	_, err = s.log.Write(append(payload, '\n'))
	return err
}

func (s *Store) ListSessions() ([]string, error) {
	rows, err := s.db.Query(`SELECT id FROM sessions ORDER BY created_at DESC`)
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()
	var ids []string
	for rows.Next() {
		var id string
		_ = rows.Scan(&id)
		ids = append(ids, id)
	}
	return ids, nil
}

func (s *Store) Events(sessionID protocol.SessionId) ([]protocol.EngineEvent, error) {
	rows, err := s.db.Query(`SELECT payload FROM events WHERE session_id = ? ORDER BY id`, string(sessionID))
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()
	var evts []protocol.EngineEvent
	for rows.Next() {
		var payload string
		_ = rows.Scan(&payload)
		var e protocol.EngineEvent
		_ = json.Unmarshal([]byte(payload), &e)
		evts = append(evts, e)
	}
	return evts, nil
}
