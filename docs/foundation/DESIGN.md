# NIKI Foundation — DESIGN (Phase 0)

**Decisions (owner, 2026-10-04):** (1) Stack = **hybrid**: Rust engine + TypeScript/React/Ink
shell, newline-delimited JSON-RPC 2.0 over stdio. Not revisited mid-build. (2) Terminal mode =
**fullscreen / alt screen**. (3) niki-agent port = **rebuild from the written spec** (that source
does not exist here; see `GAPS.md`). (4) Mascot `working` = **static ◑**, never animated —
`reference/mascot/orb_states_and_tiers.png` shows three animated frames and is **superseded**.

## 0. Reference pack verified (every path in the notes, checked)

- Neither `docs/foundation/reference/**` nor `../niki-agent-ref/` existed when this started; the
  pack arrived as an untracked `niki-tui-reference-pack.zip`, now unpacked into `reference/`.
  **All 15 supplied images were viewed.** `intro.gif` (Kimi) is **not on this machine**; the
  Claude GIF is `/home/shiva/gif/demo.gif`, verified 1552x992 / 414 frames / 42.4 s.
- A Pillow scene-change scan over all 414 frames plus 5 spot extracts: the largest changes land on
  the supplied frames and the final frame is still mid-run, which **confirms** the notes' negative
  list — no approval, diff, slash menu, plan panel or end-of-turn summary exists in the GIF. Those
  come from the written rules, never from the footage. Extracts: `EX01_*, EX02_*`.

## 1. Stack — why hybrid

One audited Rust binary with `niki run` unchanged; Ink's Yoga layout is the only stack that
reaches Claude-Code-grade typography at usable iteration speed; `src/acp/` already speaks
JSON-RPC 2.0 over stdio, so the seam has precedent. All-Rust ratatui was rejected because
`src/display/` is already 31k lines of ratatui that must be replaced anyway — ratatui buys no
leverage and iterates several times slower.

## 2. Terminal mode — why fullscreen

Both reference GIFs are fullscreen. Ink cannot repaint only the bottom region, so an inline
viewport degrades to scroll junk; fullscreen gives a clean addressable canvas, which is what makes
*render is a pure function of AppState + size* actually testable. Accepted costs: no native
scrollback (we keep our own windowed store, row G2) and no native mouse selection (row D11 adds a
one-key mouse-mode toggle). SSH and tmux are verified in `OWNER_VERIFY.md`.

## 3. "Before" frames

`tests/foundation_before.rs` drives `AppState::apply_event` into `RenderEngine<TestBackend>` and
writes six states (idle, streaming, parallel tools, failed tool, permission modal, help) at 80x24
and 120x38 into `docs/foundation/before/`. Entry pattern from `tests/tui_navigation.rs:1413`.

## 4. `RuntimeEvent` and the existing event map

`src/event/` is **dead** — 24 of 26 variants have no producer and nothing calls
`subscribe()`. `StoreEvent` (`src/display/state.rs:2188`) is dead too (tests only).
The only live model is `DisplayEvent` (`src/display/tui.rs:48-238`), so `RuntimeEvent`
is a new, narrower, UI-only enum rather than a rename. The full source-by-source map is
`EVENT_MAP.md`; the rules that matter to the shell are: a row exists only for a real event,
`StageToken` is the only reasoning source (raw private chain-of-thought is never rendered),
and every message carries a `trace_id`.

## 5. First protocol messages, and why

Lifecycle + turn + stage + tool first: those four alone make the shell render a real conversation,
and nothing else is verifiable without them — `initialize`, `shutdown`, `session.load`,
`turn.start`, `turn.delta`, `turn.end`, `stage.start/token/done/failed`, `tool.call/progress/
result`, `approval.request/reply`, `context.usage`, `cost.update`, `notice`, `final`. **Deferred to
S5+:** `plan.update`, `tool.diff`, `diff.ready`, `verdict.ready`, `branch.created`, `session.list`
— a message with no producer is an untestable lie.

## 6. Harness and slice order

S1 `crates/niki-protocol` → S2 `niki serve` + the headless CI path → S3 shell client rendering a
hello from a real event → S4 ink-testing-library snapshots (7 widths × truecolor/256/16/NO_COLOR ×
unicode/ascii × motion on/off) → S5 PTY driver (`expectrl`, hard timeouts, child cleanup) → S6
property/fuzz (`proptest` 1x1..300x100, random bytes into the parser) → S7 lint tests → S8 perf
baselines recorded **before** any optimisation. Row-by-row evidence is in `CHECKLIST.md`; honest
empty states are in `GAPS.md`.

Two engine facts shape the build: `execute_pipeline(… display: &mut AgenticDisplay …)`
(`src/orchestrator/pipeline.rs:2667`) takes a **struct, not a trait**, so extracting `PipelineSink`
is the first engine edit; and `DisplayEvent::PermissionRequest` embeds a `std::sync::mpsc::Sender`
inside a **`Clone`** enum (`src/display/tui.rs:201`), so it structurally cannot cross the seam —
the protocol uses `id` + `approval.reply` with an engine-owned oneshot.

## 7. Open items

- **Workspace conversion:** `Cargo.toml` has no `[workspace]`. New crates need
  `[workspace] members = [".", "crates/*"]`. Proceeding; `cargo build --release`,
  `niki --version --help`, `cargo deny check` and `dist-workspace.toml` are re-verified after.
- **B5 baseline:** whether the current engine has `unwrap`/`expect` in production paths is
  UNVERIFIED until a real `cargo clippy --all-targets` run. New crates get a lint test from their
  first commit regardless.