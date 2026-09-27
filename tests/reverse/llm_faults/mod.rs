//! Fault-injecting LLM doubles for the reverse harness.
//!
//! The in-process `MockProvider` (`src/llm/mock.rs`) can only ever return
//! canned, schema-valid, well-formed artifacts. That means every pipeline test
//! using it proves the pipeline wires together *given a cooperative model* —
//! it cannot detect a model that returns prose, a truncated stream, an empty
//! body, a verdict contradicting its own issues, or a tool call that loops
//! forever.
//!
//! These modules inject at the transport boundary, so every provider path is
//! covered with no production code change.

pub mod provider;
pub mod server;
pub mod sse;

pub use server::{Fault, FaultServer};
