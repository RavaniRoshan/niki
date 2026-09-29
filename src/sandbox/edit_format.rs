use anyhow::{Result, anyhow};
use similar::{ChangeTag, TextDiff};

/// A single search/replace edit block.
#[derive(Debug, Clone)]
pub struct EditBlock {
    pub search: String,
    pub replace: String,
    /// Optional target file for this block. When set, the block is applied only
    /// to that file (by exact or suffix path match) instead of being matched
    /// against every file in the workspace. Binding the file prevents a block
    /// from silently matching the wrong file (see research report S4).
    pub file: Option<String>,
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
    let mut prev: Option<&str> = None;
    while let Some(line) = lines.next() {
        if line.trim() == "<<<<<<< SEARCH" {
            // Allow an optional `FILE: <path>` (or `*** <path>`) line immediately
            // preceding the block to bind the edit to a specific file (report S4).
            let file = prev
                .map(str::trim)
                .filter(|p| p.starts_with("FILE: ") || p.starts_with("*** "))
                .map(|p| {
                    if let Some((_, f)) = p.split_once(':') {
                        f.trim().to_string()
                    } else {
                        p.trim_start_matches("*** ").to_string()
                    }
                });

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

            // A block with nothing to search for is normally noise, and
            // dropping it is right — `apply_single_edit` would match at offset
            // 0 and insert there. But a *bound* block with an empty search is
            // how the contract says "create this file", and the parser used to
            // discard it as well, so `action: "create"` could be validated
            // and then silently vanish before it reached the applier.
            if !search.is_empty() || file.is_some() {
                blocks.push(EditBlock {
                    search,
                    replace,
                    file,
                });
            }
        }
        prev = Some(line);
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
                            blocks.push(EditBlock {
                                search,
                                replace,
                                file: None,
                            });
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
            file: None,
        },
    )
}

/// Try to apply a single edit block. Returns the edited content if applied successfully.
fn apply_single_edit(content: &str, edit: &EditBlock) -> Result<Option<String>> {
    // An edit must not be able to match text it just wrote.
    //
    // `X -> X + more` leaves `X` in place, so the next round's `find(X)` hits
    // again and inserts the same thing a second time. A live revision loop did
    // exactly that and produced, after three rounds:
    //
    //     numbers.iter().sum()    numbers.iter().sum()    numbers.iter().sum()
    //
    // This is a legitimate thing for a model to mean — emit a function
    // signature, then fill it in — so rejecting it is wrong (that was tried, and
    // it rejected the common case and cost more than it saved). Instead the
    // result is made *stable*: if applying the edit again would produce exactly
    // the text that is already there, it is a no-op and we report it as applied
    // rather than leaving the caller to re-apply it forever.
    //
    // "Already applied" means the occurrence we would match is *already* the
    // replacement — not merely that the replacement appears somewhere in the
    // file, which is true of most of a file after any edit.
    if let Some(pos) = content.find(&edit.search)
        && content[pos..].starts_with(&edit.replace)
    {
        return Ok(Some(content.to_string()));
    }

    // A blank `search` means append.
    //
    // It used to fall through to Strategy 1, where `content.find("")` is
    // `Some(0)` — so the replacement was silently inserted at the *top* of the
    // file. A model writing "add a function to this file" puts the new
    // function in `replace` and has no natural anchor to quote, which is
    // exactly the empty-anchor case, and the result was new code above the
    // imports and above the item it was supposed to be near.
    //
    // Appending is what an empty anchor means, and it is the only reading
    // that cannot scramble a file. Guarded on `replace` being non-empty,
    // because an empty anchor with an empty replacement is nothing at all.
    // Blank rather than strictly empty: a model that cannot think of an anchor
    // writes `"   "` as readily as `""`, and the two mean the same thing.
    if edit.search.trim().is_empty() && !edit.replace.trim().is_empty() {
        let mut result = String::with_capacity(content.len() + edit.replace.len() + 1);
        result.push_str(content);
        if !content.ends_with('\n') && !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&edit.replace);
        if !edit.replace.ends_with('\n') {
            result.push('\n');
        }
        return Ok(Some(result));
    }

    // Strategy 1: Exact match, and it has to be *the* match.
    //
    // `find` takes the first occurrence, so a `search` that appears twice
    // silently edited the first one. That is the worst failure class there
    // is: the edit applies, the run reports success, and the change lands
    // somewhere the model did not mean. Every reference harness refuses it —
    // opencode ("Provide more surrounding context or set replaceAll"),
    // Cline ("multiple occurrences"), OpenHands (lists the line numbers),
    // pi (uniqueness counted in normalized space), Aider (matches only a
    // chunk long enough to be unique), Codex (a monotonic line index across
    // hunks so an early miss cannot silently retarget a later one).
    //
    // We report both cases distinctly, because the fix differs: zero means the
    // anchor is wrong, many means it is too short. Neither is fixed by
    // guessing which occurrence was meant.
    let mut occurrences = content.match_indices(&edit.search).map(|(i, _)| i);
    let first = occurrences.next();
    let second = occurrences.next();
    // The count is reported rather than inferred from a second offset: the
    // first version printed the offset as if it were a count, and said "matches
    // 26 times" for a file where it matched twice.
    let matches = first.is_some() as usize
        + second.is_some() as usize
        + content.matches(&edit.search).count().saturating_sub(2);
    match (first, second) {
        (Some(pos), None) => {
            let mut result = String::with_capacity(content.len() + edit.replace.len());
            result.push_str(&content[..pos]);
            result.push_str(&edit.replace);
            result.push_str(&content[pos + edit.search.len()..]);
            return Ok(Some(result));
        }
        (Some(_), Some(_)) => {
            let first_at = content.find(&edit.search).unwrap_or(0);
            let first_line = content[..first_at].lines().count().max(1);
            return Err(anyhow::anyhow!(
                "the anchor for this edit matches {matches} times in the file (first at line \
                 {first_line}). Add more surrounding context so it is unique, or split the \
                 edit. Applying it to an arbitrary one of them would change code the model \
                 did not name."
            ));
        }
        (None, _) => {
            // Fall through to the looser strategies below.
        }
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
    if let Some((start_line, similarity)) = find_fuzzy_match(&content_lines, &search_lines)
        && similarity >= 0.8
    {
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
        if change.tag() == ChangeTag::Equal {
            matches += value.len()
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
            file: None,
        }];
        let result = apply_edits(content, &edits).unwrap();
        assert!(result.contains("println!(\"world\")"));
        assert!(!result.contains("println!(\"hello\")"));
    }

    #[test]
    fn test_parse_file_binding() {
        let text = r#"
FILE: src/main.rs
<<<<<<< SEARCH
fn main() {}
=======
fn main() { println!("hi"); }
>>>>>>> REPLACE
"#;
        let blocks = parse_edit_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].file.as_deref(), Some("src/main.rs"));
    }
}
