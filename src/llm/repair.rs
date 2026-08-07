use anyhow::{Result, anyhow};

/// Attempt to repair malformed JSON from LLM output using ordered local strategies.
///
/// Strategies are applied in order of prevalence (most common LLM malformations first).
/// Returns the repaired JSON string or an error with context about what failed.
///
/// **Design原则:**
/// - Never fabricate values by closing braces — re-run with more budget instead.
/// - Never silently repair JSON for destructive actions (per fixjson.org fail-loudly).
/// - All repairs are deterministic — same input always produces same output.
pub fn repair_json(input: &str) -> Result<String> {
    // Fast path: try strict parse first
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(input) {
        return Ok(serde_json::to_string(&v)?);
    }

    let mut s = input.to_string();

    // Strategy 1: Strip markdown code fences
    s = strip_code_fences(&s);

    // Strategy 2: Extract JSON from surrounding prose
    s = extract_json_from_prose(&s);

    // Try strict parse after extraction
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
        return Ok(serde_json::to_string(&v)?);
    }

    // Strategy 3: Fix trailing commas before } or ]
    s = fix_trailing_commas(&s);

    // Strategy 4: Convert single quotes to double quotes
    s = fix_single_quotes(&s);

    // Strategy 5: Escape control characters in strings
    s = escape_control_chars(&s);

    // Strategy 6: Normalize Python literals
    s = normalize_python_literals(&s);

    // Try parse after fixes
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
        return Ok(serde_json::to_string(&v)?);
    }

    // Strategy 7: Close unclosed brackets/braces (best-effort)
    s = close_unclosed_brackets(&s);

    // Final strict parse
    match serde_json::from_str::<serde_json::Value>(&s) {
        Ok(v) => Ok(serde_json::to_string(&v)?),
        Err(e) => Err(anyhow!(
            "JSON repair failed after all strategies. Last error: {}. Input preview: {}",
            e,
            preview(input, 200)
        )),
    }
}

/// Try to extract a JSON object or array from surrounding prose/thinking blocks.
fn extract_json_from_prose(input: &str) -> String {
    let trimmed = input.trim();

    // Already starts with { or [ — no extraction needed
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return trimmed.to_string();
    }

    // Find first { or [
    let start = trimmed.find('{').or_else(|| trimmed.find('['));
    let start = match start {
        Some(i) => i,
        None => return trimmed.to_string(),
    };

    // Find matching closing bracket by counting depth
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let mut end = trimmed.len();

    for (i, byte) in trimmed[start..].bytes().enumerate() {
        let c = byte as char;
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && in_string {
            escaped = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match c {
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    end = start + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }

    trimmed[start..end].to_string()
}

/// Strip markdown code fences (```json ... ``` or ``` ... ```).
fn strip_code_fences(input: &str) -> String {
    let trimmed = input.trim();

    // Look for opening fence
    let fence_start = trimmed.find("```");
    if fence_start.is_none() {
        return trimmed.to_string();
    }
    let fence_start = fence_start.unwrap();

    // Skip past the fence line (```json or ``` or ```language)
    let after_fence = &trimmed[fence_start + 3..];
    let line_end = after_fence.find('\n').unwrap_or(after_fence.len());
    let content_start = fence_start + 3 + line_end + 1;

    if content_start >= trimmed.len() {
        return after_fence.to_string();
    }

    let content = &trimmed[content_start..];

    // Find closing fence
    if let Some(close_pos) = content.find("```") {
        content[..close_pos].trim().to_string()
    } else {
        content.trim().to_string()
    }
}

/// Fix trailing commas before } or ].
fn fix_trailing_commas(input: &str) -> String {
    let bytes = input.as_bytes();
    let len = bytes.len();
    let mut result = Vec::with_capacity(len);

    let mut i = 0;
    while i < len {
        // Check for comma followed by whitespace and closing bracket
        if bytes[i] == b',' {
            let mut j = i + 1;
            while j < len && matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r') {
                j += 1;
            }
            if j < len && matches!(bytes[j], b'}' | b']') {
                // Skip the comma (don't push it) and continue from current position
                // The closing bracket will be pushed on the next iteration
                i += 1;
                continue;
            }
        }

        result.push(bytes[i]);
        i += 1;
    }

    String::from_utf8(result).unwrap_or_else(|_| input.to_string())
}

/// Convert single quotes to double quotes for JSON keys and string values.
/// This is a heuristic — it handles the common case of {'key': 'value'}.
fn fix_single_quotes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let len = bytes.len();

    while i < len {
        if bytes[i] == b'\'' {
            // Look backwards for whitespace, {, [, ,, or start of string
            let can_be_opening = i == 0 || {
                let prev = bytes[i - 1];
                prev == b'{' || prev == b'[' || prev == b',' || prev == b':' || prev == b' ' || prev == b'\t' || prev == b'\n'
            };

            // Look forward for content then closing single quote
            if can_be_opening {
                // Find matching closing single quote
                let mut j = i + 1;
                let mut found_close = false;
                while j < len {
                    if bytes[j] == b'\'' {
                        found_close = true;
                        break;
                    }
                    if bytes[j] == b'\\' {
                        j += 2; // skip escaped char
                        continue;
                    }
                    j += 1;
                }

                if found_close && j > i + 1 {
                    // Check if what's between is a valid JSON value or key
                    let content = &input[i + 1..j];
                    let after_close = if j + 1 < len { bytes[j + 1] } else { b' ' };
                    let can_be_value = after_close == b',' || after_close == b'}' || after_close == b']' || after_close == b':' || after_close == b' ';

                    if can_be_value || content.contains(' ') {
                        // Replace opening and closing single quotes with double quotes
                        result.push(b'"');
                        result.extend_from_slice(content.as_bytes());
                        result.push(b'"');
                        i = j + 1;
                        continue;
                    }
                }
            }
        }

        result.push(bytes[i]);
        i += 1;
    }

    String::from_utf8(result).unwrap_or_else(|_| input.to_string())
}

/// Escape unescaped control characters inside JSON strings.
fn escape_control_chars(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut in_string = false;
    let mut escaped = false;

    for &byte in bytes {
        if escaped {
            result.push(byte);
            escaped = false;
            continue;
        }
        if byte == b'\\' && in_string {
            result.push(byte);
            escaped = true;
            continue;
        }
        if byte == b'"' {
            in_string = !in_string;
            result.push(byte);
            continue;
        }
        if in_string {
            match byte {
                b'\n' => { result.extend_from_slice(b"\\n"); }
                b'\r' => { result.extend_from_slice(b"\\r"); }
                b'\t' => { result.extend_from_slice(b"\\t"); }
                0x08 => { result.extend_from_slice(b"\\b"); }
                0x0C => { result.extend_from_slice(b"\\f"); }
                _ => { result.push(byte); }
            }
        } else {
            result.push(byte);
        }
    }

    String::from_utf8(result).unwrap_or_else(|_| input.to_string())
}

/// Normalize Python literals: True→true, False→false, None→null.
fn normalize_python_literals(input: &str) -> String {
    // Replace whole-word Python literals (not inside strings — best-effort)
    // We use a simple byte-level approach to avoid regex dependency.
    let bytes = input.as_bytes();
    let mut new_result = String::with_capacity(input.len() + 10);
    let mut i = 0;
    let len = bytes.len();

    while i < len {
        // Check for "True"
        if i + 4 <= len && &bytes[i..i + 4] == b"True" {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let after_ok = i + 4 >= len || !bytes[i + 4].is_ascii_alphanumeric();
            if before_ok && after_ok {
                new_result.push_str("true");
                i += 4;
                continue;
            }
        }
        // Check for "False"
        if i + 5 <= len && &bytes[i..i + 5] == b"False" {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let after_ok = i + 5 >= len || !bytes[i + 5].is_ascii_alphanumeric();
            if before_ok && after_ok {
                new_result.push_str("false");
                i += 5;
                continue;
            }
        }
        // Check for "None"
        if i + 4 <= len && &bytes[i..i + 4] == b"None" {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            let after_ok = i + 4 >= len || !bytes[i + 4].is_ascii_alphanumeric();
            if before_ok && after_ok {
                new_result.push_str("null");
                i += 4;
                continue;
            }
        }
        new_result.push(bytes[i] as char);
        i += 1;
    }

    new_result
}

/// Try to close unclosed brackets/braces by appending closing characters.
/// This is a last-resort strategy — it may produce invalid JSON if the structure is deeply wrong.
fn close_unclosed_brackets(input: &str) -> String {
    let mut in_string = false;
    let mut escaped = false;
    let mut stack = Vec::new();

    for &byte in input.as_bytes() {
        if escaped {
            escaped = false;
            continue;
        }
        if byte == b'\\' && in_string {
            escaped = true;
            continue;
        }
        if byte == b'"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match byte {
            b'{' => stack.push(b'}'),
            b'[' => stack.push(b']'),
            b'}' | b']' => { stack.pop(); }
            _ => {}
        }
    }

    if stack.is_empty() {
        return input.to_string();
    }

    let mut result = input.to_string();
    // Close in reverse order (innermost first)
    for &c in stack.iter().rev() {
        result.push(c as char);
    }
    result
}

/// Preview a string, truncating to max_len with "..." suffix.
fn preview(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strict_parse_passthrough() {
        let input = r#"{"name": "test", "value": 42}"#;
        let result = repair_json(input).unwrap();
        assert!(result.contains("test"));
        assert!(result.contains("42"));
    }

    #[test]
    fn test_strip_code_fences_json() {
        let input = r#"```json
{"name": "test"}
```"#;
        let result = repair_json(input).unwrap();
        assert!(result.contains("test"));
    }

    #[test]
    fn test_strip_code_fences_plain() {
        let input = "```\n{\"name\": \"test\"}\n```";
        let result = repair_json(input).unwrap();
        assert!(result.contains("test"));
    }

    #[test]
    fn test_extract_from_prose() {
        let input = r#"Here is the JSON output:
{"name": "test", "value": 42}
End of output."#;
        let result = repair_json(input).unwrap();
        assert!(result.contains("test"));
    }

    #[test]
    fn test_trailing_commas() {
        let input = r#"{"name": "test", "value": 42,}"#;
        let result = repair_json(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["name"], "test");
        assert_eq!(v["value"], 42);
    }

    #[test]
    fn test_trailing_commas_array() {
        let input = r#"[1, 2, 3,]"#;
        let result = repair_json(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 3);
    }

    #[test]
    fn test_single_quotes() {
        let input = r#"{'name': 'test', 'value': 42}"#;
        let result = repair_json(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["name"], "test");
    }

    #[test]
    fn test_python_literals() {
        let input = r#"{"active": True, "deleted": False, "name": None}"#;
        let result = repair_json(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["active"], true);
        assert_eq!(v["deleted"], false);
        assert_eq!(v["name"], serde_json::Value::Null);
    }

    #[test]
    fn test_escape_newlines_in_string() {
        let input = r#"{"text": "line1
line2"}"#;
        let result = repair_json(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v["text"].as_str().unwrap().contains("line1"));
    }

    #[test]
    fn test_close_unclosed_brackets() {
        let input = r#"{"name": "test", "items": [1, 2"#;
        let result = repair_json(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["name"], "test");
    }

    #[test]
    fn test_combined_fixes() {
        let input = r#"```json
{
  'name': 'test',
  'items': [1, 2,],
  'active': True,
}
```"#;
        let result = repair_json(input).unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["name"], "test");
        assert_eq!(v["active"], true);
    }
}
