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
	if _, err := s.log.Write(append(payload, '\n')); err != nil {
		return err
	}
	// fsync every append so a crash cannot corrupt or
	// lose committed history (P2).
	return s.log.Sync()
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

// ListSessionSummaries returns metadata summaries for all persisted sessions.
func (s *Store) ListSessionSummaries() ([]protocol.SessionMetadata, error) {
	rows, err := s.db.Query(`SELECT s.id, s.title, s.created_at, COUNT(e.id) FROM sessions s LEFT JOIN events e ON s.id = e.session_id GROUP BY s.id ORDER BY s.created_at DESC`)
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()
	var list []protocol.SessionMetadata
	for rows.Next() {
		var id string
		var title sql.NullString
		var createdAt int64
		var count int
		if err := rows.Scan(&id, &title, &createdAt, &count); err != nil {
			continue
		}
		t := title.String
		if t == "" {
			if len(id) > 8 {
				t = "Session " + id[:8]
			} else {
				t = "Session " + id
			}
		}
		list = append(list, protocol.SessionMetadata{
			ID:        protocol.SessionId(id),
			Title:     t,
			CreatedAt: time.Unix(createdAt, 0),
			TurnCount: count,
		})
	}
	return list, nil
}

// GetRecentHistory returns a short snippet of recent messages for preview.
func (s *Store) GetRecentHistory(sessionID protocol.SessionId, limit int) ([]string, error) {
	evts, err := s.Events(sessionID)
	if err != nil {
		return nil, err
	}
	var msgs []string
	for _, e := range evts {
		if e.Type == protocol.EventTurnStarted && e.Text != "" {
			msgs = append(msgs, "> "+e.Text)
		} else if e.Type == protocol.EventToolStarted && e.ToolName != "" {
			msgs = append(msgs, "● "+e.ToolName)
		} else if e.Type == protocol.EventAssistantTextDelta && e.Text != "" {
			if len(msgs) > 0 && len(msgs[len(msgs)-1]) < 300 {
				msgs[len(msgs)-1] += e.Text
			} else {
				msgs = append(msgs, e.Text)
			}
		}
	}
	if len(msgs) > limit {
		msgs = msgs[len(msgs)-limit:]
	}
	return msgs, nil
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

// Fork creates a new session seeded with the events of
// an existing one. The fork gets a fresh id and its own
// event log; the source session is untouched.
func (s *Store) Fork(srcID protocol.SessionId, title string) (protocol.SessionId, error) {
	evts, err := s.Events(srcID)
	if err != nil {
		return "", err
	}
	newID := protocol.NewSessionId()
	if err := s.CreateSession(newID, title); err != nil {
		return "", err
	}
	for _, e := range evts {
		if err := s.AppendEvent(newID, e); err != nil {
			return "", err
		}
	}
	return newID, nil
}

// Delete removes a session and its events.
func (s *Store) Delete(sessionID protocol.SessionId) error {
	if _, err := s.db.Exec(`DELETE FROM events WHERE session_id = ?`, string(sessionID)); err != nil {
		return err
	}
	_, err := s.db.Exec(`DELETE FROM sessions WHERE id = ?`, string(sessionID))
	return err
}
