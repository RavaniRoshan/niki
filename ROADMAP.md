# NIKI Roadmap

> A phased plan reconciled with the **actual state of the codebase** (as of this commit).
> Structure follows the product roadmap record; status reflects what is really shipped.
>
> Legend: ✅ done · 🟡 partial / scaffolded · ⬜ not started

---

## Phase 0 — Prototype  ✅ (essentially complete)

| Item | Status | Notes |
|---|---|---|
| Rust CLI scaffold (`run` / `status` / `report` / `config`) | ✅ | `src/cli/*`, `src/main.rs` |
| Docker sandboxing (bollard) | ✅ | `src/sandbox/docker.rs` — create / exec / apply_patch / destroy |
| 4-agent pipeline (Planner → Coder → Tester → Reviewer) | ✅ | `src/orchestrator/pipeline.rs` |
| Artifact protocol (JSON Schema validation) | ✅ | `src/artifacts/*`, `schemas/*.json` |
| Convergence revision loop (Reviewer → Coder, capped) | ✅ | `max_revision_rounds`, default 3 |
| Git branch output + markdown report + patch | ✅ | `src/output/{git,report,patch}.rs` |
| BYOK config + per-agent provider/model mixing | ✅ | Anthropic · OpenAI · Google · Ollama |
| CLI streaming output **+ non-TTY log fallback** | ✅ | `is_tty` branch in `src/display/agent_stream.rs` |
| Graceful Ctrl+C shutdown (container cleanup, exit 130) | ✅ | `src/cli/run.rs` + `ActiveContainers` registry |
| Project knowledge indexing (tree, langs, deps, git history) | ✅ | `src/knowledge/indexer.rs` |
| **Skills-file parsing** (`AGENTS.md`, `.cursorrules`, `.editorconfig`) | ✅ | Already implemented — *ahead of the original MVP plan* |

## Phase 1 — MVP (Months 2–3)

| Item | Status | Notes |
|---|---|---|
| Landing-page README + brand logo | ✅ | Serves as the product's public face |
| Embedded React dashboard (pipeline viz + agent inspector) | ⬜ | Needs an embedded static-file server + JSON event feed |
| Built-in quality metrics on every run | 🟡 | Token counts are now real (from provider usage) and persisted per run; coverage/complexity capture not yet implemented |
| Pipeline customization (YAML/JSON config) | ⬜ | Foundational for most v2 agent work — see v2 |
| SWE-bench evaluation harness | ⬜ | Depends on real token/cost accounting |
| Waitlist page | ⬜ | |
| Private beta launch | ⬜ | |

---

## Phase 2 — v2 (Months 4–6)

Ordered roughly by dependency. Items lower in the list build on ones above.

| # | Item | Status | Foundational for | Rough size |
|---|---|---|---|---|
| 1 | **Cost & performance analytics** — real token accounting (from API usage, not estimate), latency per stage, cost per agent/task; persisted to the task record and shown in `report` | ✅ | Per-agent model recommendations; SWE-bench; quality-moat proof | S–M |
| 2 | **User-defined agent topologies** — data-driven pipeline (a `[pipeline]` config: ordered stages, per-stage agent/model, optional skip) replacing the hardcoded flow | ✅ | Parallel coders, Security Auditor, dynamic topology, marketplace | L |
| 3 | **Parallel coder agents + synthesis** — N Coders explore the spec (each in its own git worktree); a Synthesizer merges | ✅ | — | M–L |
| 4 | **Security Auditor agent** — dedicated, adversarial vulnerability pass (enabled via `[security]`) | ✅ | — | M |
| 5 | **External source ingestion** — READMEs, linked docs, wikis, issue content into the knowledge layer (extends `index_project`, uses the currently-unused `_config` hook) | ✅ | — | M |
| 6 | **Rich terminal TUI** — `ratatui` panels over the `DisplayEvent` channel, restyled as an **agentic transcript** (⏺ bullets, ⎿ connectors, sparkle spinner, ⏵⏵ mode line) | ✅ | — | L |
| 7 | **Dashboard: static HTML diff viewer with inline Reviewer/Security annotations** | ✅ | — | M |
| 8 | **Alternative sandboxing** — lightweight `git worktree` isolation + `Sandbox` trait abstraction (Docker / Worktree / Cloud backends) | ✅ | Cloud microVMs later | M |
| 9 | **Cloud execution (beta)** — `cloud` backend implements the `Sandbox` trait (drop-in seam); gated behind `NIKI_CLOUD_ENDPOINT` until infra ships | 🟡 | Revenue tier | XL |
| 10 | **Per-agent model recommendations** — `niki recommend` with cost/quality tradeoffs per role | ✅ | Depends on #1 | M |
| 11 | **Agentic terminal UI** — replicate a sub-agent workflow's posture, glyphs, spinner and mode line so NIKI's multi-agent flow mirrors that workflow | ✅ | — | M |

### v2 — what shipped (as of this commit)

The v2 phase is essentially complete: **10 of 11 items are implemented and covered by
passing unit/integration tests**; cloud execution (#9) remains a beta scaffold.

- **Cost & performance analytics** — per-agent/task token, latency, and cost metrics pulled from real provider usage and persisted to each run's record.
- **User-defined topologies** — `[pipeline]` replaces the hardcoded flow with an ordered, per-stage provider/model list (optional `skip`).
- **Parallel coders + synthesis** — `[parallel]` runs N Coders in isolated git worktrees; a Synthesizer reconciles their diffs.
- **Security Auditor** — `[security]` injects a dedicated adversarial review pass after the Reviewer.
- **External source ingestion** — `[knowledge]` pulls project doc globs and external URLs into agent context.
- **Rich terminal TUI / agentic UI** — `--tui` renders an agentic transcript (⏺ bullets, ⎿ connectors, sparkle spinner, ⏵⏵ mode line).
- **Dashboard** — `niki dashboard` produces a static HTML diff viewer with inline Reviewer/Security annotations.
- **Alternative sandboxing** — a `Sandbox` trait with Docker, git-worktree, and cloud backends (`--backend`).
- **Per-agent recommendations** — `niki recommend` reports cost/quality tradeoffs per role.

> **Cloud execution (#9)** is a *drop-in seam*, not a working backend yet: `CloudSandbox`
> implements the `Sandbox` trait but is gated behind `NIKI_CLOUD_ENDPOINT` and fails fast
> until infra ships.

## Phase 3 — Full version (6+ months)

Living memory · pipeline marketplace · dynamic agent topology · visual pipeline builder ·
cloud execution (production) · adversarial (Red/Blue) debate mode · Team tier ·
Anthropic partnership exploration · Architect agent.

## Phase 4 — Post-full-version

Enterprise licensing (SLA, SSO/SAML, audit logging) · general-purpose domain expansion ·
Company Brain spin-out (enterprise knowledge layer) — *evaluated only if the knowledge
layer proves standalone value.*

---

## Quality strategy (cross-cutting)

1. **Built-in metrics** (Phase 1/v2 #1) — coverage, complexity, review-pass rate, disagreement points.
2. **Benchmark campaign** — SWE-bench: multi-agent (NIKI) vs. single-agent.
3. **Real-world case studies** — blind human review, NIKI vs. Cursor.

---

> Priorities within a phase are **not commitments** — they're the current best understanding
> of sequencing, to be re-evaluated as beta feedback arrives. Ordering favors items that
> unblock the most downstream work.
