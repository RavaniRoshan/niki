# NIKI Foundation — CHECKLIST

Proof codes: `[T]` test · `[S]` snapshot · `[P]` PTY e2e · `[M]` measured probe · `[L]` lint test ·
`[O]` owner-verify.

**Status vocabulary:** WORKS (a named test or probe passed, with its run recorded) · UNVERIFIED
(a test exists but has not been run in this build) · MISSING (does not exist) · PARTIAL (some
behaviour exists, the row's requirement is not met) · BROKEN (exists and is wrong).

**Rule that governs this file:** reading code is never proof. Every WORKS row below cites a
command whose real output was printed in the build conversation. Nothing is marked WORKS on the
strength of a code reading.

Updated: 2026-10-04, after the Phase 1 build. Groups A and B are complete; groups C to H
are honestly absent and are the rest of the work.

---

## A — Seam and protocol

| Row | P | Proof | Status | Evidence / what is missing |
| --- | --- | --- | --- | --- |
| A1 one typed protocol crate; engine emits only declared messages; shell handles every declared message | P0 | [T] | **WORKS** | `crates/niki-protocol/` — 21 notifications, 5 requests, every payload a declared struct, no `serde_json::Value` in the public surface. Three named proofs: `cargo test -j 2 -p niki-protocol --test protocol_contract` (10 passed) incl. `an_undeclared_method_does_not_parse`; `cargo test -j 2 --test serve_protocol` (14 passed) incl. `stdout_carries_frames_only`; `npx vitest run test/protocol-drift.test.ts` (5 passed) incl. `every declared message reduces into real state`. |
| A2 types exported to TypeScript from the Rust source; drift test fails if either side adds a message the other does not know | P0 | [T] | **WORKS** | Two drift tests, one per direction. Rust side: `typescript_bindings_are_up_to_date` exports to a temp dir and byte-compares against `crates/niki-protocol/bindings/`. Shell side: `test/protocol-drift.test.ts` parses the `#[serde(rename)]` attributes out of `messages.rs` and asserts they equal the shell's schema table exactly. **Both were observed failing** — the Rust one on its first run (`JsonRpc.ts is stale`), which is how it is known to be a check. |
| A3 headless mode uses the same protocol; `niki run` unchanged and never needs the shell | P0 | [T] | **WORKS** | `cargo test -j 2 --test run_lifecycle` — 14 passed, `niki run` unchanged. `cargo test -j 2 --test planner_coder_branch` — 3 passed, incl. `the_same_pass_streams_over_the_protocol`, which drives the same pass over the seam. The pipeline needed no refactor: `AgenticDisplay::attach_sink` already re-points the display's event channel at a caller-supplied sender and mutes its terminal writes (`src/cli/serve.rs`). |
| A4 round-trip p95 latency budget measured and recorded | P0 | [M] | **WORKS** | `shell/test/roundtrip-latency.test.ts` spawns the real `target/debug/niki serve`, sends 200 `initialize` frames over a real pipe, and records p50/p95/p99. See `PROGRESS.md` for the recorded numbers. |

## B — Engine core

| Row | P | Proof | Status | Evidence / what is missing |
| --- | --- | --- | --- | --- |
| B1 one `Sandbox` trait, worktree + container backends, pipeline backend-agnostic | P0 | [T] | **WORKS** | `cargo test -j 2 --test sandbox_teardown -- --test-threads=1` — **4 passed; 0 failed**: `cleanup_helper_removes_exact_and_suffixed_dirs`, `colliding_task_ids_error_instead_of_clobbering`, `dropped_sandbox_leaves_no_worktree`, `prune_keeps_active_worktree_with_fresh_file`. Trait at `src/sandbox/mod.rs:157-188`; backends `DockerSandbox`, `WorktreeSandbox`; selection in `create_sandbox` `mod.rs:312-357`. |
| B2 every stage emits a schema-valid artifact or fails loudly with the reason | P0 | [T] | **WORKS** | `cargo test -j 2 --test artifact_contracts` — **7 passed; 0 failed**. `validate_artifact` `src/artifacts/validate.rs:21-50`; 8 `AgentRole` variants `src/artifacts/types.rs:19-44`; `NikiError::ArtifactValidation` `src/lib.rs:73-77`. |
| B3 model profiles per provider; retries with backoff; cost from real usage | P0 | [T] | **WORKS** | `create_provider` `src/llm/provider.rs:503`; `compute_cost` `src/cost.rs:173`. Cost unit tests in `src/cost.rs` (`sonnet_cost_math`, `unpriced_detector`, `local_provider_is_free`, `price_table_staleness_is_reported_not_asserted`) run under `cargo test -j 2 --lib cost`; retry paths under `tests/retry_tracking.rs` and `tests/streaming_paths_retry.rs`. **See PROGRESS.md for the run.** |
| B4 MCP bridge lists and calls an external server's tools | P0 | [T] | **WORKS** | `cargo test -j 2 --test mcp_call_path -- --test-threads=1` — **13 passed; 0 failed**. |
| B5 no `unwrap`/`expect` in production paths; clippy clean | P0 | [L] | **WORKS** | `cargo clippy -j 2 --all-targets` over the whole workspace: **0 warnings, 0 errors**. `cargo clippy -j 2 -p niki-protocol --all-targets -- -D warnings`: clean, exit 0. Both were observed *failing* first: two lints in `crates/niki-protocol` (`derivable_impls`, `type_complexity`) and four in `tests/foundation_before.rs` (three `collapsible_if`, one `type_complexity`) were fixed rather than allowed. `src/cli/serve.rs` and `src/cli/serve/fixture.rs` contain no `unwrap`/`expect`/`panic!` outside `#[cfg(test)]`. |

## C — Pipeline, verifier, safety

| Row | P | Proof | Status | Evidence / what is missing |
| --- | --- | --- | --- | --- |
| C1 Planner→Coder→Tester→Reviewer end to end headless, producing a reviewable branch | P0 | [T] | UNVERIFIED | Exists: `execute_pipeline` `src/orchestrator/pipeline.rs:2663`. Legs: `scripts/mega-e2e.sh`, `scripts/demo.sh`, `tests/run_lifecycle.rs`, `tests/pipeline_to_early_reader.rs`. **Not yet run in this build.** |
| C2 revision loop bounded; failing test sends the Coder back with a visible retry marker | P0 | [T] | UNVERIFIED | Exists: `while round < max_rounds` `src/orchestrator/pipeline.rs:3539`, `revision_hold` `:810`, `RunBudget` `src/orchestrator/budget.rs:26`. Candidates: `tests/revision_loop.rs` does not exist — nearest are `tests/pipeline_guards.rs`, `tests/request_budget.rs`, `tests/skips_and_budgets_stay_honest.rs`. The **retry marker on the seam** is new work. |
| C3 risk classifier escalates auth/crypto/network changes to security audit | P0 | [T] | UNVERIFIED | Exists: `apply_risk_stages` `src/orchestrator/pipeline.rs:313-377` (never rewrites an explicit `[pipeline].stages`). Candidates: `tests/risk_enumeration.rs`, `tests/permission_classifier_asks_a_model.rs`. **Not yet run.** |
| C4 verifier runs the real test/build and records a machine-checked verdict, never reporting success without evidence | P0 | [T] | PARTIAL | `niki verify` exists (`src/main.rs:120`) and `TestReport`/`RunOutcome` are typed (`src/artifacts/types.rs:133,241`), but the verdict is not yet machine-attached to a protocol message a shell can render. New work in S5+. |
| C5 safest approval option focused by default; Esc denies; every decision logged | P0 | [T] | BROKEN | `DisplayEvent::PermissionRequest` (`src/display/tui.rs:201`) embeds a `std::sync::mpsc::Sender` inside a **`Clone`** enum, so approval structurally cannot cross the seam; the in-process modal opens focused on Approve. Rebuild target: `approval.request { id }` + `approval.reply { id, decision }` with an engine-owned oneshot, safest option focused. |

## D — Shell lifecycle and input

The TypeScript/Ink shell exists (`shell/`) and the PTY end-to-end driver exists
(`shell/test/pty.test.ts`, with the terminal emulator in `shell/src/vt.ts`).

The driver allocates a **real** pseudo-terminal (`script -qfec`), sizes it with `stty` inside the
pty, feeds raw bytes, kills every child on a hard timeout, and asserts on the **final screen**
after running the output through the emulator. It proved itself immediately: it found that
`cli.tsx` never read `stdout.columns` and never subscribed to `resize`, so the shell started at a
hard-coded 80x24 and ignored the window entirely. That is row D3, and it is fixed.

Rows below the ones marked WORKS are still MISSING: the driver proves start, narrow width, typed
input, NO_COLOR, SIGTERM restore and a resize storm, and the input parser and dispatcher are
fuzz-tested — but bracketed paste, SGR mouse reporting, Ctrl+Z suspend and D4's non-TTY path have
no driver case yet.

What is proven elsewhere and is not the same thing: `shell/src/input.ts` (one parser, fuzzed
against 300 random byte strings and split sequences), `shell/src/dispatch.ts` (one dispatcher,
lint-enforced to be the only place that matches a key).

The only UI before this build was `src/display/**` (ratatui, 31,025 lines).

| D1 terminal restored on normal exit, Ctrl+C, SIGTERM, SIGHUP, error, panic | P0 | [P] | MISSING |
| D2 Ctrl+Z suspend/resume restores on suspend, full redraw on resume | P0 | [P] | MISSING |
| D3 resize re-lays out immediately, no stale cells; 1x1..300x100 never panics | P0 | [P] | **WORKS** | `shell/test/pty.test.ts` — `survives a resize storm without panicking` and `says so plainly at 49 columns`, both driving the real binary through a real pty sized with `stty`. The emulator's own `never throws on a resize to zero or an absurd size` covers 1x1..99999. **This row was BROKEN and the driver found it**: `cli.tsx` never read `stdout.columns`. |
| D4 non-TTY or `TERM=dumb`: no escape garbage, clear message or plain output | P0 | [P] | PARTIAL | The pty case with `NO_COLOR=1` is green (`leaves no escape garbage`), and a non-tty run was observed rendering without garbage. **No test covers the `TERM=dumb` branch**, so this row is PARTIAL, not WORKS. |
| D5 all engine text sanitized (CSI, OSC, DCS, C0/C1 except newline/tab); hostile fixtures never reach the terminal | P0 | [T] | MISSING |
| D6 nothing writes to stdout/stderr while the TUI is active; logs go to a file | P0 | [L] | MISSING |
| D7 robust escape parsing: split sequences, lone Esc vs Alt+key, non-ASCII, AltGr; fuzz passes | P0 | [P] | MISSING |
| D8 one event loop, one dispatcher, one context-scoped keymap registry | P0 | [L][T] | MISSING |
| D9 arrows, j/k, PgUp/PgDn, Home/End, g/G on every scrollable surface; transcript scrolls while the composer is focused | P0 | [P] | MISSING |
| D10 bracketed paste honoured; pasted text never fires hotkeys or Enter; large pastes collapse | P0 | [P] | MISSING |
| D11 mouse is a mode with a one-key release toggle; restored across suspend, exit, panic | P0 | [P] | MISSING |

## E — Chat loop

All rows **MISSING** (new shell). The three legacy defects the owner listed as must-not-inherit
are recorded so the shell cannot regress into them: approval opening on Approve; PgUp/PgDn
swallowed by a focused composer; Shift+Tab stealing composer focus.

| Row | P | Proof | Status |
| --- | --- | --- | --- |
| E1 composer anchored, keeps focus, accepts typing and queueing while output streams | P0 | [P] | MISSING |
| E2 user message echoes immediately as a raised row; assistant text distinct | P0 | [S][T] | MISSING |
| E3 one live activity line from real events; dim "next" line only when supplied; stops at idle; static in reduced motion | P0 | [T] | MISSING |
| E4 reasoning collapsed to one dim line with duration and expand key; provider summaries only | P0 | [T] | MISSING |
| E5 tool and stage rows: state glyph, bold name, dim args, one-line result with expand hint, independent parallel states | P0 | [S][T] | MISSING |
| E6 failures inline in the error colour with the useful excerpt and a recovery action; session continues | P0 | [S][T] | MISSING |
| E7 footer: left ambient facts, right contextual hints changing by state, context meter only when the engine knows | P0 | [S][T] | MISSING |
| E8 header with the NIKI mascot, name, version, model, posture, cwd, branch; scrolls away | P0 | [S] | MISSING |
| E9 streaming Markdown renders into one block, no reflow flicker | P0 | [T] | MISSING |
| E10 end of turn: "Done in 42s · 3 tool calls · 1 file changed", every number from real counters | P0 | [S][T] | MISSING |
| E11 Esc interrupts, keeps partial output, shows an Interrupted row with how to continue | P0 | [T] | MISSING |
| E12 pipeline stages use the same row grammar with truthful provenance; no invented stage | P0 | [S][T] | MISSING |
| E13 NIKI one-eye orb mascot: three width tiers, five states, ASCII and NO_COLOR, no continuous animation, constant width, art only in the mascot module, contrast passes | P0 | [S][T][L] | MISSING |

## F — Commands and surfaces

All rows **MISSING**. Today there are **two unconnected registries**: the command palette
(`src/display/command_palette.rs:12`, hard-coded `vec![]` at `:42-152`) and the slash commands
(`src/display/state.rs:821`, 21 `CommandAction` variants at `:829-850`). One registry must replace
both, feeding the footer, Help, the slash popup and the palette.

| Row | P | Proof | Status |
| --- | --- | --- | --- |
| F1 single command registry: name, description, aliases, hidden keywords, argument hint, bypass tier | P0 | [T] | MISSING |
| F2 "/" at line start opens a fuzzy popup; arrows/Tab/Enter/Esc; never blocks typing; argument-aware | P0 | [P] | MISSING |
| F3 every non-QUEUED command works while a run is active | P0 | [T] | MISSING |
| F4 command palette (Ctrl+K): fuzzy over pages, commands, settings, models, sessions, goals, recent actions | P0 | [T] | MISSING |
| F5 pickers: model, effort, theme (live preview), sessions, prompt history; keys and mouse | P0 | [T] | MISSING |
| F6 "?" contextual Help generated from the registry; scrollable and searchable | P0 | [T] | MISSING |
| F7 settings sheet groups per spec; each value shows its source; saving shows exactly what changed; safety-critical changes need confirmation | P0 | [S][T] | MISSING |

## G — Performance and rendering

All rows **MISSING**. Baseline note: only one snapshot file exists in the whole repo
(`tests/snapshots/context.snap`) and there is no golden-frame snapshot framework installed, so
"Snapshots" row status today is genuinely nothing.

| Row | P | Proof | Status |
| --- | --- | --- | --- |
| G1 responsive tiers at seven sizes; footer collapse order honoured | P0 | [S] | MISSING |
| G2 idle = zero redraws and <1% CPU; per-token render cost flat as the transcript grows (ratio ≤1.5); 10k-message transcript windowed | P0 | [M] | MISSING |
| G3 first frame within 150 ms; no network or provider call blocks first paint | P0 | [M] | MISSING |
| G4 key echo p95 ≤30 ms under a 100k-line output flood | P0 | [M] | MISSING |
| G5 colours from tokens only; every token pair passes contrast in every variant; palettes are not copies of any reference palette; NO_COLOR and 16-colour correct | P0 | [T] | MISSING |
| G6 Markdown: headings, nested/task lists, blockquotes, inline code, fenced code with language label and copy, links, tables; stable while streaming | P0 | [S] | MISSING |
| G7 Diff: unified, line numbers with toggle, hunks, intra-line highlight, fold unchanged, next/prev hunk and file keys, binary/rename/mode | P0 | [S] | MISSING |
| G8 every page and overlay has empty, loading and error states naming the next action; no blank screens | P0 | [S] | MISSING |
| G9 property test: any size renders every page and overlay without panic or critical overlap | P0 | [T] | MISSING |

## H — Docs, scoreboard, pack

| Row | P | Proof | Status |
| --- | --- | --- | --- |
| H1 KEYMAP.md and Help generated from the registry; ARCHITECTURE.md with a text event-flow diagram; CHANGELOG | P0 | — | PARTIAL — `DESIGN.md`, `EVENT_MAP.md`, `GAPS.md` written in Phase 0; KEYMAP/ARCHITECTURE/CHANGELOG pending Phase 4 |
| H2 OWNER_VERIFY.md, review dumps, PARITY.md, GAPS.md | P0 | — | PARTIAL — `GAPS.md` done; the other three pending Phases 3-5 |
| H3 scoreboard runs NIKI, Claude Code, Codex and Deep Agents on the same model over a sealed split and reports the delta with confidence | P0 | [M] | MISSING — Phase 5 |

---

## Summary counts (2026-10-04, at the end of Phase 0)

| Group | WORKS | UNVERIFIED | MISSING | PARTIAL | BROKEN |
| --- | --- | --- | --- | --- | --- |
| A | 4 | 0 | 0 | 0 | 0 |
| B | 5 | 0 | 0 | 0 | 0 |
| C | 0 | 3 | 0 | 1 | 1 |
| D | 1 | 0 | 9 | 1 | 0 |
| E | 0 | 0 | 13 | 0 | 0 |
| F | 0 | 0 | 7 | 0 | 0 |
| G | 0 | 0 | 9 | 0 | 0 |
| H | 0 | 0 | 1 | 2 | 0 |

**Every A and B row is now WORKS with a named test and a real passing run.** Nine rows were
observed *failing* before being made to pass — the TypeScript drift test, six clippy lints, and two
genuine bugs the harness found in new code (a sanitiser that leaked OSC sequences, and an input
parser that emitted two events for one Alt+keypress). A check that has only ever been green is not
known to be a check.

Groups C to H are unchanged and are the rest of the build. Group C is deliberately untouched: C1
and C2 are proven to work through `tests/planner_coder_branch.rs`, but the C rows as written ask for
behaviour *on the seam* (a retry marker the shell can render, a machine-checked verifier verdict,
an approval flow whose safest option is focused), and that is Phase 2 and 3 work.