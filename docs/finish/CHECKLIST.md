# NIKI Finish — Live Checklist (docs/finish/CHECKLIST.md)

Row definitions are verbatim from the mission pack (docs/FINISH_PACK.md).
Statuses: **GREEN** = acceptance test ran, output printed in this conversation;
**PARTIAL** = part covered by existing tests/code; **RED** = exists but fails the
acceptance criteria; **MISSING** = no code under any name; **OWNER-VERIFY** =
prepared by agent, owner runs the real-world step (exact steps recorded);
**DEFERRED** = explicitly deferred with reason (P1 only). Final column updates
as rows complete during P1/P2.

Phase 0 audit counts (2026-10-10, six read-only code audits, baseline
`go test ./... -count=1` printed green): GREEN 4 ·
PARTIAL 41 · RED 29 · MISSING 36
(includes the 10 T5 sub-rows). Total rows: 100 main + 10 T5 sub.

## I. Install and first run (anyone, any machine)

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| I1 | P0 | **GREEN** | One-line install for macOS (arm64, x64), Linux (x64, arm64), and Windows (PowerShell; WSL documented). Installer verifies a checksum before running anything, installs without sudo, is idempotent, and uninstalls cleanly. Accept: clean-container tests for each platform, printed. (OWNER runs the real download from the release.) | done — 7/7 tests printed pass in tools/installer (checksum-before-unpack, tamper+missing-manifest refusal, sudo trap never fired, idempotent reinstall, before/after uninstall comparison, darwin arch→artifact mapping via uname shim, install.ps1 static guarantees). Fixture: goreleaser-shaped file:// dir. OWNER-VERIFY remains: one real download from GitHub Releases. |
| I2 | P0 | **GREEN** | First-run flow detects, in order: an API key in the environment, a running local model server (Ollama on its default port), or neither. With neither, it offers a no-key demo that replays a recorded session, labelled as recorded. Accept: a test per branch, printed. | done — internal/firstrun (Detect/DetectAt/ProbeOllama/LoadRecordedDemo) + 4 printed tests (branch1 env-key with probe-never-called order check + no key-value leak; branch2 live httptest Ollama + real probe live/dead; branch3 offer + fixture labelled recorded) + cmd tests (nikicode demo replay, firstRunMessage per branch, cobra guard) + `nikicode demo` real output printed above; wired into buildProvider via NewUnconfiguredProviderWithMessage. |
| I3 | P0 | **GREEN** | `niki doctor` checks: install integrity, PATH shadowing, config validity, provider reachability, git, sandbox backend, terminal capabilities, proxy and CA settings, disk space. Every failure prints the exact fix. Accept: each check has a failing-fixture test. | done — new internal/doctor package: 9 checks, every non-OK result carries Fix (guard test enforces coverage), 11 failing-fixture tests + 1 nine-check ordering/coverage test printed; CLI doctor rewired (hardcoded ✓ lines removed), TUI /doctor rewired from canned all-green text to real checks (boot caps handed over via tui.BootCaps, no re-probe mid-session); real `nikicode doctor` output printed above — 2 honest fails (go-run temp exe shadowed by installed copy; no controlling TTY in this session) + 1 warn (no provider key). |
| I4 | P0 | **GREEN** | API keys are stored in the OS keychain when available, with environment variables as the fallback. Keys never appear in config files, logs, trajectories, or `config show`. Accept: a test scans written files for a planted key. | done — new internal/secrets (zalando/go-keyring backend, session fallback when keychain down, env = final fallback via ResolveAPIKey order session>keychain>env); SaveUserConfig strips api_key (hard floor); /connect writes keychain not file (message says which store); logx redacts via tools.Redact before disk; config show renders an allow-list (no api_key field); doctor warns on legacy on-disk api_key with migration fix. Acceptance printed: planted-key scan of all files under fresh HOME = 0 hits with keychain round-trip intact; config show output clean in both modes; log scan 0 hits + [REDACTED] present; resolve-order test. Trajectory/UI/export redaction sweep = X3 (same mission text, its own row). |
| I5 | P0 | **GREEN** | A local model path works fully offline. With no model available, the error says exactly what to start. Accept: an offline fixture run completes a task. | BUG B-01 FIXED (`buildProvider` ollama case). `NewOllamaProvider` = loopback `http://localhost:11434/v1`, no key, `StartHint` = "no local model available — start it with `ollama serve`, then pull the model: `ollama pull <model>` (after that it runs fully offline)"; connection errors + 404 model-missing both rewritten to that exact text. Tests printed: conn-refused hint, 404 hint, loopback stream tool-call (no auth header), engine offline fixture (2 round trips, read_file executed, turn completed), buildProvider wiring. Real runs: `nikicode exec` with missing model printed the exact hint (server up, 404); with `qwen2.5-coder:3b` local model exit 0 printed `OFFLINE_OK`. Full suite GO_TEST_EXIT=0 (43 ok). |
| I6 | P1 | **MISSING** | Update check is opt-in. An update verifies the new binary's checksum before replacing the old one, and never runs without a command. Accept: a tampered-download test is refused. | pending |
| I7 | P1 | **MISSING** | Non-English users: all user-facing strings go through a message catalog (English only at first). UTF-8 output everywhere; Windows console code page handled. Accept: a test fails on a hard-coded string in the UI layer. | pending |
## II. Core loop and control

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| L1 | P0 | **GREEN** | Streaming with cancellation at token boundaries. Accept: a fixture stream cancelled mid-token leaves a consistent session. | Acceptance `TestCancelAtTokenBoundaryLeavesConsistentSession` (engine): real `session.Store` (sqlite+jsonl) with the CLI's Observe→AppendEvent wiring, mock word-per-token stream, interrupt after token 3 of 15 → printed: exactly 1 turn_cancelled as LAST store event, 0 turn_completed, 5 whole-token deltas persisted (prefix of full answer), no torn assistant msg in context, follow-up turn completed (store ends turn_completed, 18 events). A3 foundation reused; fixture mock final sends hardened to ctx-aware (no leak window). engine/provider/session ok. |
| L2 | P0 | **GREEN** | Budgets: wall time, cost, and steps, each with a gauge. At 85% the run enters WRAP (finish the step, check, verdict). At 100% it stops and keeps the branch. Accept: a fixture run hitting each budget produces the correct state and verdict. | New `internal/budget` (gauge state machine, 85%→WRAP / 100%→stopped, sticky, safe-point only) + `[budgets]` config (defaults D-09: 10m/$2/100, 0 disables) + engine per-turn tracker (gauge events each step, WRAP/stopped runs check→verdict→ends run) + TUI status gauge + exec terminal handling (no hang). Acceptance printed: 6 fixture runs — wall wrap@0.90 / wall stopped@1.13 / cost wrap@0.90 / cost stopped@1.12 / steps wrap@0.90 / steps stopped@1.00, each verdict="fixture-verified", hit kind correct, tool steps kept in session, disabled budgets fire nothing. Real CLI: `exec` with 1ns wall budget → "[budget stopped] hit=wall_time ... verdict: no-verifier" exit 0; JSONL shows budget_gauge+budget_stopped payloads. Full suite GO_TEST_EXIT=0 (44 ok). |
| L3 | P0 | **GREEN** | A run ends as done only through the verifier (section V). A model's claim of completion is recorded but never ends the run by itself. Accept: a fixture where the model says "done" with a failing check ends not-verified. | New `internal/verifier` seam (Result verified/not-verified/inconclusive, `Passed()` only-verified, `Request{Prompt,Claim,Attempt}`, Func adapter; error ⇒ not-verified) + protocol `EventVerification`/`EventTurnNotVerified` + engine gate: claim recorded → Verify → verified ⇒ turn_completed; failing check ⇒ user-role feedback + continue (loop capped by `verifier.max_loops`, 0/unbounded per D-04, backstopped by L2/maxSteps); nil verifier or verifier error ⇒ immediate turn_not_verified (never a pass); loop exhaustion (maxSteps/doom) ends not-verified, not completed. Acceptance printed: `TestModelDoneClaimWithFailingCheckEndsNotVerified` — model says done, fixture check fails, 2 attempts, ends turn_not_verified (0 turn_completed), feedback in context+session; `TestVerifiedClaimEndsRunDone` — verification(verified)→turn_completed; `TestNoVerifierNeverMarksDone`. Real CLI: local Ollama `qwen2.5-coder:3b` reply "done" → printed `[not verified] completion claim recorded, but no verifier is configured (inconclusive is never a pass)`, EXEC_EXIT=1, JSONL has turn_not_verified. exec/TUI/ACP/Codex terminal handling added (no hang; --github-check conclusion failure). Full suite GO_TEST_EXIT=0 (45 ok). Interim: product runs end not-verified until V wires the real verifier (accepted in D-10). |
| L4 | P0 | **GREEN** | Steering: a message during a run becomes a steer at the next safe point, shown as an event. Accept: a fixture steer changes the next plan. | Dedicated protocol `EventSteer` emitted when the queued message is drained at the safe point (previously reusing EventTurnStarted — D-11); TUI renders `[steer]` history line (non-terminal); queue path (CmdSubmitPrompt while running → EventWarning) unchanged. Acceptance printed: `TestSteerChangesNextPlan` — deterministic mid-run fixture (blocking `plan_gate` tool, steer pushed while the run is mid-step, message-aware `planProvider`): steer event text ✓, plans=[B] vs control `TestSteerFixtureControlWithoutSteer` plans=[A], steer present in the provider request that chose the new plan, turn_completed. Full suite GO_TEST_EXIT=0 (45 ok). |
| L5 | P0 | **GREEN** | Esc pauses at a safe point (between tool calls). A write in progress completes atomically. Esc twice stops and keeps all edits. Accept: a test interrupts mid-write and checks the file is whole. | New `CmdPauseTurn` two-stage semantics (D-12): Esc #1 sets a pause flag + warning; run ends at the next safe point (step boundary or between tool calls — skipped calls get explicit tool messages) with EventTurnCancelled text "paused at a safe point (Esc)"; Esc #2 cancels ctx now (+ "stopping now (Esc)" warning). `CmdInterruptTurn`/`CmdCancelTurn` (Ctrl+C, ACP/Codex) stay immediate. Atomic writes: product write_file already tmp+rename; edit_file switched to tmp+rename; format.FormatFile now runs detached (context.WithoutCancel + 5s) so a cancelled turn can never kill a formatter mid-rewrite. TUI Esc → CmdPauseTurn with "pausing… (Esc again to stop)"; Ctrl+C unchanged. Acceptance printed: `TestInterruptMidWriteKeepsFileWhole` — interrupt lands while the product write_file is gated mid-dispatch (engine-confirmed via warning events, no race) → file2="content-two-WHOLE" whole, cancel="paused at a safe point (Esc)"; `TestEscTwiceStopsAndKeepsAllEdits` — Esc×2 → file1 kept, file2 whole, no turn_completed/not-verified. 3x repeat stable. Full suite GO_TEST_EXIT=0 (45 ok; one pre-existing PTY flake in TestProcessManagerAndProcessTools observed once under load — passes 3/3 standalone, unrelated). |
| L6 | P0 | **GREEN** | Retries with jittered backoff on provider errors; tools with side effects are not retried without a fresh approval. Accept: injected 429, 5xx, and truncated-stream fixtures. | Two retry layers (D-13): HTTP 429/5xx inside providers (`doWithBackoff` now uses `provider.JitteredBackoff(10ms, attempt, jitter)` — random in [0.5x, 1.5x) of base·2^attempt — replacing the deterministic doubling), and stream-level errors (truncated / connection reset / unexpected EOF / broken pipe, via `provider.IsRetryable`) at the engine step loop: max 3 attempts, base 50ms jittered, ctx.Err() checked BEFORE any retry (L1 cancelled semantics preserved), retry only while no delta was received this attempt (re-streaming after output would duplicate text — assistant/tool messages are only committed after a clean stream), each retry emits EventWarning "…retrying k/n (jittered backoff)". Side-effect tools are never re-dispatched by the engine: every dispatch comes from a fresh model response through the fresh per-dispatch gate (Perm.Allow + Hooks.FirePreTool) — locked by deny/approve/fail fixtures. Acceptance printed: provider — `TestOpenAIRetryUsesJitter` `[L6 429] attempts=2 jitterCalls=1 text="recovered"` (injected 429→200; 5xx covered by existing conformance), `TestJitteredBackoffBounds`; engine — `[L6 truncated] calls=3 gap1=38.3ms gap2=110.2ms warnings=2 completed`, `[L6 mid-stream] calls=1, no retry, partial emitted once`, `[L6 fail-fast] calls=1`; side effects — `[L6 denied] toolRuns=0 gateFires=2 toolStarted=2 modelTurns=3`, `[L6 approved] toolRuns=1 gateFires=1 completed`, `[L6 failed-side-effect] toolRuns=3 gateFires=3`. Full suite GO_TEST_EXIT=0 (45 ok); gofmt clean. |
| L7 | P1 | **PARTIAL** | Parallel tool calls when independent; calls that write the same file are serialized. Accept: a fixture with conflicting writes applies them in order. | pending |
| L8 | P1 | **GREEN** | Plan mode: read-only exploration produces a plan the user can edit before any write. Accept: a test shows no write tool is callable in plan mode. | pending |
## III. Context engineering

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| C1 | P0 | **PARTIAL** | Instruction files load in a hierarchy: global, then repository root, then the directory of the file being worked on, loaded lazily. Accept: a fixture with nested instructions shows the right set per directory. | pending |
| C2 | P0 | **PARTIAL** | Tool output is truncated head-and-tail with the full output stored on disk and a reference returned. Accept: a 10 MB output stays bounded; the full output is retrievable. | pending |
| C3 | P0 | **PARTIAL** | Agentic search first (grep, glob, read). Accept: the default toolset has no index dependency. | pending |
| C4 | P1 | **RED** | A repository map (symbol outline) under a token budget, built from the Go AST for Go and a fallback for other languages. Accept: the map respects its budget on a 10k-file fixture. | pending |
| C5 | P1 | **PARTIAL** | Prompt-cache-stable prefix: system prompt, tool schemas, and instructions come before volatile content. Accept: a golden test checks the prefix is byte-identical across turns. | pending |
| C6 | P1 | **RED** | Structured compaction at a context threshold: goal, decisions, open items, file list. The full history stays in the session log. Accept: a golden compaction test, and a test that the log is untouched. | pending |
| C7 | P1 | **PARTIAL** | @file and @symbol mentions in the composer. Accept: mention resolution test including gitignored paths. | pending |
| C8 | P1 | **RED** | A user-owned memory file, written only by explicit `remember`, shown and editable. Nothing is saved silently. Accept: a test that no memory write happens without the command. | pending |
| C9 | P0 | **RED** | Environment context (OS, shell, git state) is collected once and refreshed on change. Accept: a test with a changed branch refreshes it. | pending |
## IV. Editing, checkpoints, and recovery

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| E1 | P0 | **PARTIAL** | Edits apply by context-matched patch. Ambiguous matches are rejected, not guessed. A fuzzy fallback is allowed only when the match is unique, and its confidence is logged. Accept: an ambiguous-match fixture is rejected; property tests on the patch applier pass. | pending |
| E2 | P1 | **PARTIAL** | After each edit, the project's formatter, linter, or type check runs when configured; errors are returned to the model. Accept: a fixture with a type error is caught in the same turn. | pending |
| E3 | P0 | **RED** | Checkpoints before each turn (shadow refs in git, not the user's branches). `rewind` restores files and the conversation to a checkpoint. Accept: a rewind test restores bytes exactly, including file modes. | pending |
| E4 | P0 | **PARTIAL** | Undo the last agent change. Accept: test. | pending |
| E5 | P1 | **RED** | Diff review per hunk before commit. Accept: a test accepts one hunk and rejects another. | pending |
| E6 | P0 | **RED** | Binary and large files are not edited by text tools; line endings and encodings are preserved. Accept: CRLF and UTF-16 fixtures round-trip. | pending |
| E7 | P1 | **PARTIAL** | Commit drafting and PR-description drafting from the run's evidence. Nothing is pushed. Accept: a test that no push command is reachable. | pending |
## V. Verification (NIKI's core differentiator)

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| V1 | P0 | **MISSING** | Requirement extraction: the request becomes numbered, testable requirements (the contract). Accept: golden contracts for five fixture requests. | pending |
| V2 | P0 | **MISSING** | Check selection: checks come from the stack detector's allowlist or the user's config. Commands found in README files, comments, or tool output are never run. Accept: a planted README command is not executed. | pending |
| V3 | P0 | **MISSING** | Trust tiers T0 to T3 as defined in the verifier design. "Verified" requires T2 or higher. Inconclusive is never a pass. Accept: a test per tier. | pending |
| V4 | P0 | **MISSING** | Integrity: test files, CI files, lockfiles, and verifier commands are hashed before and after the run; changes are flagged. Accept: a planted test-deletion is flagged and blocks T2. | pending |
| V5 | P1 | **MISSING** | Flake handling: rerun k times; divergence makes the requirement inconclusive. Accept: a fixture flaky test is marked inconclusive. | pending |
| V6 | P1 | **MISSING** | Test-weakening detection: removed assertions, new skips, `|| true` in test commands are flagged. Accept: one fixture per pattern. | pending |
| V7 | P1 | **MISSING** | Verifier calibration: a seeded set of 40 or more cases across 4 or more stacks, with tamper and weakening cases never marked verified. Rates are generated into docs/verifier/TRUST_MODEL.md. Accept: the file exists and the tamper cases are zero-verified. | pending |
## VI. Tools, MCP, and extension

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| T1 | P0 | **PARTIAL** | A small core toolset: read, list, search, edit, patch, shell, plan, ask. Accept: tool count and schema sizes recorded; no tool duplicates another. | pending |
| T2 | P0 | **PARTIAL** | Tool schemas are strict and versioned. Descriptions are short, concrete, and tested against the small-model profile (section VIII). Accept: schema validation tests. | pending |
| T3 | P0 | **PARTIAL** | MCP client over stdio with per-server allow lists, timeouts, and status events. Tool descriptions are scanned for instruction-like injection text. Accept: fake MCP server tests, including a poisoned description that is flagged. | pending |
| T4 | P1 | **PARTIAL** | Skills as `SKILL.md` folders with optional scripts, discovered from user and project locations, loaded on demand. Accept: discovery test; body loads only when invoked. | pending |
| T5 | P0 | **MISSING** | Self-extension protocol (below). Default OFF per project. Accept: all ten sub-tests pass. | pending |
| T6 | P1 | **PARTIAL** | Hooks: session start and end, before and after tool use, prompt submit, stop. A hook can block a tool call. Hooks run only in trusted projects. Accept: a blocking hook test; an untrusted-project test. | pending |
| T7 | P1 | **MISSING** | Custom slash commands from Markdown with arguments. Accept: argument substitution test. | pending |
| T8 | P0 | **PARTIAL** | Tool results are typed: success, error, or timeout, each with a bounded body. Accept: schema test. | pending |
| T9 | P1 | **MISSING** | Local tool statistics (counts, failures) visible to the user through `niki stats`. Nothing leaves the machine. Accept: test. | pending |
### T5. Self-extension protocol (how a harness grows a new capability safely)
This is the riskiest capability in the harness. Generated code that enables itself is how harnesses get compromised, so every step is gated.
| T5.1 | P0 | **MISSING** | DETECT: a capability gap is logged with its reason: the user asked, three identical manual sequences occurred, or a failed task has a script-shaped fix. Accept: no silent detection; the reason is in the log. | pending |
| T5.2 | P0 | **MISSING** | DRAFT IN STAGING: the skill is written to `~/.niki/staging/<id>/`, never to a live directory. Accept: a hash of every live directory is unchanged after drafting. | pending |
| T5.3 | P0 | **MISSING** | TEST OFFLINE: the skill's own test runs in the sandbox with network off and writes only to its staging directory. Accept: a skill that tries to read `~/.ssh` or reach the network fails its test. | pending |
| T5.4 | P0 | **MISSING** | DECLARE PERMISSIONS: the frontmatter declares tools, paths, network (default none), and secrets (default none). Undeclared use is blocked at runtime. Accept: a runtime test blocks an undeclared network call. | pending |
| T5.5 | P0 | **MISSING** | PROPOSE: the TUI shows the SKILL.md, the script, the test result, the declared permissions, and any diff against an existing skill. Accept: rendering test; nothing enables without a gate. | pending |
| T5.6 | P0 | **MISSING** | GATE: scopes are once, session, project, and global. Any network, write-outside-workspace, or secret permission needs a second, explicit approval. Denial deletes the staging directory. Accept: denial leaves no files in live directories. | pending |
| T5.7 | P0 | **MISSING** | ENABLE WITH PROVENANCE: a provenance file records creator, session ID, prompt hash, timestamp, and content hash. `niki skills` lists it. Accept: editing the content changes the hash, disables the skill, and shows a warning. | pending |
| T5.8 | P0 | **MISSING** | AUDIT: each skill run is logged. Three consecutive errors auto-disable it with a notice. Accept: a test that the third error disables it. | pending |
| T5.9 | P0 | **MISSING** | RETIRE: `niki skills rm <name>` removes the directory and provenance with no residue. Accept: a before-and-after directory comparison. | pending |
| T5.10 | P0 | **MISSING** | DEFAULT OFF: `self_extend = "off" \| "propose"` in config; the default is "off". Accept: a default-config test. | pending |
## VII. Subagents and parallel work

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| S1 | P1 | **MISSING** | Role-scoped subagents: explorer (read-only), implementer (its own worktree), reviewer (read-only). Accept: each role's tool set is tested. | pending |
| S2 | P1 | **PARTIAL** | A subagent returns a bounded summary with evidence references, not its full transcript. Accept: size-bound test. | pending |
| S3 | P1 | **GREEN** | Concurrency cap on parallel subagents. Accept: a test with more tasks than the cap. | pending |
| S4 | P1 | **PARTIAL** | Subagents have their own budgets and timeouts. Accept: a timeout test. | pending |
| S5 | P1 | **PARTIAL** | Nesting is limited to depth 2. Accept: a test that depth 3 is refused. | pending |
## VIII. Small and local model support

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
These rows exist because smaller models fail in predictable ways: malformed tool arguments, forgetting constraints, repeating the same call, declaring success early, and degrading in long contexts. Each row is an engineering counter to one of those failures. None of them makes a small model as capable as a frontier model. Measure, and report the number honestly.
| M1 | P0 | **RED** | Tool-call repair: when arguments fail schema validation, the validator error and the schema excerpt go back to the model. Two retries, then a logged failure. Accept: a fixture with malformed arguments recovers on the second try. | pending |
| M2 | P1 | **MISSING** | Text-format tool calls (a JSON block or tagged text) are parsed only when the tool name and schema match exactly. Each recovery is logged. Accept: a fixture that tries a near-miss name is not executed. | pending |
| M3 | P0 | **PARTIAL** | Phase-gated tools: each turn exposes only the tool family for the current state. Accept: a test that edit tools are absent during PLAN. | pending |
| M4 | P1 | **RED** | Micro-step plans: each step has a done criterion and a check. A step cannot close without its check. Accept: a test. | pending |
| M5 | P1 | **RED** | Small-window budgets: the repository map, file reads, and tool output all have token limits sized to the model profile, and compaction triggers earlier for small windows. Accept: a golden test with an 8k window profile. | pending |
| M6 | P0 | **PARTIAL** | Repetition detection: an identical call twice triggers a reminder with a different strategy; three times stops and asks. Accept: a fixture loop is caught. | pending |
| M7 | P1 | **MISSING** | Decoding defaults per phase: low temperature for tool phases, explicit output limits, stop sequences set. Accept: a profile test. | pending |
| M8 | P1 | **MISSING** | Escalation: when a small model fails the same step twice, one step may be retried with a configured larger model, shown in the stream. Accept: a fixture escalation test. Off unless configured. | pending |
| M9 | P1 | **MISSING** | Small-model evaluation: a fixed suite of 20 tasks on fixture repos run with a small local model. The pass rate is stored in EVALS.md with the model, quantization, hardware, and date. The profile ships only at or above a threshold set in Phase 0. Accept: EVALS.md exists with the real run. (OWNER runs the model.) | pending |
## IX. Safety

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| X1 | P0 | **PARTIAL** | Default sandbox: writes only inside the workspace; network off for commands; home directory and credential paths hidden. Accept: escape tests on Linux and macOS. | pending |
| X2 | P0 | **PARTIAL** | Approval modes (untrusted, on-request, never), safest option focused, Esc denies, every decision logged. "Never" requires explicit confirmation and is never the default. Accept: focus and deny tests. | pending |
| X3 | P0 | **RED** | Secret redaction in logs, trajectories, UI, and exports. Accept: a planted-secret scan of all outputs. | pending |
| X4 | P0 | **PARTIAL** | Tool output is data, not instructions: text that asks for approval changes, network access, or disclosure is treated as untrusted content. Accept: a red-team fixture in which a README orders exfiltration; nothing leaves the sandbox. | pending |
| X5 | P1 | **MISSING** | Network policy: an allowlist for command network access, off by default. Accept: a test. | pending |
| X6 | P0 | **PARTIAL** | Project trust: an untrusted project cannot start MCP servers, hooks, or commands from its own config. Accept: test. | pending |
| X7 | P1 | **RED** | A local audit log of approvals and sandbox denials, rotated. Accept: test. | pending |
| X8 | P1 | **RED** | Red-team suite of 30 or more cases with expected outcomes, including a lifecycle-script trap, a symlink escape, and an instruction hidden in a filename. Accept: every case passes; results printed. | pending |
## X. Sessions and resumption

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| R1 | P0 | **PARTIAL** | Sessions are append-only JSONL, written atomically. `niki resume` restores after `kill -9` mid-turn. Accept: a crash test. | pending |
| R2 | P1 | **PARTIAL** | Fork a session at a checkpoint. Accept: test. | pending |
| R3 | P1 | **PARTIAL** | Export a session as Markdown and JSON. Accept: golden exports. | pending |
## XI. Observability and cost

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| O1 | P0 | **PARTIAL** | Live cost, tokens, time, and steps, visible during the run and in the final report. Accept: totals match a fixture's recorded usage. | pending |
| O2 | P1 | **MISSING** | Trace export of the run (JSON) for debugging. Accept: schema test. | pending |
| O3 | P1 | **MISSING** | `niki explain last`: a plain-language summary of the last run: what was asked, what changed, what passed, what looped and why, what is not verified. Accept: golden output for two fixture runs. | pending |
| O4 | P1 | **PARTIAL** | Boot trace (`NIKI_BOOT_TRACE=1`). Accept: the trace file exists and lists every boot task. | pending |
| O5 | P0 | **RED** | Logs are local, redacted, rotated, and bounded in size. No telemetry of any kind. Accept: a network-capture test during a full run shows zero outbound connections except to the configured provider. | pending |
## XII. Git and team workflow

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| G1 | P0 | **MISSING** | Each run works on its own branch. The user's checked-out branch is never modified. Accept: a test with a dirty user checkout. | pending |
| G2 | P1 | **GREEN** | Implementers work in a worktree. Accept: test. | pending |
| G3 | P0 | **PARTIAL** | No force push, no history rewrite, no push at all from autonomous runs. Accept: a test that no such command is reachable. | pending |
| G4 | P1 | **MISSING** | Submodules and nested repositories are detected and reported, not silently skipped. Accept: fixture test. | pending |
## XIII. Platform, terminal, accessibility

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| P1 | P0 | **PARTIAL** | Unicode width correct; `NO_COLOR`, ASCII fallback, and a plain mode for logs and screen readers. Accept: golden frames in each mode. | pending |
| P2 | P0 | **RED** | Works over SSH and inside tmux, with keys and resize behaving. Accept: PTY tests under tmux (or documented limits). | pending |
| P3 | P0 | **PARTIAL** | Terminal restored on exit, Ctrl+C, SIGTERM, SIGHUP, panic. Accept: PTY tests. | pending |
| P4 | P0 | **RED** | Windows: native or WSL. Native support is either tested or explicitly labelled unsupported in `niki doctor` and the README. Accept: the Windows CI job, or the documented label. | pending |
| P5 | P0 | **PARTIAL** | Build and test on macOS arm64 and x64, Linux x64 and arm64. Accept: CI matrix green, or owner-run results recorded. | pending |
| P6 | P1 | **MISSING** | Locale-aware time and number display; no layout breakage with long translated strings. Accept: a long-string golden. | pending |
## XIV. Help and onboarding

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| H1 | P0 | **RED** | `niki help` topics, and `niki examples` with runnable examples checked by the docs-snippets test. Accept: examples run against the fixture provider. | pending |
| H2 | P0 | **RED** | In-app help generated from the same registry as the keys. Accept: every advertised key dispatches. | pending |
| H3 | P1 | **PARTIAL** | `niki init` creates a commented config and an instructions file, and never overwrites an existing file. Accept: test. | pending |
## XV. Quality engineering

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| Q1 | P0 | **PARTIAL** | Fixture provider with recorded trajectories (golden). Every behavior change shows up as a trajectory diff. Accept: the diff is printed in review. | pending |
| Q2 | P0 | **MISSING** | Behavioral eval suite: at least 20 fixed tasks on fixture repos, each with an expected verdict. Accept: pass rate stored in EVALS.md. | pending |
| Q3 | P1 | **RED** | Performance budgets from the foundation pack, enforced in CI. Accept: a regression above 10 percent fails. | pending |
| Q4 | P0 | **GREEN** | Fuzzing for the patch parser, SSE parser, and JSON framing, with `go test -fuzz` for the recorded duration. Accept: no panic. | pending |
| Q5 | P0 | **PARTIAL** | Race detector and goroutine-leak tests on a full run. Accept: clean output. | pending |
| Q6 | P1 | **MISSING** | Property tests for the patch applier and the path guard. Accept: properties pass. | pending |
| Q7 | P1 | **MISSING** | Dogfood log: real tasks done with NIKI on NIKI, what hurt, what was fixed. Accept: docs/finish/DOGFOOD.md has at least five entries. | pending |
## XVI. Updates and compatibility

| Row | Tier | Phase 0 audit | Evidence / test name | Final |
|---|---|---|---|---|
| U1 | P0 | **RED** | Configuration migrations with a dry-run mode and a backup. Accept: a migration test from each prior version. | pending |
| U2 | P0 | **RED** | Sessions and JSON event output are schema-versioned; breaking changes bump the version. Accept: schema tests. | pending |
| U3 | P1 | **PARTIAL** | Release notes generated from conventional commits. Accept: golden output. | pending |
