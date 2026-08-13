# NIKI — Pre-Launch Research & Execution Plan (target: launch 2026-08-18)

**Date:** 2026-08-13 · **Depth:** Wide (landscape + codebase audit + competitor + sandbox + GTM)
**Scope of this document:** (1) gaps & marketing-value gaps before a polished product, (2) competitor-relative positioning (how it can be better; why it is still high-value / low-competition / high-reward), (3) deep code-logic audit + improvements, (4) Docker microVM evaluation, (5) user-journey / activation / retention / viral, (6) launch + "hashtag-one" (Product Hunt #1) plan, (7) actionable execution plan.

> **Status of the previous `research/` files:** A cross-check of the 5 prior research docs
> (`niki-*.md`, `claude-code-*-refactor.md`) against the actual codebase shows they are
> **NOT all complete** — the launch plan has real blockers (no v0.3.0 tag, stale package
> manifests, config-traps silently accepted, sandbox image not published, no Docker CI e2e,
> no landing page, marketing only specs), the feature-gap items exist as **unwired modules**,
> and security has **1 unaddressed finding** (S14 dead controls). Per the standing rule
> ("delete only if everything is completed"), those files were **kept** and their remaining
> gaps are folded into this document. This file supersedes them as the launch source of truth.
>
> **Image / `token.md`:** You pasted an image for color extraction; this model cannot read
> image input (clipboard read failed). Per the 3-try rule, `research/token.md` was
> **reconstructed from the shipped `Midnight Teal` palette in `src/display/theme.rs` and
> `assets/logo.svg`** — the real brand. If your pasted image intended different colors, say so.

---

## 1. Executive Summary

NIKI is a Rust CLI that runs a **Planner → Coder → Tester → Reviewer** pipeline as *independent*
agents inside a Podman/Docker sandbox and hands back a reviewable `niki/<id>` git branch plus a
full audit trail (`report.md` + `artifacts/*.json`). That combination — **agent independence,
hermetic-by-default execution, and a first-class audit trail, all open-source and BYOK** — is a
genuinely differentiated and still relatively uncontested wedge in a very crowded market.

But the product is **not yet launch-polished**. The blockers are not the agent logic (which is
solid and security-hardened through S2–S13); they are **distribution, onboarding friction, and
trust copy**. Concretely: (a) install is ~15+ steps (clone, `cargo build`, manual `podman build`,
config) — violating the <5-min-to-first-success rule; (b) the package manifests still point at
`v0.2.0` and one carries the wrong license; (c) four parsed-but-inert config sections
(`[session]`, `[mcp]`, `[permissions]`, `[compaction]`) are silently accepted — a trust trap;
(d) there is no landing page, no "What NIKI is NOT" honesty section, and no published benchmarks;
(e) the logo brand (`#58a6ff` blue) contradicts the TUI brand (teal `#0d9488`).

**Recommendation:** Ship a **v0.3.0 "launch" cut** that closes the distribution/onboarding/trust
gaps, keep the agent engine as the differentiator, and run a **multi-channel launch** (Product
Hunt #1 attempt + staggered Show HN + GitHub + community) on/around 2026-08-18. Treat Docker
Sandboxes microVMs as an **optional hardening backend** on the roadmap, not a launch gate.

---

## 2. Competitor Landscape & Positioning

### 2.1 The field (Aug 2026)
| Tool | Maker | License | Isolation | Multi-agent? | Model lock | Notes |
|---|---|---|---|---|---|---|
| Claude Code | Anthropic | Proprietary | Permission system | Agent Teams (worktrees) | Claude only | ~82% SWE-bench Verified; deepest ecosystem |
| Codex CLI | OpenAI | Apache-2.0 | **OS-level** (Seatbelt/Landlock) | Automations | OpenAI only | 83.4% Terminal-Bench; fastest |
| Gemini CLI → **Antigravity CLI** | Google | was Apache-2.0; Antigravity closed | Permission | Multi-subagent | Gemini only | "Retired" per one source; contested — see §9 |
| OpenCode | Anomaly | MIT | Confirmation | Agents/plugins | 75+ providers | Most-starred OSS (172k★) |
| Cline | Cline | Apache-2.0 | Confirmation | Yes | Any | Best OSS CLI |
| OpenHands | All Hands | MIT | Sandbox/headless | autonomous | Any | Most autonomous OSS |
| Aider | community | Apache-2.0 | Confirmation | No | 100+ | Git-native auto-commit |
| Goose | Linux Fdn | Apache-2.0 | MCP | yes | 15+ | Broadest (beyond code) |
| Copilot CLI / Kiro | MS / Amazon | source-available | Permission | yes | vendor | Distribution moat |
| Devin | Cognition | proprietary | cloud | autonomous | vendor | $25B valuation talks |
| **NIKI** | RavaniRoshan | **Apache-2.0** | **Container + worktree + cloud** | **Independent agents** | **BYOK 4+** | **Hermetic + auditable** |

(Sources: amux.io 2026-05, morphllm.com 2026-06, hidekazu-konishi.com 2026-07, capitalandcompute.net
2026-06, presenc.ai 2026-05, gartner.com 2026-05, zylos.ai 2026-04.)

### 2.2 Where NIKI is already differentiated (lean into these)
1. **Independence as a principle, not a feature.** Competitors run one long context (context
   drift, confirmation bias) or vendor-locked subagents. NIKI's agents are isolated at *both*
   the filesystem layer (separate sandbox/worktree copy) *and* the context layer (typed
   artifacts only). This is the "stop babysitting your AI" story — and it's technically real.
2. **Hermetic by default.** The working tree is never mutated mid-run; output is a branch. Most
   competitors mutate your tree or need `--dangerously-skip-permissions`. NIKI's "proof, not
   promises" + `report.md`/`artifacts/*.json` is a trust differentiator for skeptical devs.
3. **BYOK + per-agent model routing.** Cheap Tester, strong Planner/Reviewer — explicit cost
   control competitors mostly lack.
4. **Open source + Rust + multi-backend** (Podman/Docker/worktree/cloud seam). No vendor lock-in.
5. **Parallel coders + synthesis** already implemented — the "fan-out one brief" capability
   jurniti notes most agents *lack*.

### 2.3 Where competitors beat NIKI today (gaps to close or message around)
- **Ecosystem/extensions:** Claude Code (skills, hooks, subagents, SDK), Goose (70+ MCP),
  OpenCode (plugins). NIKI's MCP/permissions/sessions are **unwired** (S14). → Either wire the
  highest-value one (sessions/history) or explicitly message "batteries-included core, not a
  kitchen sink" and ship a plugin seam.
- **Distribution:** NIKI has ~0 public presence vs Claude Code/Cursor/Codex gravity. → This is
  the launch's real job, not a code gap.
- **Model quality ceiling:** NIKI inherits model quality (BYOK). Message as a feature
  ("bring the best model for each role"), not a weakness.
- **IDE surfaces:** NIKI is terminal-only. Acceptable for the ICP (solo devs, indie hackers,
  2–5 teams) — don't over-invest in IDE before launch.

### 2.4 High-value / low-competition / high-reward thesis
The contested "multi-agent coding" space is crowded at the *single-agent* and *cloud-autonomous*
ends, but the **"independent agents + hermetic + auditable + open-source + BYOK" intersection is
thin**. Open-source multi-agent orchestrators that (a) never touch your tree, (b) produce a
reviewable branch, and (c) expose the full decision trail are rare. That is NIKI's defensible,
low-competition wedge with high reward if it captures the "I don't trust one agent with my repo"
segment. The risk is distribution (a marketing problem, solvable) and the unwired features
(a credibility problem if over-claimed — hence the honesty section in §6).

---

## 3. Gaps & Marketing-Value Gaps Before a Polished Product

### 3.1 Product/engineering gaps (launch blockers)
| # | Gap | Evidence | Launch impact |
|---|---|---|---|
| B1 | Package manifests stale (`v0.2.0`); winget license `BUSL-1.1` | homebrew/niki.rb, scoop/niki.json, winget/*.yaml | Wrong/old installs |
| B2 | No `v0.3.0` tag; Cargo.toml `0.2.0`; CHANGELOG still `Unreleased` | Cargo.toml, CHANGELOG.md | No release to point at |
| B3 | Config traps silently accepted (`[session]/[mcp]/[permissions]/[compaction]` parsed, inert) | `src/config/types.rs:855-873` | Trust damage on first run |
| B4 | Docker backend never exercised in CI (e2e uses `--backend worktree` only) | ci.yml | Unverified primary backend |
| B5 | Sandbox image not published; requires manual `podman build` | `src/config/types.rs:819` default `niki-sandbox:24.04` | Heavy first-run friction |
| B6 | No landing page (only `docs/marketing/landing.md` spec) | docs/marketing/ | PH/SEO dead on arrival |
| B7 | No "What NIKI is NOT" honesty section | README | Skeptical devs bounce |
| B8 | No `docs/benchmarks.md` | absent | "Proof not promises" unbacked |
| B9 | Spend cap (G9) unimplemented | no matches in src/ | Runaway-cost fear |
| B10 | Logo brand `#58a6ff` ≠ TUI teal `#0d9488` | assets/logo.svg vs theme.rs | Inconsistent brand |
| B11 | Live chat renders agent output as **raw text** (markdown/syntax-highlight renderer is dead code) | `src/display/chat/markdown.rs` unwired | "Polished" claim weak |
| B12 | `demo.gif` may be pre-refactor | assets/demo.gif | Stale first impression |

### 3.2 Marketing-value gaps (the ones that actually move launch)
- **Time-to-first-success is ~15+ steps** (clone → `cargo build --release` → `podman build` →
  `cp niki.example.toml` → export key → run → switch branch). Best-in-class dev tools hit <5 min.
  This is the single biggest marketing-value gap: you cannot convert a PH visitor who can't get
  to a working branch in one sitting. → One-line install + prebuilt published image + `niki init`
  wizard + a `niki quickstart` sample task.
- **No shareable artifact (viral loop missing).** NIKI's output (branch, `report.md`) is local.
  There is no "Built with NIKI" trailer, no public URL, no `niki share`. Dev-tool PLG data shows
  **built-in sharing → 2–3× higher activation**. → Add a PR trailer line + (post-launch) a hosted
  `niki share`/`dashboard` link.
- **No community surface** (Discord/Slack). Retention + word-of-mouth live there.
- **No activation instrumentation.** Can't measure "first successful run." (Telemetry must stay
  opt-in/off by default — privacy is part of the brand. Use an opt-in anonymous ping + local
  funnel via `niki status` counts.)
- **No "evaluating developer" fork.** First-run should answer "is this for me?" before asking for
  a key. → A `niki quickstart` that runs a *demo task on a bundled sample repo* with no API key
  would be huge (sandbox/preview mode). If a keyless demo is infeasible, at minimum lead the
  README/landing with the outcome ("a clean branch + audit trail"), not the setup.

---

## 4. Deep Code-Logic Audit & Improvements

(NB: the prior `claude-code-kimi-code-uiux-refactor` and `niki-*-gap-analysis` docs were
re-verified against code; the findings below consolidate them and add concrete fixes.)

### 4.1 Run pipeline (how it actually works)
`niki run "<task>"` → CLI (`src/cli/run.rs`) → `Orchestrator` sequences
`Planner → Coder → Tester → Reviewer` as a pipeline; each stage runs in a `Sandbox`
(Podman/Docker/worktree/cloud) against a *copy* of the repo; the Reviewer can loop back to the
Coder up to `max_revision_rounds`; on approval the orchestrator captures the working-tree diff,
commits to a fresh `niki/<id>` branch, and writes `report.md` + `artifacts/*.json`. User-defined
`[pipeline]`, `[parallel]` (N coders + synthesis), `[security]` auditor, and `[knowledge]` ingestion
are all implemented and reachable. **This core is solid.**

### 4.2 Improvements found (list)
1. **Wire or remove dead UI code.** `src/display/{engine,state,input,layout,components}/*` is a
   second, unwired TUI implementation (zero callers). The live path is `pages/*` + `tui.rs`.
   Either promote the dead renderer (markdown transcript, `RenderEngine`, reactive `Store`) or
   delete it. *Recommendation: delete the dead tree pre-launch to cut build/confusion risk; the
   live renderer is enough for v1.* (low risk, cleans ~1.5k LOC.)
2. **Render the agent transcript as markdown in the live chat** (the `chat/markdown.rs` +
   `code_block.rs` exist but are unused). This is the biggest "polished" win for the interactive
   experience. *Medium effort.*
3. **Fix the config-trap trust bug (B3).** `warn_unknown_sections` *excludes* the four inert
   sections so they're silently accepted. Emit an explicit "parsed but not yet active — see
   roadmap" warning at load. *Small, high-trust fix.*
4. **Wire `sessions`/`undo` or hide them.** `SessionManager`, checkpoints, undo/redo exist as
   unwired modules + tests. Retention depends on "pick up where I left off." *Wire at least
   session save/restore + a `niki history`/resume, or remove from docs until shipped.*
5. **S14 — dead security controls.** `permissions/`, `observability/`, `audit/`, `mcp/` are
   instantiated nowhere; `mcp.enabled` defaults `true` while unwired → false sense of protection.
   *Either wire MCP (it's the ecosystem differentiator) or default `mcp.enabled=false` and document
   as explicitly inactive.* Top security priority.
6. **S6 residual — worktree backend runs agent commands on the host** with the user's privileges,
   no privilege drop/seccomp, and no user-facing warning. *Add an explicit "worktree = host
   execution" warning + recommend the container backend; this is exactly what Docker microVMs
   would fix (§5).*
7. **Spinner/prompt color conflict** — `theme.rs claude()` returns purple and is used for the
   logo/spinner, contradicting the teal brand. Rename or switch to `accent.primary`.
8. **`text.muted` mis-map** — code returns `#8b949e` (fg_dim) while the design doc's `text.muted`
   = `#6e7681` (fg_subtle). Align.
9. **Auto theme detection** — `ThemeMode::Auto` only falls back to Dark; add OSC 4 / `COLORFGBG`
   probing for true auto.
10. **Spend cap (B9/G9)** — add `general.spend_cap_usd` + per-run pre-estimate abort/warning in
    `run.rs`. Addresses the #1 fear of autonomous agents (cost).
11. **Docker CI e2e (B4)** — add a CI job exercising `--backend docker` (or podman) so the primary
    backend is actually verified before launch.

---

## 5. Docker VM / MicroVM Evaluation

### 5.1 What launched
- **Docker VMM** — first-party virtualization layer under Docker Desktop, public beta
  **2026-08-12** (Docker Desktop v4.86), Mac + Windows. Replaces the third-party VMM; same engine
  powers **Docker Sandboxes (SBX)**. (docker.com/blog/docker-vmm-public-beta)
- **Docker Sandboxes (SBX)** — each agent runs in a dedicated **microVM** (its own kernel), only
  the project workspace mounted, a **private Docker daemon inside the microVM**, network
  allow/deny lists, and **credentials injected at runtime outside the microVM** (never inside).
  "YOLO mode" is safe because of the VM boundary. Install: `brew install docker/tap/sbx`,
  `winget install Docker.sbx`. (docker.com/products/docker-sandboxes, docker.com/blog/why-microvms)
- It is **microVM-based, not container-based**: hardware/kernel isolation vs NIKI's current
  shared-kernel container isolation. That is a strictly stronger boundary for "untrusted agent
  code." Firecracker is the well-known alternative but is Linux/KVM-only; Docker built its own
  VMM for cross-platform (macOS Hypervisor.framework, Windows Hypervisor Platform, Linux KVM).

### 5.2 Is it useful to NIKI?
**Yes — but as an optional hardening backend, not a launch gate.**
- **Directly addresses S6** (worktree backend executes on the host) and strengthens the
  container backend's shared-kernel limit. A `--backend docker-sandbox` would let NIKI offer
  kernel-grade isolation with a private daemon and runtime credential injection — a strong
  *security* differentiator to message.
- **Caveats:** (1) beta, Mac/Windows only (no Linux yet) — NIKI is cross-platform incl. Linux
  servers/CI, so it can't be the default; (2) requires Docker Desktop + a heavy dependency vs
  NIKI's lean rootless Podman/Docker story; (3) Docker Sandboxes currently whitelists which
  agents can run and (per reverse-engineering) doesn't yet let you run *your own* container image
  freely — NIKI would need SBX to support arbitrary images or run *inside* a sandbox; (4) launch
  is in 5 days — too late to depend on it.

**Recommendation:** Add `docker-sandbox` to the **roadmap** as a opt-in backend behind the
existing `Sandbox` trait; pilot a proof-of-concept after launch. For launch, the lever is:
default to the **container backend with the existing S2 hardening** (CapDrop ALL, PidsLimit,
network/readonly options, digest-pin warning) and **clearly warn** when using the worktree
backend (S6). This gives a credible, honest isolation story without a beta dependency.

---

## 6. User Journey: Click → Install → Engage → Retain → Viral

### 6.1 Define activation
**Activation = first `niki run` that produces an approved `niki/<id>` branch the user switches to
and opens.** Instrument it locally (count of approved runs per install) — opt-in only.

### 6.2 Click → Install (fix friction)
- One-line install must exist on day one: `brew install niki`, `winget install Niki.Niki`,
  `scoop install niki`, `cargo install niki` (or `cargo binstall`), **plus a prebuilt sandbox image
  published to `ghcr.io/ravaniRoshan/niki-sandbox`** so `niki init` can pull it instead of building.
- `niki init` wizard: detect container runtime, pull image, write `niki.toml`, validate key, run a
  smoke task. Target: **< 5 minutes, zero manual `podman build`.**

### 6.3 Engage (first run)
- Lead with the outcome, not setup: show the branch + `report.md` + dashboard link immediately.
- `niki run` with a friendly first-task suggestion; `--dry-run` to preview the plan; `niki dashboard`
  opens the static HTML diff viewer.

### 6.4 Retain
- `niki status` / `niki report <id>` / `niki dashboard` for revisit.
- Wire **sessions/history** (§4.2 #4) so users resume — the #1 retention lever for agent tools.
- `niki recommend` (cost/quality per role) keeps them tuning.
- Opt-in anonymous activation ping + a community (Discord) for stickiness.

### 6.5 Viral / marketing loops
1. **PR/branch trailer** — every NIKI branch gets a trailer: `Generated by NIKI · niki run "<task>"`
   + link. Zero-cost, high-visibility in every review. (ship in v0.3.0)
2. **Shareable dashboard** (post-launch) — `niki share` uploads the static dashboard to a hosted
   URL with a "Built with NIKI" footer → viewer → signup loop.
3. **Product Hunt #1** (§7) — badge on README/landing for a year of SEO/social proof.
4. **Show HN + GitHub + community** staggered — technical honesty converts skeptics.
5. **Demo GIF** in README + PH gallery + landing hero.
6. **Referral via "Built with NIKI"** in open-source PRs → discoverability in the dev's network.

---

## 7. Launch & "Hashtag-One" (Product Hunt #1) Plan

Interpretation: "hashtag one" = **Product Hunt Product of the Day #1** on/around 2026-08-18.
(NIKI is a dev tool, so PH is the *credibility* channel; pair with Show HN + GitHub — PH alone
rarely drives dev-tool adoption.)

### 7.1 What #1 actually requires (2026 mechanics)
- **120–180 quality upvotes in the first 3 hours**; peak early or you don't climb.
- **Maker online, replying < 8 min** for the first 4 hours (100% of #1s do this).
- **First comment 180+ words**: personal story → why existing tools failed → what you built →
  one honest limitation → a question. (Comment depth now ranks alongside upvotes.)
- **Comment-to-upvote ratio > 8%** (dense, specific comments beat thin ones).
- **Pre-built networks 3–4 weeks out** (pacts/notify-me). You're at ~5 days, so: stand up a
  "coming soon"/notify page *now*, brief 200–300 genuine contacts, and lean on Show HN + GitHub
  where dev tools actually convert.
- **Multi-channel sequence:** Day 0 PH (badge/backlink/reach) → Day 1 Show HN (plain, low-key) →
  same-week GitHub pin + community posts. Don't coordinate-spam; stagger.
- Best day: **Tue–Thu 12:01 AM PT** for traffic; **weekend** for easier #1. With 5 days' notice,
  pick the least-competitive weekday and prep assets now.

### 7.2 Assets to build before launch (all locally doable)
- Landing page (static HTML, GitHub Pages) — hero demo, one-line install, "what it is / isn't",
  `report.md` sample, benchmarks link. (B6)
- Gallery: demo GIF + 3–5 real screenshots (terminal run, branch, dashboard, audit trail).
- First comment draft (origin story + differentiator + honest limitation + ask).
- README: install matrix, "What NIKI is NOT", MSRV, docs link, PH badge slot. (B7)
- `docs/benchmarks.md`: honest — hermetic-proof explanation + "we don't publish head-to-head
  SWE-bench; here's our eval harness" (B8). Do **not** fake numbers.

### 7.3 Funnel (every search angle)
- **Owned:** README, docs, landing, demo GIF, `niki share` (post-launch).
- **Earned:** Show HN, GitHub trending/stars, Dev.to/Hashnode technical post, r/rust, r/learnrust,
  r/programming, Discord/Slack dev communities — runnable content, not link-drops.
- **PH:** listing + maker comment + comment engagement.
- **LLM-answer surface:** ensure NIKI ("turn a sentence into a reviewable PR, hermetic, multi-agent,
  open-source") is described accurately on the site so AI search surfaces it correctly.
- **Community:** Discord from day 1; "Built with NIKI" trailers in every PR.

---

## 8. Implementation Plan (actionable, in execution order)

**Phase 0 — Verify build (do first):** `cargo build --release` + `cargo clippy` clean on master.
**Phase 1 — Version & manifests (B1, B2):**
  1. Bump `Cargo.toml` → `0.3.0`; add `CHANGELOG.md` `## [0.3.0]` (move `Unreleased`).
  2. Fix `homebrew/niki.rb`, `scoop/niki.json` → `0.3.0` + correct release URLs.
  3. Fix `winget/RavaniRoshan.niki.installer.yaml` license `BUSL-1.1` → `Apache-2.0`, version `0.3.0`.
**Phase 2 — Trust & safety (B3, B9, S14, S6):**
  4. `src/config/types.rs`: warn on inert `[session]/[mcp]/[permissions]/[compaction]` at load.
  5. Default `mcp.enabled = false`; document permissions/audit/mcp as explicitly inactive (S14).
  6. `run.rs`: add `general.spend_cap_usd` pre-estimate guard (warn/abort).
  7. Worktree backend: print explicit "host execution" warning (S6).
**Phase 3 — Distribution/onboarding (B5, B11):**
  8. Publish `niki-sandbox` image to `ghcr.io/ravaniRoshan/niki-sandbox`; default `base_image` to it.
  9. `niki init` wizard polish (detect runtime, pull image, validate key, smoke task).
  10. Wire live chat markdown transcript (or delete dead UI tree) — pick one.
**Phase 4 — Docs/marketing (B6, B7, B8, B10, B12):**
  11. README: install matrix, "What NIKI is NOT", MSRV, PH badge slot, docs link.
  12. `docs/benchmarks.md` (honest).
  13. `docs/marketing/landing.html` (static, GitHub Pages) + regenerate `demo.gif`.
  14. `assets/logo.svg` → teal `#0d9488` to match TUI (B10); reconcile `token.md`.
**Phase 5 — CI (B4):** add Docker/podman e2e job.
**Phase 6 — Launch (2026-08-18):** tag `v0.3.0`, GH release, landing live, PH listing + maker
  comment, Show HN next day, Discord live, community posts.
**Phase 7 — Post-launch:** `niki share`/hosted dashboard (viral loop), wire sessions/history,
  Docker-Sandbox backend POC, instrument opt-in activation.

> Items executed in this session: see "Execution" section below. Remaining (tag/release/PH/landing
> deploy/community) require network/accounts and are left as ready-to-run steps.

---

## 9. Disagreements & Open Questions
- **Gemini CLI status.** One source (claudify.tech, 2026-07) claims Google *retired* Gemini CLI
  (2026-06-18) for closed-source **Antigravity CLI**; other June-2026 sources (amux, morphllm)
  still list Gemini CLI active with 1,000 req/day, and capitalandcompute describes Antigravity 2.0
  as a *new* standalone agent (Google I/O 2026-05-19). **Likely both are true in part**: Antigravity
  is the new Google agent; Gemini CLI's future is unclear. NIKI positioning is unaffected — we
  compete on independence/hermetic/BYOK, not on Google's model.
- **PH #1 feasibility in 5 days.** Without 3–4 weeks of network-building, a true #1 is unlikely;
  the realistic, honest target is a **strong top-5 + a credible Show HN + GitHub traction**, using
  PH #1 as the aspiration and the badge as the asset. Set expectations accordingly.
- **Dead UI tree:** promote vs delete — unresolved; recommended delete for launch simplicity.
- **Docker Sandboxes for Linux:** not available; blocks it as a default backend.

## 10. Source List
- docker.com/blog/docker-vmm-public-beta (2026-08-12)
- docker.com/products/docker-sandboxes ; docker.com/blog/why-microvms ; docker.com/blog/docker-sandboxes-run-claude-code
- rivet.dev/blog/2026-02-04-we-reverse-engineered-docker-sandbox (microVM API)
- aws.amazon.com/blogs/compute/announcing-lambda-microvms (2026-07)
- amux.io/blog/best-terminal-ai-coding-agents-2026 (2026-05)
- morphllm.com/best-ai-cli-tools-2026 (2026-06) ; hidekazu-konishi.com (2026-07)
- capitalandcompute.net/blog/the-2026-ai-coding-agent-landscape (2026-06)
- presenc.ai/research/ai-coding-agent-market-2026 (2026-05) ; gartner.com (2026-05)
- zylos.ai/research/2026-04-02-agentic-coding-production ; forrester.com (2026-07)
- launchpact.io/blog/product-hunt-launch-analysis (2026-05) ; phlaunchkit.com (2026-07)
- gingiris.tools/blog/2026/03/18 (30-day playbook) ; pristren.com (2026-05) ; producthunt.com/launch
- causo.ai/guides/product-hunt-launch-2026 ; hackmamba.io (2026-01)
- skene.ai/resources/blog/developer-onboarding-guide (2026-03) ; builtfor.dev (2026-03)
- lukestahl.io/blog/plg-for-developer-companies (2026-05) ; dev.to (PLG activation funnel, 2026-07)
- stackmatix.com/blog/gtm-for-devtools-startups (2026-08) ; gtm-labs.co (2026-05)
- Codebase: src/config/types.rs, src/cli/run.rs, src/display/{theme,tui,pages,chat}.rs,
  src/sandbox/*, src/orchestrator/*, src/session/mod.rs, src/mcp/mod.rs, src/permissions/mod.rs,
  research/niki-*.md (prior, kept).
