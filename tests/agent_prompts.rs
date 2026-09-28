//! Prompts are the product, and the one defect that made a first-time user's
//! run fail was in one of them.
//!
//! `prompts/coder.md` taught the edit format twice, in two incompatible ways: a
//! `<<<<<<< SEARCH` fenced block, and a JSON schema whose `edits[].search` field
//! wanted a plain string. Every rule and both worked examples reinforced the
//! block. A model that followed the most concrete thing on screen produced the
//! block and left `search` as `""`, which the validator then rejected — so the
//! run died at the Coder, on every attempt, with a message blaming the model.
//!
//! Measured on `qwen2.5-coder:3b`, the model this project's own README tells
//! first-time users to install. Same model, same schema, same file contents:
//!
//!   block format + JSON schema   ->  search: ""   (every attempt)
//!   single JSON format           ->  search: "<correct text>"
//!
//! Only the prompt differed. A live run then completed in 6.9s and wrote
//! correct Rust.

use std::path::Path;

/// A line that is a bare conflict-marker-style block opener, i.e. the prompt is
/// *showing* the format rather than naming it in prose.
fn teaches_block_format(prompt: &str) -> Option<usize> {
    prompt.lines().position(|l| {
        let t = l.trim();
        t.starts_with("<<<<<<<") && !t.starts_with("`") && !t.starts_with("//")
    })
}

#[test]
fn the_coder_prompt_shows_exactly_one_edit_format() {
    let prompt =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/coder.md"))
            .expect("prompts/coder.md");

    assert_eq!(
        teaches_block_format(&prompt),
        None,
        "prompts/coder.md shows a `<<<<<<< SEARCH` block format. It also asks for a JSON \
         object with an `edits[].search` STRING, and a model that follows the block leaves \
         `search` empty — which fails validation on every attempt. A live run against \
         qwen2.5-coder:3b, the model the README tells first-time users to install, fails at \
         the Coder 100% of the time with this prompt and succeeds with a single JSON format."
    );
}

#[test]
fn the_coder_prompt_demonstrates_the_format_in_the_shape_it_must_be_emitted() {
    let prompt =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts/coder.md"))
            .expect("prompts/coder.md");

    // The worked example has to be the JSON the model is graded on. A prose
    // description of the edit format, however good, is not the same thing.
    assert!(
        prompt.contains("\"search\":") && prompt.contains("\"replace\":"),
        "the worked example must be a JSON object with `search` and `replace` fields — that is \
         the shape the schema requires and the shape the model has to copy"
    );
    assert!(
        prompt.contains("Respond with ONLY the raw JSON artifact"),
        "and the output instruction must be unmissable"
    );
}

/// Every prompt that asks for a JSON artifact must actually show one, because
/// the artifact *is* the contract — there is no fallback for a model that
/// guessed wrong.
#[test]
fn every_json_producing_prompt_shows_its_schema() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("prompts");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("prompts/") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("prompt is utf-8");
        if !text.contains("{{ artifact_schema }}") {
            continue;
        }
        checked += 1;
        assert!(
            text.contains("ONLY the raw JSON artifact"),
            "{} asks for a JSON artifact but never says the response must be nothing but JSON",
            path.display()
        );
    }
    assert!(
        checked >= 5,
        "only checked {checked} prompts — the extractor drifted as prompts/ changed"
    );
}
