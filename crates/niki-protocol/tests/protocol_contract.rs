//! The protocol contract, enforced from the Rust side.
//!
//! Three properties, each with a test that fails when it stops holding:
//!
//! 1. The message set is **closed**: a frame naming a method nobody declared does not parse.
//! 2. Every declared message **round-trips**, so a shell cannot receive something the engine
//!    cannot itself read back.
//! 3. The TypeScript bindings are **not stale**: if the Rust changes and `bindings/` is not
//!    regenerated, the build fails here rather than in the shell.

use std::collections::BTreeSet;
use std::path::PathBuf;

use niki_protocol::{
    ClientRequest, JsonRpc, NotificationFrame, PROTOCOL_VERSION, ProtocolError, RequestFrame,
    RequestId, ResponseFrame, ResponseOutcome, RpcError, ServerNotification, TraceId, error_code,
    export_all_bindings, handle_line,
};

fn trace() -> TraceId {
    TraceId::new("trace-1").unwrap_or_else(|_| TraceId("trace-1".into()))
}

fn frame(n: &ServerNotification) -> NotificationFrame {
    NotificationFrame {
        jsonrpc: JsonRpc::VALUE,
        trace_id: trace(),
        event: n.clone(),
    }
}

fn line(n: &ServerNotification) -> String {
    serde_json::to_string(&frame(n))
        .unwrap_or_else(|e| panic!("declared message must serialise: {e}"))
}

#[test]
fn every_declared_notification_round_trips_through_json() {
    let all = ServerNotification::all();
    assert_eq!(
        all.len(),
        21,
        "all() must construct one of each declared notification; a new variant means adding one here too"
    );
    for n in &all {
        let encoded = line(n);
        let decoded: NotificationFrame =
            serde_json::from_str(&encoded).unwrap_or_else(|e| panic!("{encoded} must decode: {e}"));
        assert_eq!(&decoded.event, n, "round trip changed {encoded}");
        assert!(
            !encoded.contains('\n'),
            "a frame must be one line: {encoded}"
        );
    }
}

#[test]
fn the_wire_method_name_is_the_one_the_variant_declares() {
    for n in ServerNotification::all() {
        let encoded = line(&n);
        let value: serde_json::Value =
            serde_json::to_value(&n).unwrap_or_else(|e| panic!("{encoded} must be json: {e}"));
        let method = value
            .get("method")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| panic!("{encoded} carries no method"));
        assert_eq!(
            method,
            n.method(),
            "the serde rename and `method()` disagree for {method}"
        );
    }
}

#[test]
fn notification_method_names_are_unique() {
    let mut seen = BTreeSet::new();
    for n in ServerNotification::all() {
        assert!(
            seen.insert(n.method()),
            "two notifications claim the method `{}`",
            n.method()
        );
    }
    assert_eq!(seen.len(), 21);
}

#[test]
fn an_undeclared_method_does_not_parse() {
    // The whole point of a closed set: an engine that invents a message breaks loudly.
    let invented = r#"{"jsonrpc":"2.0","trace_id":"t","method":"stage.telepathy","params":{}}"#;
    assert!(serde_json::from_str::<NotificationFrame>(invented).is_err());

    let err = handle_line(invented).unwrap_err();
    assert_eq!(
        err,
        ProtocolError::UnknownMethod {
            method: "stage.telepathy".into()
        }
    );
}

#[test]
fn a_declared_request_gets_exactly_one_line_back() {
    let req = RequestFrame {
        jsonrpc: JsonRpc::VALUE,
        id: RequestId(7),
        trace_id: trace(),
        call: ClientRequest::Initialize(niki_protocol::InitializeParams {
            protocol_version: PROTOCOL_VERSION,
            client: niki_protocol::ClientInfo {
                name: "shell".into(),
                version: "0.1.0".into(),
                cols: 80,
                rows: 24,
            },
        }),
    };
    let encoded = serde_json::to_string(&req).unwrap_or_else(|e| panic!("{e}"));
    let reply = handle_line(&encoded)
        .unwrap_or_else(|e| panic!("a declared request must be answered: {e}"))
        .unwrap_or_else(|| "a request must produce a response".into());

    let response: ResponseFrame =
        serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{reply} must decode: {e}"));
    assert_eq!(response.id, RequestId(7), "the id must be echoed");
    assert!(matches!(response.outcome, ResponseOutcome::Result(_)));
}

#[test]
fn initialize_reports_the_version_the_shell_can_check() {
    let reply = handle_line(
        r#"{"jsonrpc":"2.0","id":1,"trace_id":"t","method":"initialize","params":{"protocol_version":1,"client":{"name":"s","version":"0","cols":80,"rows":24}}}"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .unwrap_or_default();
    assert!(
        reply.contains(&format!("\"protocol_version\":{PROTOCOL_VERSION}")),
        "the shell must be able to see the protocol version in the reply: {reply}"
    );
}

#[test]
fn a_declaration_the_build_does_not_serve_is_an_explicit_error_not_a_silent_success() {
    let reply = handle_line(
        r#"{"jsonrpc":"2.0","id":2,"trace_id":"t","method":"turn.start","params":{"prompt":"hi","permission_mode":"manual"}}"#,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .unwrap_or_default();
    let response: ResponseFrame =
        serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{reply} must decode: {e}"));
    match response.outcome {
        ResponseOutcome::Error(RpcError { code, message, .. }) => {
            assert_eq!(code, error_code::METHOD_NOT_FOUND);
            assert!(
                message.contains("turn.start"),
                "the error must name the method: {message}"
            );
        }
        other => panic!("expected an explicit error, got {other:?}"),
    }
}

#[test]
fn an_empty_line_and_garbage_are_both_errors() {
    assert!(handle_line("").is_err());
    assert!(handle_line("not json at all").is_err());
}

#[test]
fn an_empty_trace_id_is_refused() {
    assert!(TraceId::new("").is_err());
    assert!(TraceId::new("t").is_ok());
}

#[test]
fn typescript_bindings_are_up_to_date() {
    let tmp = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("protocol_bindings_export");
    // Start from nothing so a deleted type cannot linger in the comparison.
    let _ = std::fs::remove_dir_all(&tmp);
    export_all_bindings(&tmp).unwrap_or_else(|e| panic!("export failed: {e}"));

    let committed = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bindings");
    let mut generated: BTreeSet<String> = BTreeSet::new();
    collect_ts_files(&tmp, &tmp, &mut generated);
    let mut committed_files: BTreeSet<String> = BTreeSet::new();
    collect_ts_files(&committed, &committed, &mut committed_files);

    assert!(
        !generated.is_empty(),
        "the exporter produced nothing; the drift check would pass vacuously"
    );

    let missing: Vec<_> = generated.difference(&committed_files).collect();
    assert!(
        missing.is_empty(),
        "these TypeScript types are generated but not committed: {missing:?}. \
         Run `cargo run -p niki-protocol --bin gen-protocol-bindings`."
    );

    let stale: Vec<_> = committed_files.difference(&generated).collect();
    assert!(
        stale.is_empty(),
        "these committed TypeScript types are no longer generated by the Rust source: {stale:?}. \
         Run `cargo run -p niki-protocol --bin gen-protocol-bindings`."
    );

    for name in &generated {
        let a = std::fs::read_to_string(tmp.join(name))
            .unwrap_or_else(|e| panic!("reading generated {name}: {e}"));
        let b = std::fs::read_to_string(committed.join(name)).unwrap_or_else(|e| {
            panic!(
                "reading committed {name}: {e}. Run `cargo run -p niki-protocol --bin gen-protocol-bindings`."
            )
        });
        assert_eq!(
            a, b,
            "{name} is stale. Run `cargo run -p niki-protocol --bin gen-protocol-bindings`."
        );
    }
    println!("typescript bindings are current: {} files", generated.len());
}

fn collect_ts_files(root: &PathBuf, dir: &PathBuf, out: &mut BTreeSet<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ts_files(root, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ts")
            && let Ok(rel) = path.strip_prefix(root)
        {
            out.insert(rel.to_string_lossy().into_owned());
        }
    }
}
