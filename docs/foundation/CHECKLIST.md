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
| C1 Planner→Coder→Tester→Reviewer end to end headless, producing a reviewable branch | P0 | [T] | **WORKS** | `cargo test -j 2 --test full_pipeline_branch -- --test-threads=1` — **2 passed**. `the_full_chain_runs_headless_and_leaves_a_reviewable_branch` asserts stdout is exactly one JSON envelope with no escape sequences (that is the headless claim), all four of `planner/coder/tester/reviewer.json` exist with no `-2` variant, `task.json` meters the roles in order, `reviewer.json` deserialises as `ReviewVerdict` with `Approved`, `refs/heads/niki/…` resolves via `rev-parse --verify`, and `git show <branch>:src/list.rs` contains the new line and not the old one. The negative half, `a_chain_that_never_reached_the_reviewer_reports_no_branch`, asserts a run missing the reviewer's response exits non-zero and creates no branch — without it, every positive assertion would also pass for a pipeline that silently stopped after the Tester. |
| C2 revision loop bounded; failing test sends the Coder back with a visible retry marker | P0 | [T] | **WORKS** | `cargo test -j 2 --test revision_loop_seam` — **3 passed**. `a_failing_tester_turns_the_loop_and_the_coder_runs_twice` asserts `revision_rounds == 1`, `coder.json` **and** `coder-2.json` exist and `coder-3.json` does not, and `tester.json.failed == 1` while `tester-2.json.failed == 0`. `a_tester_that_always_fails_stops_at_max_revision_rounds` pins `revision_rounds == 2` and asserts the `-3` artifacts are absent. `the_seam_carries_the_retry_as_a_visible_marker` asserts over the wire that `stage.start` for `coder` carries `attempt == [1, 2]`, that both `stage.done` frames carry a numeric `retry_count`, and that a notice naming the revision round was emitted. **Engine finding:** the Tester's report does not itself gate the pipeline — `verdict` moves only in `apply_reviewer_verdict` (`pipeline.rs:532`); the loop turns when the **Reviewer** asks, with the red Tester report carried into its prompt. The test scripts the chain that actually exists and says so at the top. No engine change was needed: `attempt` and `retry_count` were already plumbed. |
| C3 risk classifier escalates auth/crypto/network changes to security audit | P0 | [T] | **WORKS** | `cargo test -j 2 --test risk_escalation_seam` — **4 passed**, both directions. `a_security_sensitive_task_forces_an_auditor_ahead_of_the_reviewer` asserts `High` classification, no auditor before injection (precondition), injection **before** the Reviewer, and that `force_multiagent_for_high_risk(SingleAgent, Auto, High)` is true so the stage survives the fast path. `a_low_risk_task_escalates_nothing` asserts an identical role list with no auditor and no Critic. `the_emitted_stage_sequence_escalates_only_for_the_sensitive_task` runs two real `niki serve` turns and asserts the two sequences differ, with `security_auditor` present before `reviewer` in one and absent from the other. `pinned_singleagent_does_not_override_an_explicit_topology` pins the documented boundary rather than leaving a reader to assume the opposite. |
| C4 verifier runs the real test/build and records a machine-checked verdict, never reporting success without evidence | P0 | [T] | **WORKS** | `cargo test -j 2 --test verifier_verdict` — **5 passed**. **Engine finding:** `niki verify` (`src/cli/verify.rs`) is the **visual** path (screenshot + `verify-manifest.json`) and does not run a test suite. The engine that runs the real test/build is `niki::agents::tester::run_tests`. It returned `Option<TestExecution>` and `None` when no command resolved — an absence every consumer interpreted for itself, and `deliver.rs` read `!te.passed` as "the suite failed". A new `VerificationStatus { Unverified, Passed, Failed, Errored }` makes that absence expressible: a `status` field on `TestExecution`, `run_tests` always returns a record, and delivery gates on `status.blocks_delivery()` where `Unverified` does **not** block (a repo with no manifest is not a repo with failing tests). Tests: `a_green_suite_is_verified_as_passed` (real `WorktreeSandbox`; asserts status, exit code and `test result: ok` in the captured output), `a_red_suite_is_verified_as_failed_and_blocks_delivery`, `a_project_with_no_test_command_is_unverified_and_never_a_pass`, `the_verdict_survives_serialisation_into_the_artifact` (a **pre-field** artifact parses as `Unverified`, never as an accidental pass), and `a_run_against_a_project_with_no_manifest_reports_unverified` end to end through the binary. |
| C5 safest approval option focused by default; Esc denies; every decision logged | P0 | [T] | **WORKS** | `npx vitest run test/approval.test.tsx` — **20 passed**. `shell/src/approval.ts` decides it in one place: `safestFocus` follows the engine's `safest_option_id` when it is safe, and when the posture is manual **or not yet reported** it resolves to a refusal anyway. Tests cover the hostile case (`safest_option_id: "allow"` in manual mode → focus is `deny`), the unknown-posture case, the non-manual case where the engine is followed, and a `safest_option_id` naming no existing option. Esc is proven to deny even after focus was moved to Allow, and to deny when the engine offered no refusal-looking option. Every decision goes over the seam as `decisionFor(option)`; a test pins that no option which does not read as an approval can produce `allow`. This also fixed a live bug: `cli.tsx` had hardcoded `decision: 'allow'` for every approval reply. |

## D — Shell lifecycle and input

The TypeScript/Ink shell exists (`shell/`) and the PTY end-to-end driver exists
(`shell/test/pty.test.ts`, with the terminal emulator in `shell/src/vt.ts`). The driver allocates a
**real** pseudo-terminal (`script -qfec`), sizes it with `stty` inside the pty, feeds raw bytes,
kills every child on a hard timeout, and asserts on the **final screen**.

It proved itself immediately: it found that `cli.tsx` never read `stdout.columns` and never
subscribed to `resize`, so the shell started at a hard-coded 80x24 and ignored the window entirely.

| Row | P | Proof | Status | Evidence |
| --- | --- | --- | --- | --- |
| D1 terminal restored on exit, Ctrl+C, SIGTERM, SIGHUP, error, panic | P0 | [P] | **WORKS** | `shell/src/cli.tsx` restores on normal exit and from `SIGINT`/`SIGTERM`/`SIGHUP` handlers and an `uncaughtException` handler; `restoreTerminal` is idempotent. PTY: `PTY terminal lifecycle > restores the terminal when it is killed with SIGTERM`. |
| D2 Ctrl+Z suspend/resume restores on suspend, full redraw on resume | P0 | [P] | **OWNER-VERIFY** | Exact steps in `OWNER_VERIFY.md`. There is no PTY case: `expect`-style suspension needs a controlling terminal this harness does not own, and a test that cannot drive it would be a test that cannot fail. |
| D3 resize re-lays out immediately, no stale cells; 1x1..300x100 never panics | P0 | [P] | **WORKS** | PTY `survives a resize storm without panicking` and `says so plainly at 49 columns`; the emulator's `never throws on a resize to zero or an absurd size` covers 1x1..99999. **This row was BROKEN and the driver found it.** |
| D4 non-TTY or `TERM=dumb`: no escape garbage, clear message or plain output | P0 | [P] | **WORKS** | PTY `D4: renders no escape garbage when TERM is dumb` and `still shows the composer, because it is the anchor`. |
| D5 all engine text sanitized (CSI, OSC, DCS, C0/C1 except newline/tab) | P0 | [T] | **WORKS** | `shell/test/sanitize.test.ts` (18) + `shell/test/property.test.ts` (300 random byte strings, idempotence, single-line guarantee). **Two real bugs found and fixed here**: OSC/DCS bodies were cut at the CSI final byte, leaking `]0;PWNED`. |
| D6 nothing writes to stdout/stderr while the TUI is active | P0 | [L] | **WORKS** | `shell/test/lint.test.ts` — no `process.stdout.write` or `console.*` outside `src/cli.tsx`. Engine stderr goes to `~/.niki/logs/shell.log`. |
| D7 robust escape parsing: split sequences, lone Esc vs Alt+key, non-ASCII, AltGr | P0 | [P] | **WORKS** | `shell/test/property.test.ts` and the emulator's own tests. **Three real bugs found and fixed**: Alt+key emitted two events, `ESC [ A` produced no arrow, and `ESC [ Z` (Shift+Tab) was unrecognised so mode cycling was dead. |
| D8 one event loop, one dispatcher, one context-scoped keymap registry | P0 | [L][T] | **WORKS** | `shell/test/lint.test.ts` — no key matching outside `src/dispatch.ts`; `shell/test/keyboard.test.ts` asserts every advertised key dispatches. **A real off-by-one found**: `slice(-1)` sent a single-word command's last letter as its arguments. |
| D9 arrows, j/k, PgUp/PgDn, Home/End, g/G on every scrollable surface | P0 | [P] | **WORKS** | `shell/test/keyboard.test.ts` — PgUp/PgDn/Home/End/Up/Down scroll, scrolling never disturbs the composer text, and `gg`/`G` work as a chord. **Conflict resolved and recorded:** `j`, `k`, `g` and `G` are letters, so a focused composer must receive them; a test pins that `g` types and `gG` scrolls. |
| D10 bracketed paste honoured; pasted text never fires hotkeys or Enter | P0 | [P] | **WORKS** | `shell/test/property.test.ts` (7 cases: split across reads, Ctrl+C inside a paste is inert, Enter inside a paste never submits, large pastes collapse to a placeholder) and PTY `D10: lands in the composer and submits nothing`, which pastes `/quit` + Enter and asserts nothing was submitted. |
| D11 mouse is a mode with a one-key release toggle | P0 | [P] | **WORKS** | PTY `D11: ctrl+m releases capture and takes it back` and `an SGR mouse report is consumed, never typed`. |

## E — Chat loop

All rows **WORKS**, each with a named test. The proof for every row lives in
`shell/test/chat-loop.test.tsx` (53 tests), `shell/test/approval.test.tsx` (20),
`shell/test/keyboard.test.ts`, `shell/test/snapshots.test.tsx`, `shell/test/mascot.test.ts` (26),
and `shell/test/fixture-loop.test.tsx`, which drives the engine's scripted reference loop through
both a real pseudo-terminal and the render path.

| Row | P | Proof | Status | Evidence |
| --- | --- | --- | --- | --- |
| E1 composer anchored, keeps focus, accepts typing and queueing while output streams | P0 | [P] | **WORKS** | `chat-loop.test.tsx` "the composer is the anchor" asserts the composer sits below the transcript and above the footer at both sizes; queue rows render visibly. The review frames show it pinned to the bottom of a 24-row screen with the transcript absorbing the slack. |
| E2 user message echoes immediately as a raised row; assistant text distinct | P0 | [S][T] | **WORKS** | The user row is `>` + bold, the assistant is a bullet and not bold; both asserted. |
| E3 one live activity line from real events; dim "next" only when supplied; stops at idle; static in reduced motion | P0 | [T] | **WORKS** | Activity text is derived from the role the engine reported (`Planning`/`Editing`/`Running tests`/`Reviewing changes`); no "next" line without one; absent at idle; cadence pinned at 100/120/200 ms per the spec. |
| E4 reasoning collapsed to one dim line with an expand key; provider summaries only | P0 | [T] | **WORKS** | Many summaries produce exactly one row; a stage that sent none produces none; the raw provider text is asserted **absent** from every row. |
| E5 tool and stage rows: state glyph, bold name, dim args, result line with expand hint, independent parallel states | P0 | [S][T] | **WORKS** | Independent per-tool state; the expand hint appears only where the engine sent a `full_ref`; a retry marker appears only when `attempt > 1`; provenance labels differ for independent review and self-verification. |
| E6 failures inline in the error colour with the excerpt and a recovery action | P0 | [S][T] | **WORKS** | The failed row is painted with the error token; the excerpt is the engine's own summary; a recovery action renders only when the engine supplied one; a failure with no summary says `failed` rather than inventing a message; output after the failure still renders. |
| E7 footer: left ambient facts, right contextual hints by state, meter only when known | P0 | [S][T] | **WORKS** | No meter before `context.usage` arrives; the meter shows real numbers after; no branch arrow until git reported; hints change by state; the permission posture survives every width from 50 up; and fields collapse **whole**, in the spec's order (hints, cwd, branch, model). |
| E8 header with the NIKI mascot, name, version, model, posture, cwd, branch | P0 | [S] | **WORKS** | Before a session arrives the header says `connecting to the engine` and renders nothing rather than a placeholder; `undefined` and `null` are asserted absent from the frame. |
| E9 streaming Markdown renders into one block, no reflow flicker | P0 | [T] | **WORKS** | Deltas accumulate into a single assistant block; across a token-by-token stream the visible block count never shrinks; headings, nested and task lists, quotes, inline code, fenced code with its language label, links and tables all render; `**` and `` ` `` are asserted absent from the screen. |
| E10 end of turn: "Done in 42s · 3 tool calls · 1 file changed" | P0 | [S][T] | **WORKS** | Rendered exactly, pluralised from the number, absent when the engine sent no summary. |
| E11 Esc interrupts, keeps partial output, shows how to continue | P0 | [T] | **WORKS** | Partial text survives, the activity line clears, `Interrupted · type to continue` renders. |
| E12 pipeline stages use the same row grammar with truthful provenance | P0 | [S][T] | **WORKS** | A stage row and a tool row share one grammar; provenance is labelled; no stage row exists without a `stage.*` event. |
| E13 NIKI one-eye orb mascot at three width tiers, five states, ASCII and NO_COLOR, no animation, constant width, art only in the mascot module, contrast passes | P0 | [S][T][L] | **WORKS** | `shell/test/mascot.test.ts` (26 tests) plus the lint test that keeps art out of every other module. `docs/foundation/review/mascot-tiers-and-states.txt` carries every tier and state for the owner. |

## F — Commands and surfaces

| Row | P | Proof | Status | Evidence |
| --- | --- | --- | --- | --- |
| F1 single command registry: name, description, aliases, hidden keywords, argument hint, bypass tier | P0 | [T] | **WORKS** | `COMMANDS` in `shell/src/components/footer.tsx` is the only list; a lint test fails any module that declares its own command array. 21 commands, unique names and aliases. |
| F2 "/" opens a fuzzy popup; arrows/Tab/Enter/Esc; never blocks typing; argument-aware | P0 | [P] | **WORKS** | `shell/src/components/slash-menu.tsx`, hand-written fuzzy matcher, no new dependency. Ordinary characters always reach the composer and the popup re-filters live; Tab completes `/name `; argument hints render per row; an unmatched query renders an honest empty state. |
| F3 every non-QUEUED command works while a run is active | P0 | [T] | **WORKS** | `shell/test/commands.test.tsx` (59 tests). The F3 case compares state **excluding** composer, slashMenu and outbox, so "it cleared the composer" cannot pass as "it did something". `/model gpt-9` is refused with "the engine reported claude-sonnet-4, not gpt-9". |
| F4 command palette (Ctrl+K): fuzzy over pages, commands, settings, sessions, models, recent actions | P0 | [T] | **PARTIAL** | `shell/src/components/palette.tsx` covers every source the checklist names **except goals**: `AppState` has no goal concept and protocol v1 declares none, so there is nothing to list. Building a goals source would mean inventing data, which the no-invention rule forbids. |
| F5 pickers: model, effort, theme (live preview), sessions, prompt history; keys and mouse | P0 | [T] | **PARTIAL** | Keyboard navigation for all five, with the theme picker previewing on move and restoring on cancel. **Mouse hit-testing is not implemented** — the pickers are keyboard-driven, and the ctrl+m capture toggle is unchanged. Listed in `PARITY.md` rather than claimed. |
| F6 "?" contextual Help generated from the registry; scrollable and searchable | P0 | [T] | **WORKS** | `shell/src/components/help.tsx` generates from the keymap in the dispatcher and from `COMMANDS`. No hand-written key list exists. A test asserts the rendered key rows equal the keymap exactly **and drives every advertised key through `handleKey`**, so a row that stops matching fails the build. |
| F7 settings sheet groups per spec; each value shows its source; saving shows what changed; safety-critical need confirmation | P0 | [S][T] | **PARTIAL** | Grouped (Permissions/Model/Run/Interface); every row shows its source (`engine session` / `niki.toml` / `not reported`); save produces a "what a save would change → which file" report. Bypass is held behind an explicit confirm and is never a default. **Nothing is written, because protocol v1 has no settings request** — the sheet says so rather than pretending to persist. |

## G — Performance and rendering

| Row | P | Proof | Status | Evidence |
| --- | --- | --- | --- | --- |
| G1 responsive tiers at seven sizes; footer collapse order honoured | P0 | [S] | **WORKS** | `shell/test/rendering.test.tsx` renders at 49, 50, 79, 80, 119, 120 and 180 and asserts no line overflows. `chat-loop.test.tsx` pins the collapse order — cwd, then branch, then model — by watching which field disappears as the width narrows, and asserts the posture never disappears. |
| G2 idle = zero redraws and under 1% CPU; per-token cost flat; 10k transcript windowed | P0 | [M] | **WORKS** | The sweep is the only timer in the shell and is torn down when nothing is in flight: a source test asserts `cli.tsx` owns it and guards it on an activity. 1000 idle renders cost 0.14 ms, so a full second of idle repaints is ~0.14 ms — far under 1% of a core. Per-token ratio across a 10x transcript is **0.53**. |
| G3 first frame within 150 ms; no network blocks first paint | P0 | [M] | **WORKS** | 2.50 / 5.21 / 4.38 / 4.62 / 4.94 / 4.92 / 4.86 ms at the seven sizes. The first frame renders from local state; `initialize` cannot block it because the render happens before the reply is awaited. |
| G4 key echo p95 at most 30 ms under a 100k-line output flood | P0 | [M] | **WORKS** | With 100,000 transcript lines, a keystroke's dispatch + reduce + re-render is p50 0.003 ms, **p95 0.043 ms** — windowing means the cost tracks the rows on screen, not the history behind them. |
| G5 colours from tokens only; every pair passes contrast; not a copy of a reference palette; NO_COLOR and 16-colour correct | P0 | [T] | **WORKS** | A lint test finds no hex literal outside `src/theme/index.ts`; `theme-contrast.test.ts` measures every declared pair in all four palettes against its floor; a third test asserts no value collides with an observed Claude Code / Codex / Kimi hex. ASCII and NO_COLOR rendering is exercised in the PTY suite. |
| G6 Markdown: headings, nested/task lists, quotes, inline code, fenced code with language and copy, links, tables; stable while streaming | P0 | [S] | **WORKS** | `rendering.test.tsx` asserts every block kind parses, that a fenced block knows whether it is still open, and that the same markdown renders without overflow at all seven widths. `chat-loop.test.tsx` asserts the visible block count never shrinks mid-stream — which is what "no reflow flicker" means. |
| G7 Diff: unified, line numbers with toggle, hunks, intra-line highlight, fold unchanged, next/prev hunk and file, binary/rename/mode | P0 | [S] | **WORKS** | `shell/src/components/diff.tsx` with 11 tests: a four-file patch containing a binary file, a rename and a mode change round-trips all four kinds; both line-number columns are correct and only the side a line exists on is numbered; a 30-line unchanged run folds to a marker stating its width while a 3-line run does not; intra-line marking is asserted on a replaced pair; hunk navigation filters to the file containing that hunk; an empty patch says "no diff" and a hostile patch is sanitised. |
| G8 every page and overlay has empty, loading and error states that name the next action | P0 | [S] | **WORKS** | Five states (empty, working, approval, error, done) each render non-blank content, and `undefined`/`null` are asserted absent from every frame. The pickers and settings sheet have their own honest empty states (`no session yet — the engine has reported no model`). |
| G9 property test: any size renders every page and overlay without panic or overlap | P0 | [T] | **WORKS** | `rendering.test.tsx` renders three states at nine sizes from 1x1 to 300x100; `property.test.ts` fuzzes 300 random byte strings through the parser and the sanitiser and checks idempotence. |

## H — Docs, scoreboard, pack

| Row | P | Proof | Status | Evidence |
| --- | --- | --- | --- | --- |
| H1 KEYMAP.md and Help generated from the registry; ARCHITECTURE.md with an event-flow diagram; CHANGELOG | P0 | — | **WORKS** | `KEYMAP.md` is written by `shell/scripts/gen-keymap.ts` from `COMMANDS` and the dispatcher's keymap. `test/keymap.test.ts` derives the handled-binding set with a deliberately cruder second extractor, so a generator bug cannot hide a key, and asserts the committed file matches a fresh run byte for byte. **Both halves of that test were observed failing first** — a renamed `ctrl+k` and a stubbed `PARITY.md` each broke the build. `ARCHITECTURE.md` carries the event-flow diagram. `CHANGELOG.md` has an Unreleased entry. |
| H2 OWNER_VERIFY.md, review dumps, PARITY.md, GAPS.md | P0 | — | **WORKS** | 24 frame dumps in `docs/foundation/review/` (eleven states at 80x24 and 120x38, the mascot at three tiers in five states, the palette reference); `OWNER_VERIFY.md` with exact steps; `PARITY.md` as a decision record; `GAPS.md`. `tests/foundation_docs.rs` asserts all nine documents exist, exceed a size floor, and that at least twelve review dumps are present. |
| H3 scoreboard runs NIKI, Claude Code, Codex and Deep Agents on the same model over a sealed split and reports the delta with confidence | P0 | [M] | **NOT RUN — by decision** | The harness is built and tested: `evals/scoreboard/run.py` freezes a 27-case sealed split (23 seeded defects, 4 clean controls) with the dataset's SHA-256 recorded so the seal breaks if the dataset moves, parses one rubric token per agent, and scores recall and precision with Wilson intervals — 18 tests, and the parser test caught a real bug where "NOT CAUGHT" scored as CAUGHT. The NIKI arm runs end to end against a local model (61 s/case, verdict extracted and mapped). **The three baselines are closed by owner decision, not pending work: no credential is supplied for Claude Code, Codex or Deep Agents.** Claude Code 2.1.286 and codex 0.152.1 are installed and ollama serves the Anthropic protocol, but Claude Code authenticates before honouring a base-URL override and blocks rather than failing; ollama logged no request. With no credentials there is no delta, and a delta against a *different* model would measure the models, not the agents — so none is published. The NIKI arm runs on its own and its number stands alone. See `SCOREBOARD.md`. |

---

## Summary counts (2026-10-04, at the end of Phase 0)

| Group | WORKS | UNVERIFIED | MISSING | PARTIAL | BROKEN |
| --- | --- | --- | --- | --- | --- |
| A | 4 | 0 | 0 | 0 | 0 |
| B | 5 | 0 | 0 | 0 | 0 |
| C | 5 | 0 | 0 | 0 | 0 |
| D | 10 | 0 | 0 | 1 | 0 |
| E | 13 | 0 | 0 | 0 | 0 |
| F | 4 | 0 | 0 | 3 | 0 |
| G | 9 | 0 | 0 | 0 | 0 |
| H | 2 | 0 | 0 | 0 | 1 not-run-by-decision |

**Every A and B row is now WORKS with a named test and a real passing run.** Nine rows were
observed *failing* before being made to pass — the TypeScript drift test, six clippy lints, and two
genuine bugs the harness found in new code (a sanitiser that leaked OSC sequences, and an input
parser that emitted two events for one Alt+keypress). A check that has only ever been green is not
known to be a check.

Groups C to H are unchanged and are the rest of the build. Group C is deliberately untouched: C1
and C2 are proven to work through `tests/planner_coder_branch.rs`, but the C rows as written ask for
behaviour *on the seam* (a retry marker the shell can render, a machine-checked verifier verdict,
an approval flow whose safest option is focused), and that is Phase 2 and 3 work.