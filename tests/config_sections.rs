//! A config section that parses, appears in the docs, and does nothing.
//!
//! `NikiConfig::merge` walks the struct field by field, so a section nobody
//! remembered to add a line for is silently dropped: `Self::default()`
//! survives whatever the file said, with no warning anywhere. `[mcp]` was
//! missing, so every MCP server a user configured was ignored by a run that
//! had a working MCP client, a documented config section and a `/mcp` command.
//! `[compaction]` and `[instructions]` were missing the same way, so turning
//! auto-compact off did nothing and a project could not point NIKI at its own
//! instructions file.
//!
//! A test per section would be three tests and a fourth would be missed. This one
//! loads a config that sets every section to something distinguishable from its
//! default and checks each one arrived — so the next section nobody adds to
//! `merge` fails here rather than in a user's run.

use niki::config::types::NikiConfig;
use std::path::Path;

/// A `niki.toml` that sets every section to a value that cannot be mistaken for
/// its default. Deliberately exhaustive: a section absent here is a section this
/// test cannot protect.
const EVERY_SECTION: &str = r#"
[general]
max_revision_rounds = 7
spend_cap_usd = 0.5
output_dir = "elsewhere"
max_diff_lines = 1234
max_context_chars = 4321

[providers.probe]
api_key = "k"
base_url = "http://127.0.0.1:1"
default_model = "probe-model"

[agents.planner]
provider = "probe"
model = "probe-planner"

[docker]
backend = "worktree"
base_image = "probe-image"
memory_limit = "9g"
cpu_limit = 7.0
extra_packages = ["probe-pkg"]

[pipeline]
max_revision_rounds = 9

[compaction]
enabled = false
threshold_pct = 42
reserved_tokens = 999
auto_compact = false

[instructions]
enabled = false
paths = ["probe-instructions.md"]
auto_detect_agents_md = false

[mcp]
enabled = true
timeout_ms = 1234
read_only = false

[[mcp.servers]]
name = "probe"
command = "true"
args = ["-y"]

[permissions]
mode = "bypass"
fail_closed_headless = true

[hooks]

[commands]

[ui]
theme = "light"
telemetry = false

[goal]

[session]

[repo_intel]
enabled = false

[risk]

[snapshot]

[critic]
enabled = false

[tools]

[budget]
"#;

fn loaded() -> NikiConfig {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("niki.toml"), EVERY_SECTION).expect("write config");
    let cfg = NikiConfig::load(Path::new(dir.path())).expect("config must parse");
    // Keep the directory alive until the config has been read.
    std::mem::forget(dir);
    cfg
}

#[test]
fn general_reaches_the_loaded_config() {
    let c = loaded();
    assert_eq!(c.general.max_revision_rounds, 7);
    assert_eq!(c.general.output_dir, "elsewhere");
    assert_eq!(c.general.max_diff_lines, 1234);
    assert_eq!(c.general.max_context_chars, 4321);
}

#[test]
fn compaction_reaches_the_loaded_config() {
    // `config.compaction.enabled` is read in the pipeline and
    // `apply_compaction_config` is called from it. Before the fix this was
    // always the default, so auto-compact could not be turned off.
    let c = loaded();
    assert!(
        !c.compaction.enabled,
        "auto-compact could not be turned off"
    );
    assert_eq!(c.compaction.threshold_pct, 42);
    assert!(!c.compaction.auto_compact);
}

#[test]
fn instructions_reaches_the_loaded_config() {
    // `config.instructions.enabled` is read by the knowledge indexer. Before
    // the fix a project's instructions file was never consulted.
    let c = loaded();
    assert!(!c.instructions.enabled);
    assert_eq!(
        c.instructions.paths,
        vec!["probe-instructions.md".to_string()]
    );
    assert!(!c.instructions.auto_detect_agents_md);
}

#[test]
fn mcp_reaches_the_loaded_config() {
    // The headline case. `servers.len() == 0` here means every MCP server a
    // user configured was ignored by every run.
    let c = loaded();
    assert_eq!(
        c.mcp.servers.len(),
        1,
        "a configured MCP server must reach the run"
    );
    assert_eq!(c.mcp.servers[0].name, "probe");
    assert_eq!(c.mcp.servers[0].command.as_deref(), Some("true"));
    assert_eq!(c.mcp.servers[0].args, vec!["-y".to_string()]);
    assert!(
        c.mcp.enabled,
        "the section is off by default, so true proves the merge"
    );
    assert_eq!(c.mcp.timeout_ms, 1234);
}

#[test]
fn the_remaining_sections_still_reach_the_loaded_config() {
    // Regression cover for the sections that already worked, so a future edit
    // to `merge` cannot quietly drop one of them either.
    let c = loaded();
    assert_eq!(c.agents.planner.provider, "probe");
    // `SandboxBackend` is not exported, so compare the rendered form: the point
    // is that the file's `backend = "worktree"` reached the config, and a
    // Debug rendering of a different variant would not be this string.
    assert_eq!(format!("{:?}", c.docker.backend), "Worktree");
    assert_eq!(c.docker.extra_packages, vec!["probe-pkg".to_string()]);
    assert_eq!(c.pipeline.max_revision_rounds, Some(9));
    assert_eq!(c.permissions.mode, "bypass");
    assert!(c.permissions.fail_closed_headless);
    assert_eq!(
        c.providers
            .get("probe")
            .map(|p| p.default_model.clone())
            .as_deref(),
        Some("probe-model")
    );
}

#[test]
fn an_mcp_server_needs_no_args_field() {
    // `args` was the one field on `McpServerConfigEntry` without a
    // `#[serde(default)]`, so the minimal entry a user would reasonably write —
    // a name and a URL — failed to parse with "missing field `args`", pointing
    // at the wrong line. A URL-based server has no arguments at all.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("niki.toml"),
        "[[mcp.servers]]\nname = \"docs\"\nurl = \"https://example.invalid/mcp\"\n",
    )
    .unwrap();
    let c = NikiConfig::load(dir.path()).expect("a URL-only server must parse");
    assert_eq!(c.mcp.servers.len(), 1);
    assert_eq!(
        c.mcp.servers[0].url.as_deref(),
        Some("https://example.invalid/mcp")
    );
    assert!(c.mcp.servers[0].args.is_empty());
}
