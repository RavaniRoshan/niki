//! ACP server - bridges the JSON-RPC ACP protocol to NIKI's pipeline.
//!
//! Run with `niki acp`. Reads newline-delimited JSON-RPC requests from stdin,
//! dispatches `initialize` / `prompt/send` / `session/status` / `session/cancel`
//! to the pipeline, and writes responses + progress notifications to stdout.
//! Designed to be driven by an IDE (Zed, Claude Code) over stdio.

use std::io::{BufReader, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use crate::acp::protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::config::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::orchestrator::pipeline::{Task, execute_pipeline};
use crate::orchestrator::state::TaskRecord;
use crate::sandbox::docker::ActiveContainers;

/// Server capabilities advertised on `initialize`.
fn capabilities() -> serde_json::Value {
    serde_json::json!({
        "capabilities": {
            "prompt": true,
            "session": true,
            "notifications": ["stage.start", "stage.token", "stage.done", "stage.failed", "task.completed"]
        }
    })
}

/// Emit a progress notification to stdout.
fn notify<W: Write>(out: &mut W, method: &str, params: serde_json::Value) {
    let n = JsonRpcNotification::new(method, params);
    if let Ok(s) = serde_json::to_string(&n) {
        let _ = writeln!(out, "{}", s);
        let _ = out.flush();
    }
}

/// Run the pipeline for one `prompt/send` and stream progress as notifications.
async fn run_prompt(
    out: &mut impl Write,
    prompt: &str,
    project_dir: PathBuf,
    cancel: Arc<AtomicBool>,
) -> JsonRpcResponse {
    let config = match NikiConfig::load(&project_dir) {
        Ok(c) => c,
        Err(e) => {
            return JsonRpcResponse::err(
                serde_json::json!(null),
                -32603,
                format!("config error: {}", e),
            );
        }
    };

    let task = Task {
        id: uuid::Uuid::new_v4(),
        description: prompt.to_string(),
        project_path: project_dir.clone(),
    };

    let mut display = AgenticDisplay::new();
    let task_dir = project_dir
        .join(&config.general.output_dir)
        .join("tasks")
        .join(task.id.to_string());

    // No TUI under ACP - events are buffered and replayed as notifications.
    let _ = display;

    // Container runtime: best-effort; the worktree backend needs no daemon.
    let docker = connect_runtime(&config).await;
    let containers: ActiveContainers =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));

    let result = execute_pipeline(
        &task,
        &config,
        docker.as_ref(),
        &mut display,
        containers,
        false, // dry_run
        cancel,
        &task_dir,
    )
    .await;

    // Replay every buffered pipeline event as an ACP notification so an IDE
    // sees live stage.start / stage.token / stage.done / stage.failed events.
    for ev in display.take_events() {
        let (method, params) = match ev {
            crate::display::tui::DisplayEvent::StageStart { role } => (
                "stage.start",
                serde_json::json!({ "role": format!("{:?}", role) }),
            ),
            crate::display::tui::DisplayEvent::StageToken { role, token } => (
                "stage.token",
                serde_json::json!({ "role": format!("{:?}", role), "token": token }),
            ),
            crate::display::tui::DisplayEvent::StageDone {
                role,
                summary,
                input_tokens,
                output_tokens,
                cost_usd,
                latency_ms,
            } => (
                "stage.done",
                serde_json::json!({
                    "role": format!("{:?}", role),
                    "summary": summary,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                    "cost_usd": cost_usd,
                    "latency_ms": latency_ms,
                }),
            ),
            crate::display::tui::DisplayEvent::StageFailed { role, error } => (
                "stage.failed",
                serde_json::json!({ "role": format!("{:?}", role), "error": error }),
            ),
            crate::display::tui::DisplayEvent::Revision { round, max, issues } => (
                "stage.revision",
                serde_json::json!({ "round": round, "max": max, "issues": issues }),
            ),
            crate::display::tui::DisplayEvent::DiffContent(diff) => (
                "task.diff",
                serde_json::json!({ "diff": diff }),
            ),
            _ => continue,
        };
        notify(out, method, params);
    }

    match result {
        Ok(r) => {
            let mut record = TaskRecord::new(task.id, prompt);
            record.add_metrics(&r.metrics);
            record.status = match r.verdict {
                crate::artifacts::types::Verdict::Approved => {
                    crate::orchestrator::state::TaskStatus::Completed
                }
                v => crate::orchestrator::state::TaskStatus::Failed {
                    error: format!("{:?}", v),
                },
            };
            record.branch = Some(r.final_diff.clone());
            let _ = record.save_to_disk(&task_dir);

            notify(
                out,
                "task.completed",
                serde_json::json!({
                    "task_id": task.id.to_string(),
                    "verdict": format!("{:?}", r.verdict),
                    "diff": r.final_diff,
                    "topology": format!("{:?}", r.topology),
                    "metrics": r.metrics.iter().map(|m| serde_json::json!({
                        "role": format!("{:?}", m.role),
                        "input_tokens": m.input_tokens,
                        "output_tokens": m.output_tokens,
                        "cost_usd": m.cost_usd,
                        "latency_ms": m.latency_ms,
                    })).collect::<Vec<_>>(),
                }),
            );

            JsonRpcResponse::ok(
                serde_json::json!(null),
                serde_json::json!({
                    "task_id": task.id.to_string(),
                    "verdict": format!("{:?}", r.verdict),
                    "diff": r.final_diff,
                    "topology": format!("{:?}", r.topology),
                }),
            )
        }
        Err(e) => {
            notify(
                out,
                "stage.failed",
                serde_json::json!({ "error": e.to_string() }),
            );
            JsonRpcResponse::err(serde_json::json!(null), -32603, e.to_string())
        }
    }
}

/// Best-effort container-runtime connection (mirrors `run.rs`).
async fn connect_runtime(_config: &NikiConfig) -> Option<bollard::Docker> {
    #[cfg(unix)]
    {
        if let Ok(d) = bollard::Docker::connect_with_local_defaults() {
            return Some(d);
        }
        for addr in [
            "/run/user/1000/podman.sock",
            "/run/podman/podman.sock",
            "/var/run/docker.sock",
        ] {
            if let Ok(d) =
                bollard::Docker::connect_with_local(addr, 120, bollard::API_DEFAULT_VERSION)
            {
                return Some(d);
            }
        }
    }
    None
}

/// Entry point for `niki acp`. Reads JSON-RPC from stdin, writes to stdout.
pub async fn run(project_dir: PathBuf) -> std::io::Result<()> {
    let stdin = std::io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    let cancel = Arc::new(AtomicBool::new(false));

    while let Some(line) = crate::acp::protocol::read_message(&mut reader)? {

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = JsonRpcResponse::err(
                    serde_json::json!(null),
                    -32700,
                    format!("parse error: {}", e),
                );
                let _ = writeln!(out, "{}", serde_json::to_string(&resp).unwrap());
                let _ = out.flush();
                continue;
            }
        };

        let response = match request.method.as_str() {
            "initialize" => JsonRpcResponse::ok(
                request.id.unwrap_or(serde_json::json!(null)),
                capabilities(),
            ),
            "prompt/send" => {
                let params: crate::acp::protocol::PromptParams = match request.params.clone() {
                    Some(p) => serde_json::from_value(p).unwrap_or_else(|_| {
                        crate::acp::protocol::PromptParams {
                            prompt: String::new(),
                            session_id: None,
                        }
                    }),
                    None => crate::acp::protocol::PromptParams {
                        prompt: String::new(),
                        session_id: None,
                    },
                };
                if params.prompt.is_empty() {
                    JsonRpcResponse::err(
                        request.id.unwrap_or(serde_json::json!(null)),
                        -32602,
                        "prompt/send requires a non-empty prompt".to_string(),
                    )
                } else {
                    run_prompt(
                        &mut out,
                        &params.prompt,
                        project_dir.clone(),
                        cancel.clone(),
                    )
                    .await
                }
            }
            "session/status" => JsonRpcResponse::ok(
                request.id.unwrap_or(serde_json::json!(null)),
                serde_json::json!({ "status": "idle" }),
            ),
            "session/cancel" => {
                cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                JsonRpcResponse::ok(
                    request.id.unwrap_or(serde_json::json!(null)),
                    serde_json::json!({ "cancelled": true }),
                )
            }
            _ => JsonRpcResponse::err(
                request.id.unwrap_or(serde_json::json!(null)),
                -32601,
                format!("method not found: {}", request.method),
            ),
        };

        let _ = writeln!(out, "{}", serde_json::to_string(&response).unwrap());
        let _ = out.flush();
    }

    Ok(())
}