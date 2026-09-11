//! Content-addressed structural index: an advisory symbol/call graph over the
//! repo, built on a backend ladder (AST → regex → coverage-only).
//!
//! Layout under `<output_dir>/kb/structural_index/`:
//!
//! ```text
//! structural_index/
//!   manifest.json            atomic commit point (status, snapshot, stats)
//!   SCOPE.md                 advisory-only contract for readers
//!   units/<2>/<2>/<key>.json per-file extraction units
//! ```
//!
//! The cache key is `fnv1a(schema | extractor | language | content-digest)`,
//! so unchanged files are never re-extracted. The query layer is advisory
//! only: [`crate::runtime`] grep remains the source of truth (see SCOPE.md).

use crate::config::NikiConfig;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Schema version bumped whenever unit/extraction formats change (busts cache).
pub const SCHEMA_VERSION: u32 = 1;
/// Extractor id for the guaranteed regex baseline.
pub const REGEX_EXTRACTOR: &str = "regex@1";
/// Extractor id for the tree-sitter layer. Unconditional so cache keys stay
/// stable across feature combinations; only builds with the `ast` Cargo
/// feature ever produce `ast` units (see [`resolver_version`]).
pub const AST_EXTRACTOR: &str = "ts@1";
/// Max bytes extracted per file (larger files are skipped, not failed).
const MAX_FILE_BYTES: usize = 2_000_000;
/// Cap on call edges recorded per file (bounds pathological files).
const MAX_EDGES_PER_FILE: usize = 1000;

/// Index-wide status, mirroring the Mantis honest-state contract.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IndexStatus {
    Complete,
    Partial,
    Empty,
}

/// Atomic commit point for one index build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexManifest {
    pub snapshot_id: String,
    pub status: IndexStatus,
    /// Extractor ladder that produced this index (`regex@1`, `ts@1+regex@1`).
    pub resolver_version: String,
    /// Language → backend actually used (`ast`, `regex`, `coverage`).
    pub precision: HashMap<String, String>,
    pub units_total: usize,
    pub units_indexed: usize,
    pub units_coverage_only: usize,
    pub units_skipped: usize,
    pub truncated: bool,
}

/// Precision tier of a single answer.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    Ast,
    Regex,
    Unavailable,
}

impl Precision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Precision::Ast => "ast",
            Precision::Regex => "regex",
            Precision::Unavailable => "unavailable",
        }
    }
}

/// One extracted symbol with its (possibly approximate) span.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedSymbol {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub end_line: usize,
    pub definition: String,
}

/// One caller → callee edge inside a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEdge {
    pub caller: String,
    pub callee: String,
    pub line: usize,
}

/// One cached per-file extraction unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitFile {
    pub key: String,
    pub path: String,
    pub language: String,
    /// `ast`, `regex`, or `coverage` (no symbols extracted).
    pub backend: String,
    pub digest: String,
    pub symbols: Vec<IndexedSymbol>,
    pub imports: Vec<String>,
    pub calls: Vec<CallEdge>,
}

/// What one [`build_index`] run did.
#[derive(Debug)]
pub struct IndexReport {
    pub manifest: IndexManifest,
    pub written: usize,
    pub reused: usize,
}

/// Root of the structural index store.
pub fn index_root(project_path: &Path, config: &NikiConfig) -> PathBuf {
    project_path
        .join(&config.general.output_dir)
        .join("kb")
        .join("structural_index")
}

fn units_dir(project_path: &Path, config: &NikiConfig) -> PathBuf {
    index_root(project_path, config).join("units")
}

fn manifest_path(project_path: &Path, config: &NikiConfig) -> PathBuf {
    index_root(project_path, config).join("manifest.json")
}

/// Map a file extension to the index language key.
fn lang_key(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "rs" => Some("rust"),
        "ts" | "mts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "jsx" => Some("javascript"),
        "py" => Some("python"),
        "go" => Some("go"),
        _ => None,
    }
}

/// The regex backend's language name for an index language key.
fn regex_lang(language: &str) -> &str {
    match language {
        "tsx" => "typescript",
        _ => language,
    }
}

/// Whether the AST layer can handle this language in this build.
fn ast_supported(language: &str) -> bool {
    #[cfg(feature = "ast")]
    {
        let _ = language;
        matches!(
            language,
            "rust" | "typescript" | "tsx" | "javascript" | "python" | "go"
        ) && ast::available()
    }
    #[cfg(not(feature = "ast"))]
    {
        let _ = language;
        false
    }
}

fn resolver_version() -> String {
    #[cfg(feature = "ast")]
    {
        format!("{}+{}", AST_EXTRACTOR, REGEX_EXTRACTOR)
    }
    #[cfg(not(feature = "ast"))]
    {
        REGEX_EXTRACTOR.to_string()
    }
}

/// Build (or incrementally refresh) the index. Idempotent: unchanged files
/// hit the content-addressed cache and are not re-extracted. Never fails on
/// weird repos — bad files are skipped or stored coverage-only.
pub fn build_index(
    project_path: &Path,
    config: &NikiConfig,
    snapshot_id: &str,
) -> Result<IndexReport> {
    let root = index_root(project_path, config);
    std::fs::create_dir_all(units_dir(project_path, config))?;
    let max_units = config.repo_intel.max_units.max(1);
    let want_ast = config.repo_intel.ast;

    let mut files: Vec<(PathBuf, String)> = Vec::new();
    let walker = WalkDir::new(project_path)
        .max_depth(6)
        .into_iter()
        .filter_entry(|e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            if crate::repo_intel::manifest_vendor_dirs().contains(&name.as_ref()) {
                return false;
            }
            !name.starts_with('.')
        });
    for entry in walker.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if let Some(lang) = lang_key(ext) {
            let rel = entry
                .path()
                .strip_prefix(project_path)
                .unwrap_or(entry.path())
                .to_path_buf();
            files.push((rel, lang.to_string()));
        }
        if files.len() >= max_units {
            break;
        }
    }
    files.sort();
    let truncated = files.len() >= max_units;

    let mut written = 0usize;
    let mut reused = 0usize;
    let mut skipped = 0usize;
    let mut coverage_only = 0usize;
    let mut precision: HashMap<String, String> = HashMap::new();

    for (rel, language) in &files {
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let full = project_path.join(rel);
        let bytes = match std::fs::read(&full) {
            Ok(b) if b.len() <= MAX_FILE_BYTES => b,
            _ => {
                skipped += 1;
                continue;
            }
        };
        let digest = crate::util::fnv1a64_hex(&bytes);

        // Backend ladder: AST first, regex guaranteed, coverage-only fallback
        // for non-UTF8 content.
        let (extractor, backend, unit) = match String::from_utf8(bytes.clone()) {
            Ok(content) => {
                let extracted = if want_ast && ast_supported(language) {
                    ast_extract(&content, language).map(|e| (AST_EXTRACTOR, "ast", e))
                } else {
                    None
                };
                let (extractor, backend, symbols, imports, calls) = match extracted {
                    Some((ext, be, ext_data)) => (
                        ext,
                        be.to_string(),
                        ext_data.symbols,
                        ext_data.imports,
                        ext_data.calls,
                    ),
                    None => {
                        let (symbols, imports, calls) = regex_extract(&content, language, rel);
                        (
                            REGEX_EXTRACTOR,
                            "regex".to_string(),
                            symbols,
                            imports,
                            calls,
                        )
                    }
                };
                let key = unit_key(extractor, language, &digest);
                let unit = UnitFile {
                    key: key.clone(),
                    path: rel_str,
                    language: language.clone(),
                    backend: backend.clone(),
                    digest,
                    symbols,
                    imports,
                    calls,
                };
                (extractor, backend, Some((key, unit)))
            }
            Err(_) => {
                // Non-UTF8 content: coverage-only unit so the file is at least
                // represented. Query layer reports `unavailable` for it.
                let key = unit_key(REGEX_EXTRACTOR, language, &digest);
                let unit = UnitFile {
                    key: key.clone(),
                    path: rel_str,
                    language: language.clone(),
                    backend: "coverage".to_string(),
                    digest,
                    symbols: Vec::new(),
                    imports: Vec::new(),
                    calls: Vec::new(),
                };
                (REGEX_EXTRACTOR, "coverage".to_string(), Some((key, unit)))
            }
        };
        let _ = extractor;
        precision
            .entry(language.clone())
            .or_insert_with(|| backend.clone());
        if backend == "coverage" {
            coverage_only += 1;
        }

        if let Some((key, unit)) = unit {
            let path = unit_path(project_path, config, &key);
            if path.exists() {
                reused += 1;
            } else {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                crate::knowledge::kb::write_atomic(
                    &path,
                    serde_json::to_string(&unit)?.as_bytes(),
                )?;
                written += 1;
            }
        }
    }

    let status = if files.is_empty() {
        IndexStatus::Empty
    } else if skipped > 0 || coverage_only > 0 || truncated {
        IndexStatus::Partial
    } else {
        IndexStatus::Complete
    };
    let manifest = IndexManifest {
        snapshot_id: snapshot_id.to_string(),
        status,
        resolver_version: resolver_version(),
        precision,
        units_total: files.len(),
        units_indexed: written + reused,
        units_coverage_only: coverage_only,
        units_skipped: skipped,
        truncated,
    };
    crate::knowledge::kb::write_atomic(
        &manifest_path(project_path, config),
        serde_json::to_string_pretty(&manifest)?.as_bytes(),
    )?;
    write_scope_md(&root)?;

    Ok(IndexReport {
        manifest,
        written,
        reused,
    })
}

/// Advisory-only contract, written next to the data it governs.
fn write_scope_md(root: &Path) -> Result<()> {
    let body = "# Structural index scope\n\n\
        This index is ADVISORY ONLY. It is a deterministic hint layer for\n\
        agents and humans: symbol locations, callers/callees, and dependency\n\
        edges may be approximate (regex precision) or missing (coverage-only\n\
        units). The source of truth for \"does this symbol exist / who calls\n\
        it\" is always live search (grep / the compiler / the language\n\
        server), never this cache.\n\n\
        Backends per unit, strongest first: `ast` (tree-sitter, exact spans),\n\
        `regex` (lexical heuristics), `coverage` (file presence only —\n\
        queries answer `unavailable`). A missing unit or `status: partial`\n\
        manifest means \"not indexed\", never \"does not exist\".\n";
    crate::knowledge::kb::write_atomic(&root.join("SCOPE.md"), body.as_bytes())?;
    Ok(())
}

fn unit_key(extractor: &str, language: &str, digest: &str) -> String {
    crate::util::fnv1a64_hex(format!("{SCHEMA_VERSION}|{extractor}|{language}|{digest}").as_bytes())
}

fn unit_path(project_path: &Path, config: &NikiConfig, key: &str) -> PathBuf {
    units_dir(project_path, config)
        .join(&key[..2])
        .join(&key[2..4])
        .join(format!("{key}.json"))
}

/// Regex extraction: symbols via the shared baseline, imports per language,
/// call edges via lexical `name(` scans bounded to enclosing-symbol windows.
fn regex_extract(
    content: &str,
    language: &str,
    rel: &Path,
) -> (Vec<IndexedSymbol>, Vec<String>, Vec<CallEdge>) {
    let base = crate::knowledge::symbol_index::extract_symbols(content, regex_lang(language), rel);
    let mut symbols: Vec<IndexedSymbol> = base
        .iter()
        .map(|s| IndexedSymbol {
            name: s.name.clone(),
            kind: format!("{:?}", s.kind).to_lowercase(),
            line: s.line_number,
            end_line: s.line_number,
            definition: s.definition.clone(),
        })
        .collect();
    symbols.sort_by_key(|s| s.line);
    // Approximate spans: until the next definition in the same file.
    for i in 0..symbols.len() {
        let end = if i + 1 < symbols.len() {
            symbols[i + 1].line.saturating_sub(1)
        } else {
            symbols[i].line + 50
        };
        symbols[i].end_line = end.max(symbols[i].line);
    }

    let imports = extract_imports(content, language);
    let lines: Vec<&str> = content.lines().collect();
    let names: HashSet<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    let mut calls = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if calls.len() >= MAX_EDGES_PER_FILE {
            break;
        }
        let lineno = idx + 1;
        for name in &names {
            if line_contains_call(line, name) && !is_definition_line(line, name, language) {
                let caller = enclosing_symbol(&symbols, lineno);
                calls.push(CallEdge {
                    caller,
                    callee: (*name).to_string(),
                    line: lineno,
                });
            }
        }
    }
    (symbols, imports, calls)
}

fn line_contains_call(line: &str, name: &str) -> bool {
    // Cheap lexical check: `name` followed by `(` with an identifier boundary
    // before it. Deliberately approximate (precision: regex).
    let mut search = line;
    while let Some(pos) = search.find(name) {
        let prev = if pos == 0 {
            None
        } else {
            Some(search.as_bytes()[pos - 1])
        };
        let before_ok = prev.is_none_or(|b| !b.is_ascii_alphanumeric() && b != b'_');
        let after = &search[pos + name.len()..];
        let after_ok = after.trim_start().starts_with('(');
        if before_ok && after_ok {
            return true;
        }
        search = &search[pos + 1..];
        if search.is_empty() {
            break;
        }
    }
    false
}

fn is_definition_line(line: &str, name: &str, language: &str) -> bool {
    let t = line.trim_start();
    match language {
        "rust" => t.contains(&format!("fn {name}")),
        "python" => t.starts_with(&format!("def {name}")),
        "go" => t.starts_with("func ") && t.contains(name),
        _ => {
            (t.starts_with("function ") && t.contains(name))
                || t.starts_with(&format!("const {name}"))
                || t.starts_with(&format!("let {name}"))
        }
    }
}

fn enclosing_symbol(symbols: &[IndexedSymbol], lineno: usize) -> String {
    symbols
        .iter()
        .rev()
        .find(|s| s.line <= lineno)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "<top>".to_string())
}

/// Raw import/module references per language (advisory strings, resolved
/// lazily by [`StructuralIndex::reverse_dependency_map`]).
fn extract_imports(content: &str, language: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("//") || t.starts_with('#') || t.starts_with('*') {
            continue;
        }
        match language {
            "rust" => {
                if let Some(rest) = t.strip_prefix("use ") {
                    let path = rest.split([';', ' ', '{', '(']).next().unwrap_or("").trim();
                    if !path.is_empty() {
                        out.push(path.to_string());
                    }
                } else if let Some(rest) = t.strip_prefix("mod ") {
                    let name = rest.split([';', ' ', '{']).next().unwrap_or("").trim();
                    if !name.is_empty() {
                        out.push(name.to_string());
                    }
                }
            }
            "typescript" | "tsx" | "javascript" => {
                for marker in [" from \"", " from '", "require(\"", "require('"] {
                    if let Some(pos) = t.find(marker) {
                        let rest = &t[pos + marker.len()..];
                        let end = rest.find(['\"', '\'']).unwrap_or(rest.len());
                        let dep = rest[..end].trim();
                        if !dep.is_empty() {
                            out.push(dep.to_string());
                        }
                    }
                }
            }
            "python" => {
                if let Some(rest) = t.strip_prefix("from ") {
                    let module = rest.split_whitespace().next().unwrap_or("");
                    if !module.is_empty() {
                        out.push(module.to_string());
                    }
                } else if let Some(rest) = t.strip_prefix("import ") {
                    for part in rest.split(',') {
                        let module = part.split_whitespace().next().unwrap_or("");
                        if !module.is_empty() {
                            out.push(module.to_string());
                        }
                    }
                }
            }
            "go" => {
                let trimmed = t.trim_matches('"');
                if (t.starts_with('"') || t.starts_with('`')) && trimmed.contains('/') {
                    out.push(trimmed.trim_matches('`').to_string());
                }
            }
            _ => {}
        }
    }
    out.sort();
    out.dedup();
    out
}

/// In-memory query view over a built index.
pub struct StructuralIndex {
    pub manifest: IndexManifest,
    pub units: Vec<UnitFile>,
    /// Snapshot anchor this index was built under.
    pub state_ref: String,
}

/// Load the manifest + all cached units for querying.
pub fn open_index(project_path: &Path, config: &NikiConfig) -> Result<StructuralIndex> {
    let manifest: IndexManifest = serde_json::from_str(&std::fs::read_to_string(manifest_path(
        project_path,
        config,
    ))?)?;
    let mut units = Vec::new();
    let dir = units_dir(project_path, config);
    if dir.is_dir() {
        for entry in WalkDir::new(&dir).into_iter().flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(entry.path())
                && let Ok(unit) = serde_json::from_str::<UnitFile>(&content)
            {
                units.push(unit);
            }
        }
    }
    units.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(StructuralIndex {
        state_ref: manifest.snapshot_id.clone(),
        manifest,
        units,
    })
}

/// One located symbol.
#[derive(Debug, Clone)]
pub struct SymbolLoc {
    pub file: String,
    pub line: usize,
    pub kind: String,
    pub precision: Precision,
}

/// One caller site.
#[derive(Debug, Clone)]
pub struct CallSite {
    pub file: String,
    pub line: usize,
    pub caller: String,
    pub precision: Precision,
}

fn precision_of(backend: &str) -> Precision {
    match backend {
        "ast" => Precision::Ast,
        "regex" => Precision::Regex,
        _ => Precision::Unavailable,
    }
}

impl StructuralIndex {
    /// Locate symbols by exact name across all units.
    pub fn locate_symbol(&self, name: &str) -> Vec<SymbolLoc> {
        let mut out = Vec::new();
        for unit in &self.units {
            if unit.backend == "coverage" {
                continue;
            }
            for sym in &unit.symbols {
                if sym.name == name {
                    out.push(SymbolLoc {
                        file: unit.path.clone(),
                        line: sym.line,
                        kind: sym.kind.clone(),
                        precision: precision_of(&unit.backend),
                    });
                }
            }
        }
        out
    }

    /// Who calls `name` (lexical `name(` sites attributed to enclosing symbols).
    pub fn find_callers(&self, name: &str) -> Vec<CallSite> {
        let mut out = Vec::new();
        for unit in &self.units {
            if unit.backend == "coverage" {
                continue;
            }
            for edge in &unit.calls {
                if edge.callee == name {
                    out.push(CallSite {
                        file: unit.path.clone(),
                        line: edge.line,
                        caller: edge.caller.clone(),
                        precision: precision_of(&unit.backend),
                    });
                }
            }
        }
        out.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        out
    }

    /// Who `name` calls (callees attributed to it as the enclosing caller).
    pub fn find_callees(&self, name: &str) -> Vec<CallSite> {
        let mut out = Vec::new();
        for unit in &self.units {
            if unit.backend == "coverage" {
                continue;
            }
            for edge in &unit.calls {
                if edge.caller == name {
                    out.push(CallSite {
                        file: unit.path.clone(),
                        line: edge.line,
                        caller: edge.callee.clone(),
                        precision: precision_of(&unit.backend),
                    });
                }
            }
        }
        out.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
        out
    }

    /// The function span containing (`file`, `line`), if the file is indexed.
    /// Returns `None` with honest `unavailable` semantics for coverage-only
    /// units and unindexed files — never a fabricated span.
    pub fn function_boundary(&self, file: &str, line: usize) -> Option<(usize, usize, Precision)> {
        let unit = self.units.iter().find(|u| u.path == file)?;
        if unit.backend == "coverage" || unit.symbols.is_empty() {
            return None;
        }
        let mut best: Option<&IndexedSymbol> = None;
        for sym in &unit.symbols {
            if sym.line <= line && best.is_none_or(|b: &IndexedSymbol| b.line <= sym.line) {
                best = Some(sym);
            }
        }
        best.map(|s| (s.line, s.end_line.max(line), precision_of(&unit.backend)))
    }

    /// Map each indexed file to the files that import it (basename matching
    /// on the last import segment — heuristic, advisory).
    pub fn reverse_dependency_map(&self) -> HashMap<String, Vec<String>> {
        let mut stem_to_files: HashMap<String, Vec<String>> = HashMap::new();
        for unit in &self.units {
            let stem = Path::new(&unit.path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            stem_to_files
                .entry(stem)
                .or_default()
                .push(unit.path.clone());
            // Module-path match (rust `foo::bar`, python `foo.bar`, go last
            // segment): index every dotted segment too.
            for seg in unit.path.replace('\\', "/").split('/') {
                let seg_stem = seg.split('.').next().unwrap_or("");
                if !seg_stem.is_empty() {
                    stem_to_files
                        .entry(seg_stem.to_string())
                        .or_default()
                        .push(unit.path.clone());
                }
            }
        }
        let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
        for unit in &self.units {
            for import in &unit.imports {
                let key = import
                    .rsplit(['/', '.', ':'])
                    .next()
                    .unwrap_or("")
                    .trim_matches(|c| c == '"' || c == '\'' || c == '`' || c == ')');
                if key.is_empty() {
                    continue;
                }
                if let Some(targets) = stem_to_files.get(key) {
                    for target in targets {
                        if target != &unit.path {
                            reverse
                                .entry(target.clone())
                                .or_default()
                                .push(unit.path.clone());
                        }
                    }
                }
            }
        }
        for importers in reverse.values_mut() {
            importers.sort();
            importers.dedup();
        }
        reverse
    }
}

/// Tree-sitter extraction layer. Compiled only with the `ast` Cargo feature;
/// without it every file falls through to the regex baseline.
#[cfg(feature = "ast")]
mod ast {
    use super::{IndexedSymbol, extract_imports, regex_lang};
    use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

    pub struct AstExtraction {
        pub symbols: Vec<IndexedSymbol>,
        pub imports: Vec<String>,
        pub calls: Vec<super::CallEdge>,
    }

    /// False only if the runtime itself is unusable; per-language failures
    /// fall back to regex inside [`super::ast_extract`].
    pub fn available() -> bool {
        true
    }

    fn language_for(language: &str) -> Option<tree_sitter::Language> {
        match language {
            "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
            "javascript" => Some(tree_sitter_javascript::LANGUAGE.into()),
            "typescript" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
            "python" => Some(tree_sitter_python::LANGUAGE.into()),
            "go" => Some(tree_sitter_go::LANGUAGE.into()),
            _ => None,
        }
    }

    fn query_for(language: &str) -> &'static str {
        match language {
            "rust" => {
                r#"
                (function_item name: (identifier) @fname) @func
                (struct_item name: (type_identifier) @fname)
                (trait_item name: (type_identifier) @fname)
                (mod_item name: (identifier) @fname)
                (call_expression function: (identifier) @call)
                (call_expression function: (scoped_identifier name: (identifier) @call))
                (call_expression function: (field_expression field: (field_identifier) @call))
            "#
            }
            "javascript" => {
                r#"
                (function_declaration name: (identifier) @fname) @func
                (class_declaration name: (identifier) @fname)
                (variable_declarator name: (identifier) @fname value: [(arrow_function) (function_expression)]) @func
                (method_definition name: (property_identifier) @fname)
                (call_expression function: (identifier) @call)
                (call_expression function: (member_expression property: (property_identifier) @call))
            "#
            }
            "typescript" | "tsx" => {
                r#"
                (function_declaration name: (identifier) @fname) @func
                (class_declaration name: (identifier) @fname)
                (interface_declaration name: (type_identifier) @fname)
                (type_alias_declaration name: (type_identifier) @fname)
                (variable_declarator name: (identifier) @fname value: [(arrow_function) (function_expression)]) @func
                (method_definition name: (property_identifier) @fname)
                (call_expression function: (identifier) @call)
                (call_expression function: (member_expression property: (property_identifier) @call))
            "#
            }
            "python" => {
                r#"
                (function_definition name: (identifier) @fname) @func
                (class_definition name: (identifier) @fname)
                (call function: (identifier) @call)
                (call function: (attribute attribute: (identifier) @call))
            "#
            }
            "go" => {
                r#"
                (function_declaration name: (field_identifier) @fname) @func
                (method_declaration name: (field_identifier) @fname) @func
                (type_declaration (type_spec name: (type_identifier) @fname))
                (call_expression function: (identifier) @call)
                (call_expression function: (selector_expression field: (field_identifier) @call))
            "#
            }
            _ => "",
        }
    }

    pub fn extract(content: &str, language: &str) -> Option<AstExtraction> {
        let ts_lang = language_for(language)?;
        let mut parser = Parser::new();
        parser.set_language(&ts_lang).ok()?;
        // No explicit timeout: inputs are capped at MAX_FILE_BYTES and the
        // caller falls back to regex on any failure, so a slow parse degrades
        // rather than hangs the build.
        let tree = parser.parse(content, None)?;
        let query = Query::new(&ts_lang, query_for(language)).ok()?;
        let names = query.capture_names().to_vec();
        let fname_idx = names.iter().position(|n| *n == "fname")? as u32;
        let call_idx = names.iter().position(|n| *n == "call")? as u32;

        let mut symbols = Vec::new();
        let mut raw_calls: Vec<(String, usize)> = Vec::new();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), content.as_bytes());
        while let Some(m) = matches.next() {
            for cap in m.captures {
                let node = cap.node;
                let text = node.utf8_text(content.as_bytes()).ok()?.to_string();
                if text.is_empty() {
                    continue;
                }
                let line = node.start_position().row + 1;
                if cap.index == fname_idx {
                    symbols.push(IndexedSymbol {
                        name: text.clone(),
                        kind: "function".to_string(),
                        line,
                        end_line: node.end_position().row + 1,
                        definition: text,
                    });
                } else if cap.index == call_idx {
                    raw_calls.push((text, line));
                }
            }
        }
        if symbols.is_empty() {
            return None;
        }
        symbols.sort_by_key(|s| s.line);
        symbols.dedup_by(|a, b| a.name == b.name && a.line == b.line);

        let calls = raw_calls
            .into_iter()
            .map(|(callee, line)| {
                let caller = symbols
                    .iter()
                    .rev()
                    .find(|s| s.line <= line)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "<top>".to_string());
                super::CallEdge {
                    caller,
                    callee,
                    line,
                }
            })
            .take(super::MAX_EDGES_PER_FILE)
            .collect();
        let imports = extract_imports(content, regex_lang(language));
        Some(AstExtraction {
            symbols,
            imports,
            calls,
        })
    }
}

/// AST extraction entry point: `Some` on success, `None` to fall back to
/// regex. Without the `ast` feature this always returns `None`.
fn ast_extract(content: &str, language: &str) -> Option<AstData> {
    #[cfg(feature = "ast")]
    {
        ast::extract(content, language).map(|e| AstData {
            symbols: e.symbols,
            imports: e.imports,
            calls: e.calls,
        })
    }
    #[cfg(not(feature = "ast"))]
    {
        let _ = (content, language);
        None
    }
}

struct AstData {
    symbols: Vec<IndexedSymbol>,
    imports: Vec<String>,
    calls: Vec<CallEdge>,
}
