# Convex Audit — NIKI repository (2026-09-18)

Source of truth: local tree + `https://github.com/RavaniRoshan/niki`. Verifier flags carried at bottom.

## Structure

Single-crate binary `niki` v0.7.0 (edition 2024, MSRV 1.85). `src/main.rs → cli::<cmd>::handle()`; 20+ subcommands; no-subcommand → Chat TUI. ~30 modules: `cli/ orchestrator/ agents/ sandbox/ llm/ runtime/ config/ output/ artifacts/ persistence/ mission/ session/ activity/ event/ observability/ safety/ risk/ cost/ audit/ acp/`.

## Call paths

`niki run`: `cli/run.rs::handle` → `NikiConfig::load` → runtime connect (skip worktree/dry-run) → `Task{Uuid}` → `TaskRecord::save_to_disk` → `safety::snapshot` → `pipeline::execute_pipeline` → `artifacts/*.json` → dashboard → `changes.patch` → red gate → apply diff (worktree) → `output::git::create_branch_and_commit` (`niki/<8hex>`) → `safety::prove(strict)` → `report.md` → provenance/reflect/OTLP.
`niki plan`: `cli/plan.rs` → `run::handle(dry_run=true)` Planner-only + `plan.md`; `--plan <id>` loads `planner.json` as `TaskSpec`.

## Orchestration / stages

`ensure_planner(resolve_stages)` + `apply_risk_stages` (explicit stages never rewritten). Default Planner→Coder→Tester→Reviewer + SecurityAuditor/Synthesizer/Red/Critic = 8 roles. Prompts `prompts/<role>.md` + schemas `schemas/*.schema.json` baked via `include_dir!` (rebuild required). Artifacts TaskSpec→CodeDiff(SEARCH/REPLACE)→TestReport(oracle_source)→ReviewVerdict(Approved|RevisionNeeded|Rejected)→Synthesis/SecurityVerdict/RedChallenge/Critique. Retry 3× + 2× JSON-repair + strict validation. Isolation = session/artifact-level; sequential stages share one sandbox. Loop on `RevisionNeeded` ≤3 rounds; Critic once post-loop.

## Sandbox / Git

Trait `Sandbox{ensure_tools,apply_patch,get_diff,exec,destroy}`. Docker: bollard, `niki-sandbox:24.04`, CapDrop ALL, pids 512, net none, `readonly_rootfs=false` default, `ensure_tools git,node,npm,python3`. Worktree: `git worktree add --force .niki-worktrees/<task>`, local exec, no isolation. Git local-only (git2 vendored + CLI); branch `niki/<8hex>`, diff-only staging, strips `.niki`/`niki.toml`, empty-diff no-op, `NIKI <niki@localhost>`; red gate blocks unless `--force`; `safety_proof.json` strict.

## State / persistence

Per-run `<output>/tasks/<UUID>/{task.json,artifacts/,test_execution.json,changes.patch,report.md,dashboard.html,trace.jsonl(derived),manifest.json(best-effort),safety_proof.json,plan.md,context.json}`. `TaskStatus{Running|Completed|Failed|Cancelled}` + `StageMetric` tokens/cost/latency/TTFT; context budget 200k. Separate local JSON: `.niki/missions/`, `.niki/sessions/` (checkpoints/rewind), goal store, `.niki/audit/*.jsonl` + `niki audit` bundle + fail-closed HookBus. No queue/daemon/scheduler/remote plane (in-process Tokio + broadcast 1024 + signals 130/143→Cancelled).

## Config / providers / telemetry / tests

`niki.toml` (project over `~/.config/niki/`) + env override + model aliases; keyring `niki auth`; redaction. Providers: Anthropic/OpenAI/Google/Ollama/OpenRouter/Zen/Kimi/Kilo/NVIDIA/Groq/Together/DeepSeek + mock + failover (120s, 429/5xx jitter). Logging `tracing EnvFilter` + `[SPAN]` + OTLP. CI: fmt --check, clippy warning-free, test verbose, release, deny/audit, mock_llm.py:8080 worktree e2e, VHS 12 frames. Risk deterministic; reflection Planner-only; ACP stdio JSON-RPC same pipeline.

## Control-plane-like pieces (all in-process)

EventBus domain events, 12-state AgentState, ToolRegistry+TaskStore, missions/sessions/goal/provenance/audit. Nothing remote. Roadmap Later: cloud execution beta, living memory, marketplace, Architect, Enterprise. GitHub: 0 open issues (restricted), 10 Dependabot PRs, 2★/0 forks.

## Does Convex fit?

Fits only as optional metadata mirror (runs/stages/events/artifacts-meta/projects/memberships/heartbeats/rollups) for dashboard/history/teams/scheduling. Does not fit execution, sandbox, Git, LLM calls, logs, secrets, safety proofs.

## Verifier-carried limits

README-vs-code isolation/tree-mutation/rootfs contradictions unresolved; mid-pipeline loop body truncated in one read; Tester-exec host-vs-sandbox path unconfirmed; `[session]/[compaction]/[mcp]` wiring truth open; absence-of-daemon unproven by search (confirm via `rg`); GitHub counts instant-stale; `PRICE_TABLE 2026-09-06` re-fetch at build.
