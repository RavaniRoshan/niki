# NIKI Finish Pack (archived mission text)

Archived verbatim from the owner's mission prompt on 2026-10-10 (Phase 0 approval).
Re-read this file, docs/finish/PROGRESS.md, docs/finish/CHECKLIST.md, and
docs/finish/DECISIONS.md at the start of each phase and after any /compact.

---

<role>
You are the lead engineer finishing NIKI, a personal coding-agent harness in Go. Repo root = current directory. Read AGENTS.md and docs/FINISH_PACK.md first. The codebase already exists from the foundation work; verify every item below against the code, not against notes. Verify every external fact against its official documentation or pinned source.
</role>

<mission>
Finish NIKI so that: (1) anyone on macOS, Linux, or Windows (native or WSL) can install it in under five minutes and reach a first answer; (2) it works with small local models, not only frontier APIs, by adding scaffolding that compensates for the known weaknesses of smaller models; (3) it finishes tasks honestly: "done" requires evidence; (4) it can extend itself safely, by proposing new skills that the user approves; (5) it is safe to run on a stranger's repository, with defaults that protect the user without configuration.
Do not add features outside this checklist. Do not change the architecture. Do not implement P2 items.
RULE OF PROOF: a row is GREEN only when its acceptance test runs and its real output is printed in this conversation. A row without a test is not done. Anything needing a real API key, a real model, a real terminal, or my judgment is OWNER-VERIFY with exact steps.
NO INVENTED CAPABILITIES: NIKI must not claim parity with any other tool. Parity claims are made only from measured evals, stored in docs/finish/EVALS.md.
</mission>

<tiers>
P0: must be GREEN before any P1 row starts.
P1: SOTA parity; starts only when every P0 row is GREEN.
Rows marked (OWNER) need owner action; the agent prepares everything up to that point.
</tiers>

<checklist>
<checklist>

## I. Install and first run (anyone, any machine)
I1 P0 One-line install for macOS (arm64, x64), Linux (x64, arm64), and Windows (PowerShell; WSL documented). Installer verifies a checksum before running anything, installs without sudo, is idempotent, and uninstalls cleanly. Accept: clean-container tests for each platform, printed. (OWNER runs the real download from the release.)
I2 P0 First-run flow detects, in order: an API key in the environment, a running local model server (Ollama on its default port), or neither. With neither, it offers a no-key demo that replays a recorded session, labelled as recorded. Accept: a test per branch, printed.
I3 P0 `niki doctor` checks: install integrity, PATH shadowing, config validity, provider reachability, git, sandbox backend, terminal capabilities, proxy and CA settings, disk space. Every failure prints the exact fix. Accept: each check has a failing-fixture test.
I4 P0 API keys are stored in the OS keychain when available, with environment variables as the fallback. Keys never appear in config files, logs, trajectories, or `config show`. Accept: a test scans written files for a planted key.
I5 P0 A local model path works fully offline. With no model available, the error says exactly what to start. Accept: an offline fixture run completes a task.
I6 P1 Update check is opt-in. An update verifies the new binary's checksum before replacing the old one, and never runs without a command. Accept: a tampered-download test is refused.
I7 P1 Non-English users: all user-facing strings go through a message catalog (English only at first). UTF-8 output everywhere; Windows console code page handled. Accept: a test fails on a hard-coded string in the UI layer.

## II. Core loop and control
L1 P0 Streaming with cancellation at token boundaries. Accept: a fixture stream cancelled mid-token leaves a consistent session.
L2 P0 Budgets: wall time, cost, and steps, each with a gauge. At 85% the run enters WRAP (finish the step, check, verdict). At 100% it stops and keeps the branch. Accept: a fixture run hitting each budget produces the correct state and verdict.
L3 P0 A run ends as done only through the verifier (section V). A model's claim of completion is recorded but never ends the run by itself. Accept: a fixture where the model says "done" with a failing check ends not-verified.
L4 P0 Steering: a message during a run becomes a steer at the next safe point, shown as an event. Accept: a fixture steer changes the next plan.
L5 P0 Esc pauses at a safe point (between tool calls). A write in progress completes atomically. Esc twice stops and keeps all edits. Accept: a test interrupts mid-write and checks the file is whole.
L6 P0 Retries with jittered backoff on provider errors; tools with side effects are not retried without a fresh approval. Accept: injected 429, 5xx, and truncated-stream fixtures.
L7 P1 Parallel tool calls when independent; calls that write the same file are serialized. Accept: a fixture with conflicting writes applies them in order.
L8 P1 Plan mode: read-only exploration produces a plan the user can edit before any write. Accept: a test shows no write tool is callable in plan mode.

## III. Context engineering
C1 P0 Instruction files load in a hierarchy: global, then repository root, then the directory of the file being worked on, loaded lazily. Accept: a fixture with nested instructions shows the right set per directory.
C2 P0 Tool output is truncated head-and-tail with the full output stored on disk and a reference returned. Accept: a 10 MB output stays bounded; the full output is retrievable.
C3 P0 Agentic search first (grep, glob, read). Accept: the default toolset has no index dependency.
C4 P1 A repository map (symbol outline) under a token budget, built from the Go AST for Go and a fallback for other languages. Accept: the map respects its budget on a 10k-file fixture.
C5 P1 Prompt-cache-stable prefix: system prompt, tool schemas, and instructions come before volatile content. Accept: a golden test checks the prefix is byte-identical across turns.
C6 P1 Structured compaction at a context threshold: goal, decisions, open items, file list. The full history stays in the session log. Accept: a golden compaction test, and a test that the log is untouched.
C7 P1 @file and @symbol mentions in the composer. Accept: mention resolution test including gitignored paths.
C8 P1 A user-owned memory file, written only by explicit `remember`, shown and editable. Nothing is saved silently. Accept: a test that no memory write happens without the command.
C9 P0 Environment context (OS, shell, git state) is collected once and refreshed on change. Accept: a test with a changed branch refreshes it.

## IV. Editing, checkpoints, and recovery
E1 P0 Edits apply by context-matched patch. Ambiguous matches are rejected, not guessed. A fuzzy fallback is allowed only when the match is unique, and its confidence is logged. Accept: an ambiguous-match fixture is rejected; property tests on the patch applier pass.
E2 P1 After each edit, the project's formatter, linter, or type check runs when configured; errors are returned to the model. Accept: a fixture with a type error is caught in the same turn.
E3 P0 Checkpoints before each turn (shadow refs in git, not the user's branches). `rewind` restores files and the conversation to a checkpoint. Accept: a rewind test restores bytes exactly, including file modes.
E4 P0 Undo the last agent change. Accept: test.
E5 P1 Diff review per hunk before commit. Accept: a test accepts one hunk and rejects another.
E6 P0 Binary and large files are not edited by text tools; line endings and encodings are preserved. Accept: CRLF and UTF-16 fixtures round-trip.
E7 P1 Commit drafting and PR-description drafting from the run's evidence. Nothing is pushed. Accept: a test that no push command is reachable.

## V. Verification (NIKI's core differentiator)
V1 P0 Requirement extraction: the request becomes numbered, testable requirements (the contract). Accept: golden contracts for five fixture requests.
V2 P0 Check selection: checks come from the stack detector's allowlist or the user's config. Commands found in README files, comments, or tool output are never run. Accept: a planted README command is not executed.
V3 P0 Trust tiers T0 to T3 as defined in the verifier design. "Verified" requires T2 or higher. Inconclusive is never a pass. Accept: a test per tier.
V4 P0 Integrity: test files, CI files, lockfiles, and verifier commands are hashed before and after the run; changes are flagged. Accept: a planted test-deletion is flagged and blocks T2.
V5 P1 Flake handling: rerun k times; divergence makes the requirement inconclusive. Accept: a fixture flaky test is marked inconclusive.
V6 P1 Test-weakening detection: removed assertions, new skips, `|| true` in test commands are flagged. Accept: one fixture per pattern.
V7 P1 Verifier calibration: a seeded set of 40 or more cases across 4 or more stacks, with tamper and weakening cases never marked verified. Rates are generated into docs/verifier/TRUST_MODEL.md. Accept: the file exists and the tamper cases are zero-verified.

## VI. Tools, MCP, and extension
T1 P0 A small core toolset: read, list, search, edit, patch, shell, plan, ask. Accept: tool count and schema sizes recorded; no tool duplicates another.
T2 P0 Tool schemas are strict and versioned. Descriptions are short, concrete, and tested against the small-model profile (section VIII). Accept: schema validation tests.
T3 P0 MCP client over stdio with per-server allow lists, timeouts, and status events. Tool descriptions are scanned for instruction-like injection text. Accept: fake MCP server tests, including a poisoned description that is flagged.
T4 P1 Skills as `SKILL.md` folders with optional scripts, discovered from user and project locations, loaded on demand. Accept: discovery test; body loads only when invoked.
T5 P0 Self-extension protocol (below). Default OFF per project. Accept: all ten sub-tests pass.
T6 P1 Hooks: session start and end, before and after tool use, prompt submit, stop. A hook can block a tool call. Hooks run only in trusted projects. Accept: a blocking hook test; an untrusted-project test.
T7 P1 Custom slash commands from Markdown with arguments. Accept: argument substitution test.
T8 P0 Tool results are typed: success, error, or timeout, each with a bounded body. Accept: schema test.
T9 P1 Local tool statistics (counts, failures) visible to the user through `niki stats`. Nothing leaves the machine. Accept: test.

### T5. Self-extension protocol (how a harness grows a new capability safely)
This is the riskiest capability in the harness. Generated code that enables itself is how harnesses get compromised, so every step is gated.
T5.1 DETECT: a capability gap is logged with its reason: the user asked, three identical manual sequences occurred, or a failed task has a script-shaped fix. Accept: no silent detection; the reason is in the log.
T5.2 DRAFT IN STAGING: the skill is written to `~/.niki/staging/<id>/`, never to a live directory. Accept: a hash of every live directory is unchanged after drafting.
T5.3 TEST OFFLINE: the skill's own test runs in the sandbox with network off and writes only to its staging directory. Accept: a skill that tries to read `~/.ssh` or reach the network fails its test.
T5.4 DECLARE PERMISSIONS: the frontmatter declares tools, paths, network (default none), and secrets (default none). Undeclared use is blocked at runtime. Accept: a runtime test blocks an undeclared network call.
T5.5 PROPOSE: the TUI shows the SKILL.md, the script, the test result, the declared permissions, and any diff against an existing skill. Accept: rendering test; nothing enables without a gate.
T5.6 GATE: scopes are once, session, project, and global. Any network, write-outside-workspace, or secret permission needs a second, explicit approval. Denial deletes the staging directory. Accept: denial leaves no files in live directories.
T5.7 ENABLE WITH PROVENANCE: a provenance file records creator, session ID, prompt hash, timestamp, and content hash. `niki skills` lists it. Accept: editing the content changes the hash, disables the skill, and shows a warning.
T5.8 AUDIT: each skill run is logged. Three consecutive errors auto-disable it with a notice. Accept: a test that the third error disables it.
T5.9 RETIRE: `niki skills rm <name>` removes the directory and provenance with no residue. Accept: a before-and-after directory comparison.
T5.10 DEFAULT OFF: `self_extend = "off" | "propose"` in config; the default is "off". Accept: a default-config test.

## VII. Subagents and parallel work
S1 P1 Role-scoped subagents: explorer (read-only), implementer (its own worktree), reviewer (read-only). Accept: each role's tool set is tested.
S2 P1 A subagent returns a bounded summary with evidence references, not its full transcript. Accept: size-bound test.
S3 P1 Concurrency cap on parallel subagents. Accept: a test with more tasks than the cap.
S4 P1 Subagents have their own budgets and timeouts. Accept: a timeout test.
S5 P1 Nesting is limited to depth 2. Accept: a test that depth 3 is refused.

## VIII. Small and local model support
These rows exist because smaller models fail in predictable ways: malformed tool arguments, forgetting constraints, repeating the same call, declaring success early, and degrading in long contexts. Each row is an engineering counter to one of those failures. None of them makes a small model as capable as a frontier model. Measure, and report the number honestly.
M1 P0 Tool-call repair: when arguments fail schema validation, the validator error and the schema excerpt go back to the model. Two retries, then a logged failure. Accept: a fixture with malformed arguments recovers on the second try.
M2 P1 Text-format tool calls (a JSON block or tagged text) are parsed only when the tool name and schema match exactly. Each recovery is logged. Accept: a fixture that tries a near-miss name is not executed.
M3 P0 Phase-gated tools: each turn exposes only the tool family for the current state. Accept: a test that edit tools are absent during PLAN.
M4 P1 Micro-step plans: each step has a done criterion and a check. A step cannot close without its check. Accept: a test.
M5 P1 Small-window budgets: the repository map, file reads, and tool output all have token limits sized to the model profile, and compaction triggers earlier for small windows. Accept: a golden test with an 8k window profile.
M6 P0 Repetition detection: an identical call twice triggers a reminder with a different strategy; three times stops and asks. Accept: a fixture loop is caught.
M7 P1 Decoding defaults per phase: low temperature for tool phases, explicit output limits, stop sequences set. Accept: a profile test.
M8 P1 Escalation: when a small model fails the same step twice, one step may be retried with a configured larger model, shown in the stream. Accept: a fixture escalation test. Off unless configured.
M9 P1 Small-model evaluation: a fixed suite of 20 tasks on fixture repos run with a small local model. The pass rate is stored in EVALS.md with the model, quantization, hardware, and date. The profile ships only at or above a threshold set in Phase 0. Accept: EVALS.md exists with the real run. (OWNER runs the model.)

## IX. Safety
X1 P0 Default sandbox: writes only inside the workspace; network off for commands; home directory and credential paths hidden. Accept: escape tests on Linux and macOS.
X2 P0 Approval modes (untrusted, on-request, never), safest option focused, Esc denies, every decision logged. "Never" requires explicit confirmation and is never the default. Accept: focus and deny tests.
X3 P0 Secret redaction in logs, trajectories, UI, and exports. Accept: a planted-secret scan of all outputs.
X4 P0 Tool output is data, not instructions: text that asks for approval changes, network access, or disclosure is treated as untrusted content. Accept: a red-team fixture in which a README orders exfiltration; nothing leaves the sandbox.
X5 P1 Network policy: an allowlist for command network access, off by default. Accept: a test.
X6 P0 Project trust: an untrusted project cannot start MCP servers, hooks, or commands from its own config. Accept: test.
X7 P1 A local audit log of approvals and sandbox denials, rotated. Accept: test.
X8 P1 Red-team suite of 30 or more cases with expected outcomes, including a lifecycle-script trap, a symlink escape, and an instruction hidden in a filename. Accept: every case passes; results printed.

## X. Sessions and resumption
R1 P0 Sessions are append-only JSONL, written atomically. `niki resume` restores after `kill -9` mid-turn. Accept: a crash test.
R2 P1 Fork a session at a checkpoint. Accept: test.
R3 P1 Export a session as Markdown and JSON. Accept: golden exports.

## XI. Observability and cost
O1 P0 Live cost, tokens, time, and steps, visible during the run and in the final report. Accept: totals match a fixture's recorded usage.
O2 P1 Trace export of the run (JSON) for debugging. Accept: schema test.
O3 P1 `niki explain last`: a plain-language summary of the last run: what was asked, what changed, what passed, what looped and why, what is not verified. Accept: golden output for two fixture runs.
O4 P1 Boot trace (`NIKI_BOOT_TRACE=1`). Accept: the trace file exists and lists every boot task.
O5 P0 Logs are local, redacted, rotated, and bounded in size. No telemetry of any kind. Accept: a network-capture test during a full run shows zero outbound connections except to the configured provider.

## XII. Git and team workflow
G1 P0 Each run works on its own branch. The user's checked-out branch is never modified. Accept: a test with a dirty user checkout.
G2 P1 Implementers work in a worktree. Accept: test.
G3 P0 No force push, no history rewrite, no push at all from autonomous runs. Accept: a test that no such command is reachable.
G4 P1 Submodules and nested repositories are detected and reported, not silently skipped. Accept: fixture test.

## XIII. Platform, terminal, accessibility
P1 P0 Unicode width correct; `NO_COLOR`, ASCII fallback, and a plain mode for logs and screen readers. Accept: golden frames in each mode.
P2 P0 Works over SSH and inside tmux, with keys and resize behaving. Accept: PTY tests under tmux (or documented limits).
P3 P0 Terminal restored on exit, Ctrl+C, SIGTERM, SIGHUP, panic. Accept: PTY tests.
P4 P0 Windows: native or WSL. Native support is either tested or explicitly labelled unsupported in `niki doctor` and the README. Accept: the Windows CI job, or the documented label.
P5 P0 Build and test on macOS arm64 and x64, Linux x64 and arm64. Accept: CI matrix green, or owner-run results recorded.
P6 P1 Locale-aware time and number display; no layout breakage with long translated strings. Accept: a long-string golden.

## XIV. Help and onboarding
H1 P0 `niki help` topics, and `niki examples` with runnable examples checked by the docs-snippets test. Accept: examples run against the fixture provider.
H2 P0 In-app help generated from the same registry as the keys. Accept: every advertised key dispatches.
H3 P1 `niki init` creates a commented config and an instructions file, and never overwrites an existing file. Accept: test.

## XV. Quality engineering
Q1 P0 Fixture provider with recorded trajectories (golden). Every behavior change shows up as a trajectory diff. Accept: the diff is printed in review.
Q2 P0 Behavioral eval suite: at least 20 fixed tasks on fixture repos, each with an expected verdict. Accept: pass rate stored in EVALS.md.
Q3 P1 Performance budgets from the foundation pack, enforced in CI. Accept: a regression above 10 percent fails.
Q4 P0 Fuzzing for the patch parser, SSE parser, and JSON framing, with `go test -fuzz` for the recorded duration. Accept: no panic.
Q5 P0 Race detector and goroutine-leak tests on a full run. Accept: clean output.
Q6 P1 Property tests for the patch applier and the path guard. Accept: properties pass.
Q7 P1 Dogfood log: real tasks done with NIKI on NIKI, what hurt, what was fixed. Accept: docs/finish/DOGFOOD.md has at least five entries.

## XVI. Updates and compatibility
U1 P0 Configuration migrations with a dry-run mode and a backup. Accept: a migration test from each prior version.
U2 P0 Sessions and JSON event output are schema-versioned; breaking changes bump the version. Accept: schema tests.
U3 P1 Release notes generated from conventional commits. Accept: golden output.

</checklist>

<decisions_to_make_in_phase_0>
Propose a default and a reason for each; the owner confirms:
1. The small-model pass-rate threshold for M9, and which small model and quantization to test.
2. Whether self-extension ships "off" (recommended) or "propose" by default.
3. The approval default (recommended: on-request).
4. The verifier's default maximum loops (recommended: 4).
5. Which P4 platform claims are tested versus labelled unsupported.
6. The license, the repository owner and name, and the release channels to test in I1.
</decisions_to_make_in_phase_0>
</checklist>

<engineering_workflow>
Work in this order for every row: (1) write the acceptance test first; (2) run it and show it failing; (3) implement the smallest change that passes; (4) run the test and the package tests; (5) update CHECKLIST.md with the test name and the printed result; (6) log any decision in DECISIONS.md with the rejected alternatives. Keep slices small: one row or a tight group of related rows per slice, each with a demo command in PROGRESS.md.
</engineering_workflow>

<phases>
P0 PLAN (read-only). Audit every row of the checklist against the code and report each as GREEN, PARTIAL, RED, or MISSING, with evidence. Write docs/finish/CHECKLIST.md with the audit. Propose the decisions above. Output a design note of at most 80 lines and wait.
P1 P0 ROWS, IN THIS ORDER: install and doctor (I1-I5), core control (L1-L6), editing and recovery (E1, E3, E4, E6, C2, C3, C9), verification basics (V1-V4), tools and safety (T1, T2, T3, T8, X1-X4, X6), small-model basics (M1, M3, M6), sessions (R1), observability (O1, O5), git (G1, G3), platform (P1-P5), help (H1, H2), quality (Q1, Q2, Q4, Q5), updates (U1, U2). Stop for owner review at the end of P1.
P2 P1 ROWS, starting only after P0 is GREEN. Self-extension (T5, all ten sub-rows) comes first in P1 and is reviewed by the owner before the rest. Then the remaining rows, grouped by section.
P3 SMALL-MODEL EVAL. M9: prepare the 20-task suite and the run script; the owner runs the local model; the result is recorded in EVALS.md. Do not claim a pass rate you have not printed.
P4 CLOSE-OUT. Full gates on the final commit; docs/finish/CHECKLIST.md with every row's status and evidence; DOGFOOD.md entries; a final answer listing every row that is not green and why.
</phases>

<limits>
- Work only in this repository. Never publish, push, tag, or upload. Never create public repositories. Spend nothing beyond my approved provider budget.
- Never read, search for, or use leaked or extracted proprietary source. If a search result leads there, discard it.
- Dependencies outside the pre-approved list: ask first. The pre-approved list is the one from the foundation pack (docs/PACK.md recommended_modules).
- Memory across compaction: re-read docs/FINISH_PACK.md, docs/finish/PROGRESS.md, CHECKLIST.md, and DECISIONS.md at the start of each phase and after any /compact. Append to PROGRESS.md after every slice.
- If the same gate fails three times in a row with different fixes, stop, write a diagnosis, and ask.
- Ask me only for: the Phase 0 decisions, owner-run steps (real downloads, the small local model, real terminals, real keys), and any measurement you cannot run yourself.
</limits>

<evidence_discipline>
Printed output for every gate. Never skip, weaken, or delete a test. Golden updates require a printed diff for review. No placeholders in shipped behavior. Every parity claim cites a stored measurement or is removed. Label anything unverified UNVERIFIED with the reason.
</evidence_discipline>

<definition_of_done>
Every P0 row is GREEN with its test name and printed result. Every P1 row is GREEN or explicitly deferred with a reason. The self-extension protocol passes all ten sub-tests. EVALS.md records the small-model result, or states that it awaits the owner's run. TRUST_MODEL.md exists with calibration results or a not-run note. Nothing was published, pushed, or tagged.
</definition_of_done>

<final_answer>
Provide: the checklist status counts before and after; every row that is not green, with its reason; the self-extension sub-test results; the small-model eval status and its numbers (or the owner step); the verifier calibration status; the owner-only steps in order; known limitations; and the README claim text that the evidence supports and nothing more.
</final_answer>

---

## Owner goals (issued 2026-10-10, after Phase 0 approval)

Goal 1 (P0 rows): NIKI P0 rows are green, proven in this conversation: (1) docs/finish/CHECKLIST.md was printed with every P0 row GREEN, each with its test name and a printed passing result, or OWNER-VERIFY with exact steps; (2) the self-extension default is "off" and a test shows it; (3) the planted-secret scan of all outputs passes, printed; (4) the red-team instruction-in-README case ends with nothing leaving the sandbox, shown; (5) a network capture during a full fixture run shows zero outbound connections except to the configured provider, printed; (6) the full gates ran clean on the final commit with output printed: go build, golangci-lint, go vet, the full test suite with the race detector, and the fuzz targets for the recorded duration; (7) git diff was reviewed and it is stated that no test was skipped, weakened, or deleted, and no P1 row was started; (8) git status is clean. OR stop for owner review at the end of P1 and print what must be decided.

Goal 2 (self-extension + P1 rows): NIKI self-extension and P1 parity rows are done, proven in this conversation: (1) all ten T5 sub-tests pass, printed, including the test that a denied proposal leaves no files in any live directory and the test that editing a skill's content disables it; (2) the sandbox test shows a generated skill cannot read the home directory or reach the network; (3) every P1 row is GREEN with its test name and printed result, or explicitly deferred in CHECKLIST.md with a reason; (4) the small-model profile rows (M1-M8) have fixture tests passing, shown; (5) the verifier tamper and weakening fixtures are never marked verified, shown; (6) the full gates ran clean on the final commit with output printed; (7) git diff was reviewed and it is stated that no test was skipped, weakened, or deleted and no P2 item was added; (8) git status is clean. OR stop for owner review of the self-extension design and print what must be decided.

Goal 3 (close-out): NIKI finish close-out is complete, proven in this conversation: (1) the 20-task small-model suite and its run script exist, and docs/finish/EVALS.md records the owner's run with model, quantization, hardware, date, and pass rate, or states plainly that it awaits the owner's run and no pass rate is claimed; (2) the behavioral eval suite of 20 or more tasks ran with its pass rate printed; (3) clean-container install tests for each supported platform passed, printed, or are listed as OWNER-VERIFY with exact commands; (4) docs/finish/DOGFOOD.md has at least five real entries; (5) TRUST_MODEL.md exists with calibration results or a not-run note; (6) every row in CHECKLIST.md is GREEN, DEFERRED with a reason, or OWNER-VERIFY with steps; (7) the final answer lists every non-green row and why; (8) no parity claim exists in any file without a stored measurement; (9) git diff was reviewed and no test was weakened; (10) git status is clean. OR stop and print the owner-only steps in order.

---

## Confirmed Phase 0 decisions (owner, 2026-10-10)

1. M9: profile ships at >=50% (10/20 tasks); test model qwen2.5-coder:7b-instruct Q4_K_M via local Ollama; hardware/date recorded in EVALS.md.
2. self_extend default: "off".
3. Approval default: on-request (safest focused, Esc denies); "never" requires an explicit confirmation gate and is never the default.
4. Verifier default max loops: UNBOUNDED, backstopped by L2 budgets (matches Claude Code and Codex, both of which default to no turn cap; verified from code.claude.com/docs agent-loop reference and openai/codex PR #214 "default unbounded"). Optional config cap allowed.
5. P4: drop Windows only for v1 — Linux x64/arm64 + macOS arm64/x64 stay in scope; native Windows labelled unsupported in doctor and README; WSL documented. No Windows CI job for v1.
6. License MIT; owner Ravani Roshan / github.com/RavaniRoshan/niki; I1 release channels: GitHub Releases (goreleaser checksums.txt) + install.sh + new install.ps1; clean-container tests run against a local file:// release fixture; owner runs the one real download.
