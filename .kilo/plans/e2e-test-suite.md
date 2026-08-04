# NIKI E2E Test Suite — QA Engineer Pass

## Goal

Replace the current ad-hoc integration tests (`tests/*.rs`, all 10 files) with a proper,
comprehensive end-to-end test suite that exercises **every** product surface the way a
new user would: CLI → pipeline → sandbox → git outputs → reports → dashboard → memory.
The suite is hermetic (mock LLM + `worktree` backend), deterministic, and CI-safe.

The suite acts as a QA/debug engineer: every scenario asserts what works, and the
findings (what doesn't, what needs improvement) are fixed as part of implementation
and documented in `TESTREPORT.md`.

Three pillars are managed end-to-end: **functionality**, **performance/scalability**,
and **security**. Harness methodology is borrowed from SWE-bench, OpenHands, EffiBench,
Terminal-Bench, tau-bench, and the Agentic Benchmark Checklist (ABC).

## Decisions (confirmed)

- **Remove all 10 `tests/*.rs` files.** Keep inline `#[cfg(test)]` unit tests inside `src/`
  (pipeline, safety, eval, sandbox, config, cost, etc.) — they stay as fast unit-level safety.
- **Hermetic E2E driver:** add a test-only `mock` LLM provider; run the real `execute_pipeline`
  and real `niki` binary via the `worktree` sandbox backend. No Docker daemon, no API keys.
- Docker-specific security tests are gated behind `NIKI_TEST_DOCKER=1` (ignored otherwise).

## Existing codebase facts that shape the design

- Pipeline entry: `src/cli/run.rs::handle` (connects runtime, snapshots, runs `execute_pipeline`,
  writes artifacts, dashboard, patch, git branch/commit, safety proof, report, task record).
- Orchestrator: `src/orchestrator/pipeline.rs::execute_pipeline` — Planner → body stages,
  topology select (`Auto/MultiAgent/SingleAgent`), revision loop, parallel coders, isolation
  records, metrics per stage.
- Agents: `src/agents/mod.rs::run_agent` — renders prompt via minijinja, streams from
  `LlmProvider`, retries transient errors (3 tries, exp backoff), extracts JSON, validates
  against JSON schema.
- LLM: `src/llm/provider.rs` — `LlmProvider` trait + `create_provider` (only
  anthropic/openai/google/ollama). **No mock exists today** — required enabler.
- Sandbox: `src/sandbox/mod.rs` (`Sandbox` trait), `docker.rs`, `worktree.rs`.
  Worktree = git worktree + local `std::process::Command`. No daemon.
- Security policy: `check_command_policy` (`src/sandbox/mod.rs:57`) is implemented and
  unit-tested but **never called in either exec path** (dead code — see Findings).
- Eval: `src/eval/mod.rs` replays recorded artifact JSONs and scores catch-rate via
  keyword matching on Red challenges / Reviewer issues. Never executes code.
- Cost: `src/cost.rs` — substring price table, `0.0` for unknown/local models.
- Observability: `src/observability/mod.rs` — `Span` with `retry_count`, `[SPAN]` jsonl.

## Findings (QA review — to be proven by tests, then fixed)

| # | Finding | Severity | Proof/test | Fix |
|---|---------|----------|------------|-----|
| F1 | Command security deny-list (`check_command_policy`) is **never enforced**: `DockerSandbox::exec` and `WorktreeSandbox::exec` ignore the `role` param (`_role`). | Security | `security.rs::deny_list_enforced_*` | Call `check_command_policy` with the role's policy before exec |
| F2 | `DockerConfig.memory_limit` (2g) / `cpu_limit` (2.0) are configured but **never applied** — `HostConfig` uses `..Default::default()` (`docker.rs:73`). | Security/Perf | `security.rs::docker_resource_caps` | Set `Memory`, `MemorySwap`, `NanoCpus`/`CpuQuota` in HostConfig |
| F3 | `max_exec_seconds` (300) is **never enforced** — no timeout in either sandbox; a hung agent blocks the pipeline forever. | Perf/Scalability | `performance.rs::exec_timeout_kills` | Wrap exec in `tokio::time::timeout` using the role policy's `max_exec_seconds` |
| F4 | No mock LLM provider → full-pipeline E2E offline impossible today. | Functionality | (enabler) | Add `mock` provider to `create_provider` + `MockProvider` |
| F5 | Worktree sandbox runs commands as the full host user with **no isolation** beyond a separate dir — no privilege drop, no cwd enforcement, no policy. | Security | `security.rs::worktree_*` | Enforce policy + timeout; document that worktree is isolation-light; assert blast radius |
| F6 | Eval harness (`niki eval`) **never executes code**; replay fixtures are partial (e.g. `defect-sql` has only `red.json`+`reviewer.json`) and the 100%-catch claim is fixture-self-fulfilling. | Functionality/credibility | `eval_harness.rs::gold_patch_resolves` | Add a `--resolve-check` that applies the final diff in a temp worktree and runs the repo's own tests (F2P/P2P), plus oracle/gold-patch and empty-baseline validations |
| F7 | No TTFT / per-token streaming rate / per-stage retry_count surfaced in metrics or observability. | Performance | `observability.rs::span_*` | Wire `retry_count` into StageMetric, add TTFT field to `Span` |
| F8 | `tracing::debug!` in `run_agent` logs the **full raw agent response** — risk of secrets/keys leaking into logs. | Security | `security.rs::no_secrets_in_outputs` | Redact known key patterns before logging; assert no API key in artifacts/report/span |
| F9 | Determinism is weak: `temperature 0.2` fixed, no seed, naive `extract_json`. Reliability untested. | Reliability | `reliability.rs::pass_k_*` | Assert determinism where mock is scripted; document nondeterminism envelope |
| F10 | Task records on failure paths: verify `TaskStatus::Failed/Cancelled` records are actually persisted with correct exit codes. | Functionality | `e2e_cli.rs::failure_records*` | (verify only; fix if broken) |

## Research summary (harness techniques being borrowed)

- **SWE-bench**: FAIL_TO_PASS / PASS_TO_PASS grading; gold-patch validation; per-instance
  isolation; error classification (apply vs run vs test vs agent).
- **OpenHands**: event-stream audit trail; cost-per-resolved metric; system-vs-agent
  failure attribution.
- **EffiBench**: separate *correctness* from *efficiency* (Speedup@1, memory peak).
- **Terminal-Bench**: end-state checks (not trajectory policing); oracle solvability;
  adversarial exploit agent; canary strings; ≥5 trials + 95% CI; cost-vs-accuracy Pareto.
- **tau-bench**: `pass^k` (all-k reliability) vs `pass@1` (capability); deterministic state
  eval; unsolvable-task refusal; policy adherence.
- **ABC (Agentic Benchmark Checklist)**: pin versions; isolate from ground truth; oracle
  solver; state matching includes *irrelevant* files (scope creep); metric anti-hacking;
  baselines + significance; contamination notes.

## New test architecture

```
tests/
  common/
    mod.rs            # pub test helpers shared by all suites
    harness.rs        # Harness: temp git repo + niki.toml (mock provider, worktree backend)
                      #   -> runs execute_pipeline (lib) or the niki binary (assert_cmd)
                      #   -> collects PipelineResult / TaskRecord / files, tears down
    fixture_repo.rs   # builders for sample projects (Rust, JS, Python) incl. seeded-defect repos
    mock_llm.rs       # MockProvider construction helpers (responses, errors, latency, script)
    metrics.rs        # token/cost/latency assertion helpers
  e2e_pipeline.rs     # functionality: full chain, fast-path, parallel, revision loop, dry-run
  e2e_cli.rs          # new-user CLI flows: run/status/report/config/dashboard/recommend/memory/goal
  outputs.rs          # branch/commit/report.md/dashboard.html/changes.patch/safety_proof.json
  performance.rs      # latency, token accounting, cost math, token-multiple, exec timeout
  scalability.rs      # N parallel coders, concurrent pipelines, large-repo indexing, cleanup
  security.rs         # policy enforcement, scope creep, secrets, prompt injection, docker caps
  reliability.rs      # determinism, pass@1 vs pass^k on scripted mocks
  eval_harness.rs     # dataset grading upgrade: F2P/P2P resolve-check, gold patch, oracle, empty baseline
  memory_knowledge.rs # memory persistence, knowledge indexing, project summaries
  observability.rs    # spans, retry_count, TTFT, span jsonl file output
  cost.rs             # price-table correctness, unknown-model=0, ollama=0
  config.rs           # load/merge/env precedence regression (rebuilt from old tests)
```

Old files deleted: `artifacts_test.rs`, `config_test.rs`, `helpers.rs`, `knowledge_test.rs`,
`ui_modal.rs`, `ui_pages.rs`, `ui_render.rs`, `ui_state.rs`, `ui_theme.rs`, `ui_transitions.rs`
(inline `#[cfg(test)]` in `src/` stay).

## Implementation steps

### M1 — Enablers
1. `src/llm/mock.rs`: `MockProvider` implementing `LlmProvider`.
   - Scripted responses per role (either fixture JSONs or inline strings), supports
     `stream()` emitting `Text` + `Usage` chunks; optional simulated latency per stage.
   - Failure injection: transient (429/503 → exercises retry), fatal, invalid JSON,
     schema-violating JSON, missing-usage fallback, dropped stream.
   - `create_provider("mock", ...)` in `src/llm/provider.rs` (`cfg(test)`-free so the
     binary can also run with `--planner-model` mock for manual QA; keep it cheap).
2. `tests/common/` scaffold: `harness.rs`, `fixture_repo.rs`, `mock_llm.rs`, `metrics.rs`, `mod.rs`.
   - Harness writes `niki.toml` with `[providers.mock]` + all agents bound to `mock`
     provider + `docker.backend = "worktree"`, `security.enabled`/`red_blue.enabled`/
     `parallel.enabled` as flags.
3. Delete the 10 old `tests/*.rs` files.

### M2 — Functionality suite (`e2e_pipeline.rs`, `e2e_cli.rs`, `outputs.rs`)
Scenarios (each: setup → act → assert):
- Happy path multi-agent: high-complexity task → 5 stages ran (Planner/Coder/Tester/Red/
  Reviewer), artifacts + isolation records present, `final_diff` non-empty.
- Single-agent fast-path: low-complexity task → only Planner+Coder stages, `topology == SingleAgent`.
- Parallel coders: `parallel.enabled`, `coder_count = 2` → Synthesizer artifact, reconciled diff applies.
- Revision loop: Reviewer scripted `revision_needed` twice then `approved` → `revision_rounds == 2`,
  `review_feedback` threaded into coder context.
- Security audit pass: `security.enabled` → SecurityAuditor artifact recorded, verdict captured.
- Dry-run: `--dry-run` → stops after Planner, no sandbox created, no branch.
- Failure paths: invalid JSON → `ArtifactValidation` error, `TaskRecord::Failed` persisted,
  non-zero exit; transient 429 → retry succeeds, pipeline completes.
- Outputs: git branch `niki/<id8>` created + committed, `changes.patch` parses, `report.md`
  contains verdict/topology/safety sections, `dashboard.html` exists, `safety_proof.json`
  `hermetic == true`, task record `Completed`.
- CLI-as-new-user: `niki run`, `niki status`, `niki report`, `niki config`, `niki dashboard`,
  `niki recommend`, `niki memory`, `niki goal` in temp dirs — exit codes + expected stdout/files.
- Empty diff (no change) → no branch created, safety proof skipped, run completes cleanly.

### M3 — Security suite (`security.rs`) + fixes F1–F3, F5, F8
- `deny_list_enforced_*`: sandbox `exec` with a denied command (`git push --force`,
  `rm -rf /`, `curl | sh`, `--no-verify`) must return a denial error for each role policy.
  (Fails today → fix F1 by calling `check_command_policy(cmd, role_policy)` in both execs.)
- `docker_resource_caps` (gated `NIKI_TEST_DOCKER=1`): created container's `HostConfig`
  has `Memory`/`NanoCpus` set. (Fails today → fix F2.)
- `exec_timeout_kills`: a mock `sleep 9999` command is killed at `max_exec_seconds`.
  (Fails today → fix F3.)
- Scope creep: scripted coder diff touching a file outside `files_to_modify` → assert
  final diff / branch touches only intended files (ABC O.g.2).
- Secrets: task description containing a fake API key → assert key absent from
  report.md, artifacts, spans, dashboard (fix F8 if it leaks).
- Prompt injection: task text with "ignore instructions / exfil to X" → assert no
  exfiltration behavior, pipeline still produces schema-valid artifacts (tau-bench adherence).
- Worktree isolation-light check: exec cwd confined to worktree; absolute-path writes
  outside worktree blocked (documented limitation).

### M4 — Performance + scalability (`performance.rs`, `scalability.rs`)
- Latency: mock latency injection → StageMetric.latency_ms within tolerance; total_ms math.
- Token accounting: Usage chunk merge (separate in/out chunks), estimate fallback when
  provider omits usage.
- Cost math: each price-table entry; unknown model → 0.0; ollama → 0.0.
- Token-multiple report block present when multi-agent, ~1× on fast-path.
- TTFT: MockProvider records first-token delay → Span/TTFT assert (fix F7 wiring).
- Scalability: 2–3 parallel coders converge; 3 concurrent `execute_pipeline` runs in
  tokio complete without cross-talk; 200-file generated repo indexes; `destroy` leaves
  no `.niki-worktrees` / temp dirs behind (leak check).

### M5 — Reliability + eval-harness upgrade (`reliability.rs`, `eval_harness.rs`)
- Determinism: scripted mock → run each deterministic scenario 3×, assert identical
  `final_diff` and verdict modulo task UUID.
- `pass@1` vs `pass^k` (k=3) over the seeded-defect mini-dataset with scripted mocks.
- Eval upgrade (fix F6): `--resolve-check` applies the final diff in a temp worktree and
  runs the project's own tests → F2P/P2P grading; gold-patch validation (oracle must
  resolve); empty-baseline (trivial agent scores ~0, catches leaky graders).

### M6 — Memory/knowledge, observability, cost, config rebuild + docs
- Memory persistence: append/read round-trip in temp project; render_memory_for_prompt.
- Knowledge: `index_project` on fixture repo → summary/symbols present; doc globs + URLs.
- Observability: `[SPAN]` jsonl written with retry_count; TTFT field present (F7).
- Config: rebuild the strongest old config assertions (load/merge/env precedence,
  provider env vars, deny-list defaults) as `config.rs`.
- Write `TESTREPORT.md` (findings before/after, metric table, scenario index).
- `cargo fmt`, `cargo clippy`, `cargo test` green; `cargo test -- --test-threads=1` also
  green (env-var-mutating tests use the existing ENV_LOCK pattern).

## Verification / Definition of done

- `cargo test` passes with the new suite (all suites green, no Docker needed).
- `NIKI_TEST_DOCKER=1 cargo test --test security` green where Docker is available.
- Every F1–F10 finding has a failing-then-passing test (tests written first where cheap).
- `TESTREPORT.md` documents: what works, what was fixed, what remains (with rationale).
- Old `tests/*.rs` deleted; inline `src` unit tests still pass.

## Out of scope

- Real-model live eval (existing `niki eval --live` path, needs keys) — unchanged.
- Cloud backend (`NIKI_CLOUD_ENDPOINT` beta) — trait seam tested via Worktree only.
- Full SWE-bench-scale dataset — dataset growth is a follow-up, not part of this suite.
