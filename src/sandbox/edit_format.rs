use anyhow::{Result, anyhow};
use similar::{ChangeTag, TextDiff};

/// A single search/replace edit block.
#[derive(Debug, Clone)]
pub struct EditBlock {
    pub search: String,
    pub replace: String,
}

/// Parse search/replace blocks from LLM output.
///
/// Supports two formats:
/// 1. SEARCH/REPLACE blocks (Aider-style):
///    <<<<<<< SEARCH
///    exact text to find
///    =======
///    replacement text
///    >>>>>>> REPLACE
///
/// 2. Fenced code blocks with edit markers:
///    ``` SEARCH
///    exact text to find
///    ```
///    ``` REPLACE
///    replacement text
///    ```
pub fn parse_edit_blocks(text: &str) -> Vec<EditBlock> {
    let mut blocks = Vec::new();

    // Try SEARCH/REPLACE format first
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() == "<<<<<<< SEARCH" {
            let mut search_lines = Vec::new();
            let mut replace_lines = Vec::new();
            let mut in_search = true;

            for next_line in lines.by_ref() {
                if next_line.trim() == "=======" {
                    in_search = false;
                    continue;
                }
                if next_line.trim() == ">>>>>>> REPLACE" {
                    break;
                }
                if in_search {
                    search_lines.push(next_line);
                } else {
                    replace_lines.push(next_line);
                }
            }

            let search = search_lines.join("\n");
            let replace = replace_lines.join("\n");

            if !search.is_empty() {
                blocks.push(EditBlock { search, replace });
            }
        }
    }

    // If no SEARCH/REPLACE blocks found, try fenced format
    if blocks.is_empty() {
        let mut lines = text.lines().peekable();
        while let Some(line) = lines.next() {
            if line.trim().starts_with("```") && line.contains("SEARCH") {
                let mut search_lines = Vec::new();
                for next_line in lines.by_ref() {
                    if next_line.trim() == "```" {
                        break;
                    }
                    search_lines.push(next_line);
                }

                // Look for REPLACE block
                while let Some(next_line) = lines.next() {
                    if next_line.trim().starts_with("```") && next_line.contains("REPLACE") {
                        let mut replace_lines = Vec::new();
                        for rep_line in lines.by_ref() {
                            if rep_line.trim() == "```" {
                                break;
                            }
                            replace_lines.push(rep_line);
                        }

                        let search = search_lines.join("\n");
                        let replace = replace_lines.join("\n");

                        if !search.is_empty() {
                            blocks.push(EditBlock { search, replace });
                        }
                        break;
                    }
                }
            }
        }
    }

    blocks
}

/// Apply edit blocks to file content with fuzzy matching.
///
/// Strategy:
/// 1. Try exact match first
/// 2. If exact match fails, try line-trimmed match
/// 3. If line-trimmed fails, try fuzzy match with similarity threshold
pub fn apply_edits(content: &str, edits: &[EditBlock]) -> Result<String> {
    let mut result = content.to_string();

    for edit in edits {
        match apply_single_edit(&result, edit)? {
            Some(edited) => result = edited,
            None => {
                return Err(anyhow!(
                    "Failed to apply edit: search text not found\nSearch: {:?}",
                    &edit.search[..edit.search.len().min(100)]
                ));
            }
        }
    }

    Ok(result)
}

/// Apply one search/replace pair to content. Returns the edited content if the
/// search text matched (via exact, trimmed, or fuzzy strategy), else `None`.
pub fn apply_single_edit_block(
    content: &str,
    search: &str,
    replace: &str,
) -> Result<Option<String>> {
    apply_single_edit(
        content,
        &EditBlock {
            search: search.to_string(),
            replace: replace.to_string(),
        },
    )
}

/// Try to apply a single edit block. Returns the edited content if applied successfully.
fn apply_single_edit(content: &str, edit: &EditBlock) -> Result<Option<String>> {
    // Strategy 1: Exact match
    if let Some(pos) = content.find(&edit.search) {
        let mut result = String::with_capacity(content.len() + edit.replace.len());
        result.push_str(&content[..pos]);
        result.push_str(&edit.replace);
        result.push_str(&content[pos + edit.search.len()..]);
        return Ok(Some(result));
    }

    // Strategy 2: Line-trimmed match
    let search_lines: Vec<&str> = edit.search.lines().collect();
    let content_lines: Vec<&str> = content.lines().collect();

    if let Some(start_line) = find_trimmed_match(&content_lines, &search_lines) {
        let end_line = start_line + search_lines.len();
        let mut result = String::new();

        // Write lines before the match
        for line in &content_lines[..start_line] {
            result.push_str(line);
            result.push('\n');
        }

        // Write replacement
        result.push_str(&edit.replace);
        if !edit.replace.ends_with('\n') {
            result.push('\n');
        }

        // Write lines after the match
        for line in &content_lines[end_line..] {
            result.push_str(line);
            result.push('\n');
        }

        return Ok(Some(result));
    }

    // Strategy 3: Fuzzy match with similarity threshold
    if let Some((start_line, similarity)) = find_fuzzy_match(&content_lines, &search_lines) {
        if similarity >= 0.8 {
            let end_line = start_line + search_lines.len();
            let mut result = String::new();

            // Write lines before the match
            for line in &content_lines[..start_line] {
                result.push_str(line);
                result.push('\n');
            }

            // Write replacement
            result.push_str(&edit.replace);
            if !edit.replace.ends_with('\n') {
                result.push('\n');
            }

            // Write lines after the match
            for line in &content_lines[end_line..] {
                result.push_str(line);
                result.push('\n');
            }

            return Ok(Some(result));
        }
    }

    Ok(None)
}

/// Find a match using line-trimmed comparison.
fn find_trimmed_match(content_lines: &[&str], search_lines: &[&str]) -> Option<usize> {
    if search_lines.is_empty() {
        return None;
    }

    'outer: for i in 0..=content_lines.len().saturating_sub(search_lines.len()) {
        for (j, search_line) in search_lines.iter().enumerate() {
            let content_line = content_lines[i + j].trim();
            let search_line = search_line.trim();
            if content_line != search_line {
                continue 'outer;
            }
        }
        return Some(i);
    }
    None
}

/// Find a fuzzy match using sequence matching.
fn find_fuzzy_match(content_lines: &[&str], search_lines: &[&str]) -> Option<(usize, f64)> {
    if search_lines.is_empty() {
        return None;
    }

    let search_text = search_lines.join("\n");
    let mut best_match = None;
    let mut best_similarity = 0.0;

    // Slide a window of similar size across the content. Skip when the content
    // is shorter than the search block (it can't contain a match).
    let window_size = search_lines.len();
    if content_lines.len() < window_size {
        return None;
    }
    for i in 0..=content_lines.len().saturating_sub(window_size) {
        let window = &content_lines[i..i + window_size];
        let window_text = window.join("\n");

        let similarity = calculate_similarity(&search_text, &window_text);
        if similarity > best_similarity {
            best_similarity = similarity;
            best_match = Some(i);
        }
    }

    best_match.map(|pos| (pos, best_similarity))
}

/// Calculate similarity between two strings using SequenceMatcher.
fn calculate_similarity(a: &str, b: &str) -> f64 {
    let diff = TextDiff::from_lines(a, b);
    let mut matches = 0;
    let mut total = 0;

    for change in diff.iter_all_changes() {
        let value = change.value();
        match change.tag() {
            ChangeTag::Equal => matches += value.len(),
            _ => {}
        }
        total += value.len();
    }

    if total == 0 {
        0.0
    } else {
        matches as f64 / total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_search_replace_blocks() {
        let text = r#"
<<<<<<< SEARCH
fn hello() {
    println!("hello");
}
=======
fn hello() {
    println!("world");
}
>>>>>>> REPLACE
"#;
        let blocks = parse_edit_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].search.contains("println!(\"hello\")"));
        assert!(blocks[0].replace.contains("println!(\"world\")"));
    }

    #[test]
    fn test_apply_exact_match() {
        let content = r#"fn hello() {
    println!("hello");
}
"#;
        let edits = vec![EditBlock {
            search: "println!(\"hello\")".to_string(),
            replace: "println!(\"world\")".to_string(),
        }];
        let result = apply_edits(content, &edits).unwrap();
        assert!(result.contains("println!(\"world\")"));
        assert!(!result.contains("println!(\"hello\")"));
    }

    #[test]
    fn test_apply_trimmed_match() {
        let content = r#"fn hello() {
    println!("hello");
}
"#;
        let edits = vec![EditBlock {
            search: "  println!(\"hello\");".to_string(),
            replace: "  println!(\"world\");".to_string(),
        }];
        let result = apply_edits(content, &edits).unwrap();
        assert!(result.contains("println!(\"world\")"));
    }
}
