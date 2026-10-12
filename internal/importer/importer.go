// Package importer brings a setup from another coding agent into
// NikiCode's shape: `nikicode import --from claude-code|codex`.
//
// The rule is MAP, NEVER COPY: the source's shape is read, and
// NikiCode's own files are written. Another product's instruction
// wording is never reproduced verbatim; the user's own content
// (skills, custom commands, an AGENTS.md written in the shared
// convention) is carried into NikiCode's locations. Unknown keys
// are skipped with a report, an existing NikiCode file is never
// overwritten without a backup, and every import is journalled so
// it can be undone.
package importer

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

// Source names an importable upstream layout.
type Source string

const (
	SourceNone       Source = ""
	SourceClaudeCode Source = "claude-code"
	SourceCodex      Source = "codex"
)

// Detect reports which importable project layout exists at dir.
// A plain AGENTS.md alone is not Codex: NikiCode reads AGENTS.md
// natively, so there is nothing to import.
func Detect(dir string) Source {
	if fileExists(filepath.Join(dir, "CLAUDE.md")) || dirExists(filepath.Join(dir, ".claude")) {
		return SourceClaudeCode
	}
	if dirExists(filepath.Join(dir, ".codex")) {
		return SourceCodex
	}
	if home := os.Getenv("CODEX_HOME"); home != "" {
		if fileExists(filepath.Join(home, "config.toml")) || fileExists(filepath.Join(home, "AGENTS.md")) {
			return SourceCodex
		}
	}
	return SourceNone
}

// Change is one file-level action in the plan.
type Change struct {
	// Path is the NikiCode-side file.
	Path string
	// Kind is "create", "merge", or "copy".
	Kind string
	// Detail is what will be written, for the preview.
	Detail string
	// From is the source file this maps from ("" for generated
	// files).
	From string
}

// Skip is one source item deliberately not imported.
type Skip struct {
	Key    string
	From   string
	Reason string
}

// Plan is the full preview of an import: what will be created,
// merged or copied, and what will be skipped, with reasons.
type Plan struct {
	Source  Source
	Dir     string
	Changes []Change
	Skips   []Skip
}

// server is one mapped MCP server.
type server struct {
	Command string
	Args    []string
}

// mapping is the config-level result of reading a source layout.
type mapping struct {
	settings map[string]string
	servers  map[string]server
	skips    []Skip
}

// Backup records a replaced original.
type Backup struct {
	Original string `json:"original"`
	Backup   string `json:"backup"`
}

// journal is the undo record written beside the import.
type journal struct {
	Source  Source    `json:"source"`
	Dir     string    `json:"dir"`
	When    time.Time `json:"when"`
	Created []string  `json:"created"`
	Backups []Backup  `json:"backups"`
}

// Result is what Apply did.
type Result struct {
	Plan    Plan
	Written []string
	Backups []Backup
	Journal string
}

// Preview reads the source layout at dir and maps it to NikiCode's
// shape without writing anything.
func Preview(dir string, src Source) (Plan, error) {
	plan := Plan{Source: src, Dir: dir}
	switch src {
	case SourceClaudeCode:
		return previewClaudeCode(plan)
	case SourceCodex:
		return previewCodex(plan)
	default:
		return plan, fmt.Errorf("unknown import source %q", src)
	}
}

func previewClaudeCode(plan Plan) (Plan, error) {
	dir := plan.Dir

	// Instructions: CLAUDE.md maps to NikiCode's AGENTS.md chain.
	// Their wording is never reproduced; the mapped file says what
	// happened and what to do. An existing AGENTS.md is kept.
	if fileExists(filepath.Join(dir, "CLAUDE.md")) {
		target := filepath.Join(dir, "AGENTS.md")
		if fileExists(target) {
			plan.Skips = append(plan.Skips, Skip{
				Key: "CLAUDE.md", From: "CLAUDE.md",
				Reason: "AGENTS.md already exists and is NikiCode's own format; it was kept unchanged",
			})
		} else {
			data, _ := os.ReadFile(filepath.Join(dir, "CLAUDE.md"))
			plan.Changes = append(plan.Changes, Change{
				Path: target, Kind: "create", From: "CLAUDE.md",
				Detail: fmt.Sprintf("AGENTS.md mapping note (%d bytes of CLAUDE.md reviewed; its wording is not copied — fold the rules in yourself)", len(data)),
			})
		}
	}

	// Settings and MCP servers: map the known keys, skip the unknown
	// with a report.
	cfgTarget := filepath.Join(dir, "nikicode.toml")
	var sources []string
	if p := filepath.Join(dir, ".claude", "settings.json"); fileExists(p) {
		sources = append(sources, p)
	}
	if p := filepath.Join(dir, ".claude", ".mcp.json"); fileExists(p) {
		sources = append(sources, p)
	}
	if len(sources) > 0 {
		m := mapClaudeSources(sources...)
		plan.Skips = append(plan.Skips, m.skips...)
		plan.Changes = append(plan.Changes, Change{
			Path: cfgTarget, Kind: "merge", From: strings.Join(sources, " + "),
			Detail: m.summary(),
		})
	}

	// Skills: the user's own content, into NikiCode's project
	// skills root.
	skillsSrc := filepath.Join(dir, ".claude", "skills")
	entries, err := os.ReadDir(skillsSrc)
	if err == nil {
		for _, e := range entries {
			if !e.IsDir() {
				continue
			}
			src := filepath.Join(skillsSrc, e.Name(), "SKILL.md")
			dst := filepath.Join(dir, ".agents", "skills", e.Name(), "SKILL.md")
			if fileExists(dst) {
				plan.Skips = append(plan.Skips, Skip{
					Key: "skill " + e.Name(), From: src,
					Reason: "a skill with that name already exists in NikiCode's root; kept unchanged",
				})
				continue
			}
			if !fileExists(src) {
				plan.Skips = append(plan.Skips, Skip{
					Key: "skill " + e.Name(), From: src,
					Reason: "no SKILL.md in the skill directory",
				})
				continue
			}
			plan.Changes = append(plan.Changes, Change{
				Path: dst, Kind: "copy", From: src,
				Detail: "your skill content, into NikiCode's project skills root",
			})
		}
	}

	// Custom commands: .claude/commands/*.md into NikiCode's
	// project command directory (the same frontmatter shape).
	cmdsSrc := filepath.Join(dir, ".claude", "commands")
	cmdEntries, err := os.ReadDir(cmdsSrc)
	if err == nil {
		for _, e := range cmdEntries {
			if e.IsDir() || !strings.HasSuffix(strings.ToLower(e.Name()), ".md") {
				continue
			}
			src := filepath.Join(cmdsSrc, e.Name())
			dst := filepath.Join(dir, ".nikicode", "commands", e.Name())
			if fileExists(dst) {
				plan.Skips = append(plan.Skips, Skip{
					Key: "command " + e.Name(), From: src,
					Reason: "a command with that name already exists; kept unchanged",
				})
				continue
			}
			plan.Changes = append(plan.Changes, Change{
				Path: dst, Kind: "copy", From: src,
				Detail: "your command, into NikiCode's project command directory",
			})
		}
	}
	return plan, nil
}

// mapClaudeSources maps the known keys of the given Claude Code
// settings/MCP files; everything else is skipped with a reason.
func mapClaudeSources(paths ...string) mapping {
	m := mapping{settings: map[string]string{}, servers: map[string]server{}}
	for _, p := range paths {
		data, err := os.ReadFile(p)
		if err != nil {
			continue
		}
		var doc map[string]any
		if err := json.Unmarshal(data, &doc); err != nil {
			m.skips = append(m.skips, Skip{Key: filepath.Base(p), From: p, Reason: "unreadable JSON: " + err.Error()})
			continue
		}
		for k, v := range doc {
			switch k {
			case "model":
				if s, ok := v.(string); ok {
					m.settings["model.name"] = s
				}
			case "permissions":
				if obj, ok := v.(map[string]any); ok {
					if mode, ok := obj["defaultMode"].(string); ok {
						m.settings["permissions.mode"] = normalizePermMode(mode)
					}
				}
			case "mcpServers":
				servers, ok := v.(map[string]any)
				if !ok {
					continue
				}
				for name, raw := range servers {
					obj, ok := raw.(map[string]any)
					if !ok {
						m.skips = append(m.skips, Skip{Key: "mcp server " + name, From: p, Reason: "not an object"})
						continue
					}
					cmd, _ := obj["command"].(string)
					var args []string
					if rawArgs, ok := obj["args"].([]any); ok {
						for _, a := range rawArgs {
							if s, ok := a.(string); ok {
								args = append(args, s)
							}
						}
					}
					m.servers[name] = server{Command: cmd, Args: args}
				}
			default:
				m.skips = append(m.skips, Skip{
					Key: k, From: p,
					Reason: "no NikiCode equivalent; set it by hand if you need it",
				})
			}
		}
	}
	return m
}

// summary is the preview line for a config merge.
func (m mapping) summary() string {
	var parts []string
	if len(m.settings) > 0 {
		keys := make([]string, 0, len(m.settings))
		for k := range m.settings {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		parts = append(parts, "settings: "+strings.Join(keys, ", "))
	}
	if len(m.servers) > 0 {
		names := make([]string, 0, len(m.servers))
		for k := range m.servers {
			names = append(names, k)
		}
		sort.Strings(names)
		parts = append(parts, "mcp servers: "+strings.Join(names, ", "))
	}
	if len(parts) == 0 {
		return "nothing mappable"
	}
	return strings.Join(parts, " · ")
}

// normalizePermMode maps the upstream mode names (Claude Code's
// defaultMode and Codex's approval_policy) to NikiCode's.
func normalizePermMode(mode string) string {
	switch strings.ToLower(mode) {
	case "default", "ask", "askpermission":
		return "workspace_write"
	case "acceptprompts", "accept_prompts", "auto":
		return "manual"
	case "bypasspermissions", "bypass_permissions", "yolo", "never":
		return "full_access"
	case "plan", "untrusted", "readonly":
		return "readonly"
	case "on-request", "on_request", "onrequest":
		return "workspace_write"
	default:
		return strings.ToLower(mode)
	}
}

func previewCodex(plan Plan) (Plan, error) {
	dir := plan.Dir

	// The global AGENTS.md (Codex's own chain file, a shared
	// convention) maps to NikiCode's global instruction file.
	codexHome := os.Getenv("CODEX_HOME")
	if codexHome == "" {
		codexHome = filepath.Join(dir, ".codex")
	}
	globalSrc := filepath.Join(codexHome, "AGENTS.md")
	if fileExists(globalSrc) {
		global := ""
		if home := os.Getenv("HOME"); home != "" {
			global = filepath.Join(home, ".nikicode", "AGENTS.md")
		}
		if global == "" || fileExists(global) {
			plan.Skips = append(plan.Skips, Skip{
				Key: "global AGENTS.md", From: globalSrc,
				Reason: "NikiCode's global instruction file already exists; kept unchanged",
			})
		} else {
			plan.Changes = append(plan.Changes, Change{
				Path: global, Kind: "copy", From: globalSrc,
				Detail: "your AGENTS.md, into NikiCode's global instruction location",
			})
		}
	}

	// The project's AGENTS.md is already NikiCode's own format.
	if fileExists(filepath.Join(dir, "AGENTS.md")) {
		plan.Skips = append(plan.Skips, Skip{
			Key: "project AGENTS.md", From: filepath.Join(dir, "AGENTS.md"),
			Reason: "AGENTS.md is NikiCode's own instruction format; nothing to map",
		})
	}

	// CODEX_HOME/config.toml: map the known keys.
	cfgPath := filepath.Join(codexHome, "config.toml")
	if fileExists(cfgPath) {
		data, err := os.ReadFile(cfgPath)
		if err != nil {
			return plan, err
		}
		m := mapCodexConfig(string(data))
		plan.Skips = append(plan.Skips, m.skips...)
		plan.Changes = append(plan.Changes, Change{
			Path: filepath.Join(dir, "nikicode.toml"), Kind: "merge", From: cfgPath,
			Detail: m.summary(),
		})
	}
	return plan, nil
}

// mapCodexConfig maps the known Codex config keys; unknown lines
// are skipped with a report. Line-oriented on purpose: the Codex
// config schema is theirs, and only the mapped keys are read.
func mapCodexConfig(text string) mapping {
	m := mapping{settings: map[string]string{}, servers: map[string]server{}}
	for _, line := range strings.Split(text, "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") || strings.HasPrefix(line, "[") {
			continue
		}
		key, val, ok := strings.Cut(line, "=")
		if !ok {
			continue
		}
		key = strings.TrimSpace(key)
		val = strings.Trim(strings.TrimSpace(val), `"`)
		switch key {
		case "model":
			m.settings["model.name"] = val
		case "approval_policy":
			m.settings["permissions.mode"] = normalizePermMode(val)
		default:
			m.skips = append(m.skips, Skip{
				Key: key, From: "CODEX_HOME/config.toml",
				Reason: "no NikiCode equivalent; set it by hand if you need it",
			})
		}
	}
	return m
}

// Apply writes the plan: created files are journalled, any file
// that must change is backed up first, and nothing is written
// twice.
func Apply(plan Plan) (Result, error) {
	res := Result{Plan: plan}
	cfgPath := filepath.Join(plan.Dir, "nikicode.toml")

	write := func(path string, data []byte) error {
		if fileExists(path) {
			// Never silently replace: back up first.
			bak := path + ".import-backup-" + time.Now().Format("20060102-150405")
			if orig, err := os.ReadFile(path); err == nil {
				if err := os.WriteFile(bak, orig, 0o600); err != nil {
					return err
				}
				res.Backups = append(res.Backups, Backup{Original: path, Backup: bak})
			}
		}
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			return err
		}
		tmp := path + ".import-tmp"
		if err := os.WriteFile(tmp, data, 0o600); err != nil {
			return err
		}
		if err := os.Rename(tmp, path); err != nil {
			return err
		}
		res.Written = append(res.Written, path)
		return nil
	}

	// Instruction files and copies first, the config merge last.
	for _, ch := range plan.Changes {
		if strings.HasSuffix(ch.Path, "nikicode.toml") {
			continue
		}
		if ch.Kind == "copy" {
			data, err := os.ReadFile(ch.From)
			if err != nil {
				return res, err
			}
			if err := write(ch.Path, data); err != nil {
				return res, err
			}
			continue
		}
		if err := write(ch.Path, []byte(mappedInstructions(plan))); err != nil {
			return res, err
		}
	}
	for _, ch := range plan.Changes {
		if !strings.HasSuffix(ch.Path, "nikicode.toml") {
			continue
		}
		m := planMapping(plan)
		existing := ""
		if data, err := os.ReadFile(cfgPath); err == nil {
			existing = string(data)
		}
		if err := write(cfgPath, []byte(mergeTOML(existing, m))); err != nil {
			return res, err
		}
	}

	// Journal beside the config, for undo.
	jdir := filepath.Join(plan.Dir, ".nikicode")
	if err := os.MkdirAll(jdir, 0o755); err != nil {
		return res, err
	}
	jpath := filepath.Join(jdir, fmt.Sprintf("import-journal-%s.json", time.Now().Format("20060102-150405")))
	j := journal{Source: plan.Source, Dir: plan.Dir, When: time.Now(), Created: res.Written, Backups: res.Backups}
	if data, err := json.MarshalIndent(j, "", "  "); err == nil {
		if err := os.WriteFile(jpath, data, 0o600); err != nil {
			return res, err
		}
		res.Journal = jpath
	}
	return res, nil
}

// planMapping recomputes the config mapping from the plan's source
// files (deterministic; the preview only reports it).
func planMapping(plan Plan) mapping {
	merged := mapping{settings: map[string]string{}, servers: map[string]server{}}
	for _, ch := range plan.Changes {
		if !strings.HasSuffix(ch.Path, "nikicode.toml") {
			continue
		}
		var m mapping
		switch plan.Source {
		case SourceClaudeCode:
			for _, p := range strings.Split(ch.From, " + ") {
				merged = mergeMappings(merged, mapClaudeSources(p))
			}
		case SourceCodex:
			data, err := os.ReadFile(ch.From)
			if err == nil {
				m = mapCodexConfig(string(data))
			}
		}
		if len(m.settings) > 0 || len(m.servers) > 0 {
			merged = mergeMappings(merged, m)
		}
	}
	return merged
}

func mergeMappings(a, b mapping) mapping {
	for k, v := range b.settings {
		a.settings[k] = v
	}
	for k, v := range b.servers {
		a.servers[k] = v
	}
	a.skips = append(a.skips, b.skips...)
	return a
}

// mappedInstructions is the generated AGENTS.md: NikiCode's shape,
// their wording never reproduced.
func mappedInstructions(plan Plan) string {
	var b strings.Builder
	b.WriteString("# Project instructions\n\n")
	b.WriteString("This project was imported from ")
	b.WriteString(string(plan.Source))
	b.WriteString(" by `nikicode import`.\n\n")
	b.WriteString("NikiCode reads AGENTS.md from the repository root down to the\n")
	b.WriteString("working directory. The upstream instruction file was reviewed\n")
	b.WriteString("and left in place; its rules were NOT copied here (import maps\n")
	b.WriteString("shapes, never wording). Fold the rules you care about into this\n")
	b.WriteString("file, in your own words, and delete this note.\n")
	return b.String()
}

// mergeTOML appends the mapped keys to an existing project config,
// never redefining a section that already exists.
func mergeTOML(existing string, m mapping) string {
	var b strings.Builder
	b.WriteString(existing)
	if existing != "" && !strings.HasSuffix(existing, "\n") {
		b.WriteString("\n")
	}
	if existing != "" {
		b.WriteString("\n# --- imported by nikicode import ---\n")
	} else {
		b.WriteString("# NikiCode project config (written by nikicode import)\n")
	}
	writeSection := func(section string, body func()) {
		if existing != "" && strings.Contains(existing, "["+section+"]") {
			return // the section exists: keep the user's value
		}
		b.WriteString("\n[" + section + "]\n")
		body()
	}
	writeSection("model", func() {
		if v, ok := m.settings["model.name"]; ok {
			fmt.Fprintf(&b, "name = %q\n", v)
		}
	})
	writeSection("permissions", func() {
		if v, ok := m.settings["permissions.mode"]; ok {
			fmt.Fprintf(&b, "mode = %q\n", v)
		}
	})
	for _, name := range sortedServerNames(m.servers) {
		s := m.servers[name]
		writeSection("mcp.servers."+name, func() {
			fmt.Fprintf(&b, "command = %q\n", s.Command)
			if len(s.Args) > 0 {
				quoted := make([]string, len(s.Args))
				for i, a := range s.Args {
					quoted[i] = fmt.Sprintf("%q", a)
				}
				fmt.Fprintf(&b, "args = [%s]\n", strings.Join(quoted, ", "))
			}
		})
	}
	return b.String()
}

func sortedServerNames(m map[string]server) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

// Undo reverses an import from its journal: created files are
// removed (empty directories pruned), backups are restored.
func Undo(journalPath string) error {
	data, err := os.ReadFile(journalPath)
	if err != nil {
		return err
	}
	var j journal
	if err := json.Unmarshal(data, &j); err != nil {
		return err
	}
	for _, p := range j.Created {
		if err := os.Remove(p); err != nil && !os.IsNotExist(err) {
			return err
		}
		pruneEmptyDirs(filepath.Dir(p), j.Dir)
	}
	for _, b := range j.Backups {
		data, err := os.ReadFile(b.Backup)
		if err != nil {
			return err
		}
		if err := os.WriteFile(b.Original, data, 0o600); err != nil {
			return err
		}
		_ = os.Remove(b.Backup)
	}
	_ = os.Remove(journalPath)
	return nil
}

// pruneEmptyDirs removes directories left empty by the undo,
// stopping at stop (never the project root or above).
func pruneEmptyDirs(dir, stop string) {
	for dir != "" && strings.HasPrefix(dir, stop) && dir != stop {
		entries, err := os.ReadDir(dir)
		if err != nil || len(entries) > 0 {
			return
		}
		parent := filepath.Dir(dir)
		_ = os.Remove(dir)
		dir = parent
	}
}

func fileExists(p string) bool {
	fi, err := os.Stat(p)
	return err == nil && !fi.IsDir()
}

func dirExists(p string) bool {
	fi, err := os.Stat(p)
	return err == nil && fi.IsDir()
}
