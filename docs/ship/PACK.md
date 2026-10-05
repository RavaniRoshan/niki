# NIKI — Ship + Perform Pack (for Kimi Code)

**What this does:** takes the NIKI foundation you already built (Rust engine + TypeScript/Ink shell over one JSON-RPC protocol) from "UI looks good" to **a product a stranger can install and run end to end**, and aims it at the one thing you want to be able to write in the README: **a measured, reproducible performance claim on a harness evaluation.**

Two tracks run in one pack:

- **R (Ready):** wire everything end to end, then ship a release with working install URLs.
- **P (Performance):** build the evaluation rig first, then add only the harness levers that the numbers justify.

Nothing is published, pushed, tagged, or submitted by the agent. It prepares everything and stops for you at every external action.

---

## Decision record: how NIKI can win an evaluation

I checked the current state of the leaderboards and rules before choosing. What matters:

- Terminal-Bench is now a family: 2.0 and 2.1 (the top of 2.1 is saturated), 3.0, and 4.0 (66 tasks, five trials each, long agent timeouts). Boards differ in **which harness they use**. Vals runs every model through the same neutral harness (Mini-SWE-agent). The official tbench board pairs an agent with a model.
- **4.0 is out of reach on a student budget.** Vals shows roughly $13-$17 per test for frontier models, and one board lists about $1.26k for a single full evaluation of a cheap model. So the target is **Terminal-Bench 2.x with a cheap open-weight model**, where headroom exists.
- The official leaderboard has hard rules: every task at least 5 times, timeout multiplier 1.0, no timeout or resource overrides, an ATIF trajectory for every passing trial, and an agent judge over passing trials. Cheating (task-specific information, stored solutions, edited timeouts) means takedown.
- Harbor is the official harness for Terminal-Bench 2.0. It runs arbitrary agents, has a built-in `terminus-2`, and a custom agent is an "installed agent" that installs your CLI into each task container.
- Evidence from LangChain's harness work: the same model went from 52.8% to 66.5% on Terminal-Bench 2.0 by changing only the harness. The levers that mattered were self-verification, loop detection, a reasoning-effort schedule, and environment onboarding. A minimal harness (pi: four tools, under 1,000 tokens of system prompt) is competitive on 2.0. So bigger is not better. **Measured** is better.

### Claim ladder (what you can honestly write)

| Claim | Realism | Verdict |
|---|---|---|
| "#1 on the official leaderboard" | Needs a frontier model, a large budget, and lab-grade tuning | No |
| **"Best open-source harness at a matched model"**: NIKI vs Mini-SWE-agent, Terminus-2, and other open harnesses, same model, paired confidence interval | Achievable | **Headline** |
| **"Most solved tasks per dollar"** | Achievable; verification plus budget control help here | **Headline** |
| "Best harness averaged across 3 models" | Strong but 3x the cost | Stretch |
| "Listed on the official leaderboard with verified trajectories" | Rules are strict but doable with a cheap model | Stretch (you submit) |

### Levers, ranked by evidence per effort

| # | Lever | Evidence | Effort |
|---|---|---|---|
| L1 | Completion gate and self-verification (the harness decides "done", with an evidence ledger) | Most impactful in LangChain's work | Medium |
| L2 | Wall-clock and token budget manager (leave time to verify, wrap up before the timeout) | Timeouts are a top failure | Low |
| L3 | Loop guard (repeated edits or failing commands force a strategy change) | LangChain | Low |
| L4 | Generic environment onboarding (probe, never task-specific) | LangChain | Low |
| L5 | Persistent PTY tool (interactive and long-running commands, screen snapshots) | Terminus-2 shows the value | Medium |
| L6 | Edit robustness plus an instant check after each edit | pi and others | Low |
| L7 | Context management (output offloading, compaction, typed task state) | General | Medium |
| L8 | Reasoning-effort schedule ("sandwich": high for planning and verifying, medium for doing) | LangChain | Low |
| L9 | Parallel attempts with self-verification as the selector (hard tasks only) | Expensive | High |
| L10 | Per-model harness profiles tuned on a dev split | Deep Agents ships profiles | Medium |

**My recommendation:** build L0 (a minimal strong baseline) and the rig, then L1-L4 and L8 first (cheap, evidence-backed), then L5-L7, then L10 as the differentiator. L9 only if the budget allows. **Rejected, permanently:** anything task-specific, peeking at tests, network lookups of the benchmark, timeout edits.

---

## Setup (once)

```bash
cd <niki-repo>
mkdir -p docs/ship
cp niki-ship-and-perform-mega-prompt.md docs/ship/PACK.md      # the prompt body below lives here so it survives context compaction
git checkout -b niki/ship
rustc --version && cargo --version && (bun --version || node --version) && docker --version
uv tool install harbor            # Terminal-Bench's official harness
```

Create `AGENTS.md` (or append) with these lines so Kimi re-reads the pack after compaction:

```
# NIKI ship + perform
- Read docs/ship/PACK.md completely at the start of every session and after any /compact.
- Memory lives in docs/ship/CHECKLIST.md, PROGRESS.md, DESIGN.md, BUDGET.md. Re-read them before each phase.
- Never publish, tag, push, or submit anything. Prepare it and stop for the owner.
```

## Launch (Kimi Code)

```bash
kimi --plan                       # Phase 0 runs in plan mode; you approve before any edit
```

Then paste the kickoff line: **"Read docs/ship/PACK.md completely, then run Phase 0."**

After you approve the plan:

- Switch to `/auto` for unattended runs. (`/yolo` skips approvals but Kimi asks before starting goals in YOLO mode; `/auto` is meant for unattended work.)
- Start a goal. `/goal` is experimental: launch with `KIMI_CODE_EXPERIMENTAL_GOAL_COMMAND=1 kimi`, or enable it under `/experiments`.
- **Fallback if `/goal` is unavailable:** Ralph Loop, `kimi --max-ralph-iterations 20`, which loops until the agent outputs `<choice>STOP</choice>`. The pack tells the agent to emit that only when a gate list is fully proven.
- Use `/compact` if context gets heavy. The pack is built to survive it.
- Claude Code users: `claude --permission-mode plan --effort max`, same pack, same goals.

Four goals, because each run is long and each condition must be checkable: **A** audit and wiring, **B** eval rig, baselines, and levers, **C** release pipeline dry run, **D** final evaluation, README claim, and launch kit. The goal texts are at the bottom.

---

## The prompt

````
<role>
You are the lead engineer and release engineer for NIKI. Repo root = current directory. Read AGENTS.md, docs/foundation/** (DESIGN, CHECKLIST, GAPS, PROGRESS), and follow its low-RAM test rules, security rules, and architecture patterns. NIKI today: a Rust engine (harness: orchestration, sandbox, tools, artifacts, permissions, verifier, session, evals) and a TypeScript/React/Ink shell, joined by one typed JSON-RPC protocol (niki-protocol). Paths and states in this prompt come from notes. VERIFY everything by running it. Do not create a second protocol, a second event loop, or a second state store.
</role>

<mission>
Two tracks, one product, in this order of dependency:
TRACK R, READY: make NIKI work end to end on a clean machine with a real model, and ship a release whose install URLs work.
TRACK P, PERFORM: build an evaluation rig that can be trusted, then improve NIKI with harness levers that the numbers justify, and make the result reproducible and honestly claimable in the README.

NIKI is an open-source side project. There is no commercial goal. The goal is a measured, reproducible, honest performance claim. Claude Code or another harness may still beat NIKI. That is acceptable. A false or unreproducible claim is not.

RULE OF PROOF: a row is WORKS only when an automated test that drives the real code passes, or a measured probe meets its number, with the exact command and real output printed. Anything needing a real model key, a real terminal, an external account, or human taste is OWNER-VERIFY with exact steps. Label anything unverified UNVERIFIED with the reason; unverified is never reported as done.

NO INVENTION: no number, claim, badge, or comparison in the README or docs may be typed by hand. They are generated by script from stored result files, or they are not written. The UI and docs never show data the engine did not report.

INTEGRITY (non-negotiable; a violation voids every result): see integrity_rules.

OWNERSHIP OF EXTERNAL ACTIONS: you never publish, tag, push, upload, submit, create public repositories, or spend money without a stated OK from me in this conversation. You prepare everything (dry runs, drafts, scripts) and stop.
</mission>

<claim_policy>
The only performance claims allowed in README, docs, release notes, or posts are produced by `niki bench report` from a stored results file, and always carry: benchmark and version, task count, trials per task, model, harness baselines, date, NIKI commit, paired 95% confidence interval, and a link to the trajectories. Allowed shapes:
  "On Terminal-Bench <ver> with <model>: NIKI <X>% vs Mini-SWE-agent <Y>% and Terminus-2 <Z>% (paired 95% CI [a, b], <N> trials/task, <date>, <commit>)."
  "Solved tasks per dollar: NIKI <A> vs <B>."
Forbidden: "best ever", "state of the art" without a named benchmark, model, and baselines, any rank we have not been listed at, any cross-model or cross-benchmark comparison that is not matched.
If NIKI does not beat a baseline, the report says so. A null result is published as a null result.
</claim_policy>

<integrity_rules>
Terminal-Bench's official leaderboard requires: every task at least 5 times; timeout multiplier 1.0; no timeout or resource overrides; a result and an ATIF trajectory for every trial; an agent judge over passing trials; cheating (task-specific information given to the agent, stored solutions, edited timeouts) is a takedown. Therefore:
1. NIKI contains NO task-specific knowledge: no task names, ids, paths, expected outputs, or hints in source, prompts, profiles, or fixtures. A CI "contamination" test greps the engine, prompts, and profiles for the benchmark's task identifiers and canary strings and fails on any hit.
2. NIKI never reads the tests or solution directories, never browses the benchmark's website or repository, and has no network tool during evaluation unless the benchmark allows it. Use Harbor's network policy options where available.
3. No timeout, resource, or config overrides in evaluation runs. Multiplier 1.0.
4. Generic onboarding only: the environment probe is built in memory from fixed generic commands. NIKI does NOT write helper or instruction files into the task workspace for its own context (one competitor was flagged for creating an AGENTS.md at start).
5. Every trial writes a valid ATIF trajectory (python -m harbor.utils.trajectory_validator). Passing trials must pass validation and must read as honest: no hidden reward hacking, no tampering with the verifier, no editing tests to pass.
6. Tuning uses a DEV split only. A SEALED split is run at milestones; each sealed run is logged in bench/SEALED_LOG.md and there are at most three before release. Any score on tuned tasks is labelled "tuned on".
7. Cost honesty: every report includes total cost and cost per solved task from real usage, not estimates.
</integrity_rules>

<eval_protocol>
Rig: Harbor (official harness of Terminal-Bench). Read its docs and the existing installed agents (for example the Claude Code agent file) to write NIKI's adapter. Verify every API detail by reading the installed Harbor source. Never rely on memory.
- Benchmark: Terminal-Bench 2.x, whichever 2.x version currently has an OFFICIAL leaderboard in Harbor Hub (verify with harbor datasets list). Terminal-Bench 4.0 is out of budget scope. Use it only if I explicitly approve a cost.
- Model: ONE fixed cheap open-weight model for all headline comparisons, reached through an OpenAI-compatible endpoint. Candidates: the ones I have access to (I use Kimi Code), plus any other cheap open-weight model with a good API. ASK ME the monthly budget and the model choice in Phase 0. Verify price and availability on the day.
- Baselines, same model, same dataset, same trials: Mini-SWE-agent (the neutral minimal baseline that Vals uses), Terminus-2 (built into Harbor), and every other open-source agent that Harbor supports and that can use the model (list them with harbor run --help; install what works within budget). Closed products (Claude Code, Codex) only appear with their own models, labelled "different model, not matched".
- Adapter: NIKI is an installed agent. The install step puts a pinned NIKI engine into the task container. The Rust engine builds as a single static musl binary, so NO Node, Python, or runtime may be required inside task containers. The run step calls the headless engine. It writes trajectory.json (ATIF), reports token and cost usage, honors max-time and max-cost, and exits with a clear status. The shell is never involved.
- Metrics: resolve rate (mean over trials), cost per solved task, wall time per task, false-done rate (the agent emitted a final answer but the verifier failed), tokens, loop-guard triggers, and per-task paired differences vs each baseline. Statistics: paired bootstrap over tasks (10,000 resamples), 95% CI; a difference is "real" only if the CI excludes 0. Report pass-rate variance across trials.
- Budget gates: `niki bench` requires --budget-usd and refuses to start a run whose estimate exceeds it. The estimate comes from a measured pilot (10 tasks x 1 trial per candidate), never from guesses. Record spend per run in docs/ship/BUDGET.md. Never exceed the approved budget. Ask before any run over USD 25 (adjust to my stated budget).
- Splits: freeze a seeded DEV/SEALED split of the task list now and commit its hash. Typical: about one third DEV, two thirds SEALED. Full 5-trial all-task runs are only for the final report and the leaderboard package.
- Generalization check: one pilot-size run of NIKI vs Mini-SWE-agent on a DIFFERENT Harbor dataset (cheapest available, for example SWE-bench Verified Mini or Aider Polyglot; confirm in harbor datasets list) to show the tuning did not just fit one benchmark.
- Leaderboard package (prepared, never submitted): run with 5 trials per task and no overrides, upload per Harbor's process, write metadata, validate against the leaderboard rules locally (harbor leaderboard submit validates client-side). I submit.
</eval_protocol>

<levers>
Every lever is a middleware or stage in the engine, behind a flag, with a metric, a kill criterion, and an ablation entry. Add a lever only if the sealed-paired result supports it. Order of work:
L0 Baseline: a minimal strong loop with few tools (read, write, edit, bash, grep, glob), a short system prompt, structured streaming, cost accounting. Run it first. It is the control for every ablation.
L1 Completion gate: the model cannot end the run directly. A final step extracts the task's explicit requirements into a checklist, runs real checks (existing tests, builds, linters, or small generated check scripts) and records an evidence ledger. If a requirement lacks evidence, the run continues. A separate verifier call (fresh context: task statement, checklist, evidence only) may veto. Metric: false-done rate and resolve rate. Kill: if resolve rate drops or cost per solve rises without a resolve gain.
L2 Budget manager: inject remaining wall time and tokens; at about 85% of the time budget force a wrap-up and verify step; per-command timeouts; long commands run in the background and are polled. Never override the benchmark's own timeouts.
L3 Loop guard: detect the same failing command, the same file edited N times with no new evidence, or oscillating diffs; escalate in three steps: a targeted reflection, a forced replan, then an effort bump. Log every trigger.
L4 Generic onboarding: a fixed in-memory environment probe (OS, user, cwd listing, toolchain versions, git state, free disk, network availability, running services). No task-specific content. No files written.
L5 Persistent PTY tool: interactive and long-running sessions; send keys; wait for a pattern; screen snapshots through a terminal emulator; background jobs; output head/tail with search; very large output offloaded to a file with a reference.
L6 Edit robustness: unique-match edit with fuzzy fallback and diff validation; after each edit run a fast check (formatter, type check, or syntax check) if the project has one and feed errors back immediately.
L7 Context: tool-output budgets; compaction at a threshold; a typed task state (requirements, decisions, failed attempts, verified facts) kept outside the transcript and rendered into the prompt at checkpoints; keep a stable prompt prefix for caching.
L8 Effort schedule: high effort for planning and verification, lower for execution; escalate when stuck; configurable per model profile.
L9 Parallel attempts (OFF by default): for tasks flagged hard by loop-guard history, run N attempts and pick by the L1 evidence ledger. Only if the budget allows.
L10 Per-model profiles: a profile is data {system prompt variant, tool description variant, edit format, effort schedule, truncation limits, loop thresholds}. Tune on DEV with a bounded search (bandit or small evolutionary loop). Freeze. Evaluate on SEALED. Ship profiles as data files.
TRACE-DRIVEN LOOP (the process): after each DEV run, cluster failed trajectories by cause (timeout, premature done, wrong tool use, environment issue, loop, misunderstood requirement). Propose one patch per dominant cause. Implement, ablate, keep or kill. Record each decision in docs/ship/EXPERIMENTS.md with the numbers.
</levers>

<phase_0_plan_first>
Read-only (read-only measurements allowed). Then output a design note of at most 80 lines and WAIT for my approval:
1. Audit of the foundation: run every gate from docs/foundation/CHECKLIST.md and report the REAL state (WORKS, PARTIAL, BROKEN, MISSING) with evidence. Do not trust the notes.
2. The wiring gaps (see wiring_checklist) that block a clean-machine end-to-end run.
3. The Harbor adapter design and the evaluation cost plan: pilot design, estimated per-task cost for 2-3 candidate cheap models, and a proposed budget. ASK ME: monthly budget, which model(s), and whether I accept TB 2.x only.
4. The release design: packaging of engine + shell, installer approach, npm approach, and the Bun-compile spike result for the shell (see release_checklist). Include the owner-only items you will need.
5. Lever order and the ablation plan.
6. Everything you need from me.
Save the approved note as docs/ship/DESIGN.md in the first commit.
</phase_0_plan_first>

<phases>
P1 WIRING. Close every wiring_checklist gap with tests. Provide a real-model smoke script (scripts/smoke_real_model.sh) that I run with my key; CI uses a deterministic fixture provider.
P2 RIG. niki bench: Harbor adapter (installed agent), ATIF export with validator, budget gates, pilot, paired statistics, report generator, contamination test, frozen dev/sealed split, SEALED_LOG. Run the L0 baseline and the baselines on a PILOT (10 tasks x 1 trial). Report numbers and cost in docs/ship/BUDGET.md.
P3 LEVERS. In the order above. For each: flag, implement, ablate on DEV (paired), keep or kill, record in EXPERIMENTS.md. After the DEV-driven set stabilizes, one SEALED run (logged). Then L10 profiles, one SEALED run (logged).
P4 RELEASE. Build the pipeline (release_checklist). Everything works as a DRY RUN: a workflow_dispatch release rehearsal that builds all artifacts into a DRAFT release, installers tested in clean containers. Nothing public happens without my OK.
P5 FINAL EVALUATION AND README. The final full run (5 trials, all tasks, no overrides) for NIKI and the matched baselines within the approved budget; paired statistics; ATIF validation; the generalization check; the leaderboard package (not submitted). Generate the README results section with `niki bench report`. Write the honest limitations.
P6 LAUNCH KIT. A launch checklist for me: exact order of publish steps, rollback plan, the post text with only generated numbers, the leaderboard submission steps.
</phases>

<wiring_checklist>
Priority P0 must pass, P1 must pass, P2 = recommend only. Proof: [T] test, [P] PTY or process test, [M] measured, [C] clean-container test, [O] owner-verify.
W1 P0 [C][O] Clean machine to first answer: install, run niki, pick a provider, enter a key (stored in the OS keychain, or via env var), ask a question, get a streamed answer. Friendly errors for a missing key or provider, never a stack trace.
W2 P0 [T] Config layering (project, user, env) validated against a schema, with the source of every value shown; safe migration between versions.
W3 P0 [T] Sessions persist, resume, and export (markdown and ATIF); crash-safe.
W4 P0 [T] Sandbox: worktree and container backends both work through one trait; the pipeline never knows which. Safe defaults.
W5 P0 [T] Approvals are real: the safest option is focused by default, Esc denies, every decision is logged, and a no-review mode needs explicit confirmation and is never the default.
W6 P0 [T] Headless: niki run with JSON events, exit codes, --atif-out, --max-time, --max-cost, stdin prompts, and a non-interactive approval policy; requires no shell and no TTY.
W7 P0 [T] Errors and recovery: provider outages, rate limits, partial streams, and network drops retry with backoff, then fail with a clear message; the session survives.
W8 P0 [T] Security: secrets redacted in logs and trajectories; path-traversal guard; command policy; no telemetry; no network calls the user did not ask for; dependency audits clean.
W9 P0 [P] Terminal behavior still passes the foundation TUI gates after all changes.
W10 P0 [M] Product speed: first frame, memory, and idle CPU stay within the foundation budgets.
W11 P1 [T] MCP bridge lists and calls an external server's tools end to end.
W12 P1 [T] niki doctor validates config, provider reachability, git, sandbox backend, and terminal capabilities, with exact fixes.
W13 P1 [T] Uninstall removes every file the installer created and leaves user data unless asked.
W14 P1 [T] Self-update is opt-in only and verifies checksums; it never runs by default.
W15 P0 [T] A full pipeline run (planner, coder, tester, reviewer, verifier) on a fixture repo produces a reviewable branch and a machine-checked verdict.
W16 P1 [C] Linux x64 and arm64, macOS x64 and arm64, and Windows x64 each have a smoke test in CI; unsupported combinations are documented, not silently broken.
W17 P1 Skills: SKILL.md-compatible skill loading (the format Claude Code and Kimi Code use) so existing skills work. P2 if it threatens scope.
</wiring_checklist>

<release_checklist>
R1 P0 Versioning: semver, a changelog generated from conventional commits, tag-driven release workflow. Tagging is mine.
R2 P0 CI on every PR: fmt, clippy, tests (nextest), shell lint and tests, cargo-deny and audit, npm audit, pinned GitHub Actions by commit hash, least-privilege tokens.
R3 P0 Build matrix: engine as a static musl binary on Linux (x64, arm64), macOS (x64, arm64), Windows (x64 MSVC). Shell as a standalone executable per target using Bun's compile feature. SPIKE FIRST: prove the compiled Ink shell runs on a clean ubuntu and a clean debian-slim container (and check alpine). If it fails, fall back and explain: a JS bundle that requires Node 20+, or Node SEA. Record the decision.
R4 P0 Archives per target: niki-<version>-<target>.tar.gz (zip on Windows) containing both binaries, LICENSE, NOTICE. SHA256SUMS. GitHub artifact attestations (provenance). An SBOM.
R5 P0 Installers: install.sh (POSIX sh) and install.ps1. They verify the checksum, install to a user directory with no sudo, are idempotent, support NIKI_VERSION pinning, print PATH instructions, and have a matching uninstall. Short and auditable. cargo-dist (current release 0.32.0, produces shell, PowerShell, Homebrew, and npm installers and attestations) is a candidate for the engine-only path. Decide in the plan whether it fits a two-binary bundle; if not, use your own scripts.
R6 P0 Stable install URLs that work with no domain: GitHub Releases latest/download and a GitHub Pages copy of the installers. List every URL in docs/INSTALL.md and test each in a clean container. A custom domain is an owner-only option.
R7 P1 npm package: a thin launcher with per-platform optional dependencies carrying the binaries (no postinstall scripts), published with provenance via trusted publishing. Check the package name is free. Publishing is owner-only.
R8 P1 Homebrew tap formula, updated automatically by the release workflow. I create the tap repo and token.
R9 P1 Docker image (GHCR) with the static engine for headless and benchmark use.
R10 P2 winget and scoop manifests. crates.io publication only if useful.
R11 P0 Docs: README (what it is, install, quickstart, measured results section generated from stored results, limitations), docs/ (config, protocol, architecture, evaluation, FAQ), CONTRIBUTING, SECURITY.md, issue templates, CODE_OF_CONDUCT. An honest comparison section with only generated numbers.
R12 P0 Release rehearsal: workflow_dispatch builds everything into a DRAFT release; then installers are tested against the draft assets in clean containers and on macOS and Windows runners. A publish step exists but requires my manual approval.
R13 P1 Rollback plan: yank or delete steps for each channel, documented.
R14 P2 [O] macOS notarization and Windows code signing need paid certificates. Document Gatekeeper and SmartScreen behavior. The curl installer avoids the quarantine flag; say so in the docs.
</release_checklist>

<recommended_crates>
Pre-approved for the engine, subject to the dependency admission check (license, maintenance, size, no install scripts or surprising native build steps; verify against crates.io and the source before adopting): portable-pty and vt100 (PTY tool and screen snapshots), ignore and grep-regex plus grep-searcher (fast search; the crates behind ripgrep), similar (diffs), rustix or nix (process groups and timeouts), tokio (process and timers), tracing, serde, tiktoken-rs only if provider-reported usage is missing. Dev tools: cargo-nextest, cargo-deny, cargo-audit, cargo-dist if chosen. Anything else: ask me. Never add a dependency solely for animation.
</recommended_crates>

<owner_boundary>
Things only I can do. Request each at the moment it blocks, with exact steps: provide and fund API keys and approve the evaluation budget; create or approve public GitHub settings, secrets, Pages, and the Homebrew tap repo; configure npm trusted publishing and the package name; push tags; publish the release; upload results to Harbor Hub and submit to the leaderboard; buy a domain; obtain signing certificates; run OWNER-VERIFY steps in real terminals; judge taste.
</owner_boundary>

<limits>
- Write only in the repo (engine, shell, bench, docs, workflows, scripts, tests). Touch nothing outside it. Never modify the benchmark, Harbor, or any baseline's code.
- NEVER publish, tag, push, upload, submit, or spend beyond the approved budget. Dry runs only. Ask first.
- Follow AGENTS.md low-RAM rules: focused tests, serial, never the whole suite locally; PTY tests with -j 2 and --test-threads=1 and timeouts. Evaluation runs are the exception that needs the budget gate, and they run through Docker as Harbor requires.
- Memory across compaction: at the start of each phase and after any /compact RE-READ docs/ship/PACK.md, CHECKLIST.md, PROGRESS.md, DESIGN.md, BUDGET.md, EXPERIMENTS.md. Append to PROGRESS.md after every step and print what was completed.
- Ask me ONLY for: the Phase 0 decisions, budget and model, owner-only actions, runtime dependencies outside the pre-approved list, safety-critical default changes, and any metric you cannot infer or measure. Everything else: choose conservatively and record it.
- If the same gate fails 3 times in a row with different fixes, stop, write a diagnosis, and ask.
</limits>

<evidence_discipline>
- A claim of "works", "faster", or "better" needs the exact command and real output, or the stored paired result with its confidence interval, printed in this conversation.
- NEVER skip, ignore, delete, or weaken a test. Updating a golden snapshot after an intentional, reviewed change is allowed; ignoring a failure is not.
- No placeholders, no dead commands, no hand-typed numbers. Fix root causes. Check a crate's or package's source before using its API.
- Report null results. Never p-hack: the number of variants tried and every sealed run are logged.
</evidence_discipline>

<definition_of_done>
docs/ship/CHECKLIST.md has zero P0 and P1 rows in BROKEN, MISSING, or PARTIAL. A clean-container end-to-end run passes. A dry-run release exists as a draft with tested installers. The rig produces paired, validated, budget-honest results for NIKI and matched baselines, and the README results section is generated from them (or states plainly that NIKI did not beat a baseline). Contamination, ATIF-validation, secret-redaction, and gate suites pass. Every owner-only item is listed with exact steps. Nothing external has happened without my OK.
</definition_of_done>

<final_answer>
Provide: audit before and after; the results table (generated) with CIs, cost, and baselines; the experiments log with kept and killed levers; changed files and why; exact commands and results; the dry-run release evidence; INSTALL.md URL test results; the owner-only checklist in order; known limitations; the README claim text that the data supports and nothing more.
</final_answer>

Begin with Phase 0 now. Do not edit anything until I approve the design note.
````

---

## Goal texts (use `/goal` after approving the plan and switching to `/auto`)

**Goal A — Audit and wiring (P1)**

```
/goal NIKI is end-to-end wired, proven in this conversation: (1) the Phase 0 audit ran every foundation gate and docs/ship/CHECKLIST.md was printed with real states and evidence; (2) every P0 wiring row W1-W10 and W15 is WORKS with a named test or probe and a passing run, or OWNER-VERIFY with exact steps (W1 needs my key: scripts/smoke_real_model.sh exists and a fixture-provider equivalent passes in CI); (3) a clean-container run from install to first streamed answer passed, output printed; (4) niki run headless works with no TTY and no shell, shown by test; (5) the AGENTS.md gates, fmt, clippy, cargo-deny, and the shell's lint and tests were run clean on the final commit with output printed; (6) git diff was reviewed and it is stated that no test was skipped, ignored, deleted, or weakened; (7) docs/ship/PROGRESS.md is current and git status is clean. OR stop if only owner-required items remain, and print them with their consequences.
```

**Goal B — Rig, baselines, levers (P2-P3)**

```
/goal NIKI evaluation rig and levers are done, proven in this conversation: (1) the Harbor adapter runs NIKI as an installed agent from a static musl binary with no runtime inside task containers, shown by a Harbor pilot run with output printed; (2) every trial writes ATIF that passes python -m harbor.utils.trajectory_validator, shown; (3) the contamination test passes (no task identifiers or canary strings in engine, prompts, or profiles) and the frozen DEV/SEALED split hash is committed; (4) niki bench enforces --budget-usd and docs/ship/BUDGET.md shows every run's spend within my approved budget; (5) the pilot (10 tasks x 1 trial) results for L0, Mini-SWE-agent, Terminus-2, and the other feasible matched baselines were printed with cost; (6) for each lever L1-L8 docs/ship/EXPERIMENTS.md shows a paired DEV ablation with 95% CI and a keep or kill decision, plus the SEALED runs logged in bench/SEALED_LOG.md (at most three); (7) resolve rate, cost per solved task, and false-done rate are reported for the kept configuration versus L0; (8) the AGENTS.md gates, fmt, clippy, and tests were run clean with output printed; (9) git diff was reviewed and it is stated that no test was weakened and no task-specific content exists; (10) git status is clean. OR stop if only owner-required items (keys, budget, approvals) remain, and print them.
```

**Goal C — Release pipeline dry run (P4)**

```
/goal NIKI release pipeline is ready and rehearsed with nothing published, proven in this conversation: (1) the Bun-compile spike result for the Ink shell on clean ubuntu and debian-slim containers (and alpine checked) is printed and the packaging decision is recorded; (2) a workflow_dispatch rehearsal built engine and shell artifacts for all five targets into a DRAFT release, with SHA256SUMS, attestations, and an SBOM, workflow run output printed; (3) install.sh and install.ps1 were tested against the draft assets in clean containers and on macOS and Windows runners, including pinned versions, idempotent reinstall, and uninstall, output printed; (4) docs/INSTALL.md lists every install URL and each URL was tested; (5) CI runs fmt, clippy, tests, cargo-deny, audits, with actions pinned by hash and least-privilege tokens; (6) the README, docs, CONTRIBUTING, SECURITY.md, and issue templates exist and contain no hand-typed performance numbers; (7) the rollback plan is documented; (8) the publish step requires my manual approval and nothing was tagged, pushed, published, or uploaded; (9) git diff was reviewed and git status is clean. OR stop if only owner-required items (npm trusted publishing, Homebrew tap repo, Pages settings, signing certificates) remain, and print them with exact steps.
```

**Goal D — Final evaluation, README claim, launch kit (P5-P6)**

```
/goal NIKI final evaluation and launch kit are complete, proven in this conversation: (1) the final run (5 trials per task, all tasks, no timeout or resource overrides, multiplier 1.0) completed for NIKI and the matched baselines within the approved budget, with spend printed from docs/ship/BUDGET.md; (2) niki bench report produced the paired results with 95% CIs, cost per solved task, and false-done rate, printed; (3) every passing trial's ATIF passes the validator, shown; (4) the generalization check on a different Harbor dataset was run and printed; (5) the README results section was GENERATED from the stored results (or states plainly that NIKI did not beat a baseline) and a test fails if any performance number in README is not traceable to the results file; (6) the leaderboard package passes Harbor's client-side validation and was NOT submitted; (7) docs/ship/LAUNCH.md lists the exact owner-only publish steps in order with rollback; (8) the gates were run clean on the final commit with output printed; (9) git diff was reviewed and it is stated that no test was weakened and no task-specific content exists; (10) git status is clean. OR stop if only owner-required items remain, and print them with their consequences.
```
