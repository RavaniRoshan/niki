pub mod anthropic;
pub mod failover;
pub mod google;
pub mod mock;
pub mod ollama;
pub mod openai;
pub mod provider;
pub mod repair;

pub use provider::*;

/// Walk a path through a JSON response, returning `None` if any step is missing
/// or is not a container.
///
/// A segment resolves against an object by key, or against an array when it
/// parses as a number — so `&["choices", "0", "message", "content"]` reads the
/// first choice's content. The two forms are mixed in every provider's response
/// shape, and a helper that handled only keys would push an `Index` back into
/// the call sites, which is where the panic was.
///
/// `serde_json::Value`'s `Index` panics on a non-object: `v["usage"]` on a
/// response with no `usage` key gives `Null`, and `Null["prompt_tokens"]` then
/// panics with "no entry found for key". Every provider in this module read its
/// token counts that way, so an OpenAI-compatible server that omits `usage` on
/// some response — a refusal, an error envelope from a gateway, a stream whose
/// final chunk has no usage, Ollama or vLLM or LM Studio behaving slightly
/// differently from api.openai.com — panicked the provider instead of returning
/// a response with zero usage.
///
/// The panic is also the quiet kind. Provider calls run inside spawned stage
/// tasks, so the run kept going and reported the stage's *cached* result; the
/// crash printed a line to stderr that nothing correlates with the run. It was
/// found by running the demo, not by a test.
pub fn json_path<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a serde_json::Value> {
    let mut cur = value;
    for key in path {
        cur = match cur.get(*key) {
            Some(next) => next,
            // `get` only looks in objects; fall through to positional lookup.
            None => key.parse::<usize>().ok().and_then(|i| cur.get(i))?,
        };
    }
    Some(cur)
}

/// [`json_path`] as a `u32`, defaulting to 0 for anything absent or non-numeric.
pub fn json_path_u32(value: &serde_json::Value, path: &[&str]) -> u32 {
    json_path(value, path).and_then(|v| v.as_u64()).unwrap_or(0) as u32
}

/// [`json_path`] as a `&str`, defaulting to the empty string.
pub fn json_path_str<'a>(value: &'a serde_json::Value, path: &[&str]) -> &'a str {
    json_path(value, path)
        .and_then(|v| v.as_str())
        .unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_missing_object_does_not_panic() {
        // The exact shape that panicked: `usage` absent, so `usage["x"]` is
        // `Null["x"]`.
        let body = json!({ "choices": [] });
        assert_eq!(json_path_u32(&body, &["usage", "prompt_tokens"]), 0);
        assert_eq!(
            json_path_u32(&body, &["usage", "prompt_tokens_details", "cached_tokens"]),
            0
        );
        assert_eq!(
            json_path_str(&body, &["choices", "0", "message", "content"]),
            ""
        );
    }

    #[test]
    fn a_present_value_is_read_through() {
        let body = json!({
            "usage": {
                "prompt_tokens": 11,
                "prompt_tokens_details": { "cached_tokens": 3 },
                "completion_tokens_details": { "reasoning_tokens": 5 }
            },
            "choices": [ { "message": { "content": "hi" } } ]
        });
        assert_eq!(json_path_u32(&body, &["usage", "prompt_tokens"]), 11);
        assert_eq!(
            json_path_u32(&body, &["usage", "prompt_tokens_details", "cached_tokens"]),
            3
        );
        assert_eq!(
            json_path_u32(
                &body,
                &["usage", "completion_tokens_details", "reasoning_tokens"]
            ),
            5
        );
        assert_eq!(
            json_path_str(&body, &["choices", "0", "message", "content"]),
            "hi"
        );
    }

    #[test]
    fn a_wrong_typed_node_stops_the_walk_instead_of_panicking() {
        // `usage` present but a string, an array where an object is expected,
        // a number where a string is expected: all of these used to panic.
        for body in [
            json!({ "usage": "nope" }),
            json!({ "usage": [1, 2, 3] }),
            json!({ "usage": { "prompt_tokens": "eleven" } }),
            json!({ "choices": {} }),
            json!(null),
            json!([]),
        ] {
            assert_eq!(json_path_u32(&body, &["usage", "prompt_tokens"]), 0);
            assert_eq!(
                json_path_str(&body, &["choices", "0", "message", "content"]),
                ""
            );
        }
    }
}
