# NIKI Repository Audit — Launch Readiness

> **Date:** 2026-08-17
> **Branch:** master (post-goal merge)
> **Version:** 0.4.0
> **Purpose:** Phase 0 of open-source launch plan

---

## 1. Current Architecture

### Execution Chain

```
User → CLI (clap) → handle(args) → run.rs → execute_pipeline()
  → SandboxBackend::create_sandbox() → Podman/Docker/Worktree
  → Agents: run_agent(role, llm, template, context) per stage
    → LlmProvider::stream(request) → Anthropic/OpenAI/Google/Ollama
    → Artifact output (JSON) → validate_artifact()
  → Revision loop (Reviewer → Coder) until approved or max_rounds
  → Git branch commit → report.md + changes.patch + artifacts/*.json
  → PipelineResult → display via AgenticDisplay (TUI or log)
```

### Module Map (src/)

| Module | Role | Files | Lines |
|---|---|---|---|
| `cli/` | Entry points (16 commands) | run, status, report, config, recommend, dashboard, eval, memory, goal, auth, providers, doctor, chat, smoke, research, verify | ~2,500 |
| `orchestrator/` | Pipeline sequencing, task state | pipeline.rs, state.rs | ~2,000 |
| `agents/` | Agent prompts + execution loop | mod.rs, planner.rs, coder.rs, tester.rs, reviewer.rs, errors.rs | ~1,200 |
| `llm/` | Provider abstraction + clients | provider.rs, anthropic.rs, openai.rs, google.rs, ollama.rs, mock.rs, failover.rs, repair.rs | ~1,800 |
| `sandbox/` | Container/worktree backends | mod.rs, docker.rs (Podman+Docker), worktree.rs | ~800 |
| `config/` | niki.toml loading + env overrides | mod.rs, types.rs | ~1,600 |
| `display/` | TUI + non-TTY fallback | tui.rs, state.rs, theme.rs, pages/, components/, chat/, agent_stream.rs, etc. | ~11,000 |
| `commands/` | Slash command system | mod.rs | ~800 |
| `runtime/` | Tool registry + 22 tools | mod.rs | ~2,000 |
| `mission/` | Mission/Session/Agent stores | mod.rs | ~400 |
| `event/` | Event bus (broadcast channel) | mod.rs | ~300 |
| `activity/` | AgentState grammar | mod.rs | ~270 |
| `persistence/` | Mission-scoped JSON store | mod.rs | ~270 |
| `artifacts/` | Typed artifacts + validation | mod.rs, types.rs, validate.rs | ~500 |
| `memory/` | Context compression + store | compression.rs, store.rs | ~600 |
| `safety/` | Safety proof generation | mod.rs | ~200 |
| `permissions/` | Permission checking | mod.rs | ~400 |
| `knowledge/` | Repo indexing for agent context | indexer.rs | ~300 |
| `mcp/` | MCP server integration | mod.rs | ~200 |
| `output/` | Git branch/patch/report | git.rs, patch.rs, report.rs | ~600 |

**Total:** ~132 Rust files, ~30,600 lines

### External Dependencies (Cargo.toml)

Core: `tokio`, `clap`, `serde`/`serde_json`, `reqwest`, `bollard`, `git2`, `ratatui`, `minijinja`, `anyhow`/`thiserror`
UI: `ratatui`, `console`, `indicatif`, `textwrap`, `syntect`, `figlet-rs`
Security: `keyring`, `regex`, `notify-rust`
Build: `dist-workspace.toml` (cargo-dist v0.32.0)

---

## 2. Existing Capabilities (What Works)

### Core Pipeline
- Four-agent pipeline: Planner → Coder → Tester → Reviewer
- Podman/Docker sandbox with rootless containers, CapDrop ALL, read-only rootfs
- Git worktree alternative backend (no container runtime required)
- Per-agent provider/model mixing (different providers per role)
- Reviewer-driven revision loop (up to `max_revision_rounds`)
- Custom pipeline topology via `[pipeline]` config
- Parallel coders + synthesis mode
- Red/Blue adversarial review (opt-in, off by default)
- Security auditor agent (opt-in)
- External source ingestion (`[knowledge]`)

### Tooling
- 22 baseline tools (read, write, edit, patch, glob, grep, list, bash, test, web_search, web_fetch, task_spawn/status/cancel/create/update/list, ask_user, approval, skill_list, skill_load, git)
- ToolRegistry with categories, permissions, risk levels
- ToolResult envelope with structured data
- LLM tool-calling loop (`run_tool_loop`)

### Mission Control
- EventBus (broadcast channel, 30+ typed events)
- Mission/Session/Agent stores (thread-safe RwLock)
- Activity grammar (12-state AgentState with transitions)
- Mission-scoped persistence (.niki/missions/<id>.json)
- Fleet dashboard (2-column mission cards)
- Session view (7 tabs)

### Display
- Rich TUI (ratatui, ~30fps)
- 12+ page views (Run, Pipeline, Agents, Diff, Verdict, Cost, Artifacts, History, Config, Help, TestLog, Chat, Fleet, Session)
- Chat mode with streaming, markdown, code blocks
- Command palette (Ctrl+P), slash menu
- Permission modal
- Status bar (mode, Ln/Col, typing indicator)
- OS-level notifications (notify-rust)

### CLI
- 16 subcommands (run, status, report, config, recommend, dashboard, eval, memory, goal, auth, providers, doctor, chat, smoke, research, verify)
- `niki doctor` diagnostic categories (install, config, providers, sandbox, security)
- `niki smoke` — quick pipeline verification
- `niki eval` — seeded-defect evaluation harness

### Safety & Security
- Spend cap enforcement (aborts before branch creation)
- Secret redaction in reports/artifacts
- Command deny lists (force-push, rm -rf /, curl|sh)
- Per-tool permission model (allow/ask/deny)
- Context compression (auto-compact at 80% threshold)
- Crash-safe incremental persistence

### Distribution
- 3 Unix targets via cargo-dist + GitHub Releases
- Homebrew formula (macOS)
- curl installer with SHA256 verification
- Scoop manifest (Windows, stale v0.3.1)
- Winget manifests (Windows, stale)
- CI: fmt + clippy + tests + cargo-audit + cargo-deny

---

## 3. Incomplete Capabilities (Partially Wired)

| Capability | Status | Blocker |
|---|---|---|
| `[session]` config | Parsed but not fully wired | No session resume CLI |
| `[compaction]` config | Parsed, ContextBudget exists | No live TUI context visualization |
| `[permissions]` config | Wired into PermissionChecker | No diff preview in approval modal |
| `[mcp]` | Wired (tools surfacing) | Agent→server tool-call loop is iterative |
| Cloud execution | Sandbox trait seam exists | No cloud backend |
| Session resume | `SessionManager` save/load exists | No `niki session list/resume` CLI |
| Hooks (PreToolUse/PostToolUse) | Not implemented | No hook system |
| Plan mode (read-only) | `prompts/planner.md` produces TaskSpec | No live todo tracker, no Shift+Tab cycle |
| Auto-checkpointing | `SessionManager` undo/redo exists | No automatic per-edit snapshots |
| Subagent live progress | `AgentsPage` has tabs | No streaming live progress |

---

## 4. Strongest Differentiators

1. **Four independent agents** — Not a single monolithic model. Each agent (Planner/Coder/Tester/Reviewer) has its own prompt, model, and context. Independence eliminates confirmation bias.

2. **Hermetic sandbox by default** — Podman/Docker rootless containers with CapDrop ALL, read-only rootfs, network disabled by default. Agent commands run as untrusted input. Git worktree alternative for lightweight use.

3. **Output is a git branch** — `niki/<id>` branch with real commit, diff, report, and per-agent artifacts. Reviewable like a human PR. No opaque auto-commits to main.

4. **BYOK + provider-mixing** — Different providers/models per role (strong reasoner for Planner, cheap model for Tester). No bundled gateway, no telemetry, no hosted service.

5. **Reviewer-driven revisions** — Reviewer can bounce work back to Coder until approved or max rounds reached. Not one-shot.

6. **Fully auditable** — report.md, changes.patch, artifacts/*.json, safety_proof.json capture what every agent decided and why.

7. **Proof, not promises** — Every claim backed by artifacts NIKI itself produces. Claims-audit.md maps each marketing claim to code.

---

## 5. Weakest UX Areas

1. **First-run experience** — No onboarding flow, no welcome guidance, no "what to do next" after install. README is dense (391 lines).

2. **Quickstart friction** — Requires Rust toolchain + Podman/Docker + API key. Three prerequisites before first run. Install.sh doesn't set up sandbox image.

3. **TUI empty state** — Blank session gives no guidance. No "type / to start" hint.

4. **Error messages** — Many errors use `anyhow!()` without user-friendly context. "No container runtime found" is the only user-facing sandbox error.

5. **Command discoverability** — 16 CLI subcommands but no `niki --help` that groups them logically. `niki doctor` exists but isn't promoted.

6. **Configuration** — niki.toml has 10+ sections. No config wizard, no `niki config init`.

7. **Demo/story** — README demo gif exists but no narrated walkthrough. "Add a /health endpoint" example is generic.

8. **Landing page** — Separate repo (niki-site), not in sync with README claims.

---

## 6. Installation Problems

| Problem | Impact | Fix |
|---|---|---|
| Requires Rust toolchain (1.85+) | Blocks non-Rust users | Binary releases exist but not promoted as primary |
| Requires Podman/Docker running | Blocks users without containers | `niki doctor` checks but doesn't install |
| Sandbox image build required | `podman build -t niki-sandbox:24.04` is a manual step | Could be automated in installer or documented prominently |
| No Windows support | Blocks ~40% of developers | Scoop/Winget manifests exist but are stale v0.3.1 |
| install.sh doesn't install sandbox image | User must still run `podman build` after install | Could bundle image in release or auto-build |
| Homebrew formula may not be up to date | `brew install niki` may install old version | Needs verification against latest release |
| No `niki init` or `niki setup` | Config file creation is manual (`cp niki.example.toml niki.toml`) | Add guided setup |

---

## 7. Documentation Problems

| Problem | Location | Impact |
|---|---|---|
| README is 391 lines — too dense | README.md | New users overwhelmed |
| No "What is Niki?" one-liner at top | README.md | Mental model unclear |
| Claims-audit.md may be stale | docs/claims-audit.md | "Proof, not promises" claim weaker |
| docs/ site structure is good but disconnected from README | docs/content/ | Users don't know docs exist |
| No CONTRIBUTING.md workflow | CONTRIBUTING.md | Contributor friction |
| Landing page (niki-site) not in sync | research/refs/niki-site | Claims contradicted between repos |
| `niki doctor` not promoted | README.md | Users don't know it exists |
| Architecture docs are sparse | README.md "Project Structure" section | Extenders can't navigate codebase |

---

## 8. Extensibility Problems

| Area | Difficulty | Issue |
|---|---|---|
| Adding a tool | Medium | Must implement `Tool` trait, register in `build_baseline_registry()`, no plugin system |
| Adding a pipeline stage | Medium | Add `AgentRole` variant, prompt template, JSON schema, wire in `pipeline.rs` |
| Adding an agent role | Hard | Requires changes to `AgentRole` enum, pipeline.rs, prompts/, schemas/, possibly sandbox |
| Adding a provider | Medium | Implement `LlmProvider` trait, add to `create_provider()` |
| Adding a skill | Low | Drop .md into prompts/, or use `[knowledge]` config |
| Adding a page/view | Medium | Implement `Page` trait, register in `PageRouter`, add to `PageId` |
| Adding a slash command | Low | Add to `CommandRegistry` in `commands/mod.rs` |

---

## 9. Release Problems

| Problem | Status | Impact |
|---|---|---|
| cargo-dist v0.32.0 configured | ✅ | Release workflow exists |
| Only 3 Unix targets | ✅ | Linux x86_64, macOS Intel, macOS ARM |
| No Windows builds | ❌ | Scoop/Winget manifests stale v0.3.1 |
| Homebrew formula | ⚠️ Needs verification | May not point to latest release |
| install.sh | ✅ | SHA256 verification, auto-detect platform |
| Version in Cargo.toml | 0.4.0 | Must match release tag |
| `niki --version` | Uses cargo-dist version | Should be correct |
| CHANGELOG.md | Exists but may be stale | Needs update for launch |

---

## 10. Technical Debt (Launch-Relevant)

1. **24 compiler warnings** — `cargo build` passes but has warnings (unused imports, etc.)
2. **Display code has two UI systems** — Live `pages::AppState` + dead reactive/chat architecture (from ui-audit.md). Not blocking launch but increases maintenance.
3. **Old DisplayEvent system** — Pipeline uses `mpsc::channel<DisplayEvent>` while new EventBus exists. Two event systems in parallel.
4. **Stale Windows manifests** — scoop/niki.json v0.3.1, winget/ v0.3.1. Release workflow doesn't produce Windows assets.
5. **MCP feature copy inconsistency** — Code is wired but some docs say "not wired".
6. **SECURITY.md spend-cap claim** — Says "hard mid-run ceiling" (correct for v0.4.0) but niki.example.toml may still say "warn-only".
7. **No `niki session` CLI** — SessionManager exists but no CLI to list/resume sessions.
8. **No live context visualization** — ContextBudget wired but not shown in TUI.
9. **Demo gif may be outdated** — Assets/demo.gif needs verification against current TUI.
10. **Landing page (niki-site) is a separate repo** — Changes require cross-repo coordination.

---

## Exit Criteria Verification

Can explain the full chain?

| Step | Component | File(s) |
|---|---|---|
| User → | CLI parsing | `src/main.rs`, `src/cli/mod.rs` |
| → Agent Runtime | Pipeline execution | `src/orchestrator/pipeline.rs` |
| → Model | LLM providers | `src/llm/provider.rs`, `src/llm/{anthropic,openai,google,ollama}.rs` |
| → Planning/Execution | Agent loop | `src/agents/mod.rs`, `src/agents/{planner,coder,tester,reviewer}.rs` |
| → Tool Registry | Tool definitions | `src/runtime/mod.rs` |
| → Tools | 22 implementations | `src/runtime/mod.rs` (inline) |
| → Environment | Sandbox | `src/sandbox/mod.rs`, `src/sandbox/{docker,worktree}.rs` |
| → Result | Branch + artifacts | `src/output/{git,patch,report}.rs` |

**Exit criteria: MET.** ✅
