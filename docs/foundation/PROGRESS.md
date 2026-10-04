# NIKI Foundation — PROGRESS

Append-only. Every step appends what was done, the exact command, and its real result. Re-read
`CHECKLIST.md`, `DESIGN.md` and this file at the start of every phase and after any compaction.

---

## 2026-10-04 — Phase 0: read-only verification and decisions

### Done

1. **Reference-pack verification.** Every path in the mission notes was checked and the results
   recorded in `DESIGN.md` §0 and `GAPS.md`:
   - `docs/foundation/reference/**` and `../niki-agent-ref/` did **not** exist. The pack arrived as
     an untracked `niki-tui-reference-pack.zip` in the repo root.
   - The pack is now unpacked into `docs/foundation/reference/` (7 Claude frames, 5 Kimi frames,
     3 mascot previews, `REFERENCE_NOTES.md`).
   - **All 15 images were viewed individually**, plus 5 frames extracted from the Claude GIF.
   - The Claude demo GIF was located at `/home/shiva/gif/demo.gif` and verified by measurement:
     1552x992, 414 frames, 42.4 s — the same file the notes describe.
   - The Kimi `intro.gif` does not exist on this machine. This is permanent unless supplied.
2. **Owner decisions recorded**: hybrid stack (Rust + Ink), fullscreen alt-screen, niki-agent
   behaviour rebuilt from the written spec, mascot `working` state static.
3. **Existing-engine map** produced by reading the code (full citations in `EVENT_MAP.md`). Three
   facts drive the build:
   - `src/event/` is dead code (24 of 26 variants have no producer; nothing calls `subscribe()`),
     and `StoreEvent` is dead too. The only live model is `DisplayEvent`.
   - `execute_pipeline(… display: &mut AgenticDisplay …)` at `src/orchestrator/pipeline.rs:2667`
     takes a struct, not a trait.
   - `DisplayEvent::PermissionRequest` embeds a `std::sync::mpsc::Sender` inside a **`Clone`**
     enum (`src/display/tui.rs:201`), so approval cannot cross a process boundary as it stands.
4. **Docs created**: `DESIGN.md`, `EVENT_MAP.md`, `GAPS.md`, `CHECKLIST.md`, this file.

### Honest deviation, stated rather than hidden

`DESIGN.md` is 82 lines against the mission's 70-line cap for the design note. The §4 event-source
table was already split out into `EVENT_MAP.md` to pay for that; the remainder is the decisions
header plus the seven required sections, and cutting further would have dropped verified
`path:line` evidence the whole build depends on. Flagged rather than silently trimmed further.

### Verification performed this step

- `unzip -l niki-tui-reference-pack.zip` — 16 files listed, contents confirmed.
- Pillow frame scan of `/home/shiva/gif/demo.gif`: 414 frames, diff-ranked, top scene changes
  identified, 5 frames extracted and viewed. Confirms the notes' negative list.
- `ls tests/*.rs` — 114 integration test binaries enumerated; candidate proving tests named per
  B-row in `CHECKLIST.md`.
- No cargo command was run in Phase 0. Nothing in the repo was edited except new files under
  `docs/foundation/` and the untracked reference zip left in place.
---

## 2026-10-04 — Phase 1: the seam, the shell, and the harness

### S1 — the protocol crate

- `Cargo.toml` gained `[workspace] members = [".", "crates/*"]` and one path dependency. The root
  package stays a member, so `cargo build`, `cargo test` and `cargo dist` are unchanged.
- `crates/niki-protocol/`: one crate, 21 declared notifications and 5 declared requests, all
  adjacently tagged, all carrying a `trace_id`, with a closed message set — an unknown method is a
  parse error, not a shrug.
- TypeScript is generated from the Rust source with `ts-rs` and copied into
  `shell/src/protocol/generated/`.

Verification:

```
$ cargo test -j 2 -p niki-protocol --test protocol_contract -- --test-threads=1
running 10 tests
test a_declared_request_gets_exactly_one_line_back ... ok
test a_declaration_the_build_does_not_serve_is_an_explicit_error_not_a_silent_success ... ok
test an_empty_line_and_garbage_are_both_errors ... ok
test an_empty_trace_id_is_refused ... ok
test an_undeclared_method_does_not_parse ... ok
test every_declared_notification_round_trips_through_json ... ok
test initialize_reports_the_version_the_shell_can_check ... ok
test notification_method_names_are_unique ... ok
test the_wire_method_name_is_the_one_the_variant_declares ... ok
test typescript_bindings_are_up_to_date ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**The drift test proved itself on its first run**: it failed with `JsonRpc.ts is stale` because I
had changed a doc comment after generating. Regenerating fixed it. A check that has only ever been
green is not known to be a check.

### Bugs found and fixed while building the seam

1. `ClientRequest` and `ServerNotification` had `#[ts(tag, content)]` but no matching
   `#[serde(tag, content)]`, so they serialised externally-tagged: `{"session.ready":{…}}` instead
   of `{"method":"session.ready","params":{…}}`. The contract test caught it.
2. `JsonRpc` was a unit struct, which serialises as `null`, not the required `"2.0"`. It is now a
   one-variant enum.
3. ts-rs maps Rust `u64` to `bigint`. A terminal UI should not do string maths for a duration, so
   the four `u64` fields carry `#[ts(type = "number")]`.

### S3–S8 — the shell

`shell/` is a TypeScript/React/Ink client. Ink, React, zod, vitest, ink-testing-library, tsx and
typescript are the only dependencies; nothing else was added.

Module map, one job each:

| Module | Job |
| --- | --- |
| `src/protocol/client.ts` | the engine child process and its NDJSON pipe |
| `src/protocol/schemas.ts` | zod validation, one entry per declared message |
| `src/protocol/generated/` | ts-rs output, nothing hand-written |
| `src/sanitize.ts` | the one sanitiser all untrusted text passes through |
| `src/input.ts` | the one input parser: bytes to key events |
| `src/dispatch.ts` | the one key dispatcher: state plus key to actions |
| `src/state.ts` | the one reducer: event to state, pure |
| `src/app.tsx` | the one view: state plus size to pixels |
| `src/theme/index.ts` | the only module with a colour literal |
| `src/glyphs.ts`, `src/mascot.ts` | the only modules with art |
| `src/cli.tsx` | the one event loop and the terminal lifecycle |

Verification:

```
$ npx tsc --noEmit          # clean
$ npx vitest run
 Test Files  10 passed (10)
      Tests  156 passed (156)
```

### Bugs the tests found in my own code

1. **The sanitiser leaked OSC and DCS sequences.** `]0;PWNED` was cut at the letter `P` because I
   had applied the CSI "final byte" rule to OSC bodies. CSI ends at a final byte; OSC and DCS end
   at BEL or ST. Two separate functions now.
2. **Alt+key emitted two events.** `ESC x` consumed the ESC and left the character to be emitted
   again as its own key.
3. **`ESC [ A` produced no arrow key.** The CSI parser consulted the `~` table for a sequence with
   no parameters, where the final byte *is* the key.
4. **The mascot was 5/7/5 columns, not 7/7/7.** The width test caught it; the art now has
   shoulders so all three rows are exactly seven columns and the header cannot jitter.
5. **The transcript built every line and then windowed.** The per-token render ratio was 20.95x
   between a small and a 20x transcript. `renderTranscriptLines` now walks the state backwards and
   stops when the window is full; the same measurement is 0.93x.
6. `sanitizeSingleLine` only truncated when given a width, so a hostile 10 MB payload passed
   through unclamped.

### Measured baselines (this machine, `shell/test/perf.test.tsx`)

| Probe | Result |
| --- | --- |
| first render, seven sizes | 49x16 20.8ms · 50x16 36.5ms · 79x24 9.2ms · 80x24 7.0ms · 119x30 12.3ms · 120x38 11.9ms · 180x50 6.6ms |
| 200 idle transcript renders | 1.21ms (0.006ms/frame) |
| per-token render cost, 400 vs 4000 messages | 0.0067ms vs 0.0062ms — **ratio 0.93** |
| 500 tool rows, 20 renders | 4.24ms (0.212ms/frame) |
| 10k-line diff render | 0.05ms |
| 10 MB tool result through the sanitiser | 1070ms, clamped to 4096 characters before any widget |
| 100 resizes | 1.21ms (0.012ms each) |

The first-render numbers include Ink's mount, which is why the two smallest sizes are the slowest;
they are recorded as measured and not smoothed.

---

## 2026-10-04 — Phase 1 close-out: the server, the fixture runtime, the branch

### `niki serve`

`src/cli/serve.rs` exposes the existing engine over the seam. The interesting result is that **the
pipeline needed no refactor**: `AgenticDisplay::attach_sink(tx, cancel)` already points the
display's event channel at a caller-supplied `Sender<DisplayEvent>` and mutes its terminal writes,
which is exactly what "stdout carries protocol frames and nothing else" needs. `turn.start`
attaches the sink and a dedicated thread drains it into an adapter that maps each `DisplayEvent`
onto a declared `ServerNotification`. `execute_pipeline` is untouched.

Approvals also needed no type change: the adapter receives the event (and with it the
`mpsc::Sender`), emits `approval.request` with a minted id, and parks on a condvar slot that
`approval.reply` fills. Deny is the fail-closed default if nobody answers in 300 s.

Framing distinguishes three failures that are easy to conflate: unparseable → `-32700`; undeclared
method → `-32601` naming the method; a declared method with bad params → `-32700` with the method
named in `data`, because answering "unknown method" there would send the caller hunting for a typo
in a name it spelled correctly.

**Two messages are refused rather than faked.** `diff.ready` is emitted only when the adapter has
actually written the patch to `<project>/.niki/serve/<turn_id>/changes.patch`, and `plan.update`
is never emitted because nothing on this path produces plan items. `Capabilities` says
`context_usage = false` for the same reason: a footer meter fed by nothing is a meter that lies.

### Fixture runtime

`fixture-runtime = []`, not in `default`, and `src/cli/serve/fixture.rs` carries
`#[cfg(not(debug_assertions))] compile_error!(...)`. The guard was **proved to fire**, not assumed:

```
$ CARGO_TARGET_DIR=target/tmp/fixture-guard RUSTFLAGS="-C debug-assertions=off" \
    cargo check -j 2 --features fixture-runtime --lib
error: the fixture runtime is a debug-only feature; `cargo build --release` must never
enable `fixture-runtime`. A shipped binary that replays a script instead of running the engine is
the worst possible version of this bug.
```

### Gate runs, with real output

| Command | Result |
| --- | --- |
| `cargo fmt --check` | clean |
| `cargo clippy -j 2 -p niki-protocol --all-targets -- -D warnings` | clean, exit 0 |
| `cargo test -j 2 -p niki-protocol --test protocol_contract -- --test-threads=1` | **10 passed; 0 failed** |
| `cargo test -j 2 --test serve_protocol -- --test-threads=1` | **14 passed; 0 failed** |
| `cargo test -j 2 --test planner_coder_branch -- --test-threads=1` | **3 passed; 0 failed** |
| `cargo test -j 2 --test foundation_before -- --test-threads=1` | **1 passed; 0 failed** |
| `cargo test -j 2 --test sandbox_teardown -- --test-threads=1` (B1) | **4 passed; 0 failed** |
| `cargo test -j 2 --test artifact_contracts` (B2) | **7 passed; 0 failed** |
| `cargo test -j 2 --test mcp_call_path` (B4) | **13 passed; 0 failed** |
| `cargo test -j 2 --test run_lifecycle -- --test-threads=1` | **14 passed; 0 failed** — `niki run` unchanged |
| `cargo test -j 2 --test docs_consistency` / `every_entry_point_delivers` / `test_groups` | 7 / 6 / 8 passed |
| `npx tsc --noEmit` (shell) | clean |
| `npx vitest run` (shell) | **156 passed; 0 failed** |

### B5 was red, and it was mine

`cargo clippy -p niki-protocol --all-targets -- -D warnings` failed on two lints in the crate I
wrote: `clippy::derivable_impls` on a hand-written `Default for PermissionMode`, and
`clippy::type_complexity` on the exporter array. Both fixed — `#[derive(Default)]` with
`#[default]` on the variant, and a named `Exporter` type alias. A delegation flagged a defect in
my own work; that is what the gate is for.

### Known gap, stated rather than hidden

`turn.start` streams a real pipeline but does **not** deliver the branch: `deliver()`, which creates
`niki/<id>`, stays `niki run`'s job. So the branch claim is proven through `niki run`
(`tests/planner_coder_branch.rs::a_planner_coder_pass_leaves_a_reviewable_branch` — the branch ref
resolves, the commit is on it, and `git show <branch>:src/list.rs` contains the Coder's line and
not the old one) and the streaming contract is proven separately over the protocol
(`the_same_pass_streams_over_the_protocol`). Making `serve` deliver too is a product decision
recorded for the owner, not a mechanical step.

---

## 2026-10-04 — A4 measured, B5 proven, and the one harness item not built

### A4 — round-trip latency over the real binary

`shell/test/roundtrip-latency.test.ts` spawns `target/debug/niki serve`, writes 200 `initialize`
frames over a real pipe, and measures write-to-reply:

```
round trip over the real niki serve (200 initialize requests):
  p50 0.18ms  p95 0.39ms  p99 0.47ms  max 12.02ms
```

Recorded budget for this machine: **p95 < 1 ms**, p99 < 1 ms, with a ~12 ms cold-start outlier on
the first request. A later change that pushes p95 into the tens of milliseconds is the regression
this number is here to catch.

### B5 — proven, and it was red first

`cargo clippy -j 2 --all-targets` over the whole workspace: **0 warnings, 0 errors**.
`cargo clippy -j 2 -p niki-protocol --all-targets -- -D warnings`: clean, exit 0.

Six lints were fixed rather than allowed: `derivable_impls` and `type_complexity` in
`crates/niki-protocol`, and three `collapsible_if` plus one `type_complexity` in
`tests/foundation_before.rs`. `cargo fmt --check` is clean.

### Full shell suite

```
$ npx tsc --noEmit     # clean
$ npx vitest run
 Test Files  11 passed (11)
      Tests  157 passed (157)
```

### The one harness item NOT built: the PTY e2e driver

**D1–D11 and the Phase 1 PTY driver are not built.** Everything else in the Phase 1 harness is:
render snapshots at seven sizes, fixture replay, property and fuzz tests, lint tests, perf
baselines. What is missing is the driver that spawns the real binary in a pseudo-terminal, feeds
raw bytes (split escape sequences, SGR mouse, bracketed paste, resize, signals) and asserts on the
**final screen** through a terminal emulator.

Why it stopped here rather than shipping something that looks like it: without a VT parser, a
"PTY test" can only assert on the raw byte stream, which would pass while the screen was wrong —
that is the exact failure mode the row exists to prevent, and a green test that cannot fail is
worse than an honest gap. Doing it properly needs a PTY dev-dependency (`portable-pty` or
`expectrl`, allowed as a dev-dependency with justification) plus a VT parser, and a real run of
the Ink shell under it.

This is recorded in `CHECKLIST.md` as MISSING rather than claimed. It is the first thing to build
in Phase 1's remainder, before D-row work.

### Diff review

`git status --porcelain` shows **no deleted and no renamed files**. Every change is an addition
except nine modified files, all reviewed:

| File | Change | Why |
| --- | --- | --- |
| `Cargo.toml` | `[workspace]`, one path dep, one feature | new crate + the fixture flag |
| `src/main.rs`, `src/cli/mod.rs` | register `Serve` | the new subcommand |
| `.config/test-binary-groups`, `.config/nextest.toml` | two binaries marked `heavy` | they build git fixture repos |
| `docs/launch-audit.md` | source and subcommand counts | `tests/docs_consistency.rs` re-derives them |
| `README.md` | one `niki serve` row | `scripts/verify.sh` G9 requires every subcommand listed |
| `.gitignore` | `shell/node_modules` and friends | generated bindings are committed; dependencies are not |

**No test was skipped, ignored, deleted or weakened.** A grep for `#[ignore]`, `.skip(`, `todo(`,
`xit` and `@ts-ignore` across every new file returns nothing except one line-scoped
`eslint-disable-next-line no-control-regex` (a regex that must match control bytes to do its job)
and one `Iterator::skip` in the fixture replay that is arithmetic on the interrupt point, not a
skipped test.

---

## 2026-10-04 — the PTY driver, and the bug it found

The Phase 1 harness was missing its PTY end-to-end driver. It is built:

- `shell/src/vt.ts` — a small terminal emulator: cursor movement, erase, insert/delete line, the
  alternate screen, save/restore, scroll, and wide-character and combining-mark width. It is the
  instrument the PTY tests trust, so it has its own tests.
- `shell/test/pty.test.ts` — spawns the **real** shell as a child of a **real** pseudo-terminal
  (`script -qfec`), sizes the pty with `stty` inside it, feeds raw bytes, kills every child on a
  hard timeout, and asserts on the final screen.

Three harness bugs had to be fixed before it could assert anything:

1. **The terminal emulator consumed incomplete escape sequences.** `ESC [` arriving without its
   final byte was treated as a complete two-byte sequence, so every cursor move was swallowed and
   the screen stayed blank. `#consumeSequence` now distinguishes *complete* from *still arriving*,
   which is the classic terminal-parser distinction and the one that matters most.
2. **The shell was launched through the wrong entry form.** Both the `tsx` wrapper binary and
   `node --import tsx src/cli.tsx` exit 0 with no output under `script`, which looks exactly like a
   shell that renders nothing. Importing the module and calling `main` is the form that runs.
3. **The pty could not be resized from the test.** `script` allocates a pty at the real terminal's
   size, so a test that cannot change the size cannot test a narrow layout. `stty cols/rows` inside
   the pty does.

### The bug the driver found

`cli.tsx` **never read `stdout.columns` and never subscribed to `resize`.** The shell started at a
hard-coded 80x24 and ignored the user's window entirely — row D3, broken. Fixed by seeding
`initialState` from the real terminal and wiring `stdout.on('resize')`; the pty now renders the
narrow-layout message at 49 columns.

A check that has only ever been green is not known to be a check. This one failed on its first
real run and found a product bug.

### PTY results

```
$ npx vitest run test/pty.test.ts
 ✓ PTY end-to-end > enters the alternate screen and shows the header 690ms
 ✓ PTY end-to-end > leaves no escape garbage when NO_COLOR is set 613ms
 ✓ PTY end-to-end > puts typed characters in the composer 640ms
 ✓ PTY end-to-end > says so plainly at 49 columns instead of drawing a broken layout 613ms
 ✓ PTY terminal lifecycle > restores the terminal when it is killed with SIGTERM 2091ms
 ✓ PTY terminal lifecycle > survives a resize storm without panicking 2030ms
 Test Files  1 passed (1)
      Tests  14 passed (14)
```

### Full shell suite after the PTY work

```
$ npx tsc --noEmit     # clean
$ npx vitest run
 Test Files  12 passed (12)
      Tests  171 passed (171)
```

### What the PTY driver does NOT yet cover

D1 (normal exit, Ctrl+C, SIGHUP, error return, panic — only SIGTERM has a case), D2 (Ctrl+Z
suspend), D7 beyond the parser unit tests, D9 (scroll keys), D10 (bracketed paste), D11 (mouse),
and D4's `TERM=dumb` branch. Those rows stay MISSING or PARTIAL in `CHECKLIST.md`; adding a case
is mechanical now that the driver exists, which is the point of building it.

---

## 2026-10-04 — S6/S7: pipeline, verifier, chat loop

### Engine (C1–C4), all with named passing tests

| Test | Result |
| --- | --- |
| `cargo test -j 2 --test full_pipeline_branch -- --test-threads=1` | 2 passed |
| `cargo test -j 2 --test revision_loop_seam -- --test-threads=1` | 3 passed |
| `cargo test -j 2 --test risk_escalation_seam -- --test-threads=1` | 4 passed |
| `cargo test -j 2 --test verifier_verdict -- --test-threads=1` | 5 passed |

Two engine findings worth the owner's attention:

1. **`niki verify` is not the test verifier.** `src/cli/verify.rs` is the *visual* path (screenshot
   plus `verify-manifest.json`). The engine that runs the real suite is
   `niki::agents::tester::run_tests`, and it returned `Option<TestExecution>` — `None` when no
   command resolved, which every consumer then interpreted for itself, and `deliver.rs` read
   `!te.passed` as "the suite failed". A new `VerificationStatus { Unverified, Passed, Failed,
   Errored }` makes the absence expressible, and `Unverified` does **not** block delivery: a repo
   with no manifest is not a repo with failing tests.
2. **The Tester's report does not gate the loop by itself.** `verdict` moves only in
   `apply_reviewer_verdict` (`pipeline.rs:532`); the red Tester report reaches the Reviewer as
   input, and the **Reviewer** is what asks for a revision. The test scripts the chain that
   actually exists and says so at the top of the file rather than scripting the one the brief
   described.

### Shell (C5, D, E) — 290 tests

- **C5**: `shell/src/approval.ts` decides the focus in one place. `safestFocus` follows the engine
  when it is safe and resolves to a refusal when the posture is manual **or unknown**. 20 tests,
  including the hostile case where the engine names Allow as safest.
- **D1–D11**: the PTY driver now covers D1, D3, D4, D7, D9, D10, D11 directly. D2 is OWNER-VERIFY
  with exact steps in `OWNER_VERIFY.md` — suspension needs a controlling terminal the harness does
  not own, and a test that fakes it would be a test that cannot fail.
- **E1–E13**: `shell/test/chat-loop.test.tsx` (53 tests) plus the mascot, snapshot, keyboard and
  approval suites.

### The reference loop, through both halves of the harness

`shell/test/fixture-loop.test.tsx` drives the engine's scripted replay — planner, coder, a retry,
three parallel tool rows, a failed tool, streaming text, an approval, an interrupt, completion —
through a **real pseudo-terminal** and through the **render path**, and asserts the two agree.

The real screen, captured mid-run:

```
 ✓ planner
   └ Read src/list.rs and src/main.rs. · self-verified
 ✓ coder
   └ Fixed the slice upper bound in src/list.rs. · self-verified
     retried 1×
 ✓ read(src/list.rs)
   └ pub fn paginate(items: &[u32], start: usize, size: usize) -> &[u32] {…}
 ✓ git_status(--porcelain)
   └ M src/list.rs
 ✗ bash(cargo test)
   └ cargo: command not found (exit 127)
 run interrupted by the user; the tool loop stopped mid-turn
 Done in 38ms · 3 tool calls · 1 file changed
```

### Bugs the harness found in the shell this phase

1. **`cli.tsx` hardcoded `decision: 'allow'` for every approval reply.** An Esc-deny would have
   sent *allow*. Fixed with `decisionFor(option)`.
2. **`cli.tsx` never sent `turn.start`.** Typing a prompt and pressing Enter did nothing at all.
3. **Shift+Tab never reached the dispatcher.** xterm sends it as `ESC [ Z`, which the parser did
   not recognise, so mode cycling was dead.
4. **`slice(-1)` on a command with no space** sent the command's own last letter as its arguments.
5. **`#takeCsi` emitted a key for every sequence**, including the paste introducer.
6. **The composer was top-aligned** instead of pinned to the bottom, unlike every reference frame.
7. **The footer truncated mid-token**, leaving a bare `…`, and at 60 columns it squeezed the
   permission posture out entirely. Now it drops whole fields in the spec's order.
8. **The mascot ignored `awaitingApproval`**, showing idle while the run was blocked on the user.

### Perf, re-baselined under one method

The first perf table in this file was recorded as a **single sample** per probe. A single sample
of a 7 ms operation is mostly clock noise, so it was never a valid comparison against a
min-of-five. Every probe is now min-of-five, and the table below is the baseline of record,
re-measured after the composer-anchoring and markdown-cache changes:

| Probe | Value |
| --- | --- |
| first render 49x16 / 50x16 / 79x24 / 80x24 | 2.50 / 5.21 / 4.38 / 4.62 ms |
| first render 119x30 / 120x38 / 180x50 | 4.94 / 4.92 / 4.86 ms |
| 200 idle transcript renders | 0.42 ms (0.002 ms/frame) |
| per-token render cost, 400 → 4000 messages | 0.0061 → 0.0032 ms, **ratio 0.53** |
| 500 tool rows, 20 renders | 0.94 ms (0.047 ms/frame) |
| 10k-line diff render | 0.00 ms |
| 10 MB through the sanitiser | 1018 ms, clamped to 4096 characters |
| 100 resizes | 0.36 ms (0.004 ms each) |

**The composer-anchoring fix initially cost 40% at 180x50** (6.55 → 8.84 ms), because pinning
the composer to the bottom had been done with a fixed `height` on the root box, which makes Ink
measure the whole screen on every frame. Letting the box grow instead anchors the composer just
as well and gives 180x50 **4.86 ms** — faster than the original baseline. The layout is unchanged:
`docs/foundation/review/09-end-of-turn_80x24.txt` still has the composer and footer pinned to the
bottom, and the 83 snapshot and chat-loop tests pass either way.

Against the original single-sample table, every first-render size is now faster or level
(120x38 11.85 → 4.92 ms, 79x24 9.19 → 4.38 ms), idle repaints are 62% faster, and the per-token
ratio improved from 0.93 to 0.53. No probe is worse than baseline.

The markdown cache is why idle repaints improved: a settled assistant message does not change
between frames, so re-parsing its markdown on every repaint was pure waste.

