# NIKI Finish — Decisions Log

One entry per decision with rationale and rejected alternatives. Phase 0
decisions were confirmed by the owner on 2026-10-10 (see docs/FINISH_PACK.md).

## D-01 (Phase 0) M9 small-model threshold and test model
- Decision: profile ships only at >=50% pass rate (10/20) on the fixed
  20-task suite; test model `qwen2.5-coder:7b-instruct` Q4_K_M via local
  Ollama; record model, quantization, hardware, date in docs/finish/EVALS.md.
- Rejected: 40% threshold (too weak a claim for "small-model profile ships");
  llama3.1:8b (general model, not the coder population NIKI targets).
- Owner: confirmed.

## D-02 (Phase 0) self_extend default
- Decision: `self_extend = "off"` by default (T5.10); "propose" is opt-in.
- Rejected: default "propose" (generated-code proposals active by default is
  the exact attack surface the protocol gates exist to control).
- Owner: confirmed.

## D-03 (Phase 0) Approval default
- Decision: default approval mode = on-request; prompt focuses the safest
  option; Esc denies; `never` requires an explicit one-time confirmation and
  can never be the default.
- Rejected: untrusted (prompts on every non-read-only action — too noisy for
  a personal harness, still available as a mode).
- Owner: confirmed.

## D-04 (Phase 0) Verifier default max loops
- Decision: verifier fix-loops are UNBOUNDED by default, backstopped by the
  L2 budgets (steps/wall-time/cost); optional `verifier.max_loops` config cap.
- Evidence: Claude Code docs (code.claude.com/docs/en/agent-loop) list
  `max_turns` default "No limit"; Codex PR #214 makes max-turns opt-in
  ("default unbounded") and openai/codex declined a `--max-turns` CLI flag.
- Rejected: hard cap 4 (mission recommendation) — would invent a number no
  reference tool uses; hard cap 30 (a Claude Code SDK *example*, not a default).
- Owner: "whatever Claude Code or Codex use here" -> confirmed unbounded after
  the citation above was shown.

## D-05 (Phase 0) P4 platform claims
- Decision: v1 scope = Linux (x64/arm64) + macOS (arm64/x64) + WSL.
  Native Windows is labelled UNSUPPORTED in `niki doctor` and the README;
  no Windows CI job for v1; WSL documented as the Windows path. Release
  cross-compilation of windows binaries may remain but carries no claim.
- Rejected: windows-latest CI job (owner scoped v1 to Linux-based systems);
  full native Windows support (untested PTY/terminal behavior).
- Owner: confirmed ("for this v1 building phase ... only build for linux
  based system", then "drop Windows only" — macOS stays).

## D-06 (Phase 0) License, owner, release channels
- Decision: MIT License, (c) 2026 Ravani Roshan; repo
  github.com/RavaniRoshan/niki; I1 channels = GitHub Releases (goreleaser
  `checksums.txt`), `install.sh`, new `install.ps1`. Installer acceptance
  tests run in clean containers against a local `file://` release fixture;
  the owner runs the one real download (OWNER-VERIFY).
- Rejected: keeping the checksum-less installer (fails I1); sudo-only
  install (fails I1); dropping install.ps1 (fails I1 Windows row).
- Owner: confirmed.

## D-07 (Phase 0) Provider tool loop must be completed
- Decision: the finish work must add the missing request-side `tools` field
  and SSE tool-call delta parsing for openai/anthropic/responses providers.
  This is NOT an architecture change: docs/PACK.md already specifies
  "OpenAI-compatible Chat Completions streaming ... a tool loop" and the
  engine already dispatches `DeltaToolCall`; the request path and stream
  parser were never finished. Without it, no real model or local model can
  ever call a tool, so I2/I5/M1/M3 and every live row are untestable.
- Rejected: leaving it mock-only (makes mission goal (2) — works with small
  local models — impossible); inventing a new provider interface (forbidden:
  do not change the architecture).
- Owner: implicit (goals require live-provider acceptance).

## D-08 (I1) Installer contract
- Decision: install.sh defaults to user-scope `$HOME/.local/bin` with
  explicit `--system` for `/usr/local/bin`; it never invokes sudo (a
  non-writable system dir prints the exact manual `sudo $0 --system` fix).
  Checksums are verified against the release `checksums.txt` before
  unpacking or executing anything; a missing manifest or unlisted tarball
  is a hard refusal. `NIKI_INSTALL_BASE_URL` / `NIKI_INSTALL_DIR` env
  overrides exist solely so clean-environment tests run against a local
  file:// goreleaser-shaped fixture (no network, no docker).
- Rejected: sudo auto-elevation (mission: installs without sudo);
  unpinned download+run (mission: verifies a checksum before running
  anything); skipping install.ps1 (D-06 I1 requires a Windows path —
  now a verified manual install; native Windows stays UNSUPPORTED per D-05).
- Owner: per D-05/D-06 (no new decision needed; recorded for the row).

## D-09 — L2 budget defaults, scope, and semantics (owner-review at P1 boundary)
- Scope: budgets are per RUN (one turn + its steps, including the
  future verifier loop), reset at each turn start — matches D-04's
  "verifier loops backstopped by L2 budgets" and avoids a session-long
  budget killing later turns.
- Defaults (on by default so the D-04 backstop is real):
  `[budgets] wall_time = "10m"`, `cost_usd = 2.0`, `steps = 100`.
  0/"" disables that dimension; all-zero disables budgets entirely
  (no gauge events). Invalid wall duration disables wall budget + logs.
- Semantics: gauges = used/limit fraction per dimension; observed only
  at safe points (step boundaries) so the current step always finishes
  (WRAP = "finish the step"). >=85% any dimension → WRAP; >=100% (or a
  single step crossing both) → stopped; both sticky, stopped beats
  wrap. On WRAP/stopped the engine runs the budget check (SetBudgetCheck;
  default verdict "no-verifier" until V wires the real verifier), emits
  EventBudgetWrap/EventBudgetStopped with gauges + verdict, and ends the
  run keeping all completed work in the session (branch retention joins
  at G1). Cost gauged via routing.CalculateCost (gpt-4o-mini default
  pricing for unknown models; mock/local = $0 via table).

## D-10 — L3 verifier gate semantics (P0, interim until V)
- A model claim (assistant text with no tool calls) is RECORDED but is
  never a completion by itself: the engine calls the verifier via
  `SetVerifier(v, maxLoops)`.
- verified ⇒ turn_completed (the only path to "done" from a claim).
- not-verified ⇒ the verdict is fed back as a user-role message and the
  loop CONTINUES (unbounded per D-04; optional `verifier.max_loops`
  cap, 0 = unbounded; backstopped by L2 budgets and maxSteps).
- nil verifier or verifier error ⇒ immediate EventTurnNotVerified —
  an erroring/inconclusive verifier is never a pass.
- Loop exhaustion (maxSteps, doom-loop) ends NOT-VERIFIED, never
  turn_completed.
- Interim product state (accepted): no verifier is wired into main/
  appserver until section V, so real runs that claim done end
  not-verified (exit 1 / failed update). V replaces the seam with the
  real allowlisted-check verifier (T0–T3 trust tiers); V also adds the
  `[verifier] max_loops` config consumer (field deferred — no consumer
  exists before V, and dead config would not be honest).
- Rejected: auto-pass default verifier (would make L3 vacuous);
  treating verifier failure as completed (violates only-verified);
  hard-coding a fake check selector now (rework when V lands).

## D-11 — L4 steering uses a dedicated EventSteer (P0)
- The safe-point drain previously emitted EventTurnStarted with a
  "steered: " prefix — a misleading reuse (a second turn_started for
  an existing turn). Replaced with a dedicated protocol event
  `steer` (EventSteer) carrying the steer text.
- Queue-time feedback (CmdSubmitPrompt while a turn is running) stays
  EventWarning "mid-turn prompt queued for steering" — it is a
  notification, not the steer itself.
- EventSteer is non-terminal (not in the TUI turnEnded set; exec needs
  no handling — exec never queues steers mid-run).

## D-12 — L5 Esc two-stage stop semantics (P0)
- New command `CmdPauseTurn` (pause_turn) carries the Esc intent;
  `CmdInterruptTurn`/`CmdCancelTurn` keep their immediate-cancel
  meaning (Ctrl+C, ACP/Codex protocol cancel, and the existing L1
  cancellation fixtures all rely on it — no semantics change there).
- Esc #1 (CmdPauseTurn): engine sets pauseRequested (reset at each
  turn start) and emits warning "pausing at safe point — press Esc
  again to stop now". The run ends at the next safe point: loop top
  (between steps) or between tool calls inside a step; skipped tool
  calls receive explicit tool messages so the assistant's tool_calls
  stay matched in context. Cancellation event text:
  "paused at a safe point (Esc)".
- Esc #2 (CmdPauseTurn while a pause is pending): cancel() now and
  emit warning "stopping now (Esc)". A hard stop is never labeled a
  pause: a ctx.Err() check at the loop top runs BEFORE the pause
  check, so a cancelled run emits the plain (textless) cancelled
  event.
- Writes are atomic regardless of interrupt stage: write_file keeps
  tmp+rename; edit_file moved from os.WriteFile (O_TRUNC tear window)
  to tmp+rename; format.FormatFile runs the external formatter under
  context.WithoutCancel (5s timeout) so turn cancellation can never
  kill a formatter mid-rewrite. A write in progress therefore
  completes (or never started) — never torn.
- Rejected: changing CmdInterruptTurn itself (would silently change
  L1/ACP behavior); pause/resume state machine (spec says the run
  ends at the safe point with edits kept — no resume in scope);
  checking pause only between steps (spec says "between tool calls").

## D-13 — L6 retry layers and side-effect non-retry (P0)

- Two layers: (1) HTTP 429/5xx retry stays inside the provider
  `doWithBackoff` loop (conformance fixtures already prove it), now
  jittered via `provider.JitteredBackoff(10ms, attempt, jitter)` —
  random in [0.5x, 1.5x) of base·2^attempt instead of deterministic
  doubling (each provider carries an unexported `jitter` func field,
  defaulting to rand, injectable in tests). (2) Stream-level failures
  (truncated stream, connection reset by peer, unexpected EOF, broken
  pipe — `provider.IsRetryable`) are retried by the engine step loop:
  `maxStreamAttempts = 3`, base `50ms`, jittered the same way, sleep
  is ctx-aware.
- Order of checks on a stream error: ctx.Err() first (cancelled run
  keeps L1 semantics — never a failed/retry), then `IsRetryable`,
  then `gotDelta == false`, then attempts remaining. Retrying after a
  delta would duplicate visible text and context; assistant and tool
  messages are only committed after a clean stream, so a retry can
  never duplicate context. Non-retryable errors (exhausted retries,
  api-key, start-hint) fail fast on attempt 1.
- Each retry emits a non-terminal EventWarning
  "stream error: … — retrying k/n (jittered backoff)".
- Side effects are not retried: the engine never re-dispatches a
  tool on its own — dispatch only ever happens on a fresh model
  response, and every dispatch passes the fresh per-dispatch gate
  (Perm.Allow + Hooks.FirePreTool). "Fresh approval" = that per-
  dispatch gate evaluation (v1 has no live per-tool approval UI; the
  Guard is mode-based). Locked by three fixtures: denied (tool runs
  0×, gate fires per attempt, strict model→dispatch alternation),
  approved (runs exactly once), failing (one fresh request + gate
  evaluation per attempt until the doom-loop ceiling).
- Rejected: engine-level HTTP retry (duplicate ownership with the
  provider layer); retrying after partial output (duplicates text);
  retrying non-retryable errors (wastes the attempt budget);
  auto-approving a side-effect tool for retries (spec requires a
  fresh approval each dispatch).

## D-14 — E1 context-matched patch semantics (P0)

- locate: a perfect match of the hunk context at its stated
  position always wins (confidence 1.0, never ambiguous). Otherwise
  every match within +/-20 lines is collected: 0 matches is an error
  ("context not found … refusing to guess"), more than 1 is an
  error ("ambiguous fuzzy match: N candidate positions (lines …)"),
  exactly 1 is applied with logged confidence 1/(1+|offset|).
- apply: every context (' ') and removed ('-') line is now verified
  against the file before the write; a mismatch is an error instead
  of the old silent best-effort write. The tool output logs
  "(fuzzy match, offset=…, confidence=…)" when a fallback was used.
- edit_file keeps exact-first semantics (n>1 without replace_all is
  rejected as before). When the exact text is absent, a unique
  whitespace-normalized line-window match is allowed: only the
  MATCHING is normalized (strings.Fields join), the replacement text
  is applied verbatim, and the output logs "fuzzy match confidence
  0.90 at line N". Multiple normalized matches are rejected as
  ambiguous; zero still reports not found.
- Property evidence: 300 generated hunks (globally unique contexts)
  round-trip exactly; fuzz corpus on the parser still crash-free.
- Rejected: keeping the old fallback that applied the hunk at the
  stated offset when nothing matched (a guess, and a silent corruptor);
  first-hit-wins fuzzy (it would pick one of several identical
  contexts); case-insensitive fuzzy matching (too aggressive for a
  code editor).

## D-15 — E6 text-tool guards and byte preservation (P0)

- One shared guard `nonTextReason(data)`: oversize (>2 MiB) or
  encoding smells (UTF-16/UTF-32 BOM, NUL byte in the first 8 KiB).
  It runs after read and before any write in edit_file, apply_patch,
  and notebook_edit; write_file consults it only when a target
  already exists (creating new files is always allowed).
- CRLF is preserved by construction: edits are byte-level string
  replacements, so no newline conversion happens; the test asserts
  the 
 count is unchanged and no bare LF appears.
- UTF-16 fixtures are refused (BOM sniff), so their bytes round-trip
  untouched; editing them with a text tool would corrupt them.
- Rejected: converting encodings on edit (lossy, out of scope);
  extension allow/deny lists only (misses extensionless binaries);
  capping reads instead of edits (the spec targets editing).

## D-16 — E4 undo-last-agent-change (P0)

- New session-scoped `internal/undo` stack of `Effect{Path, TurnID,
  Tool, Existed, Before, After, Mode}` with a monotonic id so pops
  stay consistent under concurrency. Scope is the four text/file
  tools only (edit_file, write_file, apply_patch, notebook_edit);
  shell side effects are covered by the turn's git tree snapshot,
  not the per-file stack.
- Verify-before-restore: `Restore` compares current bytes to the
  recorded After image. A mismatch is a refusal ("… file changed
  since the agent edit (your changes are kept)") that leaves the
  effect ON the stack, so putting the file back and retrying undoes
  it. Restore rewrites Before bytes + `Chmod` to the original mode
  (WriteFile only applies modes at create), or removes created
  files.
- `CmdUndo`/`CmdRewind`/`CmdUnrevert` all refuse while a turn is
  running ("… refused: a turn is running"), so a rewind can never
  race a write. All command feedback — including successful undos —
  is emitted as `EventWarning` so it is always visible in the TUI
  transcript (success text starts "undid …").
- Rejected: undoing through shell history (unreliable); consuming
  the effect on refusal (would delete the agent's only way back);
  a separate "info" event type for one message (warning precedent
  already exists: "turn already in progress").

## D-17 — E3 per-turn checkpoints and After-aware rewind (P0)

- A checkpoint is taken at turn start (after TurnStarted): the
  conversation snapshot (session events, mutex-guarded copy) plus a
  git tree via the isolated-GIT_INDEX_FILE `write-tree` path, with
  the SHA also persisted as shadow ref
  `refs/niki/checkpoints/<turnID>` (best-effort `git update-ref`;
  never touches user branches/index/worktree; the SHA in the
  checkpoint still stands if the ref write fails).
- File pre-images are captured at FIRST touch during the turn
  (`CapturePreImage`, no-op on repeat; missing files recorded as
  Created). `RecordAfter` stores the post-write state so
  `RewindCode` can tell the agent's own edit apart from external
  changes: current==After → restore Before + Chmod stored mode;
  current==Before → rewrite is a harmless no-op via the same path;
  anything else → skipped as external unless force. Legacy
  snapshot-only checkpoints (no After) keep the old force semantics,
  so pre-existing tests pass unchanged.
- Modes are restored with `Chmod` after every write (os.WriteFile
  applies perms only at create). `Unrevert` replays the
  pre-rewind snapshot, which now also records modes and
  absent-file markers (Created → Remove).
- TUI `/rewind [turn]` (empty arg = latest) and `/unrevert` send
  real engine commands; the old hardcoded success stubs are gone.
  Conversation restore is proven at manager level via
  `RewindConversation` (event count in the reply); rebuilding the
  live TUI history from it is out of scope and documented here.
- Empirical probe that forced the tool fixes (write_file new files
  came out 0600, edit_file clobbered 0600→0644): edit_file now
  Chmods the tmp file to the target's original perm before rename;
  write_file preserves on overwrite and uses 0644 for new files.
  apply_patch needed nothing (WriteFile on an existing path keeps
  its mode).
- Rejected: forcing rewinds through external edits silently;
  recording the full file set eagerly at turn start (cost);
  checkout-based restore (would move HEAD).

