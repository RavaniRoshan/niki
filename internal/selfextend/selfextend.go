// Package selfextend implements the self-extension protocol (T5):
// the gated pipeline by which the harness grows a new capability.
// Every step is fail-closed and the whole pipeline defaults OFF:
// nothing reaches a live directory without passing detect, draft,
// offline test, permission declaration, proposal, and an explicit
// gate approval with provenance recorded.
package selfextend

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// StagingDir is where skill drafts live before enablement.
func StagingDir(home string) string { return filepath.Join(home, "staging") }

// LiveDir is where enabled skills live.
func LiveDir(home string) string { return filepath.Join(home, "skills") }

// GapReason names why a capability gap was detected (T5.1).
type GapReason string

const (
	GapUserAsked      GapReason = "user-asked"
	GapRepeatedManual GapReason = "repeated-manual"
	GapScriptableFail GapReason = "scriptable-failure"
)

// Gap is one detected capability gap.
type Gap struct {
	Reason GapReason `json:"reason"`
	Detail string    `json:"detail"`
	Time   time.Time `json:"time"`
}

// LogGap records a detection with its reason (T5.1: no silent
// detection — the reason is always in the log).
func LogGap(home string, reason GapReason, detail string) error {
	if reason != GapUserAsked && reason != GapRepeatedManual && reason != GapScriptableFail {
		return fmt.Errorf("unknown gap reason %q", reason)
	}
	if err := os.MkdirAll(StagingDir(home), 0o755); err != nil {
		return err
	}
	g := Gap{Reason: reason, Detail: detail, Time: time.Now().UTC()}
	line, err := json.Marshal(g)
	if err != nil {
		return err
	}
	f, err := os.OpenFile(filepath.Join(StagingDir(home), "detections.jsonl"), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return err
	}
	defer f.Close()
	_, err = f.Write(append(line, '\n'))
	return err
}

// ReadGaps returns all logged detections.
func ReadGaps(home string) ([]Gap, error) {
	data, err := os.ReadFile(filepath.Join(StagingDir(home), "detections.jsonl"))
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, err
	}
	var out []Gap
	for _, ln := range strings.Split(strings.TrimRight(string(data), "\n"), "\n") {
		var g Gap
		if err := json.Unmarshal([]byte(ln), &g); err != nil {
			return nil, fmt.Errorf("corrupt detection log: %w", err)
		}
		out = append(out, g)
	}
	return out, nil
}

// Draft writes a skill draft to the staging area (T5.2): never to a
// live directory. Returns the staging path.
func Draft(home, id, skillMD, script string) (string, error) {
	if id == "" || strings.Contains(id, "/") || strings.Contains(id, ".") && strings.HasPrefix(id, ".") {
		return "", fmt.Errorf("bad skill id %q", id)
	}
	dir := filepath.Join(StagingDir(home), id)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	if err := os.WriteFile(filepath.Join(dir, "SKILL.md"), []byte(skillMD), 0o644); err != nil {
		return "", err
	}
	if script != "" {
		if err := os.WriteFile(filepath.Join(dir, "run.sh"), []byte(script), 0o755); err != nil {
			return "", err
		}
	}
	return dir, nil
}

// LiveSkillID reports whether a discovered skill path is a live
// self-extended skill, returning its id. Used by `niki skills` to
// show provenance state (T5.7).
func LiveSkillID(home, skillPath string) (string, bool) {
	rel, err := filepath.Rel(LiveDir(home), skillPath)
	if err != nil || strings.HasPrefix(rel, "..") {
		return "", false
	}
	// Live layout is <live>/<id>/SKILL.md.
	parts := strings.Split(rel, string(filepath.Separator))
	if len(parts) < 2 {
		return "", false
	}
	return parts[0], true
}

// Hash returns the hex SHA-256 of content.
func Hash(content []byte) string {
	h := sha256.Sum256(content)
	return hex.EncodeToString(h[:])
}

// DirHash hashes every file under dir (names + contents), so tests
// can prove live directories are untouched by drafting (T5.2).
func DirHash(dir string) string {
	h := sha256.New()
	_ = filepath.WalkDir(dir, func(path string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil
		}
		rel, _ := filepath.Rel(dir, path)
		h.Write([]byte(rel))
		data, rerr := os.ReadFile(path)
		if rerr == nil {
			h.Write(data)
		}
		return nil
	})
	return hex.EncodeToString(h.Sum(nil))
}
