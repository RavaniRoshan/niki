# NIKI Foundation — ARCHITECTURE

## The one decision everything else follows from

The engine is a **server** and the shell is a **pure client**. They meet at exactly one place:
newline-delimited JSON-RPC 2.0 over stdio, defined by one crate, `niki-protocol`. There is no
second event loop, no second state store, and no parallel set of key handlers in either language.

```
                    ┌──────────────────────── engine (Rust library) ────────────────────────┐
                    │                                                                       │
  prompt ────────▶ │  cli/run ─▶ orchestrator ─▶ agents ─▶ llm ─▶ tools ─▶ sandbox         │
                    │                  │            │              │         │                │
                    │                  │            │              │         └─ worktree      │
                    │                  │            │              └─ docker/podman         │
                    │                  │            └─ streaming tokens, usage, cost       │
                    │                  └─ stage graph, revision loop, budget                │
                    │                                                                       │
                    │            PipelineSink  (the only surface the UI may observe)        │
                    └───────────────────────────────┬───────────────────────────────────────┘
                                                            │  typed events, one trace_id
                                                    ┌───────▼────────┐
                                              stdio │ niki-protocol │  NDJSON JSON-RPC 2.0
                                                    └───────┬────────┘
                                                            │
                    ┌──────────────────────── shell (TypeScript + Ink) ─────────────────────┐
                    │  cli.tsx ─ the only event loop, the only owner of the terminal       │
                    │     │                                                                │
                    │     ├─▶ input.ts    bytes        ─▶ KeyEvent                         │
                    │     ├─▶ dispatch.ts state + key  ─▶ LocalAction                       │
                    │     ├─▶ state.ts    event        ─▶ AppState        (pure)            │
                    │     └─▶ app.tsx     AppState+size ─▶ pixels         (pure)            │
                    │              └─▶ theme/ · glyphs.ts · mascot.ts   (the only literals) │
                    └───────────────────────────────────────────────────────────────────────┘
```

## Where each mandate lives

| Mandate | The one place it is enforced | The test that proves it |
| --- | --- | --- |
| One protocol crate | `crates/niki-protocol/` | `crates/niki-protocol/tests/protocol_contract.rs` |
| One state machine | `shell/src/state.ts` (`reduce`, `reduceLocal`) | `shell/test/state.test.ts` |
| One key dispatcher | `shell/src/dispatch.ts` | `shell/test/lint.test.ts` (no key matching elsewhere) |
| One input parser | `shell/src/input.ts` | `shell/test/property.test.ts` (fuzz + split sequences) |
| One theme token system | `shell/src/theme/index.ts` | `shell/test/lint.test.ts` + `shell/test/theme-contrast.test.ts` |
| One art module | `shell/src/glyphs.ts` and `shell/src/mascot.ts` | `shell/test/lint.test.ts` |
| One sanitiser | `shell/src/sanitize.ts` | `shell/test/sanitize.test.ts`, `shell/test/property.test.ts` |
| One overlay/focus model | `AppState.overlay`, `AppState.approval` | `shell/test/state.test.ts` |
| Render performs no I/O | `shell/src/app.tsx` takes a state and a size | `shell/test/snapshots.test.tsx` renders with no client and no engine |

## Event flow, end to end

A user turn, from keystroke to pixels:

1. `cli.tsx` receives bytes and hands them to `InputParser.push`. Partial escape sequences are held,
   not guessed.
2. `handleKey(state, key)` returns actions. It matches no key itself; it only decides.
3. `reduceLocal` applies them to `AppState`. `turn.submit` on an active run appends to `state.queue`,
   which the composer renders as dim queued rows above itself.
4. `cli.tsx` writes a `turn.start` frame to the engine's stdin and keeps rendering. Nothing blocks.
5. The engine runs the pipeline and emits `stage.start`, `stage.token`, `tool.call`, `tool.result`,
   `stage.done`, `turn.end` and `final`.
6. The client validates each frame against a zod schema keyed by method name. A frame that matches
   nothing declared becomes a `protocolError`, never a rendered row.
7. `reduce(state, event, { nowMs })` folds it into `AppState`. All engine text passes `sanitize`
   on the way in.
8. `cli.tsx` re-renders `App`, which is a pure function of `AppState` and the terminal size.

## What crosses the seam and what does not

| Crosses | Never crosses |
| --- | --- |
| stage, tool, approval, notice, plan, diff, verdict, branch, context, cost events | colour, glyphs, fonts, layout |
| a `trace_id` per frame | free-form strings — every payload is a declared struct |
| a typed error code | engine internals, git objects, file paths outside the project |
| the protocol version, so a shell can refuse an engine it does not understand | anything the engine did not actually measure |

The shell owns every visual decision. The engine owns every fact. Neither guesses the other's job.

## Deliberately not extracted yet

`execute_pipeline(… display: &mut AgenticDisplay …)` (`src/orchestrator/pipeline.rs:2667`) still
takes a concrete struct rather than a trait. Extracting `PipelineSink` is the right next engine
step and is recorded as such in `CHECKLIST.md`; it is a 5,836-line file's worth of coupling and is
not something to land inside a UI rebuild. `niki serve` bridges it with an adapter instead of
re-plumbing the pipeline.