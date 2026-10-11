package selfextend

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// Scope is an enablement scope (T5.6).
type Scope string

const (
	ScopeOnce    Scope = "once"
	ScopeSession Scope = "session"
	ScopeProject Scope = "project"
	ScopeGlobal  Scope = "global"
)

// Proposal is the rendered gate package (T5.5): SKILL.md, script,
// test result, declared permissions, and any diff against an
// existing skill with the same name.
type Proposal struct {
	ID         string
	SkillMD    string
	Script     string
	Test       TestResult
	Decl       Declared
	DiffVsLive string
}

// RenderProposal builds the TUI proposal text. It renders only —
// enabling requires Gate with explicit approvals (T5.5: nothing
// enables without a gate).
func RenderProposal(home, id string, test TestResult) (Proposal, error) {
	dir := filepath.Join(StagingDir(home), id)
	skillMD, err := os.ReadFile(filepath.Join(dir, "SKILL.md"))
	if err != nil {
		return Proposal{}, err
	}
	script, _ := os.ReadFile(filepath.Join(dir, "run.sh"))
	decl, err := LoadDeclared(dir)
	if err != nil {
		return Proposal{}, err
	}
	p := Proposal{ID: id, SkillMD: string(skillMD), Script: string(script), Test: test, Decl: decl}
	live := filepath.Join(LiveDir(home), id, "SKILL.md")
	if old, err := os.ReadFile(live); err == nil && string(old) != p.SkillMD {
		p.DiffVsLive = "existing skill differs (update would replace " + Hash(old)[:12] + " with " + Hash(skillMD)[:12] + ")"
	}
	return p, nil
}

// Text renders the proposal for display.
func (p Proposal) Text() string {
	var b strings.Builder
	fmt.Fprintf(&b, "Skill proposal: %s\n\n--- SKILL.md ---\n%s\n", p.ID, p.SkillMD)
	if p.Script != "" {
		fmt.Fprintf(&b, "\n--- run.sh ---\n%s\n", p.Script)
	}
	status := "FAIL"
	if p.Test.Passed {
		status = "PASS"
	}
	fmt.Fprintf(&b, "\n--- offline test: %s ---\n%s\n", status, p.Test.Output)
	fmt.Fprintf(&b, "\n--- declared permissions ---\ntools=%v paths=%v network=%s secrets=%s\n",
		p.Decl.Tools, p.Decl.Paths, p.Decl.Network, p.Decl.Secrets)
	if p.DiffVsLive != "" {
		fmt.Fprintf(&b, "\n--- diff vs live ---\n%s\n", p.DiffVsLive)
	}
	return b.String()
}

// Gate decides enablement (T5.6). approvedScope is the user's scope
// approval; approvedSensitive is the SECOND explicit approval
// required when the declaration needs it (network, outside writes,
// secrets). Without both, nothing enables; denial deletes staging.
func Gate(home, id string, scope Scope, approvedScope, approvedSensitive bool) error {
	dir := filepath.Join(StagingDir(home), id)
	decl, err := LoadDeclared(dir)
	if err != nil {
		return err
	}
	validScope := scope == ScopeOnce || scope == ScopeSession || scope == ScopeProject || scope == ScopeGlobal
	denied := func(reason string) error {
		_ = os.RemoveAll(dir)
		return fmt.Errorf("denied (%s): staging deleted", reason)
	}
	if !validScope {
		return denied("bad scope")
	}
	if !approvedScope {
		return denied("scope not approved")
	}
	if decl.NeedsSecondApproval() && !approvedSensitive {
		return denied("sensitive permissions need a second explicit approval")
	}
	return nil
}

// Provenance records a skill's origin (T5.7).
type Provenance struct {
	Creator     string `json:"creator"`
	SessionID   string `json:"session_id"`
	PromptHash  string `json:"prompt_hash"`
	Timestamp   string `json:"timestamp"`
	ContentHash string `json:"content_hash"`
	Scope       string `json:"scope"`
}

// Enable copies staging to the live directory with provenance
// (T5.7). Call only after Gate returns nil.
func Enable(home, id, creator, sessionID, prompt string, scope Scope) error {
	src := filepath.Join(StagingDir(home), id)
	skillMD, err := os.ReadFile(filepath.Join(src, "SKILL.md"))
	if err != nil {
		return err
	}
	dst := filepath.Join(LiveDir(home), id)
	if err := os.MkdirAll(dst, 0o755); err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(dst, "SKILL.md"), skillMD, 0o644); err != nil {
		return err
	}
	if script, err := os.ReadFile(filepath.Join(src, "run.sh")); err == nil {
		if err := os.WriteFile(filepath.Join(dst, "run.sh"), script, 0o755); err != nil {
			return err
		}
	}
	prov := Provenance{
		Creator: creator, SessionID: sessionID,
		PromptHash: Hash([]byte(prompt)), Timestamp: time.Now().UTC().Format(time.RFC3339),
		ContentHash: Hash(skillMD), Scope: string(scope),
	}
	data, err := json.MarshalIndent(prov, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(dst, "PROVENANCE.json"), data, 0o644)
}

// Verify checks a live skill: editing the content changes the hash,
// which disables the skill with a warning (T5.7).
func Verify(home, id string) (ok bool, warning string) {
	dst := filepath.Join(LiveDir(home), id)
	skillMD, err := os.ReadFile(filepath.Join(dst, "SKILL.md"))
	if err != nil {
		return false, "skill missing"
	}
	if _, err := os.Stat(filepath.Join(dst, "DISABLED")); err == nil {
		return false, "skill disabled"
	}
	data, err := os.ReadFile(filepath.Join(dst, "PROVENANCE.json"))
	if err != nil {
		return false, "provenance missing: skill disabled, re-enable through the gate"
	}
	var prov Provenance
	if err := json.Unmarshal(data, &prov); err != nil {
		return false, "provenance corrupt: skill disabled, re-enable through the gate"
	}
	if prov.ContentHash != Hash(skillMD) {
		_ = os.WriteFile(filepath.Join(dst, "DISABLED"), []byte("content hash mismatch"), 0o644)
		return false, "WARNING: skill content changed since enablement (hash mismatch) — skill disabled, re-enable through the gate"
	}
	return true, ""
}

// RecordRun appends to the skill's audit log (T5.8). Three
// consecutive errors auto-disable it with a notice.
func RecordRun(home, id string, runErr error) (disabled bool, notice string) {
	dst := filepath.Join(LiveDir(home), id)
	logPath := filepath.Join(dst, "audit.jsonl")
	entry, _ := json.Marshal(map[string]any{
		"time": time.Now().UTC().Format(time.RFC3339),
		"ok":   runErr == nil,
		"err":  errStr(runErr),
	})
	f, err := os.OpenFile(logPath, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o644)
	if err != nil {
		return false, ""
	}
	_, _ = f.Write(append(entry, '\n'))
	_ = f.Close()

	// Count trailing errors.
	data, _ := os.ReadFile(logPath)
	lines := strings.Split(strings.TrimRight(string(data), "\n"), "\n")
	streak := 0
	for i := len(lines) - 1; i >= 0; i-- {
		var e map[string]any
		if json.Unmarshal([]byte(lines[i]), &e) != nil {
			break
		}
		if ok, _ := e["ok"].(bool); ok {
			break
		}
		streak++
	}
	if streak >= 3 {
		_ = os.WriteFile(filepath.Join(dst, "DISABLED"), []byte("three consecutive errors"), 0o644)
		return true, "NOTICE: skill " + id + " auto-disabled after 3 consecutive errors"
	}
	return false, ""
}

func errStr(err error) string {
	if err == nil {
		return ""
	}
	return err.Error()
}

// Retire removes the live directory and provenance with no residue
// (T5.9).
func Retire(home, id string) error {
	return os.RemoveAll(filepath.Join(LiveDir(home), id))
}
