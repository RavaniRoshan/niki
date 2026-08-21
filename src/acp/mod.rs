//! Agent Client Protocol (ACP) — NIKI's IDE integration surface.
//!
//! Lets an IDE (Zed, Claude Code) drive NIKI's pipeline over stdio using the
//! JSON-RPC 2.0 ACP protocol. `protocol.rs` is the wire framing; `server.rs`
//! bridges requests to the pipeline.

pub mod protocol;
pub mod server;
