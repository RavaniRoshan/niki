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

## 2026-10-11 — Slice P: V1–V4 real verifier (P0)
- New `internal/verifier` pieces (D-21): requirements.go
  (ExtractRequirements + 5 golden fixtures), stacks.go
  (DetectStack/ChecksFor/SelectChecks + README-trap test),
  tiers.go (T0–T3 Grade/VerdictFor + per-tier test),
  integrity.go (SnapshotIntegrity/Diff + deletion tests),
  standard.go (Standard.Verify with injectable Runner, 120s
  check timeout, T2-block on drift).
- Wired `verifier.Standard{Dir: cwd}` as the default verifier in
  both product paths (TUI rootCmd + exec), lifting the L3 interim.
- Acceptance printed: `[V1 empty/single/multi/bullets/prose]`;
  `[V2] allowlist only: [go-test go-vet] (README trap ignored)`;
  `[V2] unknown stack: zero checks, no guess`;
  `[V3 T0/T1/T1-timeout/T2/T3]` tier→verdict lines;
  `[V4] drift flagged: [removed: calc_test.go]`;
  `[V4] mid-run deletion -> not-verified (… T1 (weak); integrity
  drift: removed: victim_test.go)`;
  `[V] all-pass -> verified (… T3 (clean))`;
  `[V] one failure -> not-verified (… T1 (weak))`;
  `[V] no checks -> inconclusive …`;
  `[V] real toolchain (go test + go vet on fixture) -> verified
  (… T3 (clean))`.
- Packages verifier/engine/cmd green, full build OK; gofmt clean.
  V1+V2+V3+V4 GREEN (28 rows).

## 2026-10-11 — Slice Q: T1/T2/T3/T8 + X1/X2/X3/X4/X6 (P0)
- Tools: T1 count/sizes/unique test; T2 version `"1"` on all 36
  schemas + strict unknown-arg rejection (3 engine fixtures
  re-versioned after the fallout broke L6/steer); T8 `Kind`
  success/error/timeout + bounded bodies; T3 MCP `AllowedTools` +
  `ScanDescription` (poisoned `helper` in the fake server).
- Safety: X1 HOME hiding (bwrap tmpfs + Seatbelt profile) +
  loopback-off escape test; X2 cycle skips Never Ask, `/yolo`
  explicit only (pinned cycle test updated); X3 `Redacted()` at
  trajectory/store/exports/UI; X4 `ScanDirectives` + engine
  warning with README-trap fixture; X6 untrusted config loses
  MCP + approval escalation + sandbox weakening.
- Acceptance printed: `[T1] tools=36 schema_bytes=3329 …`;
  `[T2] all 36 schemas carry version "1"` + strict rejection;
  `[T8] kinds: success/error (+timeout on deadline)`;
  `[T3] poisoned description flagged + hidden …` + allow-list +
  `5 traps flagged, 4 clean pass`;
  `[X1] HOME hidden, loopback unreachable, workspace writable`;
  `[X2] default mode asks … 6 cycles, Never Ask never appears …
  explicit /yolo confirms`;
  `[X3] store files: 0 hits … scrubbed … exports: 0 hits … UI …
  trajectory scrubbed`;
  `[X4] flagged: … (approval-change, network-access, disclosure)
  … approval mode unchanged; no shell ran it`;
  `[X6] untrusted config contained …`.
- Full suite GO_TEST_EXIT=0 (46 ok); gofmt clean; lint clean
  except pre-existing S1000. T1+T2+T3+T8+X1+X2+X3+X4+X6 GREEN
  (37 rows).

## 2026-10-11 — Slice R: M1/M3/M6 + R1/O1/O5/G1/G3 (P0)
- M-rows: repair round-trip (validator error + schema, 2 tries
  then logged failure) with outcome snapshotting;
  `toolSpecs()` plan-gating (17 read-only) + dispatch denial;
  `RepeatCount` reminder-on-2 / doom-stop-on-3.
- R1: 50-append torn-line scan + resume (pre-existing kill tests
  green). O1: usage accumulation + totals on turn_completed.
  O5: `logx.OpenRotated` (1 MiB/3 backups) + `/proc` growth
  network-capture test. G1: `EnsureRunBranch` + dirty-checkout
  proof. G3: `IsPushCommand` AST refusal in shell (pre-approval).
- Fallout fixed: T2 versioning rejected engine fixture schemas
  (L6/steer broke) → fixtures versioned; `TestShiftTabCyclesModes`
  updated to the X2 cycle; tools→permissions import forced
  moving the update_plan half of the plan-mode test (cycle).
- Acceptance printed: `[M1] malformed args -> … recovered on
  retry`; `[M1] two retries then logged failure`;
  `[M6] reminder on 2nd identical call, stop on 3rd`;
  `[M3] PLAN exposes 17 read-only tools …`;
  `[R1] 50 atomic appends, 0 torn lines … resumed … 51 events`;
  `[O1] per-step 21/21 summed exactly; final report: usage: 21
  prompt + 21 completion = 42 tokens over 3 steps …`;
  `[O5] 300 entries -> 3 bounded files …`;
  `[O5] full run: external connections 1 -> 1 (no telemetry)`;
  `[G1] run on niki/run-abc123; dirty checkout intact; main
  still at 1 commit(s)`;
  `[G3] 4 push forms refused … non-push unaffected`.
- Full suite GO_TEST_EXIT=0 (46 ok); gofmt + vet clean; lint
  clean except pre-existing S1000. M1+M3+M6+R1+O1+O5+G1+G3
  GREEN (45 rows).

## 2026-10-11 — Slice S: P1-P5 + H1/H2 + Q1/Q2/Q5/U1/U2 (P0)
- P-rows: P1 cell-width truncation + ASCII chrome + 3 hermetic
  golden frames + plain mode; P2 real tmux test (exec/paint/
  resize, SSH documented); P3 RestoreOnSignal + SIGHUP/panic
  proofs; P4 Windows-unsupported label locked by test; P5 four
  cross-builds recorded (darwin arm64/x64, linux arm64/x64).
- H-rows: `help [topic]` (9) + `examples` (4, all executed by
  the snippets test incl. a mock→T3-verified exec); key
  registry (10/10 dispatch, /help generated from it; stale
  Never-Ask line fixed, handler-less Ctrl+R unadvertised).
- Q-rows: trajectory golden with printed diffs; `internal/eval`
  20-task suite at 100% with EVALS.md; race-clean subset +
  no-leak test; U1 migrate (dry-run/backup/version/key-strip);
  U2 event schema v1 + forward-compat decode.
- Acceptance printed: `[P1 …] golden frame matches` x3 +
  `cell widths correct`; `[P2] … completes inside tmux` +
  `paints … re-renders after resize`; `[P3] … idempotent` +
  `SIGHUP restored …` + `panic path restored`;
  `[P4] … labelled unsupported …`; cross-build sizes;
  `[H1] 9 help topics` + 4 example lines;
  `[H2] key … dispatches` x10; `[Q1] trajectory matches
  golden`; `[Q2] … 20/20 = 100%`; `[Q5] goroutines 2 -> 2`;
  `[U1] dry run … backup …`; `[U2] schema version 1 …`.
- Full suite GO_TEST_EXIT=0 (46 ok); gofmt + vet clean; lint
  clean except pre-existing S1000. P1-P5+H1+H2+Q1+Q2+Q5+U1+U2
  GREEN (57 rows).

## 2026-10-11 — Slice T: T5 self-extension + 10 sub-rows (P0)
- New `internal/selfextend` (D-29): gaps, staging drafts,
  offline tests, frontmatter, proposals, scope+double gate,
  provenance enable, 3-strike audit, retire; config
  `self_extend` default-off; `niki skills` provenance state +
  `skills rm`.
- Acceptance printed: `[T5.1] 3 detections logged …`;
  `[T5.2] draft in staging; live hash unchanged`;
  `[T5.3] ssh-grab + network skill fails offline` + good
  passes; `[T5.4] frontmatter parsed, defaults closed …`;
  `[T5.5] proposal shows …; nothing enabled`;
  `[T5.6] scopes gated; denial deletes staging …`;
  `[T5.7] provenance recorded; tamper disables with warning`;
  `[T5.8] NOTICE: skill a1 auto-disabled after 3 consecutive
  errors`; `[T5.9] retire removed … no residue`;
  `[T5.10] default off; propose honored; garbage fails closed`.
- Full suite GO_TEST_EXIT=0 (47 ok); gofmt + vet clean. T5 +
  T5.1–T5.10 GREEN (68 rows).

## 2026-10-11 — Slice U: P1 context/edit/observability batch (C4-C8, E2/E5/E7, H3, I6, L7/L8, P6, S3, G2, Q4)
- Resumed an interrupted slice (uncommitted WIP from the prior
  session) and drove it to green. Covers 16 rows: 12 carried in the
  WIP (C4, C5, C6, C7, C8, E2, E5, E7, H3, I6, L7, P6) + 4 that were
  GREEN at the Phase 0 audit but never had their Final evidence
  filled in (L8, S3, G2, Q4).
- New code: `internal/index` Map (C4 repo outline under a token
  budget, AST + regex fallback); `internal/engine` StablePrefix (C5
  cache-stable system→tools→instructions order) + ContextAssembler
  .Summarize (C6 structured goal/decisions/open/files record, log
  untouched); `internal/update` (I6 opt-in, checksum-before-install,
  tamper refused) + `nikicode update`; `internal/locale` (P6 number/
  date per LANG, cell-truncation) wired into the usage report;
  `git.StageHunks` (E5 per-hunk staging); `memory.Forget/Show` (C8);
  `nikicode remember` (the ONLY memory writer) + `init` now also
  writes a commented nikicode.toml (H3, never overwrites).
- Proven from pre-existing code/tests: L7 (tools.Registry.Batch
  sequential for unsafe tools), L8 (Guard + engine phase gate), E2
  (diagnostics), E7 (ProposeCommitMessage/PRSummary + G3 push guard),
  S3 (agent concurrency semaphore), G2 (implementer worktree),
  Q4 (fuzz 114k/136k/55k execs, no panic).
- Acceptance printed: `[I6] manifest fetched …` + `tampered
  download refused`; `[C4] 10k files -> map within 2000 tokens …`;
  `[C5] prefix byte-identical across turns (165 bytes) …`;
  `[C6] structured compaction golden exact …` + `session log
  untouched (5 events)`; `[C7] mentions resolve incl. gitignored
  paths`; `[C8] reads never write (0 topic files)` + `explicit
  remember stores …`; `[E2] same-turn diagnostics reached the
  model`; `[E5] hunk 1 staged, hunk 2 worktree-only` + `reject-all
  stages nothing`; `[E7] commit draft from staged evidence …` + `PR
  draft lists branch commits … nothing pushed`; `[H3] init creates
  commented config + instructions; never overwrites`; `[P6] de_DE
  1.234.567 … en_US 1,234,567 …` + `long strings truncate cleanly`;
  agent `TestSubagentLimitsAndBudgets`/`TestRunSwarmConcurrency`/
  `TestSubagentWorktreeIsolation` PASS; fuzz no panic.
- Full suite GO_TEST_EXIT=0 (49 ok); gofmt + vet clean.
  C4-C8+E2+E5+E7+H3+I6+L7+L8+P6+S3+G2+Q4 GREEN (84 rows).

## 2026-10-11 — Slice V+S: verifier hardening (V5/V6/V7) + subagents (S1/S2/S4/S5)
- V5 flake: `verifier.RerunChecks`/`detectFlake` rerun each check k
  times; divergence -> inconclusive (never a pass), stable -> normal
  tier, deterministic fail -> T1 not a flake.
- V6 weakening: `verifier.DetectWeakening`/`MeasureTests` compare
  per-test-file assertion/skip/commented counts before/after; removed
  assertions, new skips, commented-out assertions cap the tier at T1.
  Integrity now also tracks cross-stack test files (_test.rs,
  .test.js/ts, .spec.js/ts) so their deletion is caught (V4 hardening).
- V7 calibration: `verifier` SeedSuite (52 cases / 5 stacks) run
  through the real Standard verifier with a ground-truth oracle; tamper
  and weakening cases mutate the repo mid-run then pass. Writes
  docs/verifier/TRUST_MODEL.md.
- S1 roles: `agent.BuiltinRoles` explorer/implementer/reviewer with
  closed tool allowlists (read-only absolute). S2 `Summarize`/bound.
  S4 `SpawnTimed`/`SpawnRole` + per-subagent timeout. S5
  `MaxNestingDepth=2` (depth-3 refused).
- Acceptance printed: `[V5] flaky go-test -> T1 …`/`stable … T3`/
  `consistently failing … T1`; `[V6] removed-assertions … (5 -> 4)`/
  `new-skips … (0 -> 1)`/`commented-out …`/`weakened -> not-verified`/
  `strengthening never flagged`; `[V7] 52 cases across 5 stacks:
  52/52 gate-correct; tamper-verified=0 weakening-verified=0` + wrote
  TRUST_MODEL.md; `[S1] explorer … (13)`/`implementer … (12)`/
  `reviewer … (10)`/`unknown role refused`; `[S2] 100k-rune transcript
  -> 1347-byte summary, 12 refs, truncated=true`; `[S4] subagent past
  its timeout -> closed`/`generous timeout -> completed`; `[S5] depth
  1 and 2 allowed, depth 3 refused`.
- Full suite green; gofmt + vet clean. V5+V6+V7+S1+S2+S4+S5 GREEN
  (91 rows).

## 2026-10-11 — Slice T: skills/hooks/commands/stats (T4/T6/T7/T9)
- T4: skills discovery now parses frontmatter only; the body is read
  lazily on `LoadBody` (invocation), not at discovery (fixed the eager
  body-read in `parseSkill`). Discovery scans user + project roots.
- T6: new `internal/hooks` — lifecycle hooks (session start/end,
  pre/post tool use, prompt submit, stop); a blocking pre_tool_use hook
  vetoes the call; hooks run only in trusted projects.
- T7: new `internal/slashcmd` — custom slash commands from Markdown
  with $ARGUMENTS/$1/${1} substitution (single-pass, no re-expansion).
- T9: new `internal/toolstats` + `nikicode stats` — local per-tool
  counts/failures in an append-only JSONL under $HOME; nothing leaves
  the machine.
- Acceptance printed: `[T4] discovered from user + project roots …
  body empty after discovery, loaded on invoke … optional script`;
  `[T6] blocking … vetoed … / passing allows / non-blocking (exit 3)
  does not veto / untrusted skipped / trusted executed / all 6 events`;
  `[T7] discovered /review … /review 42 security -> … / missing
  positional renders empty / single-pass … / braced ${1}`;
  `[T9] recorded 6 calls … niki stats table … persist locally`.
- Full suite green; gofmt + vet clean. T4+T6+T7+T9 GREEN (95 rows).

## 2026-10-11 — Slice M: small-model support (M2/M4/M5/M7/M8/M9)
- M2 text-format tool calls: `engine.RecoverTextCall` parses
  `<tool_name>{json}</tool_name>` and ```json {"tool":...}``` blocks;
  candidate-only — registry rejects near-miss names, schema gate
  rejects bad args; recovery logged.
- M4 micro-step plans: `engine.MicroPlan` — each step has a done
  criterion + check; a step cannot close until its check passes.
- M5 small-window budgets: `contextwin.Profile`/`Budgets` size repo-map,
  file-read, tool-output limits to the window; compaction earlier for
  small windows (60% @≤32k vs 80% @200k). 8k golden frozen.
- M7 decoding per phase: `engine.DecodingFor` — tool=0.0, plan=0.1,
  text=0.7; explicit output caps + stop sequences; unknown fails safe.
- M8 escalation: `engine.Escalator` — off unless configured; 2nd
  consecutive failure of a step offers one retry on the larger model.
- M9 small-model eval: `tools/smallmodel_eval.sh` prepared (OWNER-VERIFY,
  needs a real local model); EVALS.md carries an explicit "AWAITS OWNER
  RUN — no pass rate is claimed" note. No rate fabricated.
- Acceptance printed: `[M2] exact … near-miss "read_fil" … refused`/
  `fenced bogus … refused`/`malformed args refused`/`prose recovers
  nothing`; `[M4] close refused before check …`/`failed check keeps
  open`/`closes only after check passes`/`checkless closes`/`no
  criterion refused`; `[M5] 8k profile: repo-map=1000 file-read=2000
  tool-output=1333 compact@60%`/`8k@4800 vs 200k@160000`/`surfaces all
  smaller`/`no budget exceeds window`; `[M7] temperatures: tool=0.0
  plan=0.1 text=0.7`/`output caps 1024/2048/4096`/`stops set`/`unknown
  fails safe`; `[M8] off by default`/`2nd failure -> escalate to
  "gpt-4o"`/`one retry per step`/`per-step budget`/`success resets`.
- Full suite green; gofmt + vet clean. M2+M4+M5+M7+M8 GREEN, M9
  OWNER-VERIFY (101 rows green/verified).

## 2026-10-11 — Slice X+Q6: safety policy/audit/red-team (X5/X7/X8) + path guard (Q6)
- X5 network policy: new `sandbox.NetworkPolicy` — deny-all by
  default; exact and `*.suffix` allowlist; URL host gating.
- X7 audit log: new `internal/audit` — append-only JSONL of approvals
  and sandbox denials; logx-style rotation to .1/.2/.3; ordered Read.
- X8 red-team: new table-driven `TestRedTeamSuite30Plus` (38 cases):
  shell AST, push refusal, directive flags, secret scrubbing, the new
  path guard, network policy, verification traps, the mandated
  LIFECYCLE-SCRIPT TRAP, SYMLINK ESCAPE, INSTRUCTION-IN-A-FILENAME,
  plan containment, fail-closed approvals. Caught and fixed a real gap:
  AnalyzeShell missed backgrounded statements (mvdan models `cmd &`
  via Stmt.Background) — now too-complex.
- Q6 properties: new `tools.ResolveSafe` guard; seeded tests prove
  "success ⇒ inside" over 300 adversarial paths and patch round-trip
  / error⇒empty over 250 generated cases.
- Acceptance printed: `[X5] off by default … / allowlist … / URL gate …`;
  `[X7] recorded 2 approvals + 1 sandbox denials … / rotated …`;
  `[X8] 38/38 red-team cases passed …`; `[Q6 path] 300 adversarial
  paths: 195 inside, 105 refused`; `[Q6 patch] 200 forward+reverse exact
  / 50 corrupted refused with empty output`.
- Full suite green; gofmt + vet clean. X5+X7+X8+Q6 GREEN (105 rows).

## 2026-10-11 — Slice Y: session R2/R3 + observability O3 + git G4 (P1)
- R2 checkpoint fork: `Session.ForkAtCheckpoint(checkpointsSeen)` keeps
  events through the Nth `context_compacted` marker. Fixed an off-by-one
  that dropped the bounding marker from the fork.
- R3 session export: new `session.ExportBundle` + `BuildExportBundle`
  (`MarkdownTOC`, `JSONLines`), redacted payloads, frozen-timestamp
  fixture so goldens are stable.
- O3 `niki explain last`: new `internal/last` (`Summarize`/`Render`)
  produces asked/changed/passed/looped/not-verified from the sqlite
  event log; CLI `niki explain [session-id]` (latest session by default).
- G4 submodules/nested: new `git.Submods` reports registered
  submodules (path/URL/status via `git submodule status` + `.gitmodules`)
  and unregistered nested git work trees (immediate subdir scan).
- Acceptance printed: `[R2] fork at marker 1 keeps 3 events …`;
  `[R3] markdown golden exact …`; `[R3] JSON golden exact …`;
  `[O3 verified] golden exact …`; `[O3 unverified] golden exact …`;
  `[G4] reported: registered submodule (vendor/dep, clean, URL) +
  unregistered nested-repo`.
- Packages git/last/session green; gofmt + vet clean. R2+R3+O3+G4 GREEN (109 rows).

## 2026-10-11 — Slice Z: gap closure (I7 + O2 + O4 + Q3 + Q7 + U3)
- I7 message catalog: new internal/i18n (English foundation, 14 user-facing keys) with T/Tr/MustT/Has/Untranslated. Gate: MustT panics on an unregistered key (hard-coded literal); T tracks keys absent from the catalog; all registered keys resolve. Acceptance: [I7] catalog well-formed: 14 keys; gate caught unregistered literal; Tr formats. Existing TUI literals untouched (asserted by snapshots) and migrate incrementally against the gate.
- O2 trace export: new internal/trace (Document/Recorder/Export/Validate) on engine.Observe; schema_version mirrors U2; carries session_id + event_count + ordered events; rejects zero schema, count mismatch, missing type/timestamp. Acceptance: [O2] schema_version=1, event_count=4, round-trips, validates; malformed docs rejected.
- O4 boot-trace acceptance: cmd/nikicode/main_boot_o4_test.go TestBootTraceListsEveryTask reproduces the canonical DAG (boot/config/registry/startup/detect/program/session-run) in-process, asserts file exists, header present, every task listed, durations >= 0. Acceptance: [O4] trace file exists, header + 7 tasks present.
- Q3 perf budgets: new internal/perf (Budget/Check/Violation, 10% tolerance) + tools/perfbudget (measures --version p50 + binary size vs baseline.json) + perf-budget CI job. Acceptance: within tolerance clean; at boundary (=baseline+10%) clean; over absolute limit fails; non-slim build flagged as 17.1% regression.
- Q7 dogfood log: docs/finish/DOGFOOD.md with six dated NIKI-on-NIKI entries.
- U3 release notes: new internal/changelog (Entry/Changelog/FromCommits/ScanLog/RenderMarkdown) parsing conventional commits (breaking ! + oneline hash strip) into Added/Changed/Fixed/Removed/Other; golden testdata/changelog.golden. Acceptance: [U3] golden exact; ScanLog stripped hash prefix.
- Evidence: go build ./... ok; go vet clean on all new packages; gofmt -l clean; full go test ./... -count=1 green (0 FAIL, 0 panic) including cmd/nikicode, git, last, session, trace, changelog, perf, i18n, tui.
