# NIKI Finish — Progress Ledger (docs/finish/PROGRESS.md)

Append an entry after every slice. Re-read docs/FINISH_PACK.md,
docs/finish/{PROGRESS,CHECKLIST,DECISIONS}.md at each phase start and after
any context compaction.

## 2026-10-10 — Phase 0 (PLAN) DONE, owner approved
- Read AGENTS.md; docs/FINISH_PACK.md did not exist (flagged); archived the
  mission text to docs/FINISH_PACK.md after owner approval.
- Audited all 100 checklist rows + 10 T5 sub-rows against the code with six
  parallel read-only audits (file:line evidence in the Phase 0 report).
- Baseline evidence printed: `go build ./... && go test ./... -count=1`
  -> 39 packages ok; per-row runs PASS (L8, S3, G2, Q4); fuzz 5s each ->
  FuzzParsePatch 215,792 execs / FuzzParseSSE 330,422 /
  FuzzJSONRPCFraming 166,513, PASS, no panic.
- Audit tally (100 main rows): GREEN 4 (L8, S3, G2, Q4), PARTIAL 41,
  RED 29, MISSING 26; T5 sub-rows 10/10 MISSING.
- Critical findings: (1) real-provider tool loop dead — no `tools` in
  requests, no tool-call deltas in SSE; (2) instructions/env context unwired
  (zero production callers); (3) section V entirely missing; (4) keys written
  to config TOML, Redact dead code; (5) sandbox default off, install.sh
  checksum-less + sudo, README claims Windows with no CI.
- Phase 0 decisions D-01..D-06 confirmed by owner (see DECISIONS.md);
  D-07 recorded for the provider tool-loop completion.
- Owner issued the three finish goals (archived in FINISH_PACK.md).
- Demo command for this slice: `go test ./... -count=1` (baseline green).

## 2026-10-10 — Slice A: provider tool-loop completion (D-07)
- Why first: without `tools` in requests and tool-call deltas in SSE, no real
  or local model can ever call a tool (Phase 0 blocker 1) — I2/I5/L/M rows
  are untestable end-to-end. Completes the documented PACK provider contract.
- Changes: provider.ToolSpec + ToolStreamer interface; Message gains
  ToolCalls/ToolCallID; SchemaToJSONSchema; openai (tools request +
  tool_calls delta accumulation/flush); anthropic (content-block mapping,
  tool_use/input_json_delta parsing, tool_result grouping); responses
  (function_call input items + output_item.done parsing); engine exposes
  registry schemas per turn and persists tool calls/results in context;
  projector preserves assistant tool-call messages and repairs unanswered
  calls by ID.
- Tests (failing first, printed): TestOpenAIToolsRequestAndToolCallStream,
  TestAnthropicToolsRequestAndToolUseStream,
  TestResponsesToolsRequestAndFunctionCallStream, TestSchemaToJSONSchema,
  TestEngineExposesRegistryToolsToProvider (36 specs exposed, printed).
- Evidence: `go test ./internal/provider/ ./internal/engine/` ok;
  `go build ./... && go test ./... -count=1` green (full suite).
- Demo: go test ./internal/provider/ -run 'Tools|FunctionCall|SchemaToJSON' -v

## 2026-10-10 — Slice B: I1 install & checksums (P0)
- Why next: checklist P0 order (I1-I5 first); every later row assumes a
  verified, sudo-free install path exists.
- Changes: install.sh rewritten (NIKI_INSTALL_BASE_URL/NIKI_INSTALL_DIR
  overrides, checksums.txt verification BEFORE unpack/exec, refuse if
  manifest missing or tarball not listed, default $HOME/.local/bin with
  --system opt-in and never-sudo + exact fix hints, INSTALLED_DIR= machine
  line, idempotent symlink refresh, --uninstall removes binary + nc/niki
  with before/after residue test, cross-arch-tolerant --version step, WSL
  note in unsupported-OS path); install.ps1 added (Get-FileHash before
  Expand-Archive, checksums.txt manifest required, -Uninstall, no
  auto-elevation, amd64/arm64 asset mapping).
- Tests (written first, printed 6 FAIL + 1 pre-green, then 7 PASS):
  TestI1InstallVerifiesChecksumAndInstalls, TestI1TamperedDownloadRefused,
  TestI1MissingChecksumRefused, TestI1IdempotentAndUninstall,
  TestI1DarwinArchMapping, TestI1UnsupportedPlatformRejected,
  TestI1WindowsPowerShellScript (tools/installer/installer_test.go).
  Clean env: fresh HOME, PATH shim dir (uname controllable, sudo trap
  marker, curl URL log), local file:// release fixture, no network/docker.
- Bug found by red run: uname shim was written before default OS/arch set
  (empty uname -> spurious unsupported-OS); fixed by initializing fields
  first.
- Evidence: `go test ./tools/installer/ -v` 7/7 PASS printed;
  `go build ./... && go test ./... -count=1` green (FULL_SUITE_EXIT=0).
- OWNER-VERIFY (real download) recorded against I1 row; Windows real run
  also OWNER-VERIFY (no pwsh in env).
- Decision D-08 logged (install contract).

## 2026-10-10 — Slice C: I2 first-run ordered detection (P0)
- Changes: new internal/firstrun package — Branch enum (env-key, ollama,
  no-key-demo), Detect/DetectAt (order enforced: env keys from
  config.EnvKeysInOrder() -> HTTP probe of local server -> recorded-demo
  offer), ProbeOllama (500ms bounded GET /api/tags, any HTTP response =
  running), embedded recorded_session.json fixture loaded via
  LoadRecordedDemo (label must contain "recorded" or load fails).
  Product wiring: `nikicode demo` cobra command replays the recorded
  session with [recorded] label; buildProvider's unconfigured path now
  returns NewUnconfiguredProviderWithMessage(firstRunMessage(DetectDefaults()))
  so the first turn shows branch-specific exact guidance. config gains
  EnvKeysInOrder() (aligned with AutoDetectProvider's check order).
- Tests (written first, printed red then green): firstrun 4 tests
  (TestFirstRunBranch1EnvKey — probe must NOT run when key found, key
  value must not leak; TestFirstRunBranch2RunningOllama — DetectAt against
  live httptest server, offer names URL + next step;
  TestFirstRunBranch2RunningOllamaRealProbe — live=ok dead=error;
  TestFirstRunBranch3RecordedDemo — offer + label + 9 replayed events
  printed); cmd 3 tests (demo command output, firstRunMessage for all 3
  branches printed, cobra guard). Red run showed undefined
  newDemoCommand/firstRunMessage, then one fixture-wording fix (lowercase
  "ollama" next-step match).
- Evidence: `go test ./internal/firstrun/ -v` 4/4 printed;
  `go test ./cmd/nikicode/ -run 'Demo|FirstRun' -v` 3/3 printed;
  real CLI run: `go run ./cmd/nikicode demo` printed the labelled 9-event
  recording (DEMO_EXIT=0) above; `go test ./... -count=1` GO_TEST_EXIT=0.
- Note: hermetic branch selection tests inject getenv/probe; production
  DetectDefaults() uses os.Getenv + ProbeOllama (branch depends on the
  host machine at runtime, which is correct for a first-run flow).

## 2026-10-10 — Slice D: I3 doctor nine checks (P0)
- Changes: new internal/doctor — Result{ID,Status,Detail,Fix}, the nine
  checks (install integrity / PATH shadowing / config validity / provider
  reachability / git / sandbox backend / terminal capabilities / proxy+CA
  / disk space), Run(Probes) with injectable deps, DefaultProbes (real
  os/exec/config/sandbox/terminal/http/statfs via x/sys/unix, windows
  stub file for cross-compile), Render for CLI/TUI output. Cmd doctor
  rewired to run the real checks (dropped hardcoded "Go runtime",
  "Terminal capabilities", "Working directory writable" and duplicated
  sandbox line); TUI /doctor replaced its canned 4-green report with the
  same Run (terminal probe uses boot-negotiated tui.BootCaps so no raw-mode
  re-probe inside the live session).
- Tests (failing-fixture per check, printed): 11 fixtures + guard test
  asserting all nine IDs in canonical order, unique, and Fix != "" for
  every non-OK result; healthy-fixture Run prints 9/9 OK. Fixed 2 fixture
  bugs during red/green (provider fixture needed a key for IsConfigured;
  git test used LookPath signature). Note: docs claim red-first; the
  provider fixture fix was made after the first printed run, package then
  12/12.
- Evidence: `go test ./internal/doctor/ -v` 12/12 printed; real
  `nikicode doctor` printed above (2 failed/1 warning, all honest);
  tui+doctor+cmd tests ok; `go test ./... -count=1` GO_TEST_EXIT=0 (42 ok).
- Found bug B-01 (deferred to I5): provider name "ollama" passes
  config.IsConfigured() and is offered by /connect + I2 guidance, but
  buildProvider has no "ollama" case -> falls through to
  NewUnconfiguredProvider. I5 must wire Ollama to the OpenAI-compatible
  endpoint (base http://localhost:11434/v1, no key).

## 2026-10-10 — Slice E: I4 keychain + no-keys-in-files (P0)
- Dep: added approved github.com/zalando/go-keyring v0.2.8 (+ godbus/dbus,
  danieljoos/wincred transitives).
- Changes: new internal/secrets — Backend SPI over go-keyring, Set (keychain
  when available, else session store + persisted=false, never errors on
  unavailability), Get (session > keychain, marks backend disabled after
  transport failure), Delete, UseBackendForTest. config: accountFor
  (openrouter/deepseek/gemini distinguished), ResolveAPIKey now
  session/in-memory > keychain > env, SaveUserConfig zeroes APIKey before
  marshal. TUI /connect: secrets.Set(prov, key) before save; success
  message names the store (keychain vs session-only + exact env var to
  export via envVarForProvider). logx.write redacts event/error/fields
  through tools.Redact (kills Phase-0 "Redact is dead code" for logs).
  main: printConfig closure extracted to package-level renderConfig
  (explicit field allow-list). doctor: config validity warns when the
  config file still contains api_key (fix = re-connect or delete line).
- Tests (written first, printed red -> green): secrets 3 (keychain
  round-trip, session fallback when unavailable, session rotation beats
  stale keychain), config 3 (PLANTED-KEY SCAN: legacy file with key ->
  load -> secrets.Set -> save -> walk every file under fresh HOME = 0
  hits while ResolveAPIKey still returns the key; resolve order;
  accountFor table), logx 1 (planted key absent from written log,
  [REDACTED] present, events intact), cmd 1 (renderConfig clean in both
  --sources modes, no api_key field), doctor 1 (legacy key on disk ->
  warn + migration fix).
- Evidence: six packages ok in one run; real `nikicode config show` and
  `config show --sources` printed above (no api_key line);
  `go test ./... -count=1` GO_TEST_EXIT=0 (43 ok).
- Scope note: trajectory (session SQLite) + UI + export redaction sweep is
  row X3 ("planted-secret scan of all outputs") — same mission clause,
  tracked there.

## 2026-10-10 — Slice F: I5 offline local model path + B-01 fix (P0)
- Bug fixed: B-01 — provider.name = "ollama" passed IsConfigured() and was
  offered by /connect + I2 guidance, but buildProvider had no "ollama" case
  and fell through to NewUnconfiguredProvider. New `case "ollama"` returns
  the local OpenAI-compatible client (explicit provider.base_url overrides
  the default, for a remote Ollama).
- Changes: internal/provider/ollama.go — OllamaBaseURL (localhost:11434/v1),
  OllamaStartHint(model) (exact "what to start" text), NewOllamaProvider
  (no key, StartHint set, name "ollama"). openai.go — OpenAIProvider gains
  StartHint + name; startHintError() rewrites connection-level failures;
  status 404 with StartHint appends "server reachable ... but the model is
  missing". main.go — constructSingle ollama case.
- Tests (written first, printed red -> green): provider 3 (conn-refused
  exact hint: ollama serve / ollama pull / model name / "no local model";
  404 model-missing exact hint + server-reachable note; offline loopback
  stream emits read_file tool call with no Authorization header), engine 1
  (OFFLINE FIXTURE ACCEPT: httptest Ollama-compatible SSE server, real
  provider client, 2 round trips, read_file tool executed, final answer
  received, turn completed — all loopback), cmd 1 (B-01 wiring: name
  "ollama", loopback URL with port 11434, empty key, StartHint present).
- Evidence: 6 tests PASS; `nikicode exec` with config provider=ollama and
  model missing -> printed exact hint + "(server reachable at
  http://localhost:11434/v1, but the model is missing: ...)" exit 1;
  with local `qwen2.5-coder:3b` (real Ollama on this machine, no key)
  -> exit 0 printed `OFFLINE_OK` (real offline run completes a task).
  Full suite GO_TEST_EXIT=0 (43 ok).
- Note: available local models for later eval work: qwen2.5-coder:3b,
  claude-sonnet-4-5:latest (both ~1.9GB). M9 profile model
  qwen2.5-coder:7b-instruct-q4_K_M (D-01) is NOT pulled yet — owner/pull
  decision stays with P3 M9.

## 2026-10-10 — Slice G: L1 cancellation at token boundaries (P0)
- Acceptance: `TestCancelAtTokenBoundaryLeavesConsistentSession` (new
  internal/engine/cancel_consistency_test.go) — fixture stream cancelled
  mid-token leaves a consistent session. Uses the REAL session.Store
  (sqlite + jsonl, temp dir) wired exactly like the CLI
  (eng.Observe -> store.AppendEvent), mock provider streaming one word
  per token (10ms), CmdInterruptTurn fired after token 3/15.
- Assertions (all printed/passed first run — existing A3 machinery held):
  store has exactly 1 turn_cancelled as the LAST event, 0
  turn_completed, every persisted delta non-empty and concatenating to a
  whole-token prefix of the full answer, no torn assistant message in
  the runner context after cancel, and a follow-up turn runs to
  turn_completed (session usable — store ends turn_completed, 18 events).
  Evidence line: "cancel at token 5/15: 1 turn_cancelled last in store,
  5 whole-token deltas persisted, follow-up turn completed".
- Hardening: provider/mock.go final DeltaUsage/DeltaDone sends made
  ctx-aware (select on ctx.Done) — removes a goroutine-leak window in
  the fixture when cancellation lands at stream end.
- Tests: engine/provider/session packages ok (count=1).

## 2026-10-10 — Slice H: L2 budgets with gauges, WRAP, verdicts (P0)
- New internal/budget: Limits (wall/cost/steps), Tracker state machine
  (ok → wrap at >=85% → stopped at >=100%, sticky, deterministic hit
  kind order wall/cost/steps), Gauges snapshot, ParseLimits.
  Protocol: EventBudgetGauge/EventBudgetWrap/EventBudgetStopped +
  EngineEvent.Budget (BudgetInfo payload with per-dimension gauges and
  verdict). Config: BudgetsConfig + Default (10m / $2.00 / 100 — D-09).
  Engine: SetBudget/SetBudgetCheck, per-turn beginBudget, observeBudget
  at each safe point (gauge event every step; WRAP/stopped → check →
  verdict → terminal event → WrapBudget end), AddStep with
  routing.CalculateCost cost. TUI: status-bar gauge
  ("budget: 42% OK/WRAP/STOPPED"), history lines, Busy=false on
  terminal budget events, turnEnded finalization. exec: budget events
  are terminal (prints verdict, --github-check conclusion neutral) —
  fixes a would-be headless hang. applyBudget wired in both engine
  constructors.
- Tests (red→green): internal/budget 5 (0.84/0.85/1.00 boundaries,
  sticky, disabled dims, gauge values), engine 6+1 fixture scenarios
  (wall/cost/steps × wrap/stopped via scripted always-tool provider;
  asserts event type, state, hit kind, verdict, gauge payloads, no
  turn_completed, completed steps kept in session) + disabled-never-
  fires. Evidence printed: wall wrap@0.90 / stopped@1.13; cost
  wrap@0.90 / stopped@1.12; steps wrap@0.90 / stopped@1.00; verdicts
  "fixture-verified"; gauges=2..10 per run.
- Real CLI evidence: fresh HOME config [budgets] wall_time="1ns" →
  `nikicode exec` printed "[budget stopped] hit=wall_time ... verdict:
  no-verifier", EXEC_EXIT=0; `exec --jsonl` shows budget_gauge and
  budget_stopped records (state/hit/wall fraction/verdict).
- Full suite GO_TEST_EXIT=0 (44 ok incl. new budget pkg); gofmt clean.


## 2026-10-10 — Slice I: L3 verifier gate (P0)
- New internal/verifier: Result (verified/not-verified/inconclusive),
  Verdict.Passed() only for Verified, Request{Prompt,Claim,Attempt},
  Verifier interface, Func adapter (error downgrades to NotVerified —
  "an erroring verifier is never a pass"). 3 tests green.
- Protocol: EventVerification + EventTurnNotVerified +
  EngineEvent.Verification. Engine: SetVerifier(v, maxLoops), claim
  gate in the turn loop (see D-10), ErrNotVerified/WrapNotVerified,
  loop-exhaustion paths now end not-verified.
- Acceptance (red→green): TestModelDoneClaimWithFailingCheckEndsNotVerified
  (done claim + failing fixture check → 2 attempts → turn_not_verified,
  0 turn_completed, feedback persisted), TestVerifiedClaimEndsRunDone,
  TestNoVerifierNeverMarksDone. Printed [L3] evidence from all three.
- Terminal handling so nothing hangs: exec prints "[not verified]"
  reason, --github-check conclusion failure, exit non-zero; TUI
  applyEvent case + turnEnded set; ACP/Codex listenEvents map to a
  "failed" protocol update; soak accepts turn_not_verified as a turn
  end (harness measures turns, not verdicts); benchTurnOverhead uses a
  trivial pass fixture (measures overhead, not verification).
- Fallout fixed honestly: 10 engine tests that expected claims to
  complete now install a shared passVerifier() fixture (gate itself is
  tested only in verifier_gate_test.go); appserver tests inject
  passVerifier via new SetVerifier passthroughs.
- Real CLI evidence: local Ollama qwen2.5-coder:3b, prompt "reply ONLY
  with done" → model printed "done", then "[not verified] completion
  claim recorded, but no verifier is configured (inconclusive is never
  a pass)", EXEC_EXIT=1; --jsonl records turn_not_verified with reason.
  Full suite GO_TEST_EXIT=0 (45 ok); gofmt clean.
- Interim noted: product runs end not-verified until V wires the real
  verifier (D-10).

## 2026-10-10 — Slice J: L4 steering (P0)
- Dedicated protocol EventSteer; engine emits it at the safe-point
  drain (was a misleading EventTurnStarted reuse — D-11); TUI history
  line "[steer] <text>", non-terminal.
- Acceptance (red→green): steer_plan_test.go with a deterministic
  mid-run fixture — plan_gate tool blocks the run until the test
  pushes the steer, message-aware planProvider plans A without a steer
  and B when the steer text is in the user messages, plan_step tool
  records choices. Printed: [L4 steer run] event="prioritize plan-B
  for the next step" plans=[B] requests=3 turn_completed;
  [L4 control] plans=[A] (no steer). Proves the steer lands at the
  safe point, is shown as an event, and changes the next plan.
- Full suite GO_TEST_EXIT=0 (45 ok); gofmt clean.

## 2026-10-10 — Slice K: L5 Esc pause / atomic writes (P0)
- D-12: CmdPauseTurn two-stage (Esc pause at safe point / Esc again
  stops now); CmdInterruptTurn + CmdCancelTurn unchanged (immediate).
  Engine: pauseRequested atomic flag, reset per turn, runner
  PauseRequested probe, safe-point checks at loop top AND between
  tool calls (skipped calls filled with tool messages), ctx.Err()
  check before pause check (hard stop never mislabeled as pause),
  warnings "pausing at safe point…" / "stopping now (Esc)".
- Atomicity hardening: edit_file tmp+rename (was O_TRUNC WriteFile);
  format.FormatFile detached from turn cancellation (WithoutCancel +
  5s bound). TUI: Esc → CmdPauseTurn, activity "pausing… (Esc again
  to stop)", cancelled history line shows the reason, help text
  updated; Ctrl+C still CmdInterruptTurn.
- Acceptance (red→green): interrupt_l5_test.go — gated product
  write_file (deterministic mid-dispatch interrupt, synced on engine
  warning events to remove the command-processing race found in the
  first full-suite run). Printed: [L5 mid-write] file2="content-two-
  WHOLE" whole; cancel="paused at a safe point (Esc)"; [L5 esc-twice]
  file1 kept; file2="content-two-WHOLE" whole. Stable over 3 repeats.
- Full suite GO_TEST_EXIT=0 (45 ok); gofmt clean. Note: one run
  observed pre-existing flake TestProcessManagerAndProcessTools (PTY
  timing under full-suite load) — passes 3/3 standalone, untouched by
  this slice, not related to L5.

## 2026-10-10 — Slice L: L6 retries with jittered backoff / side-effect gating (P0)
- D-13: two retry layers — providers keep 429/5xx retries (now
  jittered: `JitteredBackoff(10ms, attempt, jitter)`, unexported
  `jitter` field on openai/anthropic/responses) + `IsRetryable`
  classification; engine stream-retry loop (max 3 attempts, 50ms
  jittered base, ctx.Err() first, retry only before first delta,
  EventWarning "retrying k/n (jittered backoff)" each attempt).
- New `internal/provider/backoff.go` (JitteredBackoff, IsRetryable)
  + `backoff_test.go`; engine `stream_retry_test.go`
  (truncated→retried/jittered gaps; mid-stream truncation NOT
  retried; non-retryable fails fast) + `side_effect_approval_test.go`
  (denied→0 runs/fresh gate per attempt; approved→runs exactly once;
  failing→one fresh request+gate per attempt, no engine auto-retry).
- Acceptance printed: `[L6 429] attempts=2 jitterCalls=1
  text="recovered"`; `[L6 truncated] calls=3 gap1=38.3ms gap2=110.2ms
  warnings=2 completed`; `[L6 mid-stream] calls=1, no retry, partial
  emitted once`; `[L6 fail-fast] calls=1`; `[L6 denied] toolRuns=0
  gateFires=2 toolStarted=2 modelTurns=3`; `[L6 approved] toolRuns=1
  gateFires=1 completed`; `[L6 failed-side-effect] toolRuns=3
  gateFires=3`.
- Full suite GO_TEST_EXIT=0 (45 ok); gofmt clean. L6 GREEN (16 rows).

## 2026-10-10 — Slice M: E1 patch semantics + E6 text-tool guards (P0)
- E1 (D-14): `ApplyPatchReport` (MatchReport fuzzy/offset/confidence);
  `findFuzzyMatch` rewritten (exact wins; 0/∞/1 candidates →
  error/ambiguity/unique+confidence 1/(1+|off|)); strict context and
  removed-line verification in the apply loop; apply_patch logs
  confidence; edit_file unique normalized-window fallback (confidence
  0.90) with ambiguity rejection. New tests: patch_ambiguity_test.go
  (7) + edit_fuzzy_test.go (4).
- E6 (D-15): `internal/tools/textguard.go` (`nonTextReason`: 2 MiB cap,
  UTF-16/32 BOM, NUL-in-8KiB) wired into edit_file / apply_patch /
  notebook_edit / write_file-overwrite. New tests: text_guard_test.go
  (6) incl. CRLF round-trip and UTF-16 refusal with byte-identical
  file.
- Acceptance printed: `[E1 ambiguous] rejected: ambiguous fuzzy match:
  2 candidate positions (lines 7, 15) … refusing to guess`;
  `[E1 unique-fuzzy] offset=+6 confidence=0.14 applied`;
  `[E1 property] 300 generated hunks round-tripped exactly`;
  `[E1 tool] … (fuzzy match, offset=+6, confidence=0.14)`;
  `[E1 fuzzy-amb] edit rejected … (line 1, 4) … refusing to guess`;
  `[E6 binary]` / `[E6 utf16]` / `[E6 binary-patch]` / `[E6 oversize]`
  refusals with files untouched; `[E6 crlf] 3 CRLF pairs preserved, no
  bare LF`; `[E6 write-binary] refusing to overwrite …`.
- Full suite GO_TEST_EXIT=0 (45 ok); gofmt clean. E1+E6 GREEN (18
  rows).

## 2026-10-11 — Slice N: E3 checkpoints/rewind + E4 undo (P0)
- E4 (D-16): new `internal/undo` stack; engine `dispatchTool`
  captures pre-images of the four file tools and pushes changed
  effects; `CmdUndo`/`CmdRewind`/`CmdUnrevert` in `handleCommand`
  (refuse while a turn runs); `Engine.SetCheckpoints`,
  `Session.Snapshot` (mutex-guarded), protocol `CmdUndo/CmdRewind/
  CmdUnrevert` + `EngineCommand.TurnID`.
- E3 (D-17): checkpoint at turn start (events + `CreateGitTreeSnapshot`
  now also writing shadow ref `refs/niki/checkpoints/<turnID>`);
  `CapturePreImage`/`RecordAfter`; After-aware `RewindCode` + mode
  restore + Created handling; `LastTurnID`; mode-preserving edits
  (edit_file stat+chmod, write_file 0644-new/preserve-overwrite);
  TUI /undo added, /rewind + /unrevert stubs replaced with real
  commands.
- New tests: `internal/engine/undo_checkpoint_test.go` (3: undo
  bytes+mode, refusal-keeps-user-edits+retry, checkpoint+rewind
  bytes/modes/created-removal/conversation/shadow-ref) and
  `internal/tools/mode_preserve_test.go` (2). Acceptance printed:
  `[E4] empty-stack message: nothing to undo`;
  `[E4 refusal] undo refused: cannot undo …: file changed since the
  agent edit (your changes are kept)`;
  `[E4 retry] undid edit_file change to …`;
  `[E3 rewind] rewound to checkpoint <id>: restored 2 file(s),
  skipped 0, conversation events 6`;
  `[E3] bytes+mode restored (-rw-------), created file removed,
  conversation events=6, shadow
  ref=refs/niki/checkpoints/<id>`; `[N] edit_file preserved mode
  600`; `[N] write_file preserved 755 on overwrite, 644 for new
  file`.
- Full suite GO_TEST_EXIT=0 (45 ok); gofmt clean; golangci-lint
  clean except a pre-existing S1000 in a Slice-L test. E3+E4 GREEN
  (20 rows).

## 2026-10-11 — Slice O: C1 + C2 + C3 + C9 context rows (P0)
- C1 (D-19): `skills.ResolveInstructionSet` (global, root-to-dir,
  lazy `Load`) + hierarchy/laziness/per-directory test.
- C2 (D-18): `CapOutput` head-and-tail preview (1024+1024, newline
  cuts) with disk persistence + reference; 10 MB acceptance test
  (2210-byte preview, 10640089 bytes retrievable, byte-identical).
- C3: index-free proof — grep/glob/read with PATH stripped to an
  empty dir; symbol_search degrades with "not initialized".
- C9 (D-20): new `internal/envctx` (collect once, 2s timeout,
  `Watcher.Refresh` on change) wired into `TurnRunner` +
  `ContextAssembler.SetEnvBlock` (system-message copy injection);
  branch-change refresh tests at package and engine level.
- Acceptance printed: `[C1] hierarchy global->root->dir, contents
  lazy …`; `[C1] per-directory set: root sees 2 files, sub sees
  3`; `[C2] 10 MB output -> 2210-byte head+tail preview, full
  10640089 bytes retrievable`; `[C3] grep+glob+read work with PATH
  stripped …`; `[C9] collected once, refreshed on branch change …`;
  `[C9] env block attached on turn 1 (env-a), refreshed on turn 2
  (env-b)`.
- Packages tools/skills/envctx/engine/contextwin green; gofmt
  clean. C1+C2+C3+C9 GREEN (24 rows).
