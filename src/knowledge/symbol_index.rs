use std::path::{Path, PathBuf};

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

/// Extract symbols from source code based on language.
/// This is a lightweight regex-based approach — not a full AST parser,
/// but sufficient for ranking files by relevance to a query.
/// Shared with the structural index as its guaranteed baseline backend.
pub(crate) fn extract_symbols(content: &str, language: &str, file_path: &Path) -> Vec<Symbol> {
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
                    let name_end = rest.find('(').or(rest.find(':')).unwrap_or(rest.len());
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
}
