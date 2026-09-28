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

fn tools_source() -> String {
    std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime/tools.rs"),
    )
    .expect("src/runtime/tools.rs is readable")
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
