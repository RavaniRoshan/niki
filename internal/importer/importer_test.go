package importer

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// The import is proven on fixtures: preview contents, the skip
// report, apply, overwrite protection with backup, and reversibility.

// claudeFixture builds a Claude Code project layout. The wording is
// deliberately distinctive so the tests can prove it is not copied.
func claudeFixture(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	write := func(rel, data string) {
		p := filepath.Join(dir, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(data), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write("CLAUDE.md", "# Claude instructions\n\nALWAYS run the zanzibar build before committing.\n")
	write(".claude/settings.json", `{
  "model": "claude-sonnet-4-5",
  "permissions": {"defaultMode": "ask"},
  "env": {"ANTHROPIC_API_KEY": "sk-secret"},
  "mcpServers": {"fs": {"command": "npx", "args": ["-y", "@mcp/fs", "."]}}
}`)
	write(".claude/skills/review/SKILL.md", "---\nname: review\ndescription: review the diff\n---\n\nRead the diff and report findings.\n")
	write(".claude/commands/ship.md", "---\nname: ship\ndescription: prepare a release\n---\n\nRun the release checklist for $ARGUMENTS.\n")
	return dir
}

func codexFiixture(t *testing.T) string {
	t.Helper()
	dir := t.TempDir()
	write := func(rel, data string) {
		p := filepath.Join(dir, rel)
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte(data), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	write(".codex/AGENTS.md", "# Agent notes\n\nPrefer small diffs and explain the why.\n")
	write("AGENTS.md", "# project instructions\n\nRun the tests.\n")
	write(".codex/config.toml", "model = \"gpt-5-codex\"\napproval_policy = \"never\"\nsandbox_mode = \"workspace\"\n")
	return dir
}

func TestDetect(t *testing.T) {
	if got := Detect(claudeFixture(t)); got != SourceClaudeCode {
		t.Fatalf("claude fixture detected as %q", got)
	}
	if got := Detect(codexFiixture(t)); got != SourceCodex {
		t.Fatalf("codex fixture detected as %q", got)
	}
	if got := Detect(t.TempDir()); got != SourceNone {
		t.Fatalf("empty dir detected as %q", got)
	}
}

func hasChange(p Plan, path string) bool {
	for _, c := range p.Changes {
		if c.Path == path {
			return true
		}
	}
	return false
}

func hasSkip(p Plan, key string) (Skip, bool) {
	for _, s := range p.Skips {
		if s.Key == key {
			return s, true
		}
	}
	return Skip{}, false
}

func TestPreviewClaudeCode(t *testing.T) {
	dir := claudeFixture(t)
	plan, err := Preview(dir, SourceClaudeCode)
	if err != nil {
		t.Fatal(err)
	}
	// The mapped files.
	for _, want := range []string{
		filepath.Join(dir, "AGENTS.md"),
		filepath.Join(dir, "nikicode.toml"),
		filepath.Join(dir, ".agents", "skills", "review", "SKILL.md"),
		filepath.Join(dir, ".nikicode", "commands", "ship.md"),
	} {
		if !hasChange(plan, want) {
			t.Fatalf("preview missing change for %s:\n%+v", want, plan.Changes)
		}
	}
	// Unknown keys are skipped with a report; mcpServers is the one
	// nested key NikiCode has an equivalent for, so it is mapped.
	for _, key := range []string{"env"} {
		sk, ok := hasSkip(plan, key)
		if !ok {
			t.Fatalf("preview missing skip report for %q: %+v", key, plan.Skips)
		}
		if sk.Reason == "" {
			t.Fatalf("skip %q has no reason", key)
		}
	}
	if _, ok := hasSkip(plan, "mcpServers"); ok {
		t.Fatal("mcpServers maps to NikiCode's MCP config; it must not be skipped")
	}
	// The MCP server itself IS mapped (it lives under mcpServers,
	// which is the one nested key NikiCode has an equivalent for).
	if !strings.Contains(plan.Changes[1].Detail, "mcp servers: fs") {
		t.Fatalf("config change must map the mcp server: %q", plan.Changes[1].Detail)
	}
}

func TestPreviewCodex(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	dir := codexFiixture(t)
	plan, err := Preview(dir, SourceCodex)
	if err != nil {
		t.Fatal(err)
	}
	if !hasChange(plan, filepath.Join(home, ".nikicode", "AGENTS.md")) {
		t.Fatalf("codex global AGENTS.md must map to NikiCode's global location: %+v", plan.Changes)
	}
	if !hasChange(plan, filepath.Join(dir, "nikicode.toml")) {
		t.Fatalf("codex config must map: %+v", plan.Changes)
	}
	// Unknown codex keys are skipped with a report.
	if _, ok := hasSkip(plan, "sandbox_mode"); !ok {
		t.Fatalf("unknown codex key not reported: %+v", plan.Skips)
	}
	// The project AGENTS.md is NikiCode's own format: nothing to do.
	if _, ok := hasSkip(plan, "project AGENTS.md"); !ok {
		t.Fatalf("project AGENTS.md must be reported as already compatible: %+v", plan.Skips)
	}
}

func TestApplyAndUndoClaudeCode(t *testing.T) {
	dir := claudeFixture(t)
	plan, err := Preview(dir, SourceClaudeCode)
	if err != nil {
		t.Fatal(err)
	}
	res, err := Apply(plan)
	if err != nil {
		t.Fatal(err)
	}

	// The mapped files exist with the mapped content.
	agents := filepath.Join(dir, "AGENTS.md")
	data, err := os.ReadFile(agents)
	if err != nil {
		t.Fatalf("AGENTS.md not written: %v", err)
	}
	if strings.Contains(string(data), "zanzibar") {
		t.Fatal("the upstream instruction wording must NOT be copied into AGENTS.md")
	}
	if !strings.Contains(string(data), "nikicode import") {
		t.Fatalf("AGENTS.md must say where it came from:\n%s", data)
	}
	// The upstream file is left in place.
	if _, err := os.Stat(filepath.Join(dir, "CLAUDE.md")); err != nil {
		t.Fatalf("the upstream CLAUDE.md must be left in place: %v", err)
	}

	// Skill and command content carried over.
	skill, err := os.ReadFile(filepath.Join(dir, ".agents", "skills", "review", "SKILL.md"))
	if err != nil {
		t.Fatalf("skill not carried over: %v", err)
	}
	if !strings.Contains(string(skill), "review the diff") {
		t.Fatalf("skill content missing:\n%s", skill)
	}
	cmd, err := os.ReadFile(filepath.Join(dir, ".nikicode", "commands", "ship.md"))
	if err != nil {
		t.Fatalf("command not carried over: %v", err)
	}
	if !strings.Contains(string(cmd), "release checklist") {
		t.Fatalf("command content missing:\n%s", cmd)
	}

	// Config: mapped keys, no secret, no unknown keys.
	cfg, err := os.ReadFile(filepath.Join(dir, "nikicode.toml"))
	if err != nil {
		t.Fatalf("config not written: %v", err)
	}
	for _, want := range []string{"[model]", `name = "claude-sonnet-4-5"`, "[permissions]", `mode = "workspace_write"`, "[mcp.servers.fs]", `command = "npx"`} {
		if !strings.Contains(string(cfg), want) {
			t.Fatalf("config missing %q:\n%s", want, cfg)
		}
	}
	if strings.Contains(string(cfg), "sk-secret") {
		t.Fatal("the secret env value must never be written into the config")
	}
	if strings.Contains(string(cfg), "env") && strings.Contains(string(cfg), "ANTHROPIC") {
		t.Fatal("skipped keys must not appear in the config")
	}

	// Undo removes everything the import created.
	if err := Undo(res.Journal); err != nil {
		t.Fatal(err)
	}
	for _, p := range []string{
		filepath.Join(dir, "AGENTS.md"),
		filepath.Join(dir, "nikicode.toml"),
		filepath.Join(dir, ".agents", "skills", "review", "SKILL.md"),
		filepath.Join(dir, ".nikicode", "commands", "ship.md"),
		res.Journal,
	} {
		if _, err := os.Stat(p); !os.IsNotExist(err) {
			t.Fatalf("undo left %s behind (err=%v)", p, err)
		}
	}
	// The upstream files are untouched.
	for _, p := range []string{"CLAUDE.md", ".claude/settings.json", ".claude/skills/review/SKILL.md"} {
		if _, err := os.Stat(filepath.Join(dir, p)); err != nil {
			t.Fatalf("undo touched the upstream %s: %v", p, err)
		}
	}
}

func TestApplyNeverOverwritesWithoutBackup(t *testing.T) {
	dir := claudeFixture(t)
	// A pre-existing NikiCode config with the user's own model.
	if err := os.WriteFile(filepath.Join(dir, "nikicode.toml"), []byte("[model]\nname = \"my-model\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	plan, err := Preview(dir, SourceClaudeCode)
	if err != nil {
		t.Fatal(err)
	}
	res, err := Apply(plan)
	if err != nil {
		t.Fatal(err)
	}
	// The existing section is kept, not redefined.
	cfg, err := os.ReadFile(filepath.Join(dir, "nikicode.toml"))
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(cfg), `name = "my-model"`) {
		t.Fatalf("the existing model value must survive the merge:\n%s", cfg)
	}
	if strings.Contains(string(cfg), `name = "claude-sonnet-4-5"`) {
		t.Fatalf("the merge must not redefine an existing section:\n%s", cfg)
	}
	// The original is backed up, and undo restores it byte-identical.
	if len(res.Backups) == 0 {
		t.Fatal("replacing an existing file must back it up")
	}
	if err := Undo(res.Journal); err != nil {
		t.Fatal(err)
	}
	restored, err := os.ReadFile(filepath.Join(dir, "nikicode.toml"))
	if err != nil {
		t.Fatal(err)
	}
	if string(restored) != "[model]\nname = \"my-model\"\n" {
		t.Fatalf("undo must restore the original config byte-identical, got:\n%s", restored)
	}
}

func TestApplyKeepsExistingAgentsMD(t *testing.T) {
	dir := claudeFixture(t)
	if err := os.WriteFile(filepath.Join(dir, "AGENTS.md"), []byte("# my own instructions\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	plan, err := Preview(dir, SourceClaudeCode)
	if err != nil {
		t.Fatal(err)
	}
	if hasChange(plan, filepath.Join(dir, "AGENTS.md")) {
		t.Fatal("an existing AGENTS.md must not be in the plan's changes")
	}
	if _, ok := hasSkip(plan, "CLAUDE.md"); !ok {
		t.Fatal("the kept AGENTS.md must be reported as a skip")
	}
	res, err := Apply(plan)
	if err != nil {
		t.Fatal(err)
	}
	data, err := os.ReadFile(filepath.Join(dir, "AGENTS.md"))
	if err != nil {
		t.Fatal(err)
	}
	if string(data) != "# my own instructions\n" {
		t.Fatalf("the existing AGENTS.md must be untouched:\n%s", data)
	}
	_ = Undo(res.Journal)
}

func TestApplyCodex(t *testing.T) {
	home := t.TempDir()
	t.Setenv("HOME", home)
	dir := codexFiixture(t)
	plan, err := Preview(dir, SourceCodex)
	if err != nil {
		t.Fatal(err)
	}
	res, err := Apply(plan)
	if err != nil {
		t.Fatal(err)
	}
	global, err := os.ReadFile(filepath.Join(home, ".nikicode", "AGENTS.md"))
	if err != nil {
		t.Fatalf("global AGENTS.md not written: %v", err)
	}
	if !strings.Contains(string(global), "small diffs") {
		t.Fatalf("the user's own AGENTS.md content must carry over:\n%s", global)
	}
	cfg, err := os.ReadFile(filepath.Join(dir, "nikicode.toml"))
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{`name = "gpt-5-codex"`, `mode = "full_access"`} {
		if !strings.Contains(string(cfg), want) {
			t.Fatalf("config missing %q:\n%s", want, cfg)
		}
	}
	if strings.Contains(string(cfg), "sandbox_mode") {
		t.Fatalf("the unknown key must not reach the config:\n%s", cfg)
	}
	// Undo removes the imported file and the created config.
	if err := Undo(res.Journal); err != nil {
		t.Fatal(err)
	}
	if _, err := os.Stat(filepath.Join(home, ".nikicode", "AGENTS.md")); !os.IsNotExist(err) {
		t.Fatal("undo must remove the imported global AGENTS.md")
	}
}
