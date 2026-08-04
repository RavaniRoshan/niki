use chrono::Utc;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub struct Span {
    pub name: String,
    pub model: String,
    pub prompt_hash: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tool_name: Option<String>,
    pub tool_args: Option<String>,
    pub tool_output: Option<String>,
    pub tool_error: Option<String>,
    pub latency_ms: u64,
    pub ttft_ms: u64,
    pub retry_count: u32,
    pub timestamp: String,
}

impl Span {
    pub fn new(name: &str, model: &str, prompt_hash: &str) -> Self {
        Span {
            name: name.to_string(),
            model: model.to_string(),
            prompt_hash: prompt_hash.to_string(),
            input_tokens: 0,
            output_tokens: 0,
            tool_name: None,
            tool_args: None,
            tool_output: None,
            tool_error: None,
            latency_ms: 0,
            ttft_ms: 0,
            retry_count: 0,
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "name": self.name,
            "model": self.model,
            "prompt_hash": self.prompt_hash,
            "input_tokens": self.input_tokens,
            "output_tokens": self.output_tokens,
            "tool_name": self.tool_name,
            "tool_args": self.tool_args,
            "tool_output": self.tool_output,
            "tool_error": self.tool_error,
            "latency_ms": self.latency_ms,
            "ttft_ms": self.ttft_ms,
            "retry_count": self.retry_count,
            "timestamp": self.timestamp,
        })
    }
}

pub fn emit_span(span: &Span) {
    let json = serde_json::to_string(&span.to_json()).unwrap_or_default();
    eprintln!("[SPAN] {}", json);
}

pub fn emit_span_jsonl(span: &Span, path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string(&span.to_json()).unwrap_or_default();
    let _ = fs::write(path, format!("{}\n", json));
}
