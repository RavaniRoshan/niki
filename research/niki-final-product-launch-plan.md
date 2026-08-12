# NIKI — Final Product & Launch Plan (Prototype → Final Product)

**Prepared:** 2026-08-12 · **Depth:** Deep · **Scope:** (1) prototype→final-product gap audit (internal + external, every angle), (2) launch readiness with T-minus 5 days, (3) open-source/full-customization readiness, (4) authentication & onboarding flow, (5) marketing asset kit, (6) security/performance/scalability verification.

**Method:** 8 parallel research subagents (1 internal codebase audit + 7 web research), 1 adversarial verification subagent, direct repo spot-checks (grep + build + workflows + manifests). Every finding below is either source-cited or repo-verified; verifier issues are resolved inline or carried as explicit limitations.

---

## Executive Summary

NIKI's **architecture is genuinely strong and market-aligned**: independent agents exchanging typed artifacts (Planner→Coder→Tester→Reviewer), hermetic Podman/Docker sandboxing, adversarial red/blue review, parallel coders, goal runner, real eval harness, Apache-2.0 — 2026's winning "many readers, one writer" pattern, with adversarial review now vendor-documented best practice (Claude Code's own docs). But the product is **not what the codebase appears to be**: the recently "completed" 10x features (sessions, undo/redo, MCP, custom commands, model cycling, compaction, permissions) and the conversational chat UI are **unwired skeletons** — they compile, are even unit-tested, but have zero runtime callers, and `niki run --tui` is still a **viewer-only** interface. Packaging is internally inconsistent (package manifests say BUSL-1.1 and point at a stale v0.2.0 tag while the repo is Apache-2.0), integration tests (220) never run in CI, the sandbox image is unpublished, and there is no audience, no landing page, no demo video.

**The 5-day reality:** a full "final product" (working chat input, sessions, MCP, auth, web, marketing) cannot all land in 5 days — but a **credible, honest, launch-ready product can**, by (a) fixing the 6 launch-blockers, (b) wiring the 4 cheap-but-visible features and deleting/silencing the silent-config traps, (c) shipping the authentication flow (`niki login` + keyring + `doctor`), (d) deciding the chat-UI wiring as a gated sprint, and (e) building the marketing kit (VHS/asciinema demo, landing page, PH/HN assets). Everything larger — full chat interactivity, MCP, sessions/undo, IDE extension, headless CI mode, web — becomes a prioritized post-launch phase driven by market pull.

---

## Part 1 — Internal Audit: What Is Real vs. Skeleton (repo-verified)

### 1.1 Module wiring map

| Module | Status | Evidence |
|---|---|---|
| `memory/` | ✅ WIRED | File-backed `.niki/memory/{role}.json`, injected into prompts (pipeline.rs) |
| `knowledge/` | ✅ WIRED | Indexes project; **AGENTS.md/.cursorrules/.editorconfig read into context** (indexer.rs) |
| `goal/` | ✅ WIRED | Real loop: tasks → `execute_pipeline` → persist (runner.rs) |
| `safety/` | ✅ WIRED | Hermetic proof, now enforcing (cli/run.rs) |
| `eval/` | ✅ WIRED | Offline replay harness, 24-case dataset, real fixtures |
| `cost.rs` / `recommend.rs` | ✅ WIRED | Real usage analytics consumed by TUI + `niki recommend` |
| `agents/` (mod.rs) | ✅ REAL | Generic `run_agent` (~331 lines); but `agents/{coder,planner,tester,reviewer}.rs` are 1-line `pub fn run() {}` stubs — the "four specialized agents" are role-flavored configs of one runner, not four engines. **This is fine to ship, but the README's "four specialized agents" framing is stronger than the code.** |
| `mcp/` | ❌ SKELETON | 136 lines, zero external callers; `[mcp]` config accepted & **silently ignored** |
| `session/` | ❌ SKELETON | 349 lines, zero callers; `[session]` config silently ignored |
| `permissions/` | ❌ SKELETON | 168 lines, zero callers; permission modal state never set |
| `observability/` | ❌ SKELETON | 71 lines, zero callers |
| `audit/` | ❌ SKELETON | 61 lines, zero callers |
| `commands/` | ❌ SKELETON | 211 lines; `CommandRegistry` dead; live TUI has fixed palette only |
| `display/engine.rs`, `input.rs` | ❌ UNREACHABLE | Zero callers |
| `display/chat/*` + components | ⚠️ COMPILED, UNREACHABLE | ~1,076 lines incl. 23 passing tests; `layout/mod.rs` renders chat, but **`tui.rs` never enters chat view** — runtime shows logo + pages + status line. No input path. |
| `display/diff_display.rs` | ❌ STUB | `render_diff(_diff, _term_width) {}` — one line, no body |

### 1.2 The 10x-feature claims are false on master

The goal file `niki-10x-features-v2` is marked "complete"; **the wiring is not**. Session save/restore, undo/redo, MCP client/server, custom slash commands, model cycling, context compaction, and the permission modal exist only as modules + config schema (added in commit `db5471a`). The unmerged branch `origin/goal/niki-10x-features-v2` still contains the wiring. Worse: `[session]`, `[mcp]`, `[permissions]`, `[compaction]` in `niki.toml` are schema-validated and **silently ignored at runtime** — the most trust-damaging class of bug for a launch ("I configured it and it did nothing").

### 1.3 Build, tests, CI

- `cargo build` and `cargo test --no-run` pass. 507 tests exist; **the 220 integration tests (incl. `security_exec.rs`, `docker_resource_caps.rs`, `tui_navigation.rs`) never run in CI** — CI only runs `cargo test --lib`. The e2e job uses `--backend worktree`, so **the Docker sandbox path is never exercised in CI**.
- Warning-free build is by fiat: `#![allow(dead_code)]` + 16 clippy allows at crate root. CI itself now gates properly (no `continue-on-error`; clippy exits propagate via `pipefail`). This is honest-by-accident — the crate-level allows are what let skeletons compile silently.

### 1.4 Release & packaging (verified inconsistencies)

| Item | State |
|---|---|
| Prompts/schemas embedded | ✅ `include_dir!` + `load_asset` — release binary runs outside source tree |
| **Tag `v0.2.0`** | ⚠️ **Stale**: predates master HEAD. Tagged binary lacks the license change, SIGTERM handler, hermetic enforcement, CI pinning |
| **Package manifests** | ❌ `homebrew/niki.rb`, `scoop/niki.json`, `winget/*.yaml` all declare **BUSL-1.1** and point at `v0.2.0` download URLs, while repo LICENSE/CHANGELOG are **Apache-2.0** |
| Sandbox image | ❌ Not published to any registry; default `base_image = "niki-sandbox:24.04"` requires manual `podman build` |
| README/roadmap | ✅ Honest; no dead FINDINGS.md links; demo.gif + logo.svg real |
| Version | 0.2.0 with a large `[Unreleased]` — needs a new tag |
| `evals/` | ⚠️ In `.gitignore` — new fixtures are silently dropped from git |

### 1.5 What is genuinely launch-credible today

Red/blue adversarial verification (unique), parallel coders, hermetic proof, per-role security policies enforced, goal runner, memory, knowledge ingestion incl. AGENTS.md, cost analytics, real eval harness with fixtures, embedded assets, honest roadmap, Apache-2.0, gating CI (for what it runs), GitHub workflows for release + swe-bench.

---

## Part 2 — External Findings by Sub-Question

### Q1. Prototype → production engineering (2026 bar)

- **cargo-dist is the de-facto release standard** (used by Zed, helix, biome): tag → build matrix → installers → GH Release → package managers, with changelog-derived notes. `github-attestations = true` gives Sigstore-backed SLSA L2, verified via `gh attestation verify`. (source: cargo-dist book)
- **MSRV is now declared + CI-tested**: ripgrep tracks latest stable (1.96.0); helix 1.90; just 1.89; cargo-deny 1.88. `rust-version` + `cargo-msrv` in CI is the norm. (sources: ripgrep/helix/just/cargo-deny repos; Cargo Book)
- **release-plz** is the 2026 mainstream release automation: version-bump PR → git-cliff CHANGELOG → tag → crates.io publish. (source: release-plz)
- **Error handling**: miette is the current benchmark (error codes, source snippets, clickable doc links); clig.dev: rewrite errors for humans ("You might need to run chmod +w"), debug logs to a file, pre-populated bug-report URL for crashes. (sources: miette repo, clig.dev)
- **CLI polish**: `clap_complete` (bash/zsh/fish/powershell/fig) + `clap_mangen` (man pages) — just ships both. `--no-input`, `--json`/`--plain` machine output, exit codes mapped to failure modes, Ctrl-C exits ASAP with bounded cleanup, XDG paths. (sources: clap docs, clig.dev)
- **NO_COLOR** (spec updated 2026-07): honor it; auto-disable color when not a TTY; no animations when not a TTY. (source: no-color.org)
- **Telemetry — live contradiction**: clig.dev says consent-first/opt-in; **gh CLI shipped default-on pseudonymous telemetry in April 2026** (with `GH_TELEMETRY=log` dry-run, env/config opt-out); cargo-binstall uses opt-out stats. **HN community (2025–26) treats opt-out telemetry in a NEW tool as a "hard no"** — for NIKI's launch, opt-in-only (default off) is the defensible choice. (sources: clig.dev vs docs.github.com/gh-telemetry; HN thread 49245437)

### Q2. Authentication & onboarding flow (what "auth" means for a BYOK CLI)

- **The 2026 BYOK pattern is layered**: env vars stay supported for CI/headless (every major agent CLI does this), while *stored* credentials use OS keyring with plaintext fallback. Codex: `cli_auth_credentials_store = "file | keyring | auto"`; gh: keychain default, `--insecure-storage` to opt into plaintext. The `keyring` crate is the standard Rust path (Keychain / Credential Manager / Secret Service). (sources: Codex auth docs, gh manual, docs.rs/keyring)
- **Interactive entry**: masked/no-echo input, validate pasted keys, Keep/Replace/Clear prompts, 0600 file permissions on fallback stores, never read secrets from flags (ps/history leak). (sources: clig.dev, heygen-cli PR #17, hermes-agent PR #20162)
- **Command set**: `login` (browser OAuth with localhost callback), `login --device-auth` (RFC 8628 device flow: print code + URL, poll), `login --with-api-key` **via stdin only**, `login status`, `logout`. In-TUI alternative: OpenCode's `/connect` → provider picker → auth method → `/models`. (sources: Codex auth docs, gh manual, OpenCode providers docs)
- **Cloud-backend auth** (NIKI_CLOUD_ENDPOINT): keep it independent of BYOK — per-host tokens (gh's `--hostname` model), device flow preferred over env fallback (gptme), refresh for OAuth tokens. (sources: gh manual, gptme PR #1529, Codex auth docs)
- **First-run**: Gemini CLI's zero-friction model ("no API key management", OAuth-first); Claude Code's `/config` tabbed settings + scoped settings (user/project/local, local auto-gitignored) + `claude doctor` (verify install/config, list invalid settings). Community consensus: one `doctor` command, subcommands for drill-down. (sources: Gemini CLI README, Claude Code settings docs)
- **Config UX**: publish a JSON `$schema` for niki.toml (OpenCode/Claude Code both do; enables editor autocomplete). (source: OpenCode providers docs, Claude Code settings)

### Q3. Open-source readiness & first-100-users

- **GitHub plumbing** NIKI still lacks (it has the files): issue queue labeling + `good first issue` (explicitly ranked above ML detection), secrets scanning with push protection (28.65M secrets leaked in 2025, +34% YoY), Dependabot + dependency review, CodeQL (free for OSS), **branch protection (PR + 1 approval)** on main, **2 admins**, **FUNDING.yml** (Sponsor button), **Discussions** enabled. (sources: github.blog security post, opensource.guide, GitHub docs)
- **Governance**: formal governance docs are NOT needed at launch; CLA not needed (GitHub ToS inbound=outbound; Apache-2.0 already carries the patent grant); DCO is the lighter alternative if desired. Trademark/name + domain + handles check recommended. (sources: opensource.guide legal/leadership)
- **Release discipline**: keepachangelog 1.1.0 (CHANGELOG.md, reverse-chron, `Unreleased` on top, `[YANKED]`); GitHub Releases are "not very discoverable" — keep both. **Docs site**: mdBook (the Rust project's own tool; GitHub Pages = zero cost) is the canonical path. (sources: keepachangelog.com, mdBook docs)
- **Show HN rules** (dang's updated guidance): must be playable, **no email walls** ("everyone gets incredibly angry"), hand-written text — **new March 2026 rule: LLM-generated posts are penalized** — backstory + what's different + zero marketing language, human username, respond all day. Most Show HNs score 0–11 points; 100–800-point outliers share a playable demo + concrete problem framing. (sources: showhn.html, dang comment 22336638, HN Algolia mining)
- **Product Hunt for OSS in 2025/26: mixed-to-negative sentiment**; no source favors PH over HN for OSS dev tools. Treat PH as secondary, HN as primary. (source: HN threads 43970837, 38316936)
- **First-100-users playbook (5-day compatible)**: first 10–20 users through personal network with hands-on onboarding ("get to 10 users personally, then post more widely"); then community channels; HN is the highest-leverage free channel but a lottery — the 5-day plan is a launch sprint backed by the longer grind (community participation, SEO, directories: BetaList, Indie Hackers, r/SideProject, dev.to, build-in-public). Waitlist on landing page; cold email lists take weeks-months. (sources: HN founder reports, awesome-launch directory)

### Q4. Marketing asset kit (production process)

- **Terminal demo**: VHS (charmbracelet) renders GIF/MP4/WebM from a scripted `.tape` (exact typing, `Wait /regex/`, hide setup commands, screenshots, multiple outputs per run) — needs `ttyd` + `ffmpeg`. Or `asciinema rec` → **agg** → high-quality GIF (gifski), trimming dead time. **HN explicitly praises the "0 → running" demo** (install → first successful run). (sources: VHS repo, agg docs, HN threads 49205732/46583112/41487769)
- **Length**: sources disagree (60–120s / <60s at 68% finish / 2 min marketing); consensus: **<2 minutes, lead with the problem, one CTA**. Captions for terminal videos: no authoritative guidance found (open question — recommend OBS-style burned-in captions if audio is used; silent demo with annotations is safest). (sources: Flowjam, Levitate, Demosmith)
- **Product Hunt official spec**: tagline ≤60 chars; thumbnail 240×240 (<3MB, GIF animates on hover); gallery 1270×760, **2+ required**; description 260 vs 500 chars (PH's own pages disagree); **YouTube-only video URL, public**; 3 tags; **first comment matters — ~70% of POTD winners had a maker first comment**; ~53% of POTD had a video; no golden day (weekends get +15% Visit clicks); live at 12:01 AM PST; respond to comments in real time (take shifts). (source: producthunt.com/launch/preparing-for-launch)
- **Show HN structure**: backstory, what's different, statement of what it does, links, zero sales language, hand-written. Dual PH+HN same-day launches are a common accepted pattern (Vapi, Echo et al.). (sources: dang 22336638, HN threads)
- **og-image**: 1200×630, `@vercel/og` (Satori), allow OG route in robots.txt. (source: vercel.com/docs/og-image-generation)
- **Open questions (no primary source found)**: X-thread anatomy with engagement data, minute-by-minute launch run sheet, announcement blog template, terminal-demo captioning. These are filled from practice in Part 5.

### Q5. Security & trust bar (remaining, post-hardening)

- **Supply chain**: GitHub artifact attestations = SLSA L2 (cargo-dist integrates; enable `github-attestations = true`); `cargo-auditable` embeds the dependency tree in the binary (<4kB) so shipped binaries are auditable; `cargo-cyclonedx` → CycloneDX SBOM in releases. Linux/macOS code-signing is an open cargo-dist issue — attestations + checksums + auditable build is the realistic 2026 bar. (sources: GH docs, sigstore, cargo-auditable, cargo-dist book)
- **Reviewers check first (HN evidence)**: source integrity (no binary blobs, pinned suppliers, buildable from source), **opt-in telemetry as a hard community bar**, honest trust-boundary explanation including network + secrets. (source: HN thread 49245437)
- **Sandbox escapes 2026 — the pattern that matters for NIKI** (Pillar "Week of Sandbox Escapes", Jul 2026): escapes across Cursor/Codex/Gemini CLI/Antigravity came from (1) denylist sandboxes, (2) **workspace config files as executable code** (`.vscode/tasks.json`, git hooks, venv interpreters, fsmonitor config), (3) allowlists trusting command *names* not invocations (GitPwned → Codex RCE), (4) privileged daemons outside the sandbox. "If an agent gets to write the future inputs of systems, it was never sandboxed." **For NIKI: the worktree backend runs LLM commands on the host — this must be honestly documented as host-execution (not hermetic); the container path is the hermetic story.** (source: pillar.security)
- **Permission UX gold standard** (Claude Code): read-only default, allowlist read-only commands, explicit approval for writes/commands/network, per-tool deny, working-directory write boundary, approve-once + allowlists, fail-closed for unmatched. (source: code.claude.com/docs/en/security)
- **Prompt injection**: treat all fetched/model-visible content as untrusted; separate web-fetch context; don't auto-approve curl/wget; redaction is not exfiltration control — egress filtering is. (sources: OWASP LLM01, Claude Code security, HN 46706796)
- **CVE cluster as checklist** (Claude Code 2025–26): canonical path validation, symlink handling, startup trust-dialog injection, git-config templated execution, Yarn plugin auto-exec. NIKI's patch-path traversal fix (S3, git apply only) and SEARCH/REPLACE file binding (S4) are the right shape; a launch-time security review should re-check symlinks in the worktree backend and any config-file auto-loading. (sources: NVD, GH advisories)
- **Disclosure**: SECURITY.md exists; enable **private vulnerability reporting (PVR)** + add rustsec/advisory-db flow; pair security-fix releases with GH security advisories. (source: GitHub docs)
- **Enterprise gates** (not launch blockers, but the future path): IdP SSO, egress allowlist via non-bypassable proxy, OTel/SIEM export, org kill switch, SOC 2. (source: claude.com CISO guide)

### Q6. Performance & scalability

- **Concurrency**: production reports (claude-code issues) show real cost blowups from parallel fan-out (4 parallel agents burned a session budget in 15–20 min; 65 subagents ≈ 11.6M tokens in a day). Field consensus: **cap concurrency ~4 by default**, and pass findings **by reference (file paths)** between stages instead of interpolating 30–100kB JSON into prompts (≈800k-token contexts otherwise). (sources: claude-code issues #76742, #84323)
- **Runaway loops**: 811 open claude-code issues mention "infinite loop"; one token repeated 14,645× locked a session; one request spawned 37 agents / 2M tokens (reported in #66950, #82434). Mitigations: hard per-session token/spend caps, **~90% pre-limit warning + grace mode**, repetition detection, per-turn output caps. NIKI has max_revision_rounds; it needs a **spend cap + pre-run estimate** (G9 from the Aug-8 audit, still open).
- **Context**: every turn re-reads the prefix (1M-token session ≈3× cost/turn vs 300k); resolution *drops* with context growth (SWE-bench: 1.96% @13k → 1.22% @50k for Claude 2) — **compact early**; pass artifacts by reference. (sources: claude-code #83785, SWE-bench paper, Lost-in-the-Middle)
- **Sandbox limits**: Docker has no default limits — set `--memory`/`--cpus` per sandbox (NIKI already has memory_limit/cpu_limit config — verify enforced for all backends); bounded tokio `mpsc` channels for backpressure; jittered retry + `Retry-After` for 429s (already implemented per S12) + provider fallback. (sources: Docker docs, tokio docs, OpenHands PR #15236)
- **Long-run stability**: checkpoint-before-mutate (dying agents lose their most expensive half); mid-write aborts leave "finished-looking but unverified" trees; **an independent verifier reading `git diff` (not the builder's report) caught 5/13 agents that reported changes never written**. NIKI's Reviewer + hermetic proof partially cover this; the sandbox snapshot/teardown path should be verified. (source: claude-code #84323; OpenHands PR #14953)
- **Evals**: SWE-bench Verified (500 human-validated instances) is the defensible headline; Lite = standard medium-cost; contamination mitigation = post-training-date issues. SWE-bench authors vs OpenAI Verified disagree on how much to discount original-set numbers — publish methodology (pin model + date + harness, pass@1 and pass@k, tokens/cost). NIKI's offline replay eval is a solid foundation; publishing honest numbers is the launch differentiator, not raw scores. (sources: swebench.com, OpenAI, SWE-bench paper)

### Q7. Market patterns & what to build next

- **Category trajectory**: Cursor $100M ARR (Jan 2025) → ~$3B (May 2026), acquired by SpaceX for $60B (Jun 2026) [Reuters/Bloomberg, via Wikipedia]; Claude Code viral holiday 2025–26; every leader now ships terminal + desktop + IDE + browser. **CLI-first is strategic**: agents are moving into CI/CD pipelines. (sources: Reuters/Bloomberg refs via Wikipedia; codeaholicguy.com)
- **MCP is baseline** (donated to Linux Foundation AAIF, Dec 2025): 2026 launches list MCP client compat as table stakes. Security pattern: credentials in a vault outside the sandbox, proxy injects session-scoped creds. (sources: Wikipedia MCP, Anthropic Managed Agents)
- **Multi-agent: live disagreement** — Cognition: "don't build multi-agents" (parallel is fragile; single-threaded default; share full traces) vs Anthropic 2026 shipping parallel agent teams (Opus 4.6, Feb 2026). **The reconciliation is exactly NIKI's design**: multi-agent is safe when all agents read/write one shared durable context (the spec/artifacts), which is Planner→Coder→Tester→Reviewer. Lead marketing with this. (sources: cognition.ai/blog/dont-build-multi-agents; Wikipedia Claude)
- **Adversarial review is vendor-documented best practice** ("the agent doing the work isn't the one grading it") — validates NIKI's thesis. (source: code.claude.com/docs/en/best-practices)
- **What launches well in 2026**: sandboxing/security theme (Clawk 226 pts — "disposable Linux VM"), offline single binary (Ante 159 pts), observability/replay (Mindwalk 162 pts). **What backfires**: me-too saturation fatigue ("who is NOT building an AI sandbox"), self-promotion spam, pricing changes. NIKI's differentiation must be concrete: hermetic proof + adversarial agents + honest evals, not "another AI agent." (sources: HN threads via Algolia)
- **Demand signals for roadmap** (opencode issues): model autodiscovery 206+, VS Code extension 138+, session goals 128+, skills 114+, fallback 107+, session memory 96+, web UI 94+, async background agents 82+, VS Code diff preview 79+. (sources: opencode issue reactions)
- **Skepticism to answer**: METR 2025 (experienced devs 19% slower with AI despite feeling 20% faster) and DORA 2025 (7.2% delivery instability per 25% unstructured AI adoption) are widely cited — evidence (eval numbers, workflows) beats claims. (source: HN 48892859)

---

## Part 3 — Background / Key Terms

- **cargo-dist / cargo-binstall / release-plz**: release automation, binary install, release-PR automation for Rust.
- **SLSA L2 / Sigstore / attestations**: supply-chain provenance; artifact→source→build linkage, keyless signing.
- **RFC 8628 device flow**: headless auth (print code + URL, user approves in browser, CLI polls).
- **MCP**: Model Context Protocol, now an LF-standard interop layer; NIKI's skeleton is an opportunity, not a claim.
- **SWE-bench (Lite/Verified)**: execution-graded coding benchmarks; contamination-resistant by post-training-date instances.
- **"Many readers, one writer"**: the 2026 multi-agent consensus NIKI implements (parallel reads/analysis, single-threaded writes).
- **OWASP LLM Top 10 (2025)**: LLM01 prompt injection, LLM06 excessive agency — the numbered edition confirmed current (a claimed "Top 10 2026, Aug 4 2026" was unverifiable and is dropped; OWASP's Agentic Security Initiative is the active 2026 workstream).

---

## Part 4 — Analysis & Discussion

**Pattern 1: NIKI overclaims in structure, under-delivers in wiring.** The goal-tracker says complete; the code says skeleton. The biggest risk is not the missing features — it's the **silently-ignored config sections** and the "completed" claims. Launch users configure `[session]`, get silence, and churn. Fix = wire what's cheap, remove or clearly mark what isn't, and make the README/roadmap honest (it already is — keep it that way).

**Pattern 2: The launch window forces triage, and the evidence says triage is safe.** The market does not punish "viewer-only TUI + honest roadmap" nearly as much as it punishes broken claims, stale packaging, and no demo. Every launch-blocker (1.4) is a small, deterministic fix. The auth flow (Q2) is fully implementable in the window. The chat-UI wiring is the only genuinely risky 5-day item — hence a decision gate, not a blind sprint.

**Pattern 3: Security is now the launch theme and the trust test.** Pillar's 2026 sandbox-escape research and the Claude Code CVE cluster mean reviewers will probe NIKI's trust boundary. NIKI's answer is strong (container + deny-list enforced + hermetic proof + SSRF guard), but it must be *stated honestly*: the worktree backend executes on host (document it), egress control exists for the container, and redaction is not exfiltration control. An honest "what NIKI is not" section (the workerd pattern) builds more trust than claims.

**Pattern 4: Marketing assets are 80% reproducible in a day.** VHS scripted demos, a static landing page, PH gallery images, and a hand-written Show HN are all same-day artifacts. The scarce resource is the warm audience — 5 days cannot manufacture one, so the playbook is: launch sprint (PH/HN/direct outreach) + waitlist + community seeding as the durable engine.

**Pattern 5: Post-launch roadmap is set by market pull, and NIKI's skeletons map 1:1 to it.** MCP (wire skeleton), sessions/undo (wire skeleton), headless CI mode, VS Code diff-review extension, model fallback/autodiscovery, chat interactivity — every item is either an existing module to wire or a well-documented pattern. The plan below sequences them.

**Perspectives**: A security maximalist says don't ship until every skeleton is wired or deleted and worktree is documented as host-execution. A GTM pragmatist says fix blockers, ship beta, iterate in public with a security roadmap. Both agree on the 6 blockers; the difference is post-launch speed — which this plan resolves by treating security posture as launch-table-stakes, not roadmap.

---

## Part 5 — THE FINAL PLAN

### 5.1 Decision gates (set before Day 1)

1. **Honesty gate**: every claim in README/roadmap/goal files must be verifiable in code. Anything not wired gets either wired (cheap items) or explicitly marked (roadmap). **Non-negotiable.**
2. **Chat-UI gate**: wire the existing chat input path (`input.rs` → `ViewMode::Chat`) as a bounded 1.5-day sprint (Day 2–3). If not stable by the gate, launch with the viewer TUI (it works today) and ship chat as post-launch item #1. The compiled-but-unreachable code means the risk is integration, not greenfield.
3. **Scope gate**: no new features invented during the window. Every task below traces to a finding.

### 5.2 The 5-day execution plan

**Day 0 (today) — Truth & release plumbing**
- [ ] Delete/silence skeleton config traps: `[session]`, `[mcp]`, `[permissions]`, `[compaction]` must either work or produce a clear "not yet active — see roadmap" warning at config load (never silent).
- [ ] Fix package manifests: `homebrew/niki.rb`, `scoop/niki.json`, `winget/*.yaml` → Apache-2.0 + versioned URLs matching the new tag.
- [ ] Tag `v0.3.0` on master HEAD (not v0.2.0): bump Cargo.toml, move CHANGELOG Unreleased → 0.3.0.
- [ ] Enable `github-attestations = true` in dist config; add `cargo-auditable` + `cargo-cyclonedx` SBOM to release workflow (SLSA L2 + SBOM as the 2026 trust bar).
- [ ] Add `cargo test --test '*'` (integration) to CI and an e2e job that exercises the **Docker backend** (currently never tested in CI).
- [ ] Verify `cargo binstall niki` and the install scripts against the new release; verify release checksums page.
- [ ] Add MSRV: declare `rust-version` in Cargo.toml + `cargo-msrv` CI job.
- [ ] Remove the crate-level `#![allow(dead_code)]` and either wire or delete the modules it hides (see Day 2). *(Verifier note: allows are at src/lib.rs:18; count is 16 clippy allows.)*

**Day 1 — Authentication flow (the "full UI product" foundation)**
- [ ] `niki auth login` / `niki login`:
  - interactive: provider picker → masked key entry (no echo) → **keyring storage with plaintext fallback at 0600** (`keyring` crate; Codex-style `auto` mode), Keep/Replace/Clear prompts, key validation.
  - `--with-api-key` reads **stdin only** (never a flag); env vars remain first-class for CI (`ANTHROPIC_API_KEY` etc., precedence: env > keyring > config file).
  - `niki login status` / `niki logout` / `niki auth list`.
  - Cloud backend (`NIKI_CLOUD_ENDPOINT`) gets its own device-flow auth (`login --device-auth`, RFC 8628) independent of BYOK keys — gh-style per-host tokens.
- [ ] `niki doctor`: verify install, config validity, provider keys, sandbox runtime (podman/docker present? image built?), git, network; actionable fix commands per check (claude-doctor pattern).
- [ ] `niki init` first-run wizard: detect TTY → offer wizard; detect providers from env; write a validated `niki.toml`; always provide `--no-input` path (clig.dev).
- [ ] Publish a JSON `$schema` for `niki.toml` (editor autocomplete) + embed schema URI in `niki.example.toml`.
- [ ] Error UX pass: miette-style diagnostics for the top 5 failure modes (no key, no docker, bad config, rate limit, sandbox image missing) — each with a fix command; debug logs to file; bug-report URL on crash.
- [ ] Key hygiene: verify Google provider sends key via header (done per CHANGELOG) and secrets never appear in `--verbose` logs (redaction already exists — add a test asserting redaction of each key format incl. in errors).

**Day 2–3 — Product truth & the 4 cheap wins (+ chat gate)**
- [ ] Wire the cheap skeletons (each is a bounded task):
  - **Custom slash commands** — `commands/` registry → TUI input layer (smallest wiring; highest visible value).
  - **Model cycling** (F2-style) — provider/model list from config; mid-run switch.
  - **AGENTS.md flag** — `auto_detect_agents_md` (config exists; indexer already reads the file — just honor the flag).
  - **Context compaction** — `memory/compression.rs` has the machinery; wire a `--compact` command or auto-trigger at threshold with `[compaction]` config honored.
- [ ] **Chat-UI gate sprint (bounded 1.5 days)**: connect `input.rs` → submit → pipeline events → `ViewMode::Chat` render. Success = user can type a task in the TUI and watch the pipeline stream in chat view. If red by the gate, revert flag-free (viewer mode is the fallback) and promote to post-launch #1.
- [ ] Sessions/undo/MCP/permissions: **do not attempt to wire in the window.** Delete their config traps (Day 0) and re-claim them in the roadmap as post-launch Phase A (they are exactly the market-pull items).
- [ ] Integration-test sweep: the 220 integration tests now in CI; fix whatever they surface (they've never run — budget real time here).
- [ ] Publish sandbox image: `ghcr.io/ravaniroshan/niki-sandbox` (or pin + document the manual `podman build` in README with a one-liner script). Update default `base_image` to the published digest.
- [ ] Remove `evals/` from .gitignore; commit fixtures.

**Day 3–4 — Security & trust pass (verify each)**
- [ ] **Trust-boundary doc**: add "What NIKI is NOT" section (workerd pattern): worktree backend = host execution with enforced command policy (not hermetic); container = hermetic default; redaction ≠ exfiltration control; egress is controlled per S2 fixes — state exactly what's enforced.
- [ ] Launch security review checklist (each verified against code):
  - symlink handling in worktree backend + patch application (CVE-2025-59829 class);
  - no config-file auto-execution inside sandbox (Pillar pattern: the agent must not be able to write host-trusted configs — verify bind-mount read-only where feasible);
  - `curl|sh`, `mkfs`, `dd` deny-list enforced for all roles (verified in CHANGELOG — add a regression test);
  - SSRF guard + timeout on knowledge URLs (S5 — test localhost/metadata blocking);
  - secrets: keys never in URLs/`ps`/logs; 0600 artifact permissions (S11 — verify chmod after write);
  - LLM call timeout + 429 jittered retry + `Retry-After` (S12 — add a test with 429 mock);
  - SIGTERM cleanup (S13 — test).
- [ ] Enable GitHub: private vulnerability reporting, secret scanning + push protection, Dependabot, CodeQL, branch protection (PR + 1 approval), Discussions, `FUNDING.yml`, label hygiene + `good first issue` issues. Add 2nd admin.
- [ ] Spend safety (G9, still open): per-run token/spend cap + pre-run estimate + 90% warning — minimal version: hard cap in config with clear error, estimate printed before run.
- [ ] Telemetry decision: **opt-in, default off** (HN bar). If shipped at all: anonymous, disclosed, `DO_NOT_TRACK` honored, easy disable.
- [ ] Performance verification: run the Docker e2e with default limits (memory/cpu verified enforced); bounded channels in streaming path; long-run test (goal runner overnight); memory profile steady state.
- [ ] Eval honesty: run the existing offline harness; publish methodology + numbers in a `docs/benchmarks.md` (pin model+date+harness); no sweeping claims.

**Day 4–5 — Marketing kit & launch assets (all reproducible in a day)**
- [ ] **Demo**: VHS or asciinema+agg, **"0 → running"** (install → `niki run` → review branch), 60–90s, silent-with-annotations (no caption guidance exists for terminal video; annotations are the safe pattern), 2 versions (GIF for README, MP4 for PH/YouTube). Regenerate `assets/demo.gif` (the README one is pre-refactor).
- [ ] **Landing page** (static, GitHub Pages or single-page): headline = the thesis ("independent agents that can't influence one another"), 1 CTA (install command), hero demo GIF, 3 feature bullets, "What it's NOT" honesty section, waitlist/email capture, PH badge, og-image 1200×630 (allow OG route in robots.txt). One page, one CTA.
- [ ] **Product Hunt kit** (official spec): tagline ≤60 chars (draft: "Four independent AI agents that review each other — output: a reviewable git branch" = 77 → trim); thumbnail 240×240; **3 gallery images 1270×760** (hero run, TUI, branch+report artifact view); YouTube video (public); 3 tags; description ≤260 chars; **write the maker first comment** (features, who it's for, story, ask for feedback — never upvotes); schedule 12:01 AM PST launch day.
- [ ] **Show HN post**: hand-written (LLM-written posts are penalized), backstory + what's different (hermetic agents that can't influence each other) + link + no marketing language; prepare the account (karma needed); reply to every comment.
- [ ] **Social kit**: X thread (hook → demo clip → 3 differentiators → link; no engagement bait), LinkedIn post, dev.to cross-post (tutorial angle: "run your first multi-agent PR"), r/SideProject + r/rust + r/ClaudeAI-adjacent subreddits *where you're already active* (no cold spam).
- [ ] **README final pass**: new demo.gif, install matrix (cargo-dist script, brew/winget/scoop pending submission, cargo binstall, crates.io after publish), MSRV, badge set, "What it's NOT", docs link.
- [ ] **Announcement blog post** (dev.to/GitHub Discussions): problem → why now → what's in the box → 60-second example → caveats → contribute. (No authoritative template found — this is the workerd-pattern structure from Q3 research.)
- [ ] **Launch-day run sheet**: 00:01 PST PH live + first comment; 07:00–10:00 PST Show HN (Wed/Thu preferred); all-day comment duty (shifts); evening X/LinkedIn follow-ups; Day 2: PH badge install, respond to all day-1 feedback, waitlist nurture email, retrospective.

**Post-launch roadmap (market pull, all pre-researched):**
- **Phase A (week 1–2):** wire MCP skeleton (client, STDIO + HTTP, with the 2026 credential-vault pattern — OAuth tokens outside sandbox, proxy injects session-scoped creds) · sessions save/restore + undo/redo (wire `session/`) · headless/CI mode (`--json`, `--no-input`, exit codes) + GitHub PR-review action.
- **Phase B (weeks 3–6):** VS Code extension focused on **diff review of multi-agent runs** (the #1 IDE ask) · model autodiscovery + cross-provider fallback · chat TUI interactivity (if gated off) · `/compact` + spend dashboard in TUI.
- **Phase C (month 2+):** web/server + share links (opencode-web pattern) · Agent Skills (open standard — cheap to bolt on) · teams/enterprise gates (SSO, audit/OTel export) · benchmark page with honest methodology.

**Further research (explicitly open):** PH-vs-HN ROI for OSS in 2026 (contested); launch email-list tactics in a 5-day window (no validated case); keyring UX on headless Linux (Secret Service absence); X-thread anatomy with engagement data; terminal-demo captioning; Windows code-signing cost/benefit.

---

## Part 6 — Disagreements & Open Questions

1. **Telemetry**: clig.dev (opt-in ideal) vs gh CLI 2026 (default-on with opt-out). Resolved for NIKI: opt-in, default off — the HN community bar for new tools. (Carried as a deliberate choice, documented.)
2. **Demo video length**: 60–120s / <60s / 2-min guidance disagree. Resolved: 60–90s "0 → running" for acquisition, longer variant for sales/enterprise later.
3. **Product Hunt value for OSS 2026**: sentiment mixed-to-negative; no source favors it over HN. Resolved: HN primary, PH secondary, both same-day.
4. **Multi-agent efficacy**: Cognition vs Anthropic disagree. NIKI's shared-artifact pipeline is the documented reconciliation; lead marketing with it, never claim "swarms."
5. **SWE-bench contamination**: original-set distrust (OpenAI Verified) vs authors' date-partition analysis. Resolved: publish on Verified/Lite with methodology; no headline claims without it.
6. **OWASP Top 10 2026 (claimed Aug 4, 2026)**: **unverifiable — dropped.** Report cites the confirmed 2025 edition (LLM01/LLM06) and the Agentic Security Initiative as the active 2026 workstream. *(Verifier-resolved.)*
7. **Cursor figures**: sourced to Reuters/Bloomberg via a Wikipedia page carrying an AI-content warning banner. Used only as directional trajectory. *(Verifier-resolved.)*
8. **Single-source incident numbers** (14,645-token loop, 37 agents/2M tokens, 5/13 phantom edits): each from one claude-code issue — reported, directional, attributed by issue number; do not re-quote as facts in launch copy.
9. **"Managed Agents Apr 2026" / "Claude Code used inside MS/Google/OpenAI"**: unverifiable — removed. Verified anchor is Opus 4.6 agent teams (Feb 2026). *(Verifier-resolved.)*
10. **Reddit sentiment**: inaccessible to research (403) — community-demand signals rest on GitHub issues + HN.
11. **Internal audit line numbers**: verifier corrected several (allows at lib.rs:18, 16 clippy allows, chat ≈1,076 lines); the substantive claims (unreachable chat at runtime, skeleton modules, silent config traps, stale manifests, integration tests absent from CI) are confirmed by direct spot-checks and stand.

---

## Part 7 — Full Source List

**Internal (verified directly):** repo grep/read across src/ (module callers, config traps, tui.rs/layout/mod.rs/engine.rs reachability), `cargo build` + `cargo test --no-run`, .github/workflows/ci.yml + integration.yml, homebrew/niki.rb + scoop/niki.json + winget/*.yaml, Cargo.toml/dist-workspace.toml, git log + tags + branches, .kilo/goals/*.json, .gitignore, assets/, docs/distribution-plan.md, prior research/ files (feature-gap, launch-security, uiux-refactor, color-token).

**Engineering & CLI UX:** github.com/axodotdev/cargo-dist (book: installers, updater, attestations, supply-chain) · github.com/cargo-bins/cargo-binstall · github.com/astral-sh/uv · github.com/BurntSushi/ripgrep · github.com/helix-editor/helix · github.com/casey/just · github.com/EmbarkStudios/cargo-deny · github.com/zkat/miette · clig.dev · no-color.org · docs.rs/clap_complete · docs.rs/keyring · github.com/release-plz/release-plz · doc.rust-lang.org/cargo/guide/continuous-integration.html · rust-lang/api-guidelines discussion #231 · docs.github.com/en/github-cli/github-cli/github-cli-telemetry · keepachangelog.com · github.com/rust-lang/mdBook.

**Auth & onboarding:** developers.openai.com/codex/auth · cli.github.com/manual/gh_auth_login · opencode.ai/docs/providers · github.com/google-gemini/gemini-cli · code.claude.com/docs/en/settings · github.com/heygen-com/heygen-cli PR #17 · github.com/gptme/gptme PR #1529 · github.com/NousResearch/hermes-agent PR #20162 · github.com/coder/coder issue #20256 · docs.rs/keyring · github.com/schpet/linear-cli issue #130.

**OSS launch & community:** opensource.guide/starting-a-project · opensource.guide/legal · opensource.guide/leadership-and-governance · opensource.guide/finding-users · github.blog (6 security settings; good-first-issues) · docs.github.com (releases, FUNDING, discussions, PVR) · news.ycombinator.com/showhn.html · HN: dang comment 22336638, threads 43970837, 41862332, 38316936, 49245437, 49205732, 46583112, 41487769, 46958742, 47484259 · github.com/distr-sh/distr · github.com/KingMenes/awesome-launch.

**Marketing:** github.com/charmbracelet/vhs · github.com/asciinema/agg + docs.asciinema.org/manual/agg · producthunt.com/launch/preparing-for-launch · producthunt.com/launch/launch-day-duties · producthunt.com/launch/days-after-launch · producthunt.com/launch/sharing-your-launch · help.producthunt.com (how-to-post, badges) · flowjam.com/blog/startup-product-demo-video · levitatemedia.com/learn/best-software-demo-video · demosmith.ai · vercel.com/docs/og-image-generation · posthog.com/handbook/growth/marketing.

**Security:** genai.owasp.org (LLM Top 10 2025: LLM01, LLM06; Agentic Security Initiative) · pillar.security/blog/the-week-of-sandbox-escapes · embracethered.com/blog/posts/2025/github-copilot-remote-code-execution-via-prompt-injection · legitsecurity.com (GitLab Duo) · promptarmor.com (Antigravity) · NVD + github.com/advisories (CVE-2025-54794, 55284, 58764, 59041, 59536, 59828, 59829, 53773; CVE-2026-39861) · code.claude.com/docs/en/security · claude.com/blog/ciso-guide-to-agentic-ai · docs.sigstore.dev · github.com/rust-secure-code/cargo-auditable · CycloneDX/cargo-cyclonedx · securitytxt.org · HN threads 49245437, 46706796, 48892859, 46592344.

**Performance & evals:** anthropics/claude-code issues #76742, #84323, #66950, #68740, #82434, #83785, #77034 · docs.docker.com/engine/containers/resource_constraints · docs.rs/tokio (mpsc) · OpenHands PRs #15236, #14953 · github.com/ratatui/ratatui PRs #2276, #2646, #2647 · docs.rs/jemallocator · docs.rs/tokio-console · docs.litellm.ai/docs/routing · ar5iv.labs.arxiv.org/html/2310.06770 (SWE-bench) · openai.com/index/introducing-swe-bench-verified · swebench.com · arxiv.org/abs/2307.03172 (Lost in the Middle).

**Market:** en.wikipedia.org/wiki/Cursor_(company) (Reuters/Bloomberg refs; AI-banner caveat) · en.wikipedia.org/wiki/Claude_(AI) · en.wikipedia.org/wiki/Model_Context_Protocol · cognition.ai/blog/dont-build-multi-agents · anthropic.com/engineering/built-multi-agent-research-system · code.claude.com/docs/en/best-practices · github.com/anomalyco/opencode issues #5887, #6231, #7602, #8003, #8751, #10288, #11176, #20596, #27167, #6355 · codeaholicguy.com/2026/01/10/claude-code-vs-cursor · HN thread 48892859 (METR/DORA) · HN Algolia Show HN mining.

---

## Appendices

**A. Launch-blocker table (all fixable in hours, verified)**

| # | Blocker | Severity | Fix | Time |
|---|---|---|---|---|
| B1 | Package manifests: BUSL-1.1 + stale v0.2.0 URLs | CRITICAL | Regenerate for Apache-2.0 + new tag | 1h |
| B2 | Stale v0.2.0 tag (binary lacks license/security fixes) | CRITICAL | Tag v0.3.0 from master | 30m |
| B3 | Skeleton config traps silently ignored | CRITICAL (trust) | Warn or wire; mark roadmap | 2h |
| B4 | 220 integration tests never run in CI; Docker path untested | HIGH | Add `cargo test --test '*'` + docker e2e job | 2h |
| B5 | No published sandbox image | HIGH | Push ghcr image; update default | 2h |
| B6 | No audience/landing/demo | CRITICAL (GTM) | Days 4–5 kit | 2d |

**B. Feature-truth table (what to say at launch)**

| Feature | Say | Don't say |
|---|---|---|
| Multi-agent pipeline + adversarial review | "Four role-isolated agents exchanging typed artifacts; independent reviewers" | "Four specialized agent engines" (they're one runner with role configs) |
| Hermetic sandbox | "Container path: hermetic by default, enforced proof" | "Hermetic everywhere" (worktree = host execution, documented) |
| TUI | "Live pipeline viewer with command palette" | "Chat UI" (unreachable today) |
| Sessions/MCP/undo/permissions/compaction | "On the roadmap, wiring in progress" | "Shipped" (skeletons only) |
| Eval | "Offline replay harness, 24 cases, methodology published" | "SWE-bench score: X" (none published yet) |

**C. Post-launch feature queue (market-pull ranked, with skeleton status)**

1. MCP client (skeleton exists) + credential-vault proxy pattern — baseline interop
2. Headless/CI mode + JSON + GitHub PR-review action — agents-in-CI is the CLI trend
3. Sessions save/restore + undo/redo (skeleton exists) — top table-stakes
4. Chat TUI interactivity (compiled, unreachable; gate outcome)
5. VS Code extension: diff review of multi-agent runs (79–138+ reactions in demand data)
6. Model autodiscovery + cross-provider fallback (206+ reactions)
7. Context compaction + spend caps UI (90% warning, grace mode)
8. Web/server + share links; then Agent Skills; then teams/enterprise gates

**D. Resolved verifier issues (traceability)**

1. OWASP "Top 10 2026" claim dropped (unverifiable) → 2025 edition + Agentic Security Initiative. 2. Line-number corrections applied; substantive claims re-verified by direct spot-check. 3. Chat-layer status corrected to "compiled, unreachable at runtime" (layout/mod.rs renders it; tui.rs never selects Chat view). 4. Cursor trajectory attributed Reuters/Bloomberg, marked directional. 5. Incident figures attributed to specific claude-code issues. 6. "Managed Agents Apr 2026"/"Claude Code inside MS/Google/OpenAI" removed (unverified).

*End of report.*
