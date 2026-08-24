//! Property-based tests for the parsing/validation surfaces.
//!
//! The canonical property ladder (proptest book, see research report Phase 4):
//!   1. never panic on arbitrary input
//!   2. accept all valid generated inputs
//!   3. roundtrip: parse(serialize(x)) == x
//!
//! Applied here to the TOML config surface and the minijinja prompt-template
//! rendering path. Any failure auto-shrinks into `proptest-regressions/`,
//! which must be committed.

use niki::config::types::NikiConfig;
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Ladder rung 1: the TOML config parser must classify arbitrary byte
    /// strings as Ok or Err — a panic is always a bug.
    #[test]
    fn config_parse_never_panics_on_arbitrary_input(input in ".*") {
        let _ = toml::from_str::<NikiConfig>(&input);
    }

    /// Ladder rung 3: default config survives serialize -> parse -> equality.
    ///
    /// DISCOVERED QUIRK (kept visible on purpose): `security.policies` is a
    /// std HashMap, so re-serialization may permute its sections between
    /// runs. Byte-identical output therefore cannot be asserted; we assert
    /// the roundtrip is stable up to line order. If deterministic niki.toml
    /// writes ever matter, switch that map to BTreeMap/IndexMap and tighten
    /// this back to byte equality.
    #[test]
    fn config_roundtrips_through_toml(_ignored in 0..1u8) {
        let original = NikiConfig::default();
        let first = toml::to_string(&original).expect("default config must serialize");
        let parsed: NikiConfig = toml::from_str(&first).expect("serialized config must re-parse");
        let second = toml::to_string(&parsed).unwrap();
        let canon = |s: String| {
            let mut v: Vec<String> = s.lines().map(|l| l.to_string()).collect();
            v.sort_unstable();
            v
        };
        prop_assert_eq!(canon(second), canon(first));
    }

    /// Structurally-valid-but-arbitrary TOML tables parse without panicking
    /// even when fields have wrong types (serde defaults absorb them).
    #[test]
    fn config_absorbs_wrong_typed_fields(
        junk_name in "[a-zA-Z_][a-zA-Z0-9_]{0,15}",
        junk_val in "(\"[^\"]*\"|-?[0-9]+|true|false|\\[\\])",
    ) {
        let doc = format!("{junk_name} = {junk_val}\n[general]\nlanguage = \"en\"\n");
        let _ = toml::from_str::<NikiConfig>(&doc);
    }
}

#[cfg(test)]
mod template_rendering {
    //! Prompt templates are minijinja templates rendered with agent context.
    //! Property: rendering arbitrary strings as templates either succeeds or
    //! returns an error — it must never panic (templates are user-editable).

    use minijinja::{Environment, context};

    fn render(template: &str) -> Result<String, minijinja::Error> {
        let mut env = Environment::new();
        env.add_template("t", template)?;
        let tmpl = env.get_template("t")?;
        tmpl.render(context! { task => "add a health endpoint", language => "rust" })
    }

    #[test]
    fn valid_template_with_context_roundtrips_content() {
        assert_eq!(
            render("Task: {{ task }}").unwrap(),
            "Task: add a health endpoint"
        );
    }

    #[test]
    fn malformed_templates_error_but_never_panic() {
        // Deterministic corpus of malformed inputs; arbitrary generation of
        // *semantically broken* jinja is low-value vs these known classes.
        for bad in [
            "{{ task }",           // unclosed expression
            "{% for x %}",         // malformed tag
            "{{ 1 / 0 }}",         // runtime error
            "{% include \"x\" %}", // unknown template
            "",                    // empty is fine but must not panic
        ] {
            let _ = render(bad);
        }
    }
}
