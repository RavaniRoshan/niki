//! `niki serve` — the engine as a protocol server over stdio.
//!
//! One transport, one direction: newline-delimited JSON-RPC 2.0 on stdin and stdout, one JSON
//! object per line, flushed per line. **stdout carries protocol frames and nothing else** —
//! every diagnostic the engine produces goes to stderr. That is not tidiness: a shell draws its
//! whole view from these lines, and one stray `println!` from anywhere inside the pipeline would
//! corrupt the stream it is trying to parse.
//!
//! The types come from `niki-protocol` and are not restated here. This module owns the two
//! things that crate cannot: the real handlers behind those types, and the adapter that turns
//! the TUI's `DisplayEvent`s into the protocol's notifications.
//!
//! ## How the pipeline's events reach the wire
//!
//! `execute_pipeline` takes `&mut AgenticDisplay` and emits through the channel `AgenticDisplay`
//! already forwards to. So the adapter does not re-plumb the pipeline: `attach_sink` points that
//! channel at the adapter, a dedicated thread reads it, and each event is mapped onto a declared
//! `ServerNotification`. The pipeline is untouched.
//!
//! Approval requests cross the same way. `DisplayEvent::PermissionRequest` carries an
//! `mpsc::Sender<PermissionAction>`, but the *enum* is `Clone` — the adapter owns the sender once
//! it has received the event, so `approval.request` goes out and the `approval.reply` that comes
//! back is routed to it. No type had to change for that.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Stdout, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clap::Args;
use niki_protocol::{
    ApprovalDecision, ApprovalOption, ApprovalReplyParams, ApprovalReplyResult,
    ApprovalRequestParams, Capabilities, ClientInfo, ClientRequest, ClientResult, JsonRpc,
    PermissionMode, ServerNotification, TraceId,
};

use crate::artifacts::types::AgentRole;
use crate::cli::run::BackendArg;
use crate::config::NikiConfig;
use crate::display::agent_stream::AgenticDisplay;
use crate::display::tui::DisplayEvent;
use crate::orchestrator::pipeline::{Task, execute_pipeline};
use crate::permissions::PermissionAction;
use crate::sandbox::SandboxBackend;
use crate::sandbox::docker::ActiveContainers;

/// How long an `approval.request` waits for an `approval.reply` before it decides on the user's
/// behalf. A shell that dies mid-run must not leave a live pipeline blocked forever on a prompt
/// nobody is there to answer. Denying is the only safe answer to pick for someone else.
const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

/// How long the adapter waits for one more event before re-checking whether the run is over.
const ADAPTER_POLL: Duration = Duration::from_millis(25);

/// How long the server waits for the adapter to drain before giving up on it. A stuck adapter
/// must not wedge the server; the frames still reach the shell, only later.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// Run the engine as a `niki-protocol` server over stdio.
#[derive(Args, Clone, Debug)]
pub struct ServeArgs {
    /// Path to the project directory
    #[arg(short, long, default_value = ".")]
    pub project: PathBuf,

    /// Sandbox backend: docker (container) or worktree (git worktree + local
    /// process, no Docker). Overrides [docker] backend in config.
    #[arg(long, value_enum)]
    pub backend: Option<BackendArg>,

    /// Deterministic-inputs mode for CI, mirroring `niki run --bare`: skip project
    /// memory, MCP discovery and external knowledge fetching.
    #[arg(long)]
    pub bare: bool,

    /// Replay a scripted, deterministic run instead of driving the real engine.
    /// Debug builds only — the fixture runtime is not a release feature.
    #[cfg(feature = "fixture-runtime")]
    #[arg(long)]
    pub fixture: bool,
}

/// Everything that can go wrong in the server itself, as opposed to inside a turn.
#[derive(Debug, thiserror::Error)]
enum ServeError {
    #[error("io error on the protocol seam: {0}")]
    Io(String),
    #[error("protocol: {0}")]
    Protocol(#[from] niki_protocol::ProtocolError),
}

/// Entry point for `niki serve`.
pub async fn handle(args: &ServeArgs) -> anyhow::Result<()> {
    let project_dir = args
        .project
        .canonicalize()
        .unwrap_or_else(|_| args.project.clone());
    let backend = args.backend.map(Into::into);
    #[cfg(feature = "fixture-runtime")]
    let fixture = args.fixture;
    #[cfg(not(feature = "fixture-runtime"))]
    let fixture = false;
    let server = Server::new(project_dir, backend, args.bare, fixture);
    server.run().await.map_err(anyhow::Error::from)
}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

/// One line of input, classified. The three cases are kept apart because they get three
/// different JSON-RPC codes: unparseable is a parse error, a method nobody declared is
/// method-not-found, and a declared method with bad params is an invalid request.
enum Frame {
    Request(niki_protocol::RequestFrame),
    UnknownMethod(String),
    Malformed(String),
}

fn decode(line: &str) -> Frame {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Frame::Malformed("empty line".to_string());
    }
    if let Ok(req) = serde_json::from_str::<niki_protocol::RequestFrame>(trimmed) {
        return Frame::Request(req);
    }
    // Not a declared request. If the line is JSON at all, its `method` says which of the two
    // failures this is.
    let peeked: Option<String> = serde_json::from_str::<serde_json::Value>(trimmed)
        .ok()
        .and_then(|v| v.get("method").and_then(|m| m.as_str()).map(str::to_owned));
    match peeked {
        Some(method) if declared_methods().contains(&method.as_str()) => Frame::Malformed(format!(
            "invalid params or shape for the declared method `{method}`"
        )),
        Some(method) => Frame::UnknownMethod(method),
        None => Frame::Malformed("not a JSON object".to_string()),
    }
}

/// Every method name the protocol declares, derived from the types rather than retyped.
///
/// A shell that sends `turn.strat` must be told it does not exist; a shell that sends
/// `turn.start` with broken `params` must be told its params are broken. Answering both with
/// "unknown method" sends the caller hunting for a typo that is not there.
fn declared_methods() -> Vec<&'static str> {
    let blank = ClientInfo {
        name: String::new(),
        version: String::new(),
        cols: 0,
        rows: 0,
    };
    vec![
        ClientRequest::Initialize(niki_protocol::InitializeParams {
            protocol_version: 0,
            client: blank.clone(),
        })
        .method(),
        ClientRequest::Shutdown(niki_protocol::ShutdownParams {
            user_initiated: false,
        })
        .method(),
        ClientRequest::SessionLoad(niki_protocol::SessionLoadParams {
            session_id: None,
            project_path: String::new(),
        })
        .method(),
        ClientRequest::TurnStart(niki_protocol::TurnStartParams {
            prompt: String::new(),
            permission_mode: PermissionMode::Manual,
        })
        .method(),
        ClientRequest::ApprovalReply(ApprovalReplyParams {
            id: String::new(),
            decision: ApprovalDecision::Deny,
            reason: None,
        })
        .method(),
    ]
}

/// Serialise one frame as a single line and flush it.
///
/// Flushed per line, not per buffer: a shell renders as the engine talks, and a buffered frame
/// is a frame that does not arrive.
fn write_frame<W: Write>(out: &mut W, value: &serde_json::Value) -> Result<(), ServeError> {
    let line = serde_json::to_string(value)
        .map_err(|e| ServeError::Io(format!("could not serialise: {e}")))?;
    out.write_all(line.as_bytes())
        .map_err(|e| ServeError::Io(e.to_string()))?;
    out.write_all(b"\n")
        .map_err(|e| ServeError::Io(e.to_string()))?;
    out.flush().map_err(|e| ServeError::Io(e.to_string()))
}

fn response_value(
    id: &niki_protocol::RequestId,
    trace: &TraceId,
    outcome: niki_protocol::ResponseOutcome,
) -> Result<serde_json::Value, ServeError> {
    let frame = niki_protocol::ResponseFrame {
        jsonrpc: JsonRpc::VALUE,
        id: id.clone(),
        trace_id: trace.clone(),
        outcome,
    };
    serde_json::to_value(frame).map_err(|e| ServeError::Io(format!("could not serialise: {e}")))
}

fn notification_value(
    trace: &TraceId,
    event: ServerNotification,
) -> Result<serde_json::Value, ServeError> {
    let frame = niki_protocol::NotificationFrame {
        jsonrpc: JsonRpc::VALUE,
        trace_id: trace.clone(),
        event,
    };
    serde_json::to_value(frame).map_err(|e| ServeError::Io(format!("could not serialise: {e}")))
}

fn err(
    code: i32,
    message: impl Into<String>,
    data: Option<String>,
) -> niki_protocol::ResponseOutcome {
    let mut e = niki_protocol::RpcError::new(code, message);
    if let Some(d) = data {
        e = e.with_data(d);
    }
    niki_protocol::ResponseOutcome::Error(e)
}

/// A frame the engine could not parse carries no trace id of its own. Every frame still carries
/// one, because "every frame carries a trace_id" is the rule a shell greps by — so this is a
/// real, greppable value rather than an empty string.
fn fallback_trace() -> TraceId {
    TraceId("unparsed".to_string())
}

// ---------------------------------------------------------------------------
// Approvals
// ---------------------------------------------------------------------------

/// A decision a user has not made yet.
///
/// The adapter learns a permission prompt needs an answer by *receiving* an
/// `mpsc::Sender<PermissionAction>`, and the read loop learns about it by parsing a JSON line.
/// Those are two different channels with nothing between them, so the answer is parked here:
/// the reply lands in the slot, the adapter is woken, and the adapter then forwards it to the
/// sender the tool is actually blocked on.
struct PendingApproval {
    action: Mutex<Option<PermissionAction>>,
    ready: std::sync::Condvar,
}

impl PendingApproval {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            action: Mutex::new(None),
            ready: std::sync::Condvar::new(),
        })
    }

    fn decide(&self, action: PermissionAction) {
        let mut slot = match self.action.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        *slot = Some(action);
        self.ready.notify_all();
    }

    /// Block until a decision arrives, or the deadline passes. `None` means nobody answered.
    fn wait(&self, timeout: Duration) -> Option<PermissionAction> {
        let slot = match self.action.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let (mut slot, _) = match self
            .ready
            .wait_timeout_while(slot, timeout, |decided| decided.is_none())
        {
            Ok(pair) => pair,
            Err(poisoned) => poisoned.into_inner(),
        };
        slot.take()
    }
}

/// Pending approvals, keyed by the id the shell is told to quote back.
#[derive(Default)]
struct Approvals {
    inner: Mutex<HashMap<String, Arc<PendingApproval>>>,
}

impl Approvals {
    fn put(&self, id: String, pending: Arc<PendingApproval>) {
        match self.inner.lock() {
            Ok(mut map) => {
                map.insert(id, pending);
            }
            Err(poisoned) => {
                poisoned.into_inner().insert(id, pending);
            }
        }
    }

    fn take(&self, id: &str) -> Option<Arc<PendingApproval>> {
        match self.inner.lock() {
            Ok(mut map) => map.remove(id),
            Err(poisoned) => poisoned.into_inner().remove(id),
        }
    }

    /// The oldest unanswered approval, if any. Only the fixture runtime answers on a user's
    /// behalf; the real path waits for a shell to reply.
    #[cfg(feature = "fixture-runtime")]
    fn oldest(&self) -> Option<Arc<PendingApproval>> {
        let map = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        map.values()
            .min_by_key(|p| Arc::as_ptr(p) as usize)
            .cloned()
    }
}

// ---------------------------------------------------------------------------
// The server
// ---------------------------------------------------------------------------

struct Server {
    project_dir: PathBuf,
    backend: Option<SandboxBackend>,
    bare: bool,
    /// Always `false` without the `fixture-runtime` feature — the flag does not exist there.
    #[cfg_attr(not(feature = "fixture-runtime"), allow(dead_code))]
    fixture: bool,
    out: Arc<Mutex<Stdout>>,
    /// The session `session.load` established. `turn.start` before one is not an error — a
    /// shell may reasonably skip it — but the fallback names the project `--project` chose
    /// rather than guessing at something else.
    session: Mutex<Option<niki_protocol::SessionLoadResult>>,
    approvals: Arc<Approvals>,
}

impl Server {
    fn new(
        project_dir: PathBuf,
        backend: Option<SandboxBackend>,
        bare: bool,
        fixture: bool,
    ) -> Self {
        Self {
            project_dir,
            backend,
            bare,
            fixture,
            out: Arc::new(Mutex::new(std::io::stdout())),
            session: Mutex::new(None),
            approvals: Arc::new(Approvals::default()),
        }
    }

    fn emit(&self, value: &serde_json::Value) {
        let mut out = match self.out.lock() {
            Ok(g) => g,
            // A poisoned lock means another writer panicked. Dropping one frame is strictly
            // better than panicking here and taking the whole server down with it.
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Err(e) = write_frame(&mut *out, value) {
            eprintln!("niki serve: could not write a frame: {e}");
        }
    }

    fn reply(
        &self,
        id: &niki_protocol::RequestId,
        trace: &TraceId,
        outcome: niki_protocol::ResponseOutcome,
    ) {
        match response_value(id, trace, outcome) {
            Ok(v) => self.emit(&v),
            Err(e) => eprintln!("niki serve: {e}"),
        }
    }

    fn notify(&self, trace: &TraceId, event: ServerNotification) {
        match notification_value(trace, event) {
            Ok(v) => self.emit(&v),
            Err(e) => eprintln!("niki serve: {e}"),
        }
    }

    async fn run(self) -> Result<(), ServeError> {
        let stdin = std::io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        let mut line = String::new();

        loop {
            line.clear();
            let read = reader
                .read_line(&mut line)
                .map_err(|e| ServeError::Io(e.to_string()))?;
            if read == 0 {
                // EOF: the shell went away.
                return Ok(());
            }

            match decode(&line) {
                Frame::Malformed(detail) => self.reply(
                    &niki_protocol::RequestId(0),
                    &fallback_trace(),
                    err(
                        niki_protocol::error_code::PARSE_ERROR,
                        "malformed frame",
                        Some(detail),
                    ),
                ),
                Frame::UnknownMethod(method) => self.reply(
                    &niki_protocol::RequestId(0),
                    &fallback_trace(),
                    err(
                        niki_protocol::error_code::METHOD_NOT_FOUND,
                        format!(
                            "unknown method `{method}`; this build speaks protocol v{}",
                            niki_protocol::PROTOCOL_VERSION
                        ),
                        None,
                    ),
                ),
                Frame::Request(req) => {
                    let id = req.id.clone();
                    let trace = req.trace_id.clone();
                    if !self.dispatch(&id, &trace, req.call).await {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Returns `false` when the loop should stop (after `shutdown`).
    async fn dispatch(
        &self,
        id: &niki_protocol::RequestId,
        trace: &TraceId,
        call: ClientRequest,
    ) -> bool {
        match call {
            ClientRequest::Initialize(params) => {
                if params.protocol_version != niki_protocol::PROTOCOL_VERSION {
                    self.reply(
                        id,
                        trace,
                        err(
                            niki_protocol::error_code::INTERNAL_ERROR,
                            format!(
                                "peer speaks protocol v{}, this build speaks v{}",
                                params.protocol_version,
                                niki_protocol::PROTOCOL_VERSION
                            ),
                            None,
                        ),
                    );
                    return true;
                }
                self.reply(
                    id,
                    trace,
                    niki_protocol::ResponseOutcome::Result(ClientResult::Initialize(
                        niki_protocol::InitializeResult {
                            protocol_version: niki_protocol::PROTOCOL_VERSION,
                            engine_version: env!("CARGO_PKG_VERSION").to_string(),
                            capabilities: capabilities(),
                        },
                    )),
                );
            }
            ClientRequest::Shutdown(_) => {
                self.reply(
                    id,
                    trace,
                    niki_protocol::ResponseOutcome::Result(ClientResult::Shutdown(
                        niki_protocol::ShutdownResult { ok: true },
                    )),
                );
                return false;
            }
            ClientRequest::SessionLoad(params) => self.session_load(id, trace, params),
            ClientRequest::TurnStart(params) => {
                self.turn_start(id, trace, params).await;
            }
            ClientRequest::ApprovalReply(params) => self.approval_reply(id, trace, params),
        }
        true
    }

    fn session_load(
        &self,
        id: &niki_protocol::RequestId,
        trace: &TraceId,
        params: niki_protocol::SessionLoadParams,
    ) {
        // The caller names the project; `--project` is only the default when they did not.
        let project_dir = if params.project_path.trim().is_empty() {
            self.project_dir.clone()
        } else {
            let requested = PathBuf::from(&params.project_path);
            match requested.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    self.reply(
                        id,
                        trace,
                        err(
                            niki_protocol::error_code::INVALID_PARAMS,
                            format!(
                                "session.load: {} cannot be resolved ({e})",
                                params.project_path
                            ),
                            None,
                        ),
                    );
                    return;
                }
            }
        };

        // The branch is reported only when git actually reports one. A repository with no
        // commits, or a detached HEAD, has no branch, and inventing the repository's name or
        // defaulting to `main` would be a claim the caller could act on.
        let branch = current_branch(&project_dir);
        let (ahead, behind) = upstream_counts(&project_dir);

        let result = niki_protocol::SessionLoadResult {
            // Resuming a checkpoint is not implemented, and a caller that asks for one gets
            // `resumed_messages: 0` below rather than a fabricated history.
            session_id: params
                .session_id
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            resumed_messages: 0,
            project_path: project_dir.to_string_lossy().into_owned(),
            branch: branch.clone(),
        };

        let model = NikiConfig::load(&project_dir)
            .map(|c| c.agents.coder.model.clone())
            .unwrap_or_default();
        let ready = niki_protocol::SessionReadyParams {
            session_id: result.session_id.clone(),
            project_path: result.project_path.clone(),
            model,
            permission_mode: PermissionMode::Manual,
            branch,
            ahead,
            behind,
            resumed_messages: 0,
        };

        self.reply(
            id,
            trace,
            niki_protocol::ResponseOutcome::Result(ClientResult::SessionLoad(result.clone())),
        );
        self.notify(trace, ServerNotification::SessionReady(ready));
        match self.session.lock() {
            Ok(mut slot) => *slot = Some(result),
            Err(poisoned) => *poisoned.into_inner() = Some(result),
        }
    }

    fn approval_reply(
        &self,
        id: &niki_protocol::RequestId,
        trace: &TraceId,
        params: ApprovalReplyParams,
    ) {
        let action = match params.decision {
            ApprovalDecision::Allow | ApprovalDecision::AllowAlways => PermissionAction::Allow,
            ApprovalDecision::Deny | ApprovalDecision::DenyWithReason => PermissionAction::Deny,
        };
        match self.approvals.take(&params.id) {
            Some(pending) => {
                // A closed tool channel means the work gave up while the user was deciding.
                // The user did answer, so it is still reported as answered; the adapter that
                // forwards it is the one that finds the receiver gone.
                pending.decide(action);
                self.reply(
                    id,
                    trace,
                    niki_protocol::ResponseOutcome::Result(ClientResult::ApprovalReply(
                        ApprovalReplyResult {
                            id: params.id.clone(),
                            decision: params.decision,
                        },
                    )),
                );
            }
            None => {
                self.reply(
                    id,
                    trace,
                    err(
                        niki_protocol::error_code::INTERNAL_ERROR,
                        format!("no pending approval with id `{}`", params.id),
                        Some("the id must be the one carried by an approval.request".to_string()),
                    ),
                );
            }
        }
    }

    async fn turn_start(
        &self,
        id: &niki_protocol::RequestId,
        trace: &TraceId,
        params: niki_protocol::TurnStartParams,
    ) {
        if params.prompt.trim().is_empty() {
            self.reply(
                id,
                trace,
                err(
                    niki_protocol::error_code::INVALID_PARAMS,
                    "turn.start needs a non-empty prompt",
                    None,
                ),
            );
            return;
        }

        let session = match self.session.lock() {
            Ok(slot) => slot.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
        .unwrap_or(niki_protocol::SessionLoadResult {
            session_id: String::new(),
            resumed_messages: 0,
            project_path: self.project_dir.to_string_lossy().into_owned(),
            branch: current_branch(&self.project_dir),
        });

        let turn_id = uuid::Uuid::new_v4().to_string();
        // Answered before the run starts: the result is an acceptance, not a promise. The run's
        // progress arrives as notifications after this line.
        self.reply(
            id,
            trace,
            niki_protocol::ResponseOutcome::Result(ClientResult::TurnStart(
                niki_protocol::TurnStartResult {
                    turn_id: turn_id.clone(),
                },
            )),
        );
        self.notify(
            trace,
            ServerNotification::TurnStarted(niki_protocol::TurnStartedParams {
                turn_id: turn_id.clone(),
                prompt: params.prompt.clone(),
            }),
        );

        let started = Instant::now();
        let sink = Arc::new(AdapterSink::new(
            self.out.clone(),
            trace.clone(),
            turn_id.clone(),
            PathBuf::from(&session.project_path)
                .join(".niki")
                .join("serve")
                .join(&turn_id),
            self.approvals.clone(),
        ));

        // One display, one sink, one adapter — for both paths. The fixture runtime walks the
        // same `DisplayEvent`s through the same mapping, so it exercises the adapter rather
        // than standing in for it. `attach_sink` also mutes every terminal write, which is
        // what keeps stdout carrying protocol frames only.
        let mut display = AgenticDisplay::new();
        let cancel = Arc::new(AtomicBool::new(false));
        let (event_tx, event_rx) = std::sync::mpsc::channel::<DisplayEvent>();
        display.attach_sink(event_tx.clone(), cancel.clone());
        let adapter = sink.spawn(event_rx);

        // The fixture branch is compiled out entirely without the feature: `ServeArgs::fixture`
        // does not exist, `self.fixture` is always false, and the module is never linked.
        #[allow(unused_mut)]
        let outcome = {
            #[cfg(feature = "fixture-runtime")]
            if self.fixture {
                fixture::replay(&event_tx, &sink, &cancel).await
            } else {
                self.real_turn(&params, &session, &mut display, &sink, &cancel)
                    .await
            }
            #[cfg(not(feature = "fixture-runtime"))]
            {
                self.real_turn(&params, &session, &mut display, &sink, &cancel)
                    .await
            }
        };

        sink.mark_done();
        display.finish_tui();
        adapter.join();

        self.notify(
            trace,
            ServerNotification::TurnEnd(niki_protocol::TurnEndParams {
                turn_id,
                summary: outcome.summary,
                duration_ms: started.elapsed().as_millis() as u64,
                tool_calls: outcome.tool_calls,
                files_changed: outcome.files_changed,
            }),
        );
    }

    /// The real engine. Returns what `turn.end` reports.
    async fn real_turn(
        &self,
        params: &niki_protocol::TurnStartParams,
        session: &niki_protocol::SessionLoadResult,
        display: &mut AgenticDisplay,
        sink: &Arc<AdapterSink>,
        cancel: &Arc<AtomicBool>,
    ) -> TurnOutcome {
        let project_dir = PathBuf::from(&session.project_path);

        let mut config = match NikiConfig::load(&project_dir) {
            Ok(c) => c,
            Err(e) => {
                let message = format!("config error: {e}");
                sink.notice(message.clone(), niki_protocol::Severity::Error);
                sink.final_event(None, Some(message.clone()));
                return TurnOutcome::failed(message.clone());
            }
        };
        if let Some(backend) = self.backend {
            config.docker.backend = backend;
        }

        let task = Task {
            id: uuid::Uuid::new_v4(),
            description: params.prompt.clone(),
            project_path: project_dir.clone(),
        };
        let task_dir = project_dir
            .join(&config.general.output_dir)
            .join("tasks")
            .join(task.id.to_string());

        // Only connect to a container runtime when the Docker backend is in use. The worktree
        // backend never touches Podman/Docker, so it runs without a daemon.
        let docker = if matches!(config.docker.backend, SandboxBackend::Docker) {
            match crate::cli::run::connect_container_runtime().await {
                Ok(d) => Some(d),
                Err(e) => {
                    let message = format!("container runtime unavailable: {e}");
                    sink.notice(message.clone(), niki_protocol::Severity::Error);
                    sink.final_event(None, Some(message.clone()));
                    return TurnOutcome::failed(message.clone());
                }
            }
        } else {
            None
        };

        let containers: ActiveContainers = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let result = execute_pipeline(
            &task,
            &config,
            docker.as_ref(),
            display,
            containers,
            false,
            cancel.clone(),
            &task_dir,
            None,
            self.bare,
        )
        .await;

        // The same two display calls `niki run` makes, which is what puts `Final` — and every
        // page of evidence behind it — on the event channel.
        match &result {
            Ok(r) => display.show_completion(r, "", &task_dir),
            Err(e) => {
                if cancel.load(Ordering::SeqCst) {
                    display.show_cancelled();
                } else {
                    display.show_failure(&e.to_string());
                }
            }
        }

        let tool_calls = sink.tool_calls();
        match result {
            Ok(r) => TurnOutcome {
                files_changed: count_changed_files(&r.final_diff),
                tool_calls,
                summary: format!("{:?}", r.verdict),
            },
            Err(e) => {
                let message = if cancel.load(Ordering::SeqCst) {
                    format!("cancelled: {e}")
                } else {
                    e.to_string()
                };
                TurnOutcome::failed(format!("run failed: {message}"))
            }
        }
    }
}

/// What `turn.end` reports about a finished turn. The failure itself already went out as
/// `final.error`; this is the one-line summary a transcript keeps.
pub(crate) struct TurnOutcome {
    files_changed: u32,
    tool_calls: u32,
    summary: String,
}

impl TurnOutcome {
    fn failed(summary: String) -> Self {
        Self {
            files_changed: 0,
            tool_calls: 0,
            summary,
        }
    }
}

/// The current branch, or `None` when git does not report one.
///
/// `None` covers the three real cases — not a repository, no commits yet, and a detached HEAD.
/// Naming a branch in any of them would be a guess presented as a fact.
fn current_branch(project: &Path) -> Option<String> {
    let repo = git2::Repository::discover(project).ok()?;
    if repo.head_detached().unwrap_or(false) {
        return None;
    }
    repo.head().ok()?.shorthand().map(str::to_owned)
}

/// Ahead/behind against the tracked upstream, and nothing else. `None` when there is no
/// upstream: the engine does not invent an origin it was not told about.
fn upstream_counts(project: &Path) -> (Option<u32>, Option<u32>) {
    let Ok(repo) = git2::Repository::discover(project) else {
        return (None, None);
    };
    let Ok(head) = repo.head() else {
        return (None, None);
    };
    let branch = head
        .shorthand()
        .and_then(|s| repo.find_branch(s, git2::BranchType::Local).ok());
    let Some(branch) = branch else {
        return (None, None);
    };
    let Ok(upstream) = branch.upstream() else {
        return (None, None);
    };
    let (Some(local_oid), Some(remote_oid)) = (branch.get().target(), upstream.get().target())
    else {
        return (None, None);
    };
    match repo.graph_ahead_behind(local_oid, remote_oid) {
        Ok((ahead, behind)) => (Some(ahead as u32), Some(behind as u32)),
        Err(_) => (None, None),
    }
}

/// How many files a unified diff says it touched, counted from its own `+++` headers.
///
/// Read off the diff the pipeline produced. An empty diff is zero files changed, not an error.
fn count_changed_files(diff: &str) -> u32 {
    let mut seen: Vec<&str> = Vec::new();
    for line in diff.lines() {
        let Some(path) = line.strip_prefix("+++ ") else {
            continue;
        };
        let path = path.trim();
        let path = path.strip_prefix("b/").unwrap_or(path);
        if path != "/dev/null" && !seen.contains(&path) {
            seen.push(path);
        }
    }
    seen.len() as u32
}

/// What this build actually does, and nothing it does not.
///
/// Each flag is read against the mapping in [`AdapterSink::map`]. `diffs` is on because the
/// adapter writes the pipeline's own diff to disk and points `diff.ready` at that real path;
/// `context_usage` is off because this build never emits `context.usage`, and a footer meter
/// fed by nothing is a meter that lies.
fn capabilities() -> Capabilities {
    Capabilities {
        streaming: true,
        approvals: true,
        sessions: true,
        diffs: true,
        context_usage: false,
        cost: true,
    }
}

// ---------------------------------------------------------------------------
// The adapter: `DisplayEvent` -> `ServerNotification`
// ---------------------------------------------------------------------------

/// Maps one run's `DisplayEvent`s onto the protocol's notifications.
pub(crate) struct AdapterSink {
    out: Arc<Mutex<Stdout>>,
    trace: TraceId,
    #[allow(dead_code)]
    turn_id: String,
    /// Where `diff.ready` points: the adapter writes the pipeline's own diff here, so the ref
    /// it hands the shell is a file that exists rather than a label for one somewhere.
    diff_dir: PathBuf,
    approvals: Arc<Approvals>,
    next_id: AtomicU64,
    /// Per-role attempt counter, so a revision round reports 2, 3, … instead of every pass
    /// claiming to be the first.
    attempts: Mutex<HashMap<&'static str, u32>>,
    /// Tool rows in flight, matched the way the TUI matches them: the first unmatched
    /// `ToolCall` of that name. Two `read` calls run in parallel and each result answers
    /// its own row.
    pending_tools: Mutex<HashMap<String, Vec<String>>>,
    tool_calls: AtomicU64,
    done: AtomicBool,
}

impl AdapterSink {
    fn new(
        out: Arc<Mutex<Stdout>>,
        trace: TraceId,
        turn_id: String,
        diff_dir: PathBuf,
        approvals: Arc<Approvals>,
    ) -> Self {
        Self {
            out,
            trace,
            turn_id,
            diff_dir,
            approvals,
            next_id: AtomicU64::new(0),
            attempts: Mutex::new(HashMap::new()),
            pending_tools: Mutex::new(HashMap::new()),
            tool_calls: AtomicU64::new(0),
            done: AtomicBool::new(false),
        }
    }

    fn id(&self, prefix: &str) -> String {
        format!("{prefix}-{}", self.next_id.fetch_add(1, Ordering::SeqCst))
    }

    fn tool_calls(&self) -> u32 {
        self.tool_calls.load(Ordering::SeqCst) as u32
    }

    fn mark_done(&self) {
        self.done.store(true, Ordering::SeqCst);
    }

    fn send(&self, event: ServerNotification) {
        let frame = match notification_value(&self.trace, event) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("niki serve: {e}");
                return;
            }
        };
        let mut out = match self.out.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Err(e) = write_frame(&mut *out, &frame) {
            eprintln!("niki serve: could not write a notification: {e}");
        }
    }

    fn notice(&self, text: impl Into<String>, level: niki_protocol::Severity) {
        self.send(ServerNotification::Notice(niki_protocol::NoticeParams {
            text: text.into(),
            level,
        }));
    }

    fn final_event(&self, verdict: Option<String>, error: Option<String>) {
        self.send(ServerNotification::Final(niki_protocol::FinalParams {
            verdict,
            error,
        }));
    }

    /// One `DisplayEvent` -> zero or more protocol notifications.
    fn map(&self, ev: DisplayEvent) {
        use niki_protocol::ServerNotification as N;
        match ev {
            DisplayEvent::StageStart { role } => {
                let name = role.as_str();
                let attempt = {
                    let mut attempts = match self.attempts.lock() {
                        Ok(g) => g,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let slot = attempts.entry(name).or_insert(0);
                    *slot += 1;
                    *slot
                };
                self.send(N::StageStart(niki_protocol::StageStartParams {
                    stage_id: name.to_string(),
                    role: stage_role(role),
                    attempt,
                }));
            }
            DisplayEvent::StageToken { role, token } => {
                if token.trim().is_empty() {
                    return;
                }
                self.send(N::StageToken(niki_protocol::StageTokenParams {
                    stage_id: role.as_str().to_string(),
                    role: stage_role(role),
                    text: token,
                }));
            }
            DisplayEvent::StageDone {
                role,
                summary,
                input_tokens,
                output_tokens,
                cost_usd,
                latency_ms,
                retry_count,
            } => {
                self.send(N::StageDone(niki_protocol::StageDoneParams {
                    stage_id: role.as_str().to_string(),
                    role: stage_role(role),
                    // The transcript's own lines, joined: the shell renders what the run said,
                    // not a separate summary invented for the wire.
                    summary: summary.join("\n"),
                    tokens_in: input_tokens,
                    tokens_out: output_tokens,
                    cost_usd,
                    latency_ms,
                    retry_count,
                    artifact_ref: None,
                    provenance: provenance_of(role),
                }));
            }
            DisplayEvent::StageFailed { role, error } => {
                self.send(N::StageFailed(niki_protocol::StageFailedParams {
                    stage_id: role.as_str().to_string(),
                    role: stage_role(role),
                    error,
                    severity: niki_protocol::Severity::Error,
                    recovery: None,
                }));
            }
            DisplayEvent::Notice { text, warning } => self.notice(
                text,
                if warning {
                    niki_protocol::Severity::Warning
                } else {
                    niki_protocol::Severity::Info
                },
            ),
            // A revision round is not itself a declared notification, so it reaches the shell
            // as the notice it is on every other surface: something happened, no verdict yet.
            DisplayEvent::Revision { round, max, issues } => self.notice(
                format!(
                    "revision round {round} of {max}: {}",
                    issues.join("; ")
                ),
                niki_protocol::Severity::Info,
            ),
            DisplayEvent::Final { verdict, error } => self.final_event(verdict, error),
            DisplayEvent::CostJson(json) => {
                if let Some(usd) = serde_json::from_str::<serde_json::Value>(&json)
                    .ok()
                    .and_then(|v| v.get("total_cost_usd").and_then(|c| c.as_f64()))
                {
                    self.send(N::CostUpdate(niki_protocol::CostUpdateParams { usd }));
                }
            }
            DisplayEvent::StageTotals { cost_usd, .. } => {
                self.send(N::CostUpdate(niki_protocol::CostUpdateParams { usd: cost_usd }));
            }
            DisplayEvent::BranchName(name) => {
                self.send(N::BranchCreated(niki_protocol::BranchCreatedParams { name }));
            }
            DisplayEvent::PermissionRequest {
                command,
                response_tx,
            } => self.request_approval(command, response_tx),
            DisplayEvent::ToolCall {
                tool_name, summary, ..
            } => {
                let tool_id = self.id("tool");
                match self.pending_tools.lock() {
                    Ok(mut pending) => pending
                        .entry(tool_name.clone())
                        .or_default()
                        .push(tool_id.clone()),
                    Err(poisoned) => poisoned
                        .into_inner()
                        .entry(tool_name.clone())
                        .or_default()
                        .push(tool_id.clone()),
                }
                self.tool_calls.fetch_add(1, Ordering::SeqCst);
                self.send(N::ToolCall(niki_protocol::ToolCallParams {
                    tool_id,
                    name: tool_name,
                    args: summary,
                }));
            }
            DisplayEvent::ToolResult {
                tool_name,
                success,
                error,
                output,
                duration_ms,
                ..
            } => {
                let tool_id = match self.pending_tools.lock() {
                    Ok(mut pending) => {
                        let queue = pending.entry(tool_name.clone()).or_default();
                        if queue.is_empty() {
                            // A result with no pending call still gets a row: dropping it
                            // would leave the shell showing a tool that finished without
                            // ever having been started.
                            self.id("orphan")
                        } else {
                            queue.remove(0)
                        }
                    }
                    Err(_) => self.id("orphan"),
                };
                self.send(N::ToolResult(niki_protocol::ToolResultParams {
                    tool_id,
                    ok: success,
                    summary: error.unwrap_or_else(|| first_line(output.unwrap_or_default())),
                    full_ref: None,
                    duration_ms,
                }));
            }
            DisplayEvent::DiffContent(diff) => {
                // The ref has to point at something. The pipeline produces the text and
                // nothing on this path writes it to disk, so the adapter writes it: the file
                // it names is one it just created.
                if diff.trim().is_empty() {
                    return;
                }
                match write_diff(&self.diff_dir, &diff) {
                    Ok(path) => self.send(N::DiffReady(niki_protocol::RefParams { ref_: path })),
                    Err(e) => self.notice(
                        format!("could not write the diff for the shell to read: {e}"),
                        niki_protocol::Severity::Warning,
                    ),
                }
            }
            // A question the engine asked a person. This server has nowhere to put a modal,
            // so it says so rather than dropping the question and letting the run continue as
            // if it had been answered.
            DisplayEvent::AskUser { question, .. } => self.notice(
                format!(
                    "the agent asked a question, which this surface cannot answer: {question}"
                ),
                niki_protocol::Severity::Warning,
            ),
            // Page-only payloads with no wire meaning, and the steer channel, which is a TUI
            // affordance. Dropped deliberately rather than mapped onto something they are not.
            DisplayEvent::Banner { .. }
            | DisplayEvent::ReportContent(_)
            | DisplayEvent::TestLogContent(_)
            | DisplayEvent::ArtifactsDir(_)
            | DisplayEvent::SteerChannel(_)
            // Chat-only transcript rows. `niki serve` runs the pipeline, not a conversation,
            // so nothing on this path produces them; they are named rather than swallowed by
            // a wildcard so a future `turn.start` that does emit one fails here instead of
            // going quiet on the wire.
            | DisplayEvent::ChatMessage { .. }
            | DisplayEvent::ChatPending
            | DisplayEvent::ChatDelta { .. }
            | DisplayEvent::ChatError { .. }
            | DisplayEvent::ChatFinished { .. } => {}
        }
    }

    /// Emit `approval.request` and hold until the reply arrives.
    ///
    /// The tool is blocked on its own channel; if nobody answers within the deadline the
    /// decision is Deny. Denying is the fail-closed answer and the only one safe to choose on
    /// the user's behalf.
    fn request_approval(&self, command: String, response_tx: Sender<PermissionAction>) {
        let id = self.id("approval");
        let pending = PendingApproval::new();
        self.approvals.put(id.clone(), Arc::clone(&pending));
        self.send(ServerNotification::ApprovalRequest(ApprovalRequestParams {
            id: id.clone(),
            tool: "shell".to_string(),
            command,
            options: vec![
                ApprovalOption {
                    id: "allow".to_string(),
                    label: "Allow".to_string(),
                },
                ApprovalOption {
                    id: "deny".to_string(),
                    label: "Deny".to_string(),
                },
            ],
            // The engine names the safest option, so a shell that focuses the first row by
            // accident still lands on Deny.
            safest_option_id: "deny".to_string(),
        }));

        let answered = pending.wait(APPROVAL_TIMEOUT).unwrap_or_else(|| {
            eprintln!(
                "niki serve: approval `{id}` was never answered; denying on the user's behalf"
            );
            PermissionAction::Deny
        });
        self.approvals.take(&id);
        let _ = response_tx.send(answered);
    }

    /// Answer the oldest pending approval on a user's behalf.
    ///
    /// Exists for the fixture runtime, where there is no shell to press a key. Returns whether
    /// there was anything to answer, so the caller can keep waiting rather than assume.
    #[cfg(feature = "fixture-runtime")]
    pub(crate) fn resolve_oldest_approval(&self, action: PermissionAction) -> bool {
        let Some(pending) = self.approvals.oldest() else {
            return false;
        };
        pending.decide(action);
        true
    }

    /// Run the mapping loop on its own thread while the pipeline is async.
    ///
    /// The loop ends on `mark_done`, not on channel disconnection: a `Sender` parked inside a
    /// live sandbox would keep the channel open well past the end of the run.
    fn spawn(self: &Arc<Self>, rx: Receiver<DisplayEvent>) -> AdapterHandle {
        let sink = Arc::clone(self);
        let (finished_tx, finished_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            loop {
                match rx.recv_timeout(ADAPTER_POLL) {
                    Ok(ev) => sink.map(ev),
                    Err(RecvTimeoutError::Timeout) => {
                        if sink.done.load(Ordering::SeqCst) {
                            break;
                        }
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            let _ = finished_tx.send(());
        });
        AdapterHandle {
            finished: finished_rx,
        }
    }
}

/// The reader half of an adapter, running on its own thread while the pipeline is async.
struct AdapterHandle {
    finished: Receiver<()>,
}

impl AdapterHandle {
    /// Wait for every mapped notification to reach stdout.
    fn join(self) {
        if self.finished.recv_timeout(DRAIN_TIMEOUT).is_err() {
            eprintln!("niki serve: the notification adapter did not drain in time");
        }
    }
}

fn write_diff(dir: &Path, diff: &str) -> Result<String, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join("changes.patch");
    std::fs::write(&path, diff).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

fn first_line(text: String) -> String {
    let line = text.lines().next().unwrap_or("").trim().to_string();
    if line.is_empty() {
        "done".to_string()
    } else {
        line
    }
}

/// `AgentRole` -> `StageRole`. One arm per role, so adding a role is a compile error here
/// rather than a silent fallthrough onto the wrong label.
fn stage_role(role: AgentRole) -> niki_protocol::StageRole {
    use niki_protocol::StageRole;
    match role {
        AgentRole::Planner => StageRole::Planner,
        AgentRole::Coder => StageRole::Coder,
        AgentRole::Tester => StageRole::Tester,
        AgentRole::Reviewer => StageRole::Reviewer,
        AgentRole::Synthesizer => StageRole::Synthesizer,
        AgentRole::SecurityAuditor => StageRole::SecurityAuditor,
        AgentRole::Red => StageRole::Red,
        AgentRole::Critic => StageRole::Critic,
    }
}

/// Whether a stage could see the code independently of the stage that wrote it.
fn provenance_of(role: AgentRole) -> niki_protocol::Provenance {
    use niki_protocol::Provenance;
    match role {
        // These never write the diff they judge.
        AgentRole::Reviewer | AgentRole::Red | AgentRole::Critic | AgentRole::SecurityAuditor => {
            Provenance::Independent
        }
        // These produce the work, so what they say about it is self-verification at best.
        AgentRole::Planner | AgentRole::Coder | AgentRole::Tester | AgentRole::Synthesizer => {
            Provenance::SelfVerification
        }
    }
}

#[cfg(feature = "fixture-runtime")]
pub(crate) mod fixture;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_methods_is_the_whole_message_set() {
        // Pinned here by hand rather than derived from `declared_methods()`, so that adding a
        // variant to `ClientRequest` fails this test instead of quietly widening the list
        // that decides which parse errors are reported as method-not-found.
        assert_eq!(
            declared_methods(),
            vec![
                "initialize",
                "shutdown",
                "session.load",
                "turn.start",
                "approval.reply"
            ]
        );
    }

    #[test]
    fn an_unknown_method_is_not_a_parse_error_and_a_bad_frame_is() {
        assert!(matches!(
            decode(
                r#"{"jsonrpc":"2.0","id":1,"trace_id":"t","method":"turn.strat","params":{}}"#
            ),
            Frame::UnknownMethod(m) if m == "turn.strat"
        ));
        assert!(matches!(
            decode(
                r#"{"jsonrpc":"2.0","id":1,"trace_id":"t","method":"turn.start","params":{"prompt":42}}"#
            ),
            Frame::Malformed(_)
        ));
        assert!(matches!(decode("this is not json"), Frame::Malformed(_)));
        assert!(matches!(decode(""), Frame::Malformed(_)));
    }

    #[test]
    fn a_malformed_line_is_answered_with_a_parse_error_and_no_result() {
        let mut buf: Vec<u8> = Vec::new();
        write_frame(
            &mut buf,
            &response_value(
                &niki_protocol::RequestId(0),
                &fallback_trace(),
                err(
                    niki_protocol::error_code::PARSE_ERROR,
                    "malformed frame",
                    None,
                ),
            )
            .expect("serialise"),
        )
        .expect("write");
        let text = String::from_utf8(buf).expect("utf-8");
        let parsed: serde_json::Value =
            serde_json::from_str(text.trim()).expect("one json object per line");
        assert_eq!(
            parsed["error"]["code"],
            niki_protocol::error_code::PARSE_ERROR
        );
        assert!(
            parsed.get("result").is_none(),
            "a parse error carries no result member: {parsed}"
        );
        assert_eq!(parsed["trace_id"], "unparsed");
    }

    #[test]
    fn capabilities_only_claim_what_the_adapter_emits() {
        let caps = capabilities();
        assert!(caps.streaming && caps.approvals && caps.sessions && caps.diffs && caps.cost);
        assert!(
            !caps.context_usage,
            "this build never emits `context.usage`, so claiming it would put a meter on the \
             shell's footer that nothing ever moves"
        );
    }

    #[test]
    fn changed_files_are_counted_from_the_diffs_own_headers() {
        let diff = "diff --git a/a.rs b/a.rs\n+++ b/a.rs\n@@\n+x\ndiff --git a/b.rs b/b.rs\n+++ b/b.rs\n@@\n+y\n";
        assert_eq!(count_changed_files(diff), 2);
        assert_eq!(count_changed_files(""), 0);
    }

    #[test]
    fn an_unreachable_project_reports_no_branch() {
        // Not a repository: there is no branch to report, and reporting one would be a guess.
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(current_branch(dir.path()), None);
        assert_eq!(upstream_counts(dir.path()), (None, None));
    }
}
