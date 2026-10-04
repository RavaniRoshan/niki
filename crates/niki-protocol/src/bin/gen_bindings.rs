//! Regenerates the TypeScript bindings from the Rust source.
//!
//! Run with `cargo run -p niki-protocol --bin gen-protocol-bindings` after changing any message
//! type. `tests/protocol_contract.rs::typescript_bindings_are_up_to_date` is what enforces the
//! contract: it exports into a temp directory and compares. Nothing here runs automatically, so
//! the comparison can never be satisfied by quietly rewriting the files first.

use std::path::PathBuf;

fn main() -> std::process::ExitCode {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bindings");
    match niki_protocol::export_all_bindings(&out) {
        Ok(()) => {
            println!("wrote TypeScript bindings to {}", out.display());
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("gen-protocol-bindings: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
