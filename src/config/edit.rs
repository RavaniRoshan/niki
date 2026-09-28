//! Editing `niki.toml` in place without destroying it.
//!
//! The TUI is meant to be the whole product surface, so a user must be able to
//! change a setting without leaving it. That makes this file load-bearing: every
//! settings change the product makes goes through `set_value`.
//!
//! Two rules, both learned the hard way:
//!
//! 1. **Comments survive.** `toml::to_string_pretty(&toml::Value)` round-trips
//!    the DATA and silently drops every `#` line and blank line in between.
//!    A config file people annotate — and this repo's own `niki.example.toml`
//!    is extensively annotated — cannot be rewritten that way. `toml_edit`
//!    preserves the document, so a user who changes one setting from the TUI
//!    does not come back to find their notes deleted.
//!
//! 2. **The write is atomic.** A settings change that truncates `niki.toml`
//!    destroys the configuration of every agent, model and provider the user
//!    has set up. Write to a sibling temp file, then rename.
//!
//! Values are addressed by dotted path (`"general.max_revision_rounds"`) rather
//! than by struct field, so the settings UI and this module cannot drift apart,
//! and a setting can be edited before the corresponding struct field exists.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Which config file a change lands in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigScope {
    /// `<project>/niki.toml` — the project the user is working in.
    Project,
    /// `~/.config/niki/niki.toml` — preferences that follow the user.
    Global,
}

impl ConfigScope {
    /// The path this scope writes to.
    ///
    /// Takes the project directory rather than reading it from ambient state,
    /// so a caller cannot accidentally write a project's settings into the
    /// user's global file.
    pub fn path(self, project_dir: &Path) -> Result<PathBuf> {
        match self {
            ConfigScope::Project => Ok(project_dir.join("niki.toml")),
            ConfigScope::Global => {
                let home = dirs::home_dir()
                    .context("cannot determine the home directory for the global config")?;
                Ok(home.join(".config").join("niki").join("niki.toml"))
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ConfigScope::Project => "project niki.toml",
            ConfigScope::Global => "global ~/.config/niki/niki.toml",
        }
    }
}

/// Read the config at `path` as an editable document, or an empty one.
///
/// A file that does not exist is not an error: creating a settings value in a
/// project that has no `niki.toml` yet is the normal first-run case.
pub fn read_doc(path: &Path) -> Result<toml_edit::DocumentMut> {
    if !path.exists() {
        return Ok(toml_edit::DocumentMut::new());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    text.parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("{} is not valid TOML", path.display()))
}

/// Read a value by dotted path, if present.
pub fn get_value(path: &Path, dotted: &str) -> Result<Option<toml_edit::Item>> {
    let doc = read_doc(path)?;
    let mut item: toml_edit::Item = doc.as_item().clone();
    for segment in dotted.split('.') {
        match item.as_table_like_mut().and_then(|t| t.get_mut(segment)) {
            Some(next) => item = next.clone(),
            None => return Ok(None),
        }
    }
    Ok(Some(item))
}

/// Set a value by dotted path, creating intermediate tables, and write the file
/// atomically with comments intact.
///
/// A path that currently holds a *non-scalar* value (a table or array) is
/// rejected rather than replaced: a settings UI offering to overwrite
/// `[agents.planner]` with a string is a bug, and silently doing it would
/// destroy a provider the user configured.
pub fn set_value(path: &Path, dotted: &str, value: toml_edit::Value) -> Result<()> {
    let mut doc = read_doc(path)?;
    let segments: Vec<&str> = dotted.split('.').collect();
    let (last, parents) = segments
        .split_last()
        .ok_or_else(|| anyhow::anyhow!("{dotted:?} is not a setting path"))?;

    // Where the walk currently is. A TOML document is a tree of tables, but
    // `[[mcp.servers]]` stores its entries in an array of tables, and
    // `TableLike` cannot see into one — the elements are `Table`s inside an
    // `Item::Value::Array`. Modelling the cursor explicitly is what lets one
    // dotted path address both.
    enum Cursor<'a> {
        Table(&'a mut dyn toml_edit::TableLike),
        Array(&'a mut toml_edit::ArrayOfTables),
    }

    let mut cursor = Cursor::Table(doc.as_table_mut());
    for segment in parents {
        cursor = match cursor {
            Cursor::Table(t) => {
                // An array of tables lives under a key; the *next* segment is
                // the index.
                let is_array = t
                    .get(segment)
                    .is_some_and(|i| i.as_array_of_tables().is_some());
                if is_array {
                    let item = t.get_mut(segment).ok_or_else(|| {
                        anyhow::anyhow!("cannot set {dotted:?}: {segment:?} vanished")
                    })?;
                    let arr = item.as_array_of_tables_mut().ok_or_else(|| {
                        anyhow::anyhow!("cannot set {dotted:?}: {segment:?} is not an array")
                    })?;
                    Cursor::Array(arr)
                } else {
                    if !t.contains_key(segment) {
                        t.insert(segment, toml_edit::Item::Table(toml_edit::Table::new()));
                    }
                    let next = t.get_mut(segment).ok_or_else(|| {
                        anyhow::anyhow!("cannot set {dotted:?}: {segment:?} vanished")
                    })?;
                    let next = next.as_table_like_mut().ok_or_else(|| {
                        anyhow::anyhow!(
                            "cannot set {dotted:?}: {segment:?} is a value, not a table"
                        )
                    })?;
                    Cursor::Table(next)
                }
            }
            Cursor::Array(a) => {
                let index: usize = segment.parse().map_err(|_| {
                    anyhow::anyhow!(
                        "cannot set {dotted:?}: {segment:?} is not an index into an array"
                    )
                })?;
                let len = a.len();
                let entry = a.get_mut(index).ok_or_else(|| {
                    anyhow::anyhow!(
                        "cannot set {dotted:?}: index {index} is out of range ({len} entries)"
                    )
                })?;
                Cursor::Table(entry)
            }
        };
    }

    let table = match cursor {
        Cursor::Table(t) => t,
        // A path may not end *inside* an array: `mcp.servers` is a list, and
        // there is no value to put there.
        Cursor::Array(_) => {
            anyhow::bail!("cannot set {dotted:?}: the path ends at an array")
        }
    };

    // A path that currently holds a *non-scalar* value (a table or array) is
    // refused rather than replaced: a settings UI offering to overwrite
    // `[agents.planner]` with a string is a bug, and silently doing it would
    // destroy a provider the user configured.
    if let Some(existing) = table.get(last)
        && !existing.is_value()
    {
        anyhow::bail!(
            "{last} is a table or array in {}; refusing to replace it",
            path.display()
        );
    }

    // Assign through the EXISTING item; never re-`insert`.
    //
    // `TableLike::insert` replaces the key as well as the value, and the key is
    // where a preceding comment lives — so the obvious
    // `table.insert(last, toml_edit::value(v))` silently deletes the comment
    // sitting directly above the line it rewrote. That was measured, not
    // assumed: `# note about the field` disappeared.
    //
    // Assigning through `get_mut` leaves the key — and its decor — alone, and
    // carrying the old *value* decor across keeps the spacing on the line
    // itself. For a key that is not present yet there is nothing to preserve and
    // `insert` is the only option.
    match table.get_mut(last) {
        Some(existing) => {
            if let Some(old) = existing.as_value() {
                let decor = old.decor().clone();
                let mut new_item = toml_edit::value(value);
                if let Some(v) = new_item.as_value_mut() {
                    *v.decor_mut() = decor;
                }
                *existing = new_item;
            } else {
                *existing = toml_edit::value(value);
            }
        }
        None => {
            table.insert(last, toml_edit::value(value));
        }
    }

    write_atomic(path, &doc.to_string())
}

/// Remove a key, if it is present. Returns whether anything was removed.
pub fn remove_value(path: &Path, dotted: &str) -> Result<bool> {
    let mut doc = read_doc(path)?;
    let mut segments = dotted.split('.');
    let Some(last) = segments.next_back() else {
        return Ok(false);
    };
    let prefix: Vec<&str> = segments.collect();
    let mut table: &mut dyn toml_edit::TableLike = doc.as_table_mut();
    for segment in prefix {
        let Some(entry) = table.get_mut(segment) else {
            return Ok(false);
        };
        match entry.as_table_like_mut() {
            Some(t) => table = t,
            None => return Ok(false),
        }
    }
    let removed = table.remove(last).is_some();
    if removed {
        write_atomic(path, &doc.to_string())?;
    }
    Ok(removed)
}

/// Write `contents` to `path` via a temp file and a rename.
///
/// A settings change is a write to the file every part of the product reads on
/// its next run. If that write is interrupted halfway, the user loses their
/// whole configuration. The temp file is a sibling so the rename stays on one
/// filesystem, which is the only case where `rename` is atomic.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{} has no parent directory", path.display()))?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("could not create {}", parent.display()))?;

    let tmp = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("niki.toml"),
        std::process::id()
    ));
    std::fs::write(&tmp, contents).with_context(|| format!("could not write {}", tmp.display()))?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(anyhow::Error::new(e).context(format!("could not replace {}", path.display())))
        }
    }
}

/// Parse a JSON object out of text that may be wrapped in a code fence.
///
/// For a model that was asked for a tool call and answered in prose anyway, the
/// payload is usually still there, fenced.
pub fn json_value_of(text: &str) -> Option<serde_json::Value> {
    let trimmed = text.trim();
    let candidate = if let Some(rest) = trimmed.strip_prefix("```json") {
        rest.split("```").next().unwrap_or(rest)
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        rest.split("```").next().unwrap_or(rest)
    } else {
        trimmed
    };
    serde_json::from_str(candidate.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANNOTATED: &str = r#"# NIKI configuration
# Lines like this one are why we do not round-trip through a bare Value.

[general]
# How many times the Coder may be asked to revise.
max_revision_rounds = 3
output_dir = ".niki"

[ui]
theme = "kiln"
"#;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn comments_and_blank_lines_survive_an_edit() {
        // The whole reason this module exists. A plain `toml::Value`
        // round-trip returns the same data and none of this text, and a user
        // who changed one number from a settings screen would find their notes
        // gone.
        let dir = tmp();
        let path = dir.path().join("niki.toml");
        std::fs::write(&path, ANNOTATED).unwrap();

        set_value(&path, "general.max_revision_rounds", 5i64.into()).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.contains("# How many times the Coder may be asked to revise."),
            "the comment directly above an edited key must survive:\n{after}"
        );
        assert!(after.contains("# NIKI configuration"), "{after}");
        assert!(after.contains("max_revision_rounds = 5"), "{after}");
        assert!(
            after.contains("output_dir = \".niki\""),
            "untouched values must be untouched:\n{after}"
        );
        assert!(after.contains("theme = \"kiln\""), "{after}");
    }

    #[test]
    fn a_missing_intermediate_table_is_created_rather_than_failing() {
        let dir = tmp();
        let path = dir.path().join("niki.toml");
        set_value(&path, "budget.max_usd", 5.0.into()).unwrap();
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(after.contains("[budget]"), "{after}");
        assert!(after.contains("max_usd"), "{after}");
    }

    #[test]
    fn a_missing_file_is_created() {
        // The first-run case: a project with no niki.toml yet and a user
        // changing a setting from the TUI.
        let dir = tmp();
        let path = dir.path().join("nested").join("niki.toml");
        set_value(&path, "ui.theme", "kiln".into()).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("kiln"));
    }

    #[test]
    fn replacing_a_table_with_a_scalar_is_refused() {
        let dir = tmp();
        let path = dir.path().join("niki.toml");
        std::fs::write(&path, ANNOTATED).unwrap();
        let err = set_value(&path, "general", "oops".into())
            .expect_err("replacing a table must be refused");
        assert!(err.to_string().contains("refusing to replace"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            ANNOTATED,
            "a refused edit must leave the file byte-for-byte alone"
        );
    }

    #[test]
    fn values_round_trip_through_get() {
        let dir = tmp();
        let path = dir.path().join("niki.toml");
        std::fs::write(&path, ANNOTATED).unwrap();
        set_value(&path, "general.max_revision_rounds", 7i64.into()).unwrap();
        let v = get_value(&path, "general.max_revision_rounds")
            .unwrap()
            .expect("just set");
        assert_eq!(v.as_integer(), Some(7));
        assert!(
            get_value(&path, "nothing.here").unwrap().is_none(),
            "an absent path is None, not an error"
        );
    }

    #[test]
    fn remove_deletes_the_key_and_reports_whether_it_existed() {
        let dir = tmp();
        let path = dir.path().join("niki.toml");
        std::fs::write(&path, ANNOTATED).unwrap();
        assert!(remove_value(&path, "ui.theme").unwrap());
        let after = std::fs::read_to_string(&path).unwrap();
        assert!(!after.contains("theme = "), "{after}");
        assert!(
            after.contains("# NIKI configuration"),
            "removing one key must not drop the comments:\n{after}"
        );
        assert!(!remove_value(&path, "ui.theme").unwrap());
    }

    #[test]
    fn the_scope_path_is_the_one_the_caller_asked_for() {
        let dir = tmp();
        assert_eq!(
            ConfigScope::Project.path(dir.path()).unwrap(),
            dir.path().join("niki.toml")
        );
        let global = ConfigScope::Global.path(dir.path()).unwrap();
        assert!(
            global.to_string_lossy().contains(".config/niki/niki.toml"),
            "{global:?}"
        );
    }

    #[test]
    fn a_rejected_edit_leaves_no_temp_file_behind() {
        // The atomic-write path cleans up on failure; a stray `.niki.toml.<pid>.tmp`
        // in a config directory is litter the user would have to recognise.
        let dir = tmp();
        let path = dir.path().join("niki.toml");
        std::fs::write(&path, ANNOTATED).unwrap();
        let _ = set_value(&path, "general", "oops".into());
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp files left behind: {leftovers:?}"
        );
    }

    #[test]
    fn a_value_inside_an_array_of_tables_can_be_set() {
        // `[[mcp.servers]]` stores its entries in an array of tables, and
        // `TableLike` cannot see into one. The first version of `set_value`
        // walked with `TableLike` throughout and failed with "servers is a
        // value, not a table" — so no array-backed setting could be edited at
        // all, which is most of what a list-shaped setting is.
        let dir = tmp();
        let path = dir.path().join("niki.toml");
        std::fs::write(
            &path,
            "[[mcp.servers]]\nname = \"a\"\nenabled = true\n\n[[mcp.servers]]\nname = \"b\"\nenabled = true\n",
        )
        .unwrap();

        set_value(
            &path,
            "mcp.servers.1.enabled",
            toml_edit::Value::from(false),
        )
        .unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        assert!(
            after.contains("name = \"a\"\nenabled = true"),
            "the first entry must be untouched:\n{after}"
        );
        assert!(
            after.contains("name = \"b\"\nenabled = false"),
            "the second entry must be the one that changed:\n{after}"
        );
        assert_eq!(
            after.matches("[[mcp.servers]]").count(),
            2,
            "and the array must not have grown or shrunk:\n{after}"
        );
    }

    #[test]
    fn an_out_of_range_index_is_an_error_not_a_panic() {
        let dir = tmp();
        let path = dir.path().join("niki.toml");
        std::fs::write(&path, "[[mcp.servers]]\nname = \"a\"\n").unwrap();
        let err = set_value(
            &path,
            "mcp.servers.7.enabled",
            toml_edit::Value::from(false),
        )
        .expect_err("an out-of-range index must be refused");
        assert!(err.to_string().contains("out of range"), "{err}");
    }

    #[test]
    fn a_path_may_not_end_inside_an_array() {
        let dir = tmp();
        let path = dir.path().join("niki.toml");
        std::fs::write(&path, "[[mcp.servers]]\nname = \"a\"\n").unwrap();
        set_value(&path, "mcp.servers", toml_edit::Value::from("nope"))
            .expect_err("replacing a whole array with a scalar must be refused");
    }
}
