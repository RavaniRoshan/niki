// Package journal records `nikicode do` actions for follow-up context,
// corrections, undo, and redo (G3). The journal is an append-only JSONL
// file under the canonical home; every undo verifies the world still
// looks the way the action left it, and refuses otherwise.
package journal

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/RavaniRoshan/niki/internal/git"
	"github.com/RavaniRoshan/niki/internal/paths"
	"github.com/RavaniRoshan/niki/internal/recipes"
)

// Entry is one recorded action.
type Entry struct {
	Time time.Time `json:"time"`
	Kind string    `json:"kind"` // recipe | git | explain
	Name string    `json:"name"` // recipe name, git op, or "explain"
	// Input is the exact step text, so redo can re-parse it.
	Input   string            `json:"input,omitempty"`
	Dir     string            `json:"dir"`
	Vars    map[string]string `json:"vars,omitempty"`
	Effects []recipes.FileEffect `json:"effects,omitempty"`
	// HeadBefore/HeadAfter track the git tip across the action.
	HeadBefore string `json:"head_before,omitempty"`
	HeadAfter  string `json:"head_after,omitempty"`
	// Extra carries op-specific context: subject (explain), branch
	// bookkeeping (branch create/switch), or refusal notes.
	Extra map[string]string `json:"extra,omitempty"`
	// Undone marks entries consumed by undo. Redo appends a NEW entry.
	Undone bool `json:"undone,omitempty"`
}

func journalPath() string {
	return filepath.Join(paths.Dir(), "do-journal.jsonl")
}

// Append records an entry (fsync-appended so a crash cannot lose it).
func Append(e Entry) error {
	if e.Time.IsZero() {
		e.Time = time.Now()
	}
	if err := os.MkdirAll(paths.Dir(), 0o755); err != nil {
		return err
	}
	f, err := os.OpenFile(journalPath(), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	data, err := json.Marshal(e)
	if err != nil {
		_ = f.Close()
		return err
	}
	if _, err := f.Write(append(data, '\n')); err != nil {
		_ = f.Close()
		return err
	}
	if err := f.Sync(); err != nil {
		_ = f.Close()
		return err
	}
	return f.Close()
}

// Read returns all entries in order. Missing file means no history.
func Read() ([]Entry, error) {
	data, err := os.ReadFile(journalPath())
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []Entry
	for _, line := range bytes.Split(data, []byte{'\n'}) {
		if len(bytes.TrimSpace(line)) == 0 {
			continue
		}
		var e Entry
		if err := json.Unmarshal(line, &e); err != nil {
			return nil, err
		}
		out = append(out, e)
	}
	return out, nil
}

// markUndone flags entry i as consumed.
func markUndone(entries []Entry, i int) error {
	entries[i].Undone = true
	return rewrite(entries)
}

func rewrite(entries []Entry) error {
	var b bytes.Buffer
	for _, e := range entries {
		data, err := json.Marshal(e)
		if err != nil {
			return err
		}
		b.Write(append(data, '\n'))
	}
	if err := os.MkdirAll(paths.Dir(), 0o755); err != nil {
		return err
	}
	tmp := journalPath() + ".tmp"
	if err := os.WriteFile(tmp, b.Bytes(), 0o644); err != nil {
		return err
	}
	f, err := os.OpenFile(tmp, os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	serr := f.Sync()
	cerr := f.Close()
	if serr != nil {
		return serr
	}
	if cerr != nil {
		return cerr
	}
	return os.Rename(tmp, journalPath())
}

// Last returns the most recent non-undone entry, or nil.
func Last() (*Entry, error) {
	entries, err := Read()
	if err != nil {
		return nil, err
	}
	for i := len(entries) - 1; i >= 0; i-- {
		if !entries[i].Undone {
			e := entries[i]
			return &e, nil
		}
	}
	return nil, nil
}

// Undo reverts the most recent non-undone entry and marks it consumed.
// File effects restore in reverse; a moved git tip resets --soft only
// when HEAD is exactly where the action left it; anything else refuses
// with the reason instead of guessing.
func Undo() (string, error) {
	entries, err := Read()
	if err != nil {
		return "", err
	}
	idx := -1
	for i := len(entries) - 1; i >= 0; i-- {
		if !entries[i].Undone {
			idx = i
			break
		}
	}
	if idx < 0 {
		return "", fmt.Errorf("nothing to undo")
	}
	e := entries[idx]
	var notes []string
	// 1. Git tip: restore only when untouched since.
	if e.HeadBefore != "" && e.HeadAfter != "" && e.HeadBefore != e.HeadAfter {
		now := git.Head(e.Dir)
		if now == "" {
			return "", fmt.Errorf("cannot undo %s: HEAD unreadable in %s", e.Name, e.Dir)
		}
		if now != e.HeadAfter {
			return "", fmt.Errorf("cannot undo %s: history moved since (%s is not %s); back it out by hand", e.Name, now[:7], e.HeadAfter[:7])
		}
		if err := git.ResetSoft(e.Dir, e.HeadBefore); err != nil {
			return "", fmt.Errorf("cannot undo %s: %v", e.Name, err)
		}
		notes = append(notes, "git tip restored (changes kept staged)")
	}
	// 2. Branch bookkeeping.
	if e.Kind == "git" && e.Extra["undo"] == "branch" {
		name, prev := e.Extra["name"], e.Extra["prev"]
		if prev != "" {
			if err := git.BranchSwitch(e.Dir, prev); err != nil {
				return "", fmt.Errorf("cannot undo branch %s: %v", name, err)
			}
		}
		if name != "" {
			if err := git.DeleteBranch(e.Dir, name); err != nil {
				return "", fmt.Errorf("cannot undo branch %s: %v", name, err)
			}
			notes = append(notes, "branch "+name+" removed")
		}
	}
	// 3. File effects, newest first.
	for i := len(e.Effects) - 1; i >= 0; i-- {
		fx := e.Effects[i]
		current, rerr := os.ReadFile(fx.Path)
		if fx.Existed {
			if rerr != nil {
				return "", fmt.Errorf("cannot undo: %s was deleted since; restore it by hand", fx.Path)
			}
			if fx.After != nil && !bytes.Equal(current, fx.After) {
				return "", fmt.Errorf("cannot undo: %s changed since; your edits are kept, nothing reverted", fx.Path)
			}
			mode := fx.Mode
			if mode == 0 {
				mode = 0o644
			}
			if err := os.WriteFile(fx.Path, fx.Before, mode.Perm()); err != nil {
				return "", fmt.Errorf("cannot undo: %v", err)
			}
			notes = append(notes, "restored "+fx.Path)
		} else {
			if rerr == nil && fx.After != nil && !bytes.Equal(current, fx.After) {
				return "", fmt.Errorf("cannot undo: %s changed since it was created; kept", fx.Path)
			}
			if rerr == nil {
				if err := os.Remove(fx.Path); err != nil {
					return "", fmt.Errorf("cannot undo: %v", err)
				}
				notes = append(notes, "removed "+fx.Path)
			}
		}
	}
	if len(notes) == 0 {
		notes = append(notes, "nothing to revert (read-only action)")
	}
	if err := markUndone(entries, idx); err != nil {
		return "", err
	}
	return "undid " + e.Kind + " " + e.Name + ": " + joinNotes(notes), nil
}

// RedoEntry returns the most recent undone entry for re-execution.
// The caller runs it through the normal path, which appends a fresh
// entry — history is preserved, never rewritten.
func RedoEntry() (*Entry, error) {
	entries, err := Read()
	if err != nil {
		return nil, err
	}
	for i := len(entries) - 1; i >= 0; i-- {
		if entries[i].Undone {
			e := entries[i]
			e.Undone = false
			return &e, nil
		}
	}
	return nil, fmt.Errorf("nothing to redo")
}

func joinNotes(notes []string) string {
	out := ""
	for i, n := range notes {
		if i > 0 {
			out += "; "
		}
		out += n
	}
	return out
}
