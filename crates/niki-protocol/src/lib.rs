//! The one protocol between the NIKI engine and any shell.
//!
//! Transport is newline-delimited JSON-RPC 2.0 over stdio: one JSON object per line, UTF-8, `\n`
//! terminated. The engine runs as a child process of the shell. Headless CI drives the same
//! protocol through the same types.
//!
//! Three rules hold here and are tested, not merely intended:
//!
//! 1. **Nothing untyped crosses the seam.** Every payload is a declared struct. There is no
//!    `serde_json::Value` in this crate's public surface, so a shell cannot receive a shape it has
//!    never heard of.
//! 2. **Every frame carries a `trace_id`.** A user complaint is one string.
//! 3. **The message set is closed.** An unknown `method` is a parse error, not a shrug, so an
//!    engine that invents a message breaks loudly on the client instead of doing nothing.
//!
//! ## Wire shape
//!
//! ```text
//! shell -> engine   {"jsonrpc":"2.0","id":1,"trace_id":"t1","method":"turn.start","params":{...}}
//! engine -> shell   {"jsonrpc":"2.0","id":1,"trace_id":"t1","result":{...}}
//! engine -> shell   {"jsonrpc":"2.0","id":1,"trace_id":"t1","error":{"code":-32001,"message":...}}
//! engine -> shell   {"jsonrpc":"2.0","trace_id":"t1","method":"stage.start","params":{...}}
//! ```

use serde::{Deserialize, Serialize};
use ts_rs::TS;

mod messages;

pub use messages::*;

/// Bumped whenever a declared message changes shape. A shell refuses to run against an engine
/// whose version it does not recognise.
pub const PROTOCOL_VERSION: u32 = 1;

/// The literal JSON-RPC version string, serialised into every frame. A one-variant enum rather
/// than a unit struct, because a unit struct serialises as `null` and the wire format requires
/// the string `"2.0"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(type = "\"2.0\"")]
pub enum JsonRpc {
    #[serde(rename = "2.0")]
    Version,
}

impl JsonRpc {
    pub const VALUE: JsonRpc = JsonRpc::Version;
}

/// A correlation id. Requests carry one; responses echo it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(type = "number")]
pub struct RequestId(pub u64);

/// Correlates every frame belonging to one run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(type = "string")]
pub struct TraceId(pub String);

impl TraceId {
    /// A trace id is never empty: an empty string cannot be grepped for.
    pub fn new(raw: impl Into<String>) -> Result<Self, ProtocolError> {
        let raw = raw.into();
        if raw.is_empty() {
            return Err(ProtocolError::EmptyTraceId);
        }
        Ok(Self(raw))
    }
}

/// Every way the protocol itself can fail, independent of anything the engine is doing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("a frame carried an empty trace_id")]
    EmptyTraceId,
    #[error("malformed json-rpc frame: {0}")]
    Malformed(String),
    #[error("unknown method `{method}`; this build speaks protocol v{PROTOCOL_VERSION}")]
    UnknownMethod { method: String },
    #[error("peer speaks protocol v{peer}, this build speaks v{PROTOCOL_VERSION}")]
    VersionMismatch { peer: u32 },
    #[error("io error on the seam: {0}")]
    Io(String),
}

/// JSON-RPC 2.0 error codes, plus the two NIKI reserves on top of them.
pub mod error_code {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    /// The engine ran and failed. `data` carries the typed reason.
    pub const ENGINE_ERROR: i32 = -32000;
    /// A permission decision is required and the run stays blocked until one arrives.
    pub const APPROVAL_REQUIRED: i32 = -32001;
}

/// The `error` member of a JSON-RPC error response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    /// Always a string, never a nested free-form value: the shell must be able to render it.
    pub data: Option<String>,
}

impl RpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn with_data(mut self, data: impl Into<String>) -> Self {
        self.data = Some(data.into());
        self
    }

    pub fn parse_error(detail: impl Into<String>) -> Self {
        Self::new(error_code::PARSE_ERROR, "malformed frame").with_data(detail)
    }

    pub fn unknown_method(method: &str) -> Self {
        Self::new(
            error_code::METHOD_NOT_FOUND,
            format!("unknown method `{method}`"),
        )
    }
}

/// A request from the shell to the engine. One line, one JSON object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RequestFrame {
    pub jsonrpc: JsonRpc,
    pub id: RequestId,
    pub trace_id: TraceId,
    #[serde(flatten)]
    pub call: ClientRequest,
}

/// The response to exactly one request. Exactly one of the two arms is present on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ResponseFrame {
    pub jsonrpc: JsonRpc,
    pub id: RequestId,
    pub trace_id: TraceId,
    #[serde(flatten)]
    pub outcome: ResponseOutcome,
}

/// JSON-RPC's own two-valued result: a result or an error, never both, never neither.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ResponseOutcome {
    Result(ClientResult),
    Error(RpcError),
}

/// A message the engine pushes without being asked. One line, one JSON object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NotificationFrame {
    pub jsonrpc: JsonRpc,
    pub trace_id: TraceId,
    #[serde(flatten)]
    pub event: ServerNotification,
}

/// Reads one line and returns the reply line, or `None` for a notification.
///
/// A line that is neither a declared request nor a declared notification is an error, never a
/// silent success. That is what makes "the engine emits only declared messages" checkable.
pub fn handle_line(line: &str) -> Result<Option<String>, ProtocolError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(ProtocolError::Malformed("empty line".into()));
    }
    if let Ok(req) = serde_json::from_str::<RequestFrame>(line) {
        let outcome = dispatch(&req.call);
        let frame = ResponseFrame {
            jsonrpc: JsonRpc::VALUE,
            id: req.id.clone(),
            trace_id: req.trace_id,
            outcome,
        };
        return serde_json::to_string(&frame)
            .map(Some)
            .map_err(|e| ProtocolError::Malformed(e.to_string()));
    }
    // A notification is recognised only if it is a *declared* one; `serde_json` rejects the rest.
    match serde_json::from_str::<NotificationFrame>(line) {
        Ok(_) => Ok(None),
        Err(_) => Err(ProtocolError::UnknownMethod {
            method: peek_method(line).unwrap_or_else(|| "<unparseable>".into()),
        }),
    }
}

fn peek_method(line: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()?
        .get("method")?
        .as_str()
        .map(str::to_owned)
}

/// The minimal in-crate dispatcher. It exists so the crate can test its own contract without a
/// running engine; `niki serve` supplies the real handlers behind the same types.
fn dispatch(call: &ClientRequest) -> ResponseOutcome {
    match call {
        ClientRequest::Initialize(_) => {
            ResponseOutcome::Result(ClientResult::Initialize(InitializeResult {
                protocol_version: PROTOCOL_VERSION,
                engine_version: env!("CARGO_PKG_VERSION").to_string(),
                capabilities: Capabilities::default(),
            }))
        }
        ClientRequest::Shutdown(_) => {
            ResponseOutcome::Result(ClientResult::Shutdown(ShutdownResult { ok: true }))
        }
        other => ResponseOutcome::Error(RpcError::new(
            error_code::METHOD_NOT_FOUND,
            format!(
                "`{}` is declared but not served by this build",
                other.method()
            ),
        )),
    }
}

/// The types the shell imports directly. Their dependencies are exported alongside them.
const BINDING_ROOTS: &[&str] = &[
    "ClientRequest",
    "ServerNotification",
    "RequestFrame",
    "ResponseFrame",
    "NotificationFrame",
];

/// Writes the TypeScript bindings for every root type, and everything they depend on, into
/// `out_dir`. The generator binary calls this to refresh `bindings/`; the contract test calls it
/// into a temp directory to prove `bindings/` is current. One list, two callers.
pub fn export_all_bindings(out_dir: impl AsRef<std::path::Path>) -> Result<(), String> {
    let out = out_dir.as_ref();
    std::fs::create_dir_all(out).map_err(|e| format!("create {}: {e}", out.display()))?;
    let exporters: [Exporter; 5] = [
        export::<ClientRequest>,
        export::<ServerNotification>,
        export::<RequestFrame>,
        export::<ResponseFrame>,
        export::<NotificationFrame>,
    ];
    for export_one in exporters {
        export_one(out)?;
    }
    Ok(())
}

/// One monomorphised exporter. Named so the array above reads as a list rather than as a type.
type Exporter = fn(&std::path::Path) -> Result<(), String>;

fn export<T: ts_rs::TS + 'static>(out: &std::path::Path) -> Result<(), String> {
    T::export_all_to(out).map_err(|e| format!("export {}: {e:?}", BINDING_ROOTS.len()))
}
