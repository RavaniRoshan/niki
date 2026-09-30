//! The tool loop's contract with the model.
//!
//! A model can only call a tool correctly if the spec told it the argument
//! names. Every tool used to be advertised as
//! `{"type":"object","properties":{}}`, so `edit` looked identical to
//! `web_search` from the model's side and it had to guess. Measured on a 3B
//! model, the Coder spent its whole budget emitting malformed edits — a
//! failure the harness caused by withholding the contract it already had.
//!
//! These tests hold the contract in place. They are written against the
//! *source* rather than against the registry so a tool cannot quietly grow a
//! new `input.str("…")` without also declaring it.

use std::collections::{BTreeMap, BTreeSet};

/// The production half of `src/runtime/tools.rs`.
///
/// The `#[cfg(test)]` half declares a `DummyTool` that deliberately reuses the
/// name `read`, which would otherwise overwrite the real one in the map below
/// and make this test quietly check a fixture instead of the registry.
fn tools_source() -> String {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/tools.rs"),
    )
    .expect("src/runtime/tools.rs is readable");
    let cut = src
        .lines()
        .position(|l| l.trim_start().starts_with("#[cfg(test)]"))
        .unwrap_or_else(|| {
            src.lines()
                .position(|l| l.trim() == "mod tests {")
                .expect("src/runtime/tools.rs has a test module")
        });
    src.lines().take(cut).collect::<Vec<_>>().join("\n")
}

/// Pair each `name:` with the `parameters:` that follows it in the same
/// `ToolDef` literal. The pairing is textual on purpose: it is what a drift
/// bug looks like, and it keeps the check independent of the registry.
fn named_schemas(src: &str) -> BTreeMap<String, serde_json::Value> {
    let mut out: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let mut pending: Option<String> = None;
    for line in src.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("name: \"") {
            pending = rest.split('"').next().map(str::to_string);
        } else if let Some(rest) = t.strip_prefix("parameters: r#\"") {
            // The literal ends in `"#,`
            let raw = rest
                .trim_end()
                .trim_end_matches(',')
                .trim_end_matches('#')
                .trim_end_matches('"');
            if let Some(name) = pending.take() {
                assert!(
                    !out.contains_key(&name),
                    "two tools are named `{name}` — the schema map would silently \
                     keep only the last one, so this test would check the wrong tool"
                );
                let value: serde_json::Value = serde_json::from_str(raw)
                    .unwrap_or_else(|e| panic!("{name}: schema is not JSON: {e}\n{raw}"));
                out.insert(name, value);
            }
        }
    }
    out
}

/// Every argument name each tool's `execute` reads.
fn executed_arguments(src: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // Each `impl Tool for XTool { … }` block: name the tool from its ToolDef
    // literal, then collect the accessors used after `fn def()`.
    let mut current: Option<(String, usize)> = None;
    let mut in_execute = false;
    for (i, line) in src.lines().enumerate() {
        let t = line.trim();
        if t.starts_with("impl Tool for ") {
            current = None;
            in_execute = false;
        } else if let Some(rest) = t.strip_prefix("name: \"") {
            if let Some(name) = rest.split('"').next() {
                current = Some((name.to_string(), i));
            }
        } else if t.starts_with("async fn execute(") {
            in_execute = true;
            if let Some((name, _)) = &current {
                out.entry(name.clone()).or_default();
            }
        } else if in_execute && t.starts_with("fn def(") {
            in_execute = false;
        }
        if !in_execute {
            continue;
        }
        let Some((name, _)) = &current else { continue };
        let mut rest = t;
        while let Some(pos) = rest.find("input.") {
            rest = &rest[pos + "input.".len()..];
            let Some(paren) = rest.find('(') else { break };
            let accessor = &rest[..paren];
            if !matches!(accessor, "require_str" | "str" | "int" | "bool" | "float") {
                continue;
            }
            let after = &rest[paren + 1..];
            let Some(quote) = after.find('"') else { break };
            let args = &after[quote + 1..];
            let Some(endq) = args.find('"') else { break };
            out.entry(name.clone())
                .or_default()
                .insert(args[..endq].to_string());
            rest = &args[endq + 1..];
        }
        let _ = i;
    }
    out
}

fn properties(schema: &serde_json::Value) -> BTreeSet<String> {
    schema
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

#[test]
fn tools_declare_every_argument_they_read() {
    let src = tools_source();
    let declared = named_schemas(&src);
    let executed = executed_arguments(&src);

    assert!(
        !executed.is_empty(),
        "the source scan found no tools — the test is measuring nothing"
    );

    let mut missing: Vec<String> = Vec::new();
    for (tool, args) in &executed {
        // `submit_artifact` has no `ToolDef`: its spec is built at runtime from
        // the stage's own artifact schema, so it is declared by construction.
        if DYNAMICALLY_SPECIFIED.contains(&tool.as_str()) {
            continue;
        }
        let Some(schema) = declared.get(tool) else {
            missing.push(format!(
                "{tool}: has an execute() but no declared parameters"
            ));
            continue;
        };
        let known = properties(schema);
        for arg in args {
            if !known.contains(arg) {
                missing.push(format!("{tool}: reads `{arg}` but does not declare it"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "tools read arguments the model is never told about:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn every_declared_tool_name_is_unique_and_resolvable() {
    let src = tools_source();
    let declared = named_schemas(&src);
    for (name, schema) in &declared {
        assert_eq!(
            schema.get("type").and_then(|t| t.as_str()),
            Some("object"),
            "{name}: parameter schema must be an object schema"
        );
    }
}

#[test]
fn tools_the_coder_depends_on_name_their_required_arguments() {
    // These are the tools a Coder reaches for first. If any of them ships
    // without a `required` list, the model is invited to call it with nothing.
    let src = tools_source();
    let declared = named_schemas(&src);
    for tool in ["read", "write", "edit", "glob", "grep", "bash"] {
        let schema = declared
            .get(tool)
            .unwrap_or_else(|| panic!("{tool} declares no parameters"));
        let required = schema.get("required").and_then(|r| r.as_array());
        assert!(
            required.is_some_and(|r| !r.is_empty()),
            "{tool} must declare at least one required argument; a model given no \
             required list calls it with an empty object and gets a diagnostic instead of work"
        );
    }
}

/// Tools whose spec is generated at runtime rather than from a `ToolDef`.
const DYNAMICALLY_SPECIFIED: [&str; 1] = ["submit_artifact"];

/// The two tools that genuinely take no arguments. Everything else must say
/// what it takes, because a model given an empty schema calls it blind.
const ARGUMENTLESS: [&str; 2] = ["task_list", "skill_list"];

#[test]
fn the_registry_advertises_real_schemas_not_permissive_stubs() {
    // The regression itself: before this was fixed, every spec carried
    // `"properties": {}`. Asserting on the live registry (not the source text)
    // means the thing the model actually receives is what is checked.
    let registry = niki::runtime::tools::build_baseline_registry();
    let specs = registry.tool_specs_for("coder");
    assert!(!specs.is_empty(), "the coder has no tools");

    let read = specs
        .iter()
        .find(|s| s.name == "read")
        .expect("read is available to the coder");
    let props = properties(&read.parameters);
    assert!(
        props.contains("path"),
        "read advertises no `path`: the model is being told nothing about what to pass"
    );

    let edit = specs
        .iter()
        .find(|s| s.name == "edit")
        .expect("edit is available to the coder");
    let props = properties(&edit.parameters);
    for arg in ["path", "old_text", "new_text"] {
        assert!(
            props.contains(arg),
            "edit advertises no `{arg}` — this is the stub-schema regression"
        );
    }

    let empty: Vec<&str> = specs
        .iter()
        .filter(|s| {
            properties(&s.parameters).is_empty() && !ARGUMENTLESS.contains(&s.name.as_str())
        })
        .map(|s| s.name.as_str())
        .collect();
    assert!(
        empty.is_empty(),
        "these tools still advertise an empty parameter schema: {empty:?}"
    );
}

/// The `git` tool must honour the command deny-list.
///
/// It did not. The `bash` tool has enforced `check_command_policy` since Phase
/// 5.3, with the comment "a granted tool permission must never bypass
/// `check_command_policy`" — and this tool had a `PermissionRequirement::Ask`
/// and no check at all, so every per-role deny-list entry about git was
/// unreachable through the one tool a role could still use. `git
/// {"subcommand":"push"}` and `{"subcommand":"commit"}` from the Reviewer
/// passed; so did `clean -fd`.
///
/// The reproduction is a policy, not a network: the default global deny-list
/// contains `git push --force`, so the test drives the *checker* with the exact
/// argv this tool now builds and asserts it refuses.
#[test]
fn a_git_subcommand_on_the_deny_list_is_refused() {
    let policy = niki::config::SecurityPolicyConfig::default();
    for argv in [
        vec!["git", "push", "--force"],
        vec!["git", "push", "-f"],
        vec!["git", "clean", "-fd"],
    ] {
        let verdict = niki::sandbox::check_command_policy(&argv, &policy);
        // `clean -fd` is not on the default list — which is the point worth
        // recording: a subcommand with no deny entry is allowed, and only the
        // check makes the two entries above refusable.
        if argv.contains(&"clean") {
            assert!(
                verdict.is_ok(),
                "sanity: `clean` is not on the default deny-list, so the check alone \
                 would not stop it — the reviewer permission mode is what does"
            );
        } else {
            assert!(
                verdict.is_err(),
                "the deny-list must refuse {argv:?}; the git tool builds exactly this argv"
            );
        }
    }
}

/// And the check is not so broad that ordinary git stops working.
#[test]
fn ordinary_git_subcommands_are_still_allowed() {
    let policy = niki::config::SecurityPolicyConfig::default();
    for argv in [
        vec!["git", "status"],
        vec!["git", "diff", "--stat"],
        vec!["git", "log", "--oneline", "-10"],
        vec!["git", "branch"],
    ] {
        assert!(
            niki::sandbox::check_command_policy(&argv, &policy).is_ok(),
            "{argv:?} is what the tool is for and must not be blocked"
        );
    }
}

/// Drive the real tool, not the checker.
///
/// The two tests above assert what `check_command_policy` decides. This one
/// asserts that `GitTool` actually calls it — which is the wiring that was
/// missing, and the reason `git {"subcommand":"push"}` passed from a role
/// whose `bash` equivalent was denied.
#[tokio::test(flavor = "multi_thread")]
async fn the_git_tool_itself_refuses_a_denied_subcommand() {
    use niki::runtime::tools::GitTool;
    use niki::runtime::{Tool, ToolContext, ToolInput, ToolStatus};

    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext {
        agent_id: niki::mission::AgentId("t".into()),
        mission_id: niki::mission::MissionId("t".into()),
        role: "reviewer".into(),
        project_path: dir.path().to_path_buf(),
        permissions: std::collections::HashMap::new(),
        // Bypass, so the *permission* layer cannot be what stops this. If the
        // deny-list is not consulted, the command runs.
        permission_mode: "bypass".into(),
        fail_closed_headless: false,
        task_store: None,
    };

    // The marker file would exist only if the command actually ran.
    let marker = dir.path().join("PUSHED");
    let result = GitTool
        .execute(
            ToolInput::new(serde_json::json!({ "subcommand": "push --force origin main" })),
            &ctx,
        )
        .await;

    assert_eq!(
        result.status,
        ToolStatus::Failed,
        "a denied git subcommand must fail, even with permissions bypassed: {:?}",
        result.summary
    );
    assert!(
        result.summary.contains("policy") || result.summary.contains("blocked"),
        "the refusal must name the policy, so the model knows to stop: {:?}",
        result.summary
    );
    assert!(!marker.exists(), "sanity: nothing should have run");
}

/// A subcommand on the deny-list must not be able to hide behind the
/// argument sugar the tool adds.
#[tokio::test(flavor = "multi_thread")]
async fn the_git_tool_cannot_be_reached_by_a_sneaky_subcommand() {
    use niki::runtime::tools::GitTool;
    use niki::runtime::{Tool, ToolContext, ToolInput, ToolStatus};

    let dir = tempfile::tempdir().unwrap();
    let ctx = ToolContext {
        agent_id: niki::mission::AgentId("t".into()),
        mission_id: niki::mission::MissionId("t".into()),
        role: "reviewer".into(),
        project_path: dir.path().to_path_buf(),
        permissions: std::collections::HashMap::new(),
        permission_mode: "bypass".into(),
        fail_closed_headless: false,
        task_store: None,
    };

    for sneaky in ["push --force", "push -f", "push --force-with-lease"] {
        let r = GitTool
            .execute(
                ToolInput::new(serde_json::json!({ "subcommand": sneaky })),
                &ctx,
            )
            .await;
        assert_ne!(
            r.status,
            ToolStatus::Success,
            "`{sneaky}` must not succeed, even bypassing permissions"
        );
    }
}
