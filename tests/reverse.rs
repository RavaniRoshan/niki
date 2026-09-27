//! The reverse harness: tests that attack the product rather than confirm it.
//!
//! Everything under `tests/reverse/` shares one discipline — a test here must
//! be able to fail, and must fail for a reason that names the bug it caught.
//! A test that passes on a broken product is worse than no test, because it
//! is trusted.
//!
//! Layers:
//! - `llm_faults` — a model that misbehaves, at the transport boundary.
//! - `invariants`  — properties that must hold for every recorded run.
//! - `injection`   — untrusted content reaching a terminal or a role.
//! - `journeys`    — what a real user hits, on a throwaway profile.

#[path = "reverse/invariants.rs"]
pub mod invariants;

#[path = "reverse/llm_faults/mod.rs"]
pub mod llm_faults;

mod common;

#[path = "reverse/invariants_ratchet.rs"]
pub mod invariants_ratchet;

#[path = "reverse/injection.rs"]
pub mod injection;

#[path = "reverse/cost.rs"]
pub mod cost;
