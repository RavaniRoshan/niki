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

/// The error for an anchor that fits more than one place.
///
/// One message for both tiers that can detect it, so the fix a model is given
/// is the same either way: quote more context, or split the edit. The line
/// number is in lines rather than bytes because the reader is looking at a
/// file, not a buffer.
fn ambiguous_anchor(search: &str, matches: usize) -> anyhow::Error {
    let first_line = search.lines().count().max(1);
    anyhow::anyhow!(
        "the anchor for this edit matches {matches} times in the file (the first candidate \
         starts around line {first_line}). Add more surrounding context so it is unique, or \
         split the edit. Applying it to an arbitrary one of them would change code the model \
         did not name."
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
            return Err(ambiguous_anchor(&edit.search, matches));
        }
        (None, _) => {
            // Fall through to the looser strategies below.
        }
    }

    // Strategy 2: Line-trimmed match
    let search_lines: Vec<&str> = edit.search.lines().collect();
    let content_lines: Vec<&str> = content.lines().collect();

    // Uniqueness is enforced here too, and for the same reason as the exact
    // match: an anchor that fits two places is not a choice the harness may make
    // for the model. A search differing only in indentation from two blocks of
    // code matches exactly once and trimmed twice, and the second case was
    // editing the first silently.
    //
    // pi enforces uniqueness in *normalized* space for the same reason — the
    // match that counts is the one the strategy actually uses.
    let trimmed = find_trimmed_matches(&content_lines, &search_lines);
    if trimmed.len() > 1 {
        // Reported, not quietly treated as "no match". A caller that gets
        // `None` here learns only that the anchor was not found; the two
        // situations have different fixes — one is a typo, the other is an
        // anchor that needs more surrounding context — and the second is the
        // one a model cannot guess at.
        return Err(ambiguous_anchor(&edit.search, trimmed.len()));
    }
    if let Some(&start_line) = trimmed.first() {
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
fn find_trimmed_matches(content_lines: &[&str], search_lines: &[&str]) -> Vec<usize> {
    if search_lines.is_empty() || content_lines.len() < search_lines.len() {
        return Vec::new();
    }

    let mut found = Vec::new();
    'outer: for i in 0..=(content_lines.len() - search_lines.len()) {
        for (j, search_line) in search_lines.iter().enumerate() {
            if content_lines[i + j].trim() != search_line.trim() {
                continue 'outer;
            }
        }
        found.push(i);
        // Two is enough: the decision is "unique or not", and scanning the rest
        // of a large file to count a number nothing uses is not worth it.
        if found.len() == 2 {
            break;
        }
    }
    found
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

/// Whether an edit's `replace` text is **already present** in `content`.
///
/// This is the "already done" test, and it exists because of a measured
/// failure. A live run against `stealth/space-bunny-alpha` had the Coder use
/// the `edit` tool — applying the change to the worktree — and then submit a
/// `CodeDiff` describing the same change. The pipeline applies the submitted
/// artifact on top of what the tools already did, so every block's `search`
/// text was gone, every block went unmatched, and the run reported
/// *"the patch did not apply — asking the Coder to rebuild it"*.
///
/// That message describes a different failure. The patch was not wrong; it was
/// **already applied**, and a Coder that used the tools *and* submitted an
/// artifact — which the protocol invites — produced an unapplicable artifact by
/// construction.
pub fn is_already_applied(content: &str, search: &str, replace: &str) -> bool {
    // How much `replace` has to be for its presence to mean anything.
    //
    // The first version claimed "already applied" for a one-token replacement,
    // and its own test caught it: after a real edit, the file legitimately
    // contains `a + b` for reasons that have nothing to do with a later block
    // asking to replace *that*. A short replace is exactly where a coincidental
    // match is likely, and the consequence of being wrong is a silently
    // dropped edit.
    //
    // So a small replacement is never claimed. Being wrong in that direction is
    // the *safe* direction: it falls back to today's behaviour — reported as
    // unmatched — rather than accepting something that was not done.
    const MIN_SIGNIFICANT_CHARS: usize = 12;
    let significant = replace.chars().filter(|c| !c.is_whitespace()).count();
    if significant < MIN_SIGNIFICANT_CHARS && !replace.contains('\n') {
        return false;
    }
    if !content.contains(replace) {
        return false;
    }
    // If the search is *also* still present, the edit is ambiguous rather than
    // done. Only claim "already applied" when the thing we were told to find is
    // genuinely not there.
    !content.contains(search)
}

/// Apply one search/replace pair to content, treating an already-applied edit
/// as a success.
///
/// `None` still means "this block did not match and was not already done" —
/// the same contract as `apply_single_edit_block`. The difference is only for
/// the case the pipeline must not report as a failure.
pub fn apply_edit_block_or_already_done(
    content: &str,
    search: &str,
    replace: &str,
) -> Result<Option<String>> {
    if let Some(next) = apply_single_edit_block(content, search, replace)? {
        return Ok(Some(next));
    }
    if is_already_applied(content, search, replace) {
        return Ok(Some(content.to_string()));
    }
    Ok(None)
}

#[cfg(test)]
mod already_applied_tests {
    //! The "already done" test, and what it must not accept.
    //!
    //! Found by a live run: a Coder that used the `edit` tool and then
    //! submitted a `CodeDiff` describing the same change. The pipeline applies
    //! the artifact on top of what the tools already did, so the `search` text
    //! was gone, every block went unmatched, and the run said *"the patch did
    //! not apply"* — a failure that is not one.

    use super::{apply_edit_block_or_already_done, is_already_applied};

    const BEFORE: &str = "fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n";
    const AFTER: &str = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";

    /// The measured case: the change is in the file, the text it was told to
    /// find is not. The `replace` is a whole signature rather than a token,
    /// which is what the size rule requires.
    #[test]
    fn a_change_that_is_already_in_the_file_is_already_applied() {
        let search = "fn add(a: i32, b: i32) -> i32 {\n    a - b\n}";
        let replace = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}";
        assert!(
            is_already_applied(AFTER, search, replace),
            "the change is in the file and the thing it was told to find is not"
        );
        assert_eq!(
            apply_edit_block_or_already_done(AFTER, search, replace).unwrap(),
            Some(AFTER.to_string()),
            "and it must succeed without changing the file a second time"
        );
    }

    /// The ordinary case is untouched: a normal edit still applies.
    #[test]
    fn an_ordinary_edit_still_applies() {
        let search = "fn add(a: i32, b: i32) -> i32 {\n    a - b\n}";
        let replace = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}";
        assert_eq!(
            apply_edit_block_or_already_done(BEFORE, search, replace).unwrap(),
            Some(AFTER.to_string())
        );
    }

    /// A search that is nowhere and a replace that is present: claimed as
    /// already applied.
    ///
    /// That is the *intended* reading and the measured case, and it is worth
    /// being explicit that the helper cannot do better. It cannot know whether
    /// some earlier block wrote that text or whether it was always there; it
    /// knows only that the thing it was told to find is gone and the thing it
    /// was told to write is present, which is what "this was already done"
    /// looks like. The size rule below keeps that from firing on a coincidence.
    #[test]
    fn a_vanished_search_with_a_present_replace_counts_as_done() {
        let search = "a whole function that is not in this file at all";
        let replace = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}";
        assert!(is_already_applied(AFTER, search, replace));
    }

    /// The ambiguous case: the `search` is *still in the file*, so nothing can
    /// be said about whether the edit was done.
    ///
    /// Claiming "already applied" here would accept a block on the wrong
    /// evidence, and the all-or-nothing guarantee the sandboxes rely on is
    /// exactly what stops a stage writing half its changes.
    #[test]
    fn an_edit_whose_search_is_still_present_is_not_already_applied() {
        let search = "fn add(a: i32, b: i32) -> i32 {\n    a - b\n}";
        let replace = "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}";
        let content = format!("{AFTER}\n// kept for reference:\n{search}\n");
        assert!(
            content.contains(search),
            "the premise: the search is still there"
        );
        assert!(
            !is_already_applied(&content, search, replace),
            "with the search text still present this is ambiguous, not done"
        );
        let applied = apply_edit_block_or_already_done(&content, search, replace).unwrap();
        assert!(
            applied.is_some_and(|out| out != content),
            "the ordinary matcher must still handle an ambiguous block"
        );
    }

    /// A short replacement is never claimed, however much of it is present.
    ///
    /// This is the false positive the first version of the helper had: after a
    /// real edit the file contains `a + b` for reasons that have nothing to do
    /// with a later block asking to replace *that*. Claiming "already applied"
    /// there would silently accept an edit that was never made.
    #[test]
    fn a_short_replacement_is_never_claimed_as_applied() {
        assert!(
            !is_already_applied(AFTER, "a text that is nowhere", "a + b"),
            "a three-character replacement is far too likely to coincide"
        );
        assert_eq!(
            apply_edit_block_or_already_done(AFTER, "a text that is nowhere", "a + b").unwrap(),
            None
        );
    }

    /// An empty `replace` is never claimed. A deletion whose search is absent
    /// is indistinguishable from a typo, and accepting it would silently drop
    /// a change that was asked for.
    #[test]
    fn an_empty_replacement_is_never_already_applied() {
        assert!(
            !is_already_applied(AFTER, "gone from here", ""),
            "an empty replacement must not be treated as a done deletion"
        );
    }
}
