use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// A symbol extracted from source code (function, class, constant, etc.)
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file_path: PathBuf,
    pub line_number: usize,
    /// The raw text of the definition line (for context matching).
    pub definition: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Class,
    Struct,
    Module,
    Const,
    Trait,
    Interface,
    TypeAlias,
    Other,
}

/// Index of all symbols extracted from the project's source files.
#[derive(Debug, Clone, Default)]
pub struct SymbolIndex {
    /// The root directory this index was built from.
    project_dir: PathBuf,
    /// All symbols across all files.
    symbols: Vec<Symbol>,
    /// Reverse index: symbol name → files that define or reference it.
    references: HashMap<String, Vec<PathBuf>>,
}

impl SymbolIndex {
    /// Build a symbol index by scanning the project's source files.
    /// Supports Rust, TypeScript/JavaScript, Python, and Go.
    pub fn build(project_dir: &Path) -> Result<Self> {
        let mut symbols = Vec::new();
        let mut references: HashMap<String, Vec<PathBuf>> = HashMap::new();

        for entry in WalkDir::new(project_dir)
            .max_depth(5)
            .into_iter()
            .filter_entry(|e| {
                if e.depth() == 0 {
                    return true;
                }
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.')
                    && name != "node_modules"
                    && name != "target"
                    && name != "__pycache__"
                    && name != "venv"
                    && name != ".venv"
                    && name != "dist"
                    && name != "build"
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            let ext = path.extension().and_then(|e| e.to_str());
            let language = match ext {
                Some("rs") => Some("rust"),
                Some("ts") | Some("tsx") => Some("typescript"),
                Some("js") | Some("jsx") => Some("javascript"),
                Some("py") => Some("python"),
                Some("go") => Some("go"),
                _ => None,
            };

            let Some(language) = language else {
                continue;
            };

            let rel_path = path.strip_prefix(project_dir).unwrap_or(path);
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let file_symbols = extract_symbols(&content, language, rel_path);
            for sym in &file_symbols {
                references
                    .entry(sym.name.clone())
                    .or_default()
                    .push(rel_path.to_path_buf());
            }
            symbols.extend(file_symbols);
        }

        Ok(SymbolIndex {
            project_dir: project_dir.to_path_buf(),
            symbols,
            references,
        })
    }

    /// Rank files by relevance to the given query terms.
    /// This is a lightweight PageRank-like approach: files that define or
    /// reference the query terms get higher scores. Terms that appear in
    /// multiple files get lower per-file scores (specificity weighting).
    ///
    /// Returns (file_path, score) pairs sorted by descending score.
    pub fn rank_files_by_relevance(&self, query_terms: &[String]) -> Vec<(PathBuf, f64)> {
        let mut scores: HashMap<PathBuf, f64> = HashMap::new();

        for term in query_terms {
            let term_lower = term.to_lowercase();
            if let Some(files) = self.references.get(&term_lower) {
                // Specificity weighting: rarer terms contribute more
                let weight = 1.0 / (files.len() as f64).sqrt();
                for file in files {
                    *scores.entry(file.clone()).or_insert(0.0) += weight;
                }
            }

            // Also match symbols that contain the query term
            for sym in &self.symbols {
                if sym.name.to_lowercase().contains(&term_lower) {
                    let weight = 1.0 / (self.symbols.len() as f64).sqrt();
                    *scores.entry(sym.file_path.clone()).or_insert(0.0) += weight;
                }
            }
        }

        let mut result: Vec<_> = scores.into_iter().collect();
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        result
    }

    /// Get a summary of the most important files for a given query,
    /// formatted as a token-budgeted string suitable for LLM context injection.
    pub fn render_relevant_context(
        &self,
        query_terms: &[String],
        max_files: usize,
        max_per_file: usize,
    ) -> String {
        let ranked = self.rank_files_by_relevance(query_terms);
        let mut out = String::new();
        let mut file_count = 0;

        for (path, score) in ranked.iter().take(max_files) {
            let full_path = self.project_dir.join(path);
            if let Ok(content) = fs::read_to_string(&full_path) {
                out.push_str(&format!(
                    "### {} (relevance: {:.2})\n",
                    path.display(),
                    score
                ));
                let preview: String = content.chars().take(max_per_file).collect();
                out.push_str(&format!("```\n{}\n```\n\n", preview));
                file_count += 1;
            }
            if file_count >= max_files {
                break;
            }
        }

        if file_count == 0 {
            out.push_str("(no source files matched the query)\n");
        }

        out
    }

    /// Get the project directory from any indexed symbol.
    fn project_dir(&self) -> PathBuf {
        if let Some(first) = self.symbols.first() {
            first
                .file_path
                .parent()
                .unwrap_or(Path::new("."))
                .to_path_buf()
        } else {
            PathBuf::from(".")
        }
    }

    /// Return all symbols for a given file.
    pub fn symbols_for_file(&self, file_path: &Path) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|s| s.file_path == file_path)
            .collect()
    }

    /// Return all symbol names.
    pub fn all_symbol_names(&self) -> Vec<&str> {
        self.symbols.iter().map(|s| s.name.as_str()).collect()
    }

    /// Get the project root directory this index was built from.
    pub fn project_root(&self) -> &Path {
        &self.project_dir
    }
}

/// Extract symbols from source code based on language.
/// This is a lightweight regex-based approach — not a full AST parser,
/// but sufficient for ranking files by relevance to a query.
fn extract_symbols(content: &str, language: &str, file_path: &Path) -> Vec<Symbol> {
    let mut symbols = Vec::new();

    match language {
        "rust" => {
            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("fn ") || trimmed.starts_with("pub fn ") {
                    if let Some(name_start) = trimmed.find("fn ") {
                        let rest = &trimmed[name_start + 3..];
                        if let Some(name_end) = rest.find('(') {
                            let name = rest[..name_end].trim();
                            if !name.is_empty() && !name.contains(' ') {
                                symbols.push(Symbol {
                                    name: name.to_string(),
                                    kind: SymbolKind::Function,
                                    file_path: file_path.to_path_buf(),
                                    line_number: line_num + 1,
                                    definition: line.trim().to_string(),
                                });
                            }
                        }
                    }
                }
                if trimmed.starts_with("struct ") {
                    let rest = trimmed.trim_start_matches("pub ").trim_start();
                    if rest.starts_with("struct ") {
                        let name = rest[7..].split_whitespace().next().unwrap_or("");
                        if !name.is_empty() {
                            symbols.push(Symbol {
                                name: name.to_string(),
                                kind: SymbolKind::Struct,
                                file_path: file_path.to_path_buf(),
                                line_number: line_num + 1,
                                definition: line.trim().to_string(),
                            });
                        }
                    }
                }
                if trimmed.starts_with("trait ") {
                    let rest = trimmed.trim_start().trim_start_matches("pub ");
                    if rest.starts_with("trait ") {
                        let name = rest[6..].split_whitespace().next().unwrap_or("");
                        if !name.is_empty() {
                            symbols.push(Symbol {
                                name: name.to_string(),
                                kind: SymbolKind::Trait,
                                file_path: file_path.to_path_buf(),
                                line_number: line_num + 1,
                                definition: line.trim().to_string(),
                            });
                        }
                    }
                }
                if trimmed.starts_with("mod ") {
                    let name = trimmed[4..].split_whitespace().next().unwrap_or("");
                    // Strip trailing semicolons, braces, etc.
                    let name = name.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_');
                    if !name.is_empty() && !name.starts_with("pub") {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Module,
                            file_path: file_path.to_path_buf(),
                            line_number: line_num + 1,
                            definition: line.trim().to_string(),
                        });
                    }
                }
            }
        }
        "typescript" | "javascript" => {
            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("function ") {
                    let rest = &trimmed[9..];
                    if let Some(name_end) = rest.find('(') {
                        let name = rest[..name_end].trim();
                        if !name.is_empty() {
                            symbols.push(Symbol {
                                name: name.to_string(),
                                kind: SymbolKind::Function,
                                file_path: file_path.to_path_buf(),
                                line_number: line_num + 1,
                                definition: line.trim().to_string(),
                            });
                        }
                    }
                }
                if trimmed.starts_with("class ") {
                    let name = trimmed[6..].split_whitespace().next().unwrap_or("");
                    if !name.is_empty() {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Class,
                            file_path: file_path.to_path_buf(),
                            line_number: line_num + 1,
                            definition: line.trim().to_string(),
                        });
                    }
                }
                if trimmed.starts_with("interface ") {
                    let name = trimmed[10..].split_whitespace().next().unwrap_or("");
                    if !name.is_empty() {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Interface,
                            file_path: file_path.to_path_buf(),
                            line_number: line_num + 1,
                            definition: line.trim().to_string(),
                        });
                    }
                }
            }
        }
        "python" => {
            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("def ") {
                    let rest = &trimmed[4..];
                    if let Some(name_end) = rest.find('(') {
                        let name = rest[..name_end].trim();
                        if !name.is_empty() {
                            symbols.push(Symbol {
                                name: name.to_string(),
                                kind: SymbolKind::Function,
                                file_path: file_path.to_path_buf(),
                                line_number: line_num + 1,
                                definition: line.trim().to_string(),
                            });
                        }
                    }
                }
                if trimmed.starts_with("class ") {
                    let rest = &trimmed[6..];
                    let name_end = rest
                        .find('(')
                        .or_else(|| rest.find(':'))
                        .unwrap_or_else(|| rest.len());
                    let name = rest[..name_end].trim();
                    if !name.is_empty() {
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind: SymbolKind::Class,
                            file_path: file_path.to_path_buf(),
                            line_number: line_num + 1,
                            definition: line.trim().to_string(),
                        });
                    }
                }
            }
        }
        "go" => {
            for (line_num, line) in content.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("func ") {
                    let rest = &trimmed[5..];
                    if let Some(name_end) = rest.find('(') {
                        let name = rest[..name_end].trim();
                        if !name.is_empty() && !name.contains(' ') && !name.starts_with('(') {
                            symbols.push(Symbol {
                                name: name.to_string(),
                                kind: SymbolKind::Function,
                                file_path: file_path.to_path_buf(),
                                line_number: line_num + 1,
                                definition: line.trim().to_string(),
                            });
                        }
                    }
                }
            }
        }
        _ => {}
    }

    symbols
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_rust_symbols() {
        let code = r#"
pub fn calculate_total(items: Vec<Item>) -> u32 {
    items.iter().sum()
}

struct Order {
    id: u64,
}

trait Processable {
    fn process(&self);
}

mod utils;
"#;
        let symbols = extract_symbols(code, "rust", Path::new("test.rs"));
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"calculate_total"));
        assert!(names.contains(&"Order"));
        assert!(names.contains(&"Processable"));
        assert!(names.contains(&"utils"));

        let funcs: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .collect();
        assert_eq!(funcs.len(), 2); // calculate_total + process in trait
        assert!(funcs.iter().any(|f| f.name == "calculate_total"));
    }

    #[test]
    fn test_extract_python_symbols() {
        let code = r#"
def calculate_total(items):
    return sum(items)

class Order:
    def __init__(self, id):
        self.id = id
"#;
        let symbols = extract_symbols(code, "python", Path::new("test.py"));
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"calculate_total"));
        assert!(names.contains(&"Order"));
    }

    #[test]
    fn test_extract_ts_symbols() {
        let code = r#"
function calculateTotal(items: Item[]): number {
    return items.length;
}

class Order {
    constructor(public id: number) {}
}

interface Processable {
    process(): void;
}
"#;
        let symbols = extract_symbols(code, "typescript", Path::new("test.ts"));
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"calculateTotal"));
        assert!(names.contains(&"Order"));
        assert!(names.contains(&"Processable"));
    }

    #[test]
    fn test_symbol_index_ranking() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        fs::write(
            dir.join("user_service.rs"),
            "pub fn get_user(id: u64) -> User {\n    User { id }\n}\n",
        )
        .unwrap();

        fs::write(
            dir.join("order_service.rs"),
            "pub fn process_order(id: u64) -> Order {\n    Order { id }\n}\n",
        )
        .unwrap();

        let index = SymbolIndex::build(dir).unwrap();

        let ranked = index.rank_files_by_relevance(&["user".to_string()]);
        assert!(!ranked.is_empty());
        assert_eq!(ranked[0].0, PathBuf::from("user_service.rs"));

        let ranked = index.rank_files_by_relevance(&["order".to_string()]);
        assert!(!ranked.is_empty());
        assert_eq!(ranked[0].0, PathBuf::from("order_service.rs"));
    }

    #[test]
    fn test_build_handles_nonexistent_dir() {
        let result = SymbolIndex::build(Path::new("/nonexistent/path/12345"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_render_relevant_context() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        fs::write(
            dir.join("api.rs"),
            "pub fn handle_user_query(query: String) -> User {\n    User { name: query }\n}\n",
        )
        .unwrap();

        let index = SymbolIndex::build(dir).unwrap();
        let rendered = index.render_relevant_context(&["user".to_string()], 5, 200);
        assert!(rendered.contains("api.rs"));
        assert!(rendered.contains("handle_user_query"));
    }

    #[test]
    fn test_render_relevant_context_empty_query() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path();

        fs::write(dir.join("api.rs"), "pub fn test_fn() {}\n").unwrap();

        let index = SymbolIndex::build(dir).unwrap();
        let rendered =
            index.render_relevant_context(&["nonexistent_symbol".to_string()], 5, 200);
        assert!(rendered.contains("no source files matched"));
    }
}
