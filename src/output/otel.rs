//! Minimal OTLP/HTTP trace export (no new dependencies).
//!
//! Translates the run's `trace.jsonl` spans into an OTLP `TracesData` JSON
//! payload and POSTs it to `<endpoint>/v1/traces`. Times are derived from
//! recorded stage latencies stacked in order (same honesty rule as
//! [`super::trace`]: offsets are measured intervals, the absolute anchor is
//! export time, and every span carries `niki.timeline = "derived"`).
//!
//! Export is best-effort and warn-only: telemetry must never fail a run.
//! Configure via `niki run --otel-endpoint URL` or the standard
//! `OTEL_EXPORTER_OTLP_ENDPOINT` env var.

use serde_json::{Value, json};

/// Build the OTLP JSON payload from our trace lines. Pure function, tested.
pub fn otlp_payload(
    service_name: &str,
    service_version: &str,
    trace_id_hex: &str,
    spans: &[Value],
    export_time_unix_nano: u64,
) -> Value {
    let total_offset_ms: u64 = spans
        .iter()
        .filter_map(|s| s.get("total_offset_ms").and_then(|v| v.as_u64()))
        .sum();
    let base_nano = export_time_unix_nano.saturating_sub(total_offset_ms * 1_000_000);

    let otlp_spans: Vec<Value> = spans
        .iter()
        .map(|s| {
            let name = s.get("span").and_then(|v| v.as_str()).unwrap_or("unknown");
            let offset_ms = s
                .get("start_offset_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            // Marker spans (task/verdict/test) carry no duration.
            let duration_ms = s
                .get("duration_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let start = base_nano + offset_ms * 1_000_000;
            let mut attributes: Vec<Value> = vec![
                attr("niki.timeline", "derived"),
                attr(
                    "niki.trace_id",
                    s.get("trace_id").and_then(|v| v.as_str()).unwrap_or(""),
                ),
            ];
            for key in [
                "provider",
                "model",
                "verdict",
                "branch",
                "topology",
                "command",
                "description",
            ] {
                if let Some(v) = s.get(key).and_then(|v| v.as_str()) {
                    attributes.push(attr(&format!("niki.{key}"), v));
                }
            }
            for key in [
                "input_tokens",
                "output_tokens",
                "cached_input_tokens",
                "reasoning_tokens",
                "cost_usd",
                "retries",
                "revision_rounds",
                "exit_code",
                "total_offset_ms",
            ] {
                if let Some(v) = s.get(key) {
                    attributes.push(json!({"key": format!("niki.{key}"), "value": num_value(v)}));
                }
            }
            if let Some(passed) = s.get("passed").and_then(|v| v.as_bool()) {
                attributes.push(json!({"key": "niki.passed", "value": {"boolValue": passed}}));
                if !passed {
                    attributes.push(json!({"key": "error", "value": {"boolValue": true}}));
                }
            }
            json!({
                "traceId": trace_id_hex,
                "spanId": rand_span_id(),
                "name": format!("niki.{name}"),
                "startTimeUnixNano": start.to_string(),
                "endTimeUnixNano": (start + duration_ms * 1_000_000).to_string(),
                "attributes": attributes,
                "status": {},
            })
        })
        .collect();

    json!({
        "resourceSpans": [{
            "resource": {"attributes": [
                attr("service.name", service_name),
                attr("service.version", service_version),
            ]},
            "scopeSpans": [{
                "scope": {"name": "niki.pipeline"},
                "spans": otlp_spans,
            }],
        }],
    })
}

fn attr(key: &str, value: &str) -> Value {
    json!({"key": key, "value": {"stringValue": value}})
}

fn num_value(v: &Value) -> Value {
    if let Some(i) = v.as_u64() {
        json!({"intValue": i.to_string()})
    } else if let Some(f) = v.as_f64() {
        json!({"doubleValue": f})
    } else {
        json!({"stringValue": v.to_string()})
    }
}

fn rand_span_id() -> String {
    let bytes: [u8; 8] = fastrand::u64(..).to_be_bytes();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Normalize a task UUID string into a 32-hex-char OTLP trace id.
/// Falls back to hashing the raw string when it is not a UUID.
pub fn trace_id_hex(task_id: &str) -> String {
    let hex: String = task_id.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if hex.len() == 32 {
        return hex.to_lowercase();
    }
    // Deterministic fallback: FNV-1a duplicated to 32 hex chars.
    let mut h: u64 = 0xcbf29ce484222325;
    for b in task_id.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}{h:016x}")
}

/// POST the payload to `<endpoint>/v1/traces`. Warn-only failures.
pub async fn export_trace(endpoint: &str, payload: &Value) -> Result<(), String> {
    let url = format!("{}/v1/traces", endpoint.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(payload)
        .send()
        .await
        .map_err(|e| format!("OTLP export request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("OTLP export rejected: HTTP {}", resp.status()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_maps_spans_with_derived_flag() {
        let spans = vec![
            json!({"trace_id": "t", "span": "task", "description": "x"}),
            json!({"trace_id": "t", "span": "coder", "parent": "task",
                   "provider": "mock", "model": "m", "start_offset_ms": 0,
                   "duration_ms": 250, "input_tokens": 10, "output_tokens": 5,
                   "cost_usd": 0.01, "retries": 0}),
            json!({"trace_id": "t", "span": "verdict", "parent": "task",
                   "verdict": "Approved", "total_offset_ms": 250}),
        ];
        let p = otlp_payload(
            "niki",
            "0.6.0",
            &trace_id_hex("t"),
            &spans,
            1_000_000_000_000,
        );
        let out_spans = &p["resourceSpans"][0]["scopeSpans"][0]["spans"];
        assert_eq!(out_spans.as_array().unwrap().len(), 3);
        assert_eq!(out_spans[1]["name"], "niki.coder");
        let attrs = out_spans[1]["attributes"].as_array().unwrap();
        assert!(
            attrs
                .iter()
                .any(|a| a["key"] == "niki.timeline" && a["value"]["stringValue"] == "derived")
        );
        // Stacked timeline: coder starts at base, duration preserved.
        assert_eq!(out_spans[1]["startTimeUnixNano"], "999750000000");
        assert_eq!(out_spans[1]["endTimeUnixNano"], "1000000000000");
    }

    #[test]
    fn trace_id_prefers_uuid_hex() {
        assert_eq!(
            trace_id_hex("0126724f-e2b8-445f-81b6-146100123f79"),
            "0126724fe2b8445f81b6146100123f79"
        );
        assert_eq!(trace_id_hex("abc").len(), 32);
    }
}
