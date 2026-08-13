# NIKI Launch Readiness — Wide Research Report (2026-08-13)

**Scope:** Fresh, code-grounded + web research for the **Aug 18, 2026 public launch** of NIKI v0.3.0 ("a sentence → a reviewable pull request"; Planner→Coder→Tester→Reviewer agents in a local Podman/Docker sandbox, BYOK). Covers product gaps, competitive positioning, Docker microVM technology, first-user journey (activation → retention → virality), security/trust bar, benchmark credibility, and the marketing/search funnel.

**Method:** 7 parallel research streams (1 code-audit agent, 6 web-research agents), then an adversarial verification agent that spot-checked ~15 load-bearing claims against primary sources. Conflicting claims were adjudicated against code or carried here as flagged disagreements. Every claim carries its source.

---

## 1. Executive summary

NIKI is launch-ready at the *engineering* level — a full code-vs-docs audit shows every launch-blocking item (security items S1–S13, launch gates G1–G11, packaging, eval harness) is implemented, HEAD compiles clean, and what remains is cosmetic debt plus four small trust/UX drift items. The market research shows NIKI occupies a genuinely **unoccupied combination** — single-binary Rust CLI + four-agent pipeline + *local* sandbox + BYOK + spend cap + audit trail — in a category where the biggest noise (CodeRabbit-class review bots) is publicly hated for low signal, the biggest autonomy player (Devin) is priced 5-10x out, and the nearest functional analogue (OpenHands) is a heavyweight Python platform. The launch itself should be run as a **48-hour, GitHub-Trending-targeted, Rust-community-first event** (Show HN + This Week in Rust + Reddit + PH as badge channel), with honest benchmark disclosure as the trust wedge — 2026 punishes benchmark hype, not self-restraint. Critical pre-launch work: fix four code/trust drift items found by the audit, finalize the design-token system, publish the funnel plan, and make the README convert in 10–15 seconds. Security posture is solid but **egress is not default-blocked** and the **spend cap warns instead of aborts** — both acceptable for v0.3 if honestly documented, both should be hardened post-launch.

---

## 2. Background & key terms

- **NIKI:** Apache-2.0, Rust, CLI. User types a sentence; Planner→Coder→Tester→Reviewer agents run in a local Podman/Docker sandbox; output = git branch + diff + audit trail. BYOK (Anthropic/OpenAI/Google/local). v0.3.0 "Launch cut" is committed (trust warnings, spend cap, sandbox image published to GHCR, manifests for Homebrew/Scoop/Winget, marketing kit).
- **SWE-bench Verified:** long the coding-agent benchmark; **dead as a credibility signal in 2026** (OpenAI's own audit found 59.4% broken tests + contamination, OpenAI stopped reporting Feb 2026; leaderboard submissions academic-only since Nov 2025).
- **MicroVM / sandboxing:** Firecracker (AWS, microVMs), gVisor (userspace kernel), Kata (VM-per-container), Docker Sandboxes (see §4).
- **GEO/AEO:** making content get cited by AI assistants (ChatGPT/Perplexity/Claude) — separate from Google SEO; llms.txt helps third-party engines, not Google.
- **Show HN / Product Hunt mechanics:** empirical 2026 data, see §5.

---

## 3. Findings by sub-question

### SQ1 — Product readiness: what the research docs promised vs. what the code actually does

Method: one agent read all 6 research files, extracted a compliance matrix (~40 items), verified each against source with file:line evidence; `cargo check --all-targets` passed twice (fresh rebuild, 33s, warnings only).

**Verdict: everything launch-blocking is implemented.** Highlights:

| Area | Status | Evidence |
|---|---|---|
| S1 per-role + global command deny-list | DONE | `src/sandbox/mod.rs:71-72`, `worktree.rs:215`, `docker.rs:495` |
| S2 container hardening (CapDrop ALL, PidsLimit, network modes, readonly rootfs) | DONE | `src/sandbox/docker.rs:89-124` |
| S3 `patch -p1` fallback dropped (git apply only) | DONE | `src/sandbox/worktree.rs:266-284` (stale comment only) |
| S4 SEARCH/REPLACE block file-binding | DONE | `src/display/chat/edit_format.rs:42-47` + test |
| S5 knowledge-fetch SSRF guard + timeout | DONE | `src/knowledge/indexer.rs:289-356` |
| S6 worktree non-isolation warning | DONE | `README.md:125`, `src/cli/run.rs:193-199` |
| S7 Google key via `x-goog-api-key` header | DONE | `src/llm/providers/google.rs:63,132` |
| S9 hermetic proof (strict) | DONE | `src/cli/run.rs:341-348` |
| S10 CI SHA-pinned + cargo-audit/deny | PARTIAL | audit job's rust-toolchain unpinned; no SBOM/attestations |
| S11 artifact perms | PARTIAL | `src/util.rs:18` write_restricted(0o600) widely used, **but `run.rs:435` (per-agent JSON) and `:542` (safety_proof.json) still plain write (0644)** |
| S12 LLM timeout + 429/5xx retry | DONE | `src/llm/provider.rs:47-93` |
| S13 SIGTERM cleanup | DONE | `src/cli/run.rs:319-362` (exit 130/143) |
| S14 dead-skeleton config traps | DONE via trust warnings | `mcp.enabled=false` default; `[session]/[compaction]/[permissions]` warned as unwired |
| Custom pipeline topologies, parallel coders (default off), Red/Blue review | DONE | `src/orchestrator/pipeline.rs:121-142`, `src/config/types.rs:311` |
| Eval harness | DONE | `evals/dataset.toml` — 23 cases, committed fixtures |
| Landing + marketing kit + benchmarks.md + distribution plan | DONE | `docs/marketing/*`, `docs/benchmarks.md`, `docs/distribution-plan.md` |
| Package manifests v0.3.0 (Homebrew/Scoop/Winget) | DONE | in-repo; external submissions unverifiable |

**Gaps found (the "improvements list" the audit produced):**

1. **Winget install command mismatch (install-funnel breaker).** README says `winget install Niki.Niki` but the manifest `PackageIdentifier` is `RavaniRoshan.niki` (`winget/RavaniRoshan.niki*.yaml`). README's command would fail.
2. **Spend cap is warn-only, docs overstate.** Code verifies: start-of-run note + end-of-run warning only (`src/cli/run.rs:221,457`) — it does **not** abort. CHANGELOG claims "so autonomous runs can't run away on cost," which is false today. 2026 research says agents ignore abstract budgets (Ramp: 14k messages, 0 budget references) — a hard controller is the only real guard. For v0.3: document honestly; post-launch: hard mid-run kill.
3. **Chat is viewer-only.** Tab exists + copy/select, but no input box (`src/display/input.rs`/`engine.rs` have zero runtime callers). Fine for launch (deferred by design, warned in README) — but "interactive chat" is the #1 retention-driver for CLI agents, worth the post-launch roadmap slot ("retention §5").
4. **Two artifact writes at 0644** (`run.rs:435,542`) — inconsistent with the 0600 policy everywhere else.
5. **Red/Blue + parallel coders are opt-in gates never exercised in CI**; Docker-backend e2e tests absent (only worktree backend tested). Not launch-blocking, but the two headline differentiators have zero test coverage — add CI smoke before marketing them hard.
6. **5 of 6 design-token conflicts remain open** (see §7) — visual inconsistency (purple spinner vs teal brand, `text.muted` mis-map, missing tokens, dead styles, no light-mode auto-detection).
7. **README hero image is a `github.com/user-attachments` URL** (`README.md:10`) — renders on GitHub but breaks offline/forks; `assets/demo.gif` is local and current.
8. **demo.gif never regenerated post-refactor** (last touched 2024) — the TUI has changed since; the marketing asset does not show the current product. Regenerate with the VHS script (`docs/marketing/vhs-script.tape`) before launch.
9. **No SBOM / Sigstore attestations on releases; sandbox image digest-pinning is comment-only, not enforced** (`S10` tail) — see §6: this is now a documented 2026 baseline, not nice-to-have.
10. **`ThemeMode::Auto` always falls back to dark** — no `COLORFGBG`/OSC4 detection.

### SQ2 — Competitive landscape & whitespace ("high-value, low-competition, high-reward")

**Direct space (2026):**

- **Sweep** — original "ticket → PR" bot; pivoted to JetBrains assistant, stalled since Feb 2026; the lane was called "narrow" by its own analysts (https://datarekha.com/blog/coding-agents-2026/).
- **OpenHands** — closest functional analogue (issue→PR, container sandbox, BYOK, MIT, ~84K stars, $18.8M Series A Nov 2025) but Python-heavy, "10-minute Docker setup," GUI/cloud-oriented, not a single-binary CLI (https://rywalker.com/research/ai-coding-assistants, https://codeables.dev/article/openhands-vs-devin-which-one-is-better-at-producing-pr-ready-diffs).
- **Aider** — 46K stars, maintenance mode since Aug 2025; chat-first diff CLI, no pipeline, no sandbox (https://datarekha.com/blog/coding-agents-2026/).
- **Claude Code** — 131K+ stars, ~$2.5B reported run-rate, $20–125/seat, Claude-only, no built-in sandbox (https://rywalker.com/research/ai-coding-assistants).
- **Codex CLI** — local, OS-enforced sandbox (bwrap/seccomp, Seatbelt), **network-off + workspace-write by default** (https://developers.openai.com/codex/concepts/sandboxing).
- **Devin** — cloud VMs, ~45% SWE-bench, ~$10.92/fix unassisted (alatirok Feb 2026 benchmark, per verification) vs $1.20–1.80 for open tools; "AI software engineer" framing widely judged a strategic mistake (https://toolhalla.ai/blog/devin-vs-openhands-vs-swe-agent-2026).
- **CodeRabbit** — AI PR *review* + autofix; $24–48/user/mo; 2M repos, 13M PRs, 8K+ paying customers; **independent analyses put actionable comments in the low single digits**; CEO's public meltdown on a customer thread damaged trust (https://umurinan.com/pages/posts/ai-code-review-is-mostly-noise.html, https://dev.to/rahxuls/i-tested-13-ai-code-review-tools-so-you-dont-have-to-2026-ml1).
- **OpenCode** — 172K stars, Rust, terminal, BYOK, MIT — proof the *form factor* can win at scale; chat-first, no PR pipeline (https://rywalker.com/research/ai-coding-assistants).
- **2026 entrants in "sentence→PR":** Ivan/Ariso (OSS, on Claude Code), Specship (cloud), Foxl Code (parallel Claude Code fleets, micro-VMs, $0.04/credit), baro (parallel event-bus agents), Cobalt (per-task cloud computers), CodeBot (local, **hash-chained audit trail**, issue→PR), SITU-Agent (Podman `--network=none`, local models), blumi (Rust). Lane is filling fast but none combine everything NIKI does (https://ariso.ai/ivan, https://specship.dev/, https://foxl.ai/code, https://github.com/jigjoy-ai/baro, https://github.com/Ascendral/codebot-ai).

**Category-level dynamics (2025–2026):**
- Consolidation: Roo Code shut down May 2026 (~3M installs); Aider maintenance-mode; Sweep stalled. Buyers actively ask "which tool will still ship in 2 years?" (https://rywalker.com/research/ai-coding-assistants).
- SWE-bench Verified saturated/contaminated (OpenAI stopped reporting); differentiators moving to **workflow, verification, trust** (https://datarekha.com/blog/coding-agents-2026/).
- The loudest documented pain: **reviewing agent-written PRs** — AI-co-authored code has ~1.7x more issues (CodeRabbit Dec 2025 study); CodeRabbit noise analyses; "human attention is the bottleneck"; Stripe ships ~1,300 agent PRs/week and review time +91%/PR (https://tianpan.co/blog/2026-04-27-reviewing-agent-prs-different-not-faster, https://umurinan.com/pages/posts/ai-code-review-is-mostly-noise.html).
- Sentiment: HN threads "There is an AI code review bubble" (id=46766961), "Don't trust AI agents" (id=47194611), "2x-not-10x" (id=49047839); counterweight "An Honest Review of AI Programming" — agent+verification loop defenders (id=49166230).

**Whitespace analysis — the defensible position:**
1. **The combination is unoccupied in 2026:** no tool found that is single-binary Rust CLI + four-agent Planner→Coder→Tester→Reviewer + local Podman/Docker sandbox + BYOK + spend cap + tamper-evident-spirit audit trail, PR-first.
2. **Verified-before-PR answers the industry's loudest complaint** (review burden on unverified AI PRs). The Tester agent running the suite *inside the sandbox before the PR exists* is the moat — few competitors put verification in the loop.
3. **Privacy/air-gap demand with weak supply** — compliance buyers can't ship code to cloud agents; SITU/CodeBot target it but are tiny; Aider+local-LLM is the default today; a PR-producing pipeline with the same property is unclaimed.
4. **Exact cost control:** Devin ~$10.92/fix, Greptile credit overages, CodeRabbit seat pricing — BYOK + hard spend visibility attacks all of them.
5. **Audit trail + reviewable handoff** is what enterprise buyers ask for ("can you replay, audit, lock down the run") — OpenHands/CodeBot only peers shipping it.
6. **Consolidation timing:** Aider (CLI lane) and Sweep (PR lane) both effectively exited; NIKI's Apache-2.0 single-binary CLI fits the praised "small focused CLI that does one thing well" survival pattern (https://datarekha.com/blog/coding-agents-2026/).
7. **Caveats (say them first):** the lane is filling (Ivan/Specship/Foxl/baro/Cobalt/DevFlow all <12 months old); "PR-first" alone is no longer unique — NIKI must win on *local + sandboxed + audited + BYOK* as a combination; local-model capability trails frontier models, so the wedge is workflow+trust, not model quality.

### SQ3 — Docker VM / microVMs: decision for the sandbox backend

**What "Docker VM" actually is (verified against docker.com + independent sources):**
- **Docker Sandboxes** (the product the team heard about): experimental Nov 2025 (containers inside Docker Desktop's VM) → **Jan 2026: per-agent microVMs with a private in-VM Docker daemon** → full product ~Apr 2026; now a standalone **`sbx` CLI** (free, no Docker Desktop needed, v0.38.0 as of 2026-08-06) (https://www.docker.com/blog/docker-sandboxes-run-claude-code-and-other-coding-agents-unsupervised-but-safely/, https://docs.docker.com/ai/sandboxes/release-notes/).
- **Docker VMM** — first-party hypervisor layer under Docker Desktop, public beta in Docker Desktop v4.86 (2026-08-12, Mac+Windows); **Linux support only at GA, targeted end of Oct 2026** (https://www.docker.com/blog/docker-vmm-public-beta/).
- Crucially: **not Firecracker-based** (Firecracker has no macOS/Windows support); cross-platform VMM on Hypervisor.framework / Windows Hypervisor Platform / KVM, with `libkrun`-family code reportedly used for the Linux build (https://www.docker.com/blog/why-microvms-the-architecture-behind-docker-sandboxes/, https://github.com/docker/desktop-linux/issues/318).
- **Isolation design (5 layers):** hypervisor (dedicated kernel), host-side network-filtering proxy with deny-by-default presets and **credentials injected as proxy auth headers (keys never sit in the VM)**, per-sandbox Docker Engine, workspace mount, credential proxy (https://docs.docker.com/ai/sandboxes/security/isolation/, https://andrewlock.net/running-ai-agents-safely-in-a-microvm-using-docker-sandbox/).
- Natively profiles **8 agent CLIs including OpenCode** + custom "own agent" definitions (https://www.docker.com/blog/untrusted-autonomous-workload-ai-sandboxes/).
- **Gotchas:** Docker-account login required (breaks scripts), closed-source VMM/sbx, documented performance complaints (Andrew Lock), **no independently published cold-start numbers**, Linux needs Ubuntu on KVM (bare metal), Linux/ARM64 builds paused in v0.35.x (https://news.ycombinator.com/item?id=49239751, https://andrewlock.net/running-ai-agents-safely-in-a-microvm-using-docker-sandbox/).

**Alternatives table** (full details in SQ3 research; key numbers):

| Tech | Isolation | Boot | Rootless | Fit for NIKI |
|---|---|---|---|---|
| Podman rootless (current) | userns+caps | 100–500ms | Yes | **4** — keep |
| Docker+seccomp (old) | shared kernel | <1s | No | 2 — downgrade |
| **Docker Sandboxes (sbx)** | microVM + private dockerd | "seconds w/ pull", unverified | Yes (KVM/HVF/WHP) | **4 for later** — purpose-built, but login/closed-source/young-Linux/perf |
| gVisor (runsc) | userspace kernel | 10–50ms | Yes | 3 — drop-in upgrade path |
| Kata 4.0 (rust runtime-rs) | HW VM/container | 150–500ms | Needs KVM | 3 — Linux-only, heavier |
| Firecracker raw | microVM | ~125ms | Needs KVM | 1–2 — DIY overkill |
| E2B/Daytona/Upstash/CF Sandbox | cloud APIs | 80ms–3s | n/a | 1–2 — wrong shape (local-first) |

**Decision (grounded):** **Adopt later — keep Podman rootless now.** NIKI's threat model is single-user localhost; the dominant risks (prompt-injected destructive commands, dotfile exfiltration) are already bounded by rootless containers + CapDrop/deny-lists at near-zero cost; microVM hard isolation buys little at single-user trust level and costs maturity (closed-source, login friction, Linux GA only Oct 2026). **Revisit triggers:** (1) Docker VMM Linux GA (Oct 2026) + independent boot benchmarks; (2) NIKI gains multi-tenant/remote execution → then gVisor (`runsc`) first; (3) need for macOS/Windows parity or in-sandbox Docker builds → sbx becomes attractive (https://h5i.dev/blog/sandboxing-ai-agents-landscape/, https://www.bigiron.cc/guides/rootless-podman-where-it-works-where-it-bites). Applies unchanged to the "worktree backend" warning: document, don't engineer around.

### SQ4 — First-user journey: activation → retention → virality

**Activation (measured norms):**
- Dev tools: "time to first API call/success" is the single most predictive metric; first-call <10 min → 3–4x paid conversion; devs abandon after ~20 min of evaluation (https://www.saashero.net/strategy/devtools-saas-growth-marketing-strategies/, https://usertourkit.com/blog/onboarding-developer-tools-cli-dashboard-api).
- CLI pattern that wins: interactive init wizard + auto-config + **a real first success in <5 min** (Stripe CLI first-call 47% faster than dashboard users); tooltip tours skip rate 73%; guided empty-state ≥ tours (https://usertourkit.com/blog/onboarding-developer-tools-cli-dashboard-api).
- NIKI already ships this shape: `niki init` (detects runtime, pulls image, validates key, smoke task, "first branch < 5 min" target per CHANGELOG). **Instrument time-to-first-PR as the north-star metric**; print a shareable artifact at first success.

**Retention (honest numbers):**
- Standalone agent tools collapse: Week-1 ~23%, Week-4 ~15%, Week-10 <10% (Jellyfish, Oct 2025 — enterprise agent use) (https://jellyfish.co/blog/how-prevalent-is-autonomous-agent-use-heres-what-the-data-says/).
- Workflow-embedded wins: AI PR-review bots grew from 14.8% → 51.4% company adoption in 2025 (https://jellyfish.co/blog/2025-ai-metrics-in-review/).
- Microsoft study (tens of thousands of engineers, 2026): habit forms in the **first two weeks**; "retained" (5-of-14 days) users merged +24% more PRs; visible peer use drives spread (https://arxiv.org/html/2607.01418).
- Daily-use baseline: 72% of devs who tried AI tools use them daily; #1 pain = "code looks correct but isn't reliable" (67%) (Sonar 2026 survey).
- **Implication for NIKI:** bias hard for the first 14 days. Session persistence + fast iteration loop + a *workflow touchpoint* (GitHub App / PR-comment bot / CI) — the CodeRabbit loop (PR bot → every PR is a touchpoint → viral + retention) is the strongest proven mechanic in the category: free tier → #1 GitHub Marketplace AI app, 150K+ installs, 100K+ OSS projects, ~$5M→$40M ARR in 12 months (https://www.coderabbit.ai/blog/coderabbit-series-b-60-million-quality-gates-for-code-reviews, https://sacra.com/c/coderabbit/).

**Virality:**
- **Shareable PR artifacts** — every generated PR (description + audit trail) is a live product demo. Auto-"AI-made" framing on the PR body.
- **Terminal/GIF demos** — 23–45s clips of the 4-agent chain with per-agent progress outperform explanations (agmsg: 5→320 stars/week on a 23s silent video; zerobrew's "presentation of speed" insight — each stage gets its own loading bar: 6.8K stars, 380K X impressions) (https://dev.to/fujibee/lessons-from-open-sourcing-a-cli-agent-messaging-layer-320-stars-in-a-week-1b75, https://substack.lucasgelfond.online/p/reverse-engineering-a-viral-open).
- **Category fit:** agent-workflow terminals went viral in 2026 (cmux: 0→3.5K stars in 2 weeks on category fit + scriptable surface + demo video) (https://cmux.com/blog/show-hn-launch).
- **Benchmarks with reproducible commands** = currency in r/LocalLLaMA-grade communities (https://github.com/anthony-chaudhary/fak/blob/main/docs/launch/landscape-research.md).
- **Trend-fit:** "vibe coding" is now pejorative (Collins WOTY 2025; Karpathy retired the term Feb 2026 in favor of "agentic engineering"). Position as **agentic-engineering/verification**, not vibe-coding (https://thenewstack.io/vibe-coding-agentic-engineering/, https://www.coderabbit.ai/blog/a-semantic-history-how-the-term-vibe-coding-went-from-a-tweet-to-prod).
- BYOK/local-first alone is now crowded (blumi, Zone, OpenAgentd, DuckAgent, CodeBot...) — **privacy is necessary-but-not-sufficient messaging; the sharp wedge is "reviewable AI PRs, sandboxed locally, full audit trail"** (https://github.com/ankurCES/blumi-cli, https://github.com/Ascendral/codebot-ai).

**Launch-day mechanics (2026 evidence):**
- **Show HN** (largest empirical dataset, 28K posts Apr 2026): median score = 2; ~1.4 GitHub stars per HN point; 92% of star impact inside 48h; comments do *not* predict stars (r=0.10); working demo tryable in <30s with no signup is the biggest front-page predictor; author must be in-thread 2–6h with an honest-limitations top comment (https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/, https://hub.causo.ai/guides/show-hn-launch-playbook-technical-founders-2026).
- **Product Hunt** (100-launch analysis May 2026): #1 = 120–180 quality upvotes in first 3h; first-hour votes ~4x weight; **comments 40–50x upvotes**; >100 upvotes/h spikes get cleared; maker reply <8 min median for 4h; 180+ word maker comment; badge lifts ongoing signups ~17%; 72h follow-through drives ~40% of traffic (https://www.launchpact.io/blog/product-hunt-launch-analysis, https://gingiris.tools/blog/2026/03/18/product-hunt-launch-the-2026-playbook-for-winning-1/).
- **GitHub Trending** = velocity-based; Rust-filtered threshold est. 30–60 stars/day (vs 80–150 all-language); AFFiNE pattern: concentrate channels into 48h, hit Trending day 5, then newsletter wave with "we hit Trending" as proof; fake-star bursts get discounted (https://gingiris.tools/blog/2026/04/06/how-to-get-on-github-trending/).
- **FLAGGED DISAGREEMENTS (how to handle):** Show HN best time — empirical dataset says Mon 00:00 UTC; 2026 playbooks say Tue–Thu 8–11am ET. PH day — 2026 sources converge Tue (weekly-badge math); 2024 sources said Sunday. **Recommendation:** don't over-optimize; pick Tue, be in-thread regardless — presence beats timing.

### SQ5 — Security & trust bar (what launch scrutiny will probe)

**Threat landscape 2025–2026 (primary sources):**
- **Repo prompt injection is the #1 documented attack class:** AIShellJack — up to 84% malicious-command success on Copilot/Cursor via poisoned rule files/READMEs (https://arxiv.org/html/2509.22040v1); Trail of Bits demonstrated a backdoor slipped into an OSS project via an injected GitHub issue, plus argument-injection RCE against production agents citing Claude Code CVE-2025-54795 (https://blog.trailofbits.com/2025/10/22/prompt-injection-to-rce-in-ai-agents/); Mindgard found 3 critical Cline issues reachable by opening a malicious repo (DNS key-exfil, `.clinerules` RCE, TOCTOU) (https://mindgard.ai/blog/cline-coding-agent-vulnerabilities).
- **Sandbox escapes:** runc CVE-2025-31133/52565/52881 (Nov 2025) — reliably mitigated by user namespaces (rootless) + AppArmor + no-new-privs; Docker default seccomp blocks only ~44/300+ syscalls; gVisor has no full-escape CVE to date (https://github.com/opencontainers/runc/security/advisories/GHSA-9493-h29p-rfm2, https://www.tianpan.co/blog/2026-03-09-agent-sandboxing-secure-code-execution).
- **Egress-off is the peer norm:** Codex = network-off by default; Claude sandbox = network-denied-by-default; OWASP 2026 says block egress if the agent doesn't need it (https://developers.openai.com/codex/agent-approvals-security, https://www.anthropic.com/engineering/claude-code-sandboxing, https://cheatsheetseries.owasp.org/cheatsheets/Secure_Coding_with_AI_Cheat_Sheet.html).
- **BYOK key leaks are the most embarrassing failure class:** CVE-2026-21852 (malicious repo sets ANTHROPIC_BASE_URL → keys exfiltrated before the trust prompt), CVE-2025-55284 (DNS exfil of .env), env vars in plaintext logs (https://github.com/anthropics/claude-code/security/advisories/GHSA-jh7p-qr78-84p7, https://embracethered.com/blog/posts/2025/claude-code-exfiltration-via-dns-requests/).
- **Approval-UI deception:** GhostApproval (Wiz, July 2026) — symlinks let 6 agents write outside the workspace while the dialog showed a benign path (https://www.theregister.com/security/2026/07/08/bug-in-top-ai-coding-agents-shows-that-unix-era-security-headaches-never-really-die/5268025).
- **Spend runaway:** agents ignore injected budgets (Ramp: 14k messages, 0 references); $47k/11-day runaway loop anecdote; Claude Code sub-agents ran 234 tool calls unbounded (https://labs.ramp.com/research/coding-agents-ignore-spend/, https://github.com/anthropics/claude-code/issues/36727).
- **Supply chain:** Trivy (a security scanner!) compromised Mar 2026, infostealer via GHCR/Docker Hub/ECR/npm — official remediation = cosign verify + digest pinning; ghrc.io typosquat harvests GHCR creds; 2026 floor = SLSA L3/attestations/SBOM (~14% org adoption) (https://github.com/advisories/GHSA-69fq-xp46-6x23, https://bmitch.net/blog/2025-08-22-ghrc-appears-malicious/, https://safeguard.sh/resources/blog/container-image-supply-chain-security-deep-dive-2026).
- **Telemetry:** Claude Code's Statsig/Datadog telemetry drew sustained public scrutiny — BYOK local tools get MITM-tested (https://speedscale.com/blog/peeking-under-the-hood-with-claude-code/).

**The 2026 trust bar (what reviewers probe):** deterministic (OS-enforced) sandbox, not model-judged gates; egress default-off; BYOK keys kept on host, never in sandbox env; approval dialogs showing *resolved real targets*; digest-pinned + signed images; no-telemetry verified by network inspection; documented threat model + SECURITY.md; hard kill switch; "outside our threat model" is no longer an acceptable defense. Windsurf's unresponsive disclosure (May 2025–2026) is cited as the negative playbook.

**NIKI-specific risk table:**

| Risk | Severity for NIKI | Pre-launch or post? |
|---|---|---|
| Repo prompt injection → agent executes attacker command | **Critical** (core product action; 84% ASR documented) | PRE: deny-lists + CapDrop already in; add: scrub invisible Unicode, treat repo as untrusted input, resolve symlink targets in any approval UI, `--` at exec boundaries |
| BYOK key exfiltration | **Critical** | PRE: keep redaction; never seed host `~/.aws`, `.ssh`, `.env` into context; validate provider base-URL from host config, not repo config; README security section states redaction policy |
| Egress abuse / SSRF | High | PRE: document that network mode exists; **post-launch (v0.4): default egress-blocked with per-run allowlist** (Codex/Claude norm) — for launch, honest docs + spend cap |
| Unsigned/mutable sandbox image | High | PRE: enforce digest-pinned pull in NIKI itself (currently comment-only); **sign image with Sigstore/cosign** (cheap, keyless) |
| Container→host escape | Medium (single-user) | PRE: docs + threat model statement; POST: gVisor documented path |
| Spend runaway | Medium–High (BYOK = user's wallet) | PRE: fix docs to "warn-only"; POST: hard mid-run kill + velocity detection |
| Telemetry distrust | Medium | PRE: explicit no-telemetry statement in README + SECURITY.md (verify zero hidden outbound) |
| Audit trail trust | Medium | PRE: audit trail exists; make per-run log exportable (command, exit codes, files touched, approvals, spend) |

### SQ6 — Benchmarks & evals: what NIKI may claim at launch

- **SWE-bench Verified is dead as a credibility signal** (OpenAI's Feb 2026 audit: 59.4% flawed tests; contamination; OpenAI stopped reporting; academic-only leaderboard) — publishing any SWE-bench number in 2026 is a red flag, not a credential (https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/).
- Pro is already damaged (OpenAI audited ~30% broken tasks, retracted its own endorsement; graders mis-graded ~1/3 in an independent audit) (https://aiweekly.co/alerts/openai-drops-swe-bench-verified-backs-pro-after-flaw-audit).
- **Harness effect is the #1 credibility question:** same model spans 58.0–79.8% on Terminal-Bench purely by harness; harness choice shifts pass rates ±8.5–13pp and up to 40x tokens-per-solved; cost disclosure across agent benchmarks is **0.00/1.0** (https://arxiv.org/html/2606.17799, https://arxiv.org/html/2605.21404v1).
- Dunk templates (documented 2026 cases): README numbers contradicted by the repo's own committed results (caveman #234); charts not reproducible from committed harness (fff #540); mid-claim goalpost moves (hk/lefthook); overstated savings vs measured (rtk #839); the Devin subsample controversy (13.86% on 570/2294 tasks); Ponytail's repaired-harness response being itself the trust win (https://github.com/DietrichGebert/ponytail/issues/126, https://www.infoq.com/news/2026/08/ponytail-agent-skill-benchmark/).
- **The credible small-tool pattern = Aider's:** open harness in-repo, open dataset, **every run as a YAML record: model, edit format, repo commit hash (dirty flag), date, seconds/case, total cost, pass rates, failures**; community PR-able results (https://github.com/Aider-AI/aider/blob/main/benchmark/README.md).
- **What NIKI should publish (launch):** the harness itself ("run it yourself"), per-run disclosure manifest (harness+dataset commit hashes, seeds, per-stage model+version+date, temperature, image digests, timeouts, n≥3 runs with mean+min–max, **cost per run and per solved case**), functional (F2P+P2P) separate from quality rubric (minimality/tests/no-unrelated-edits), per-case results + failure taxonomy, framed as "internal litmus suite, n=23, smoke-level, not a capability benchmark," and the honest note that BYOK evals describe pipeline+model pairs.
- **Must NOT publish:** any SWE-bench number; "100% pass"; single-run single-point claims; comparisons vs Claude Code/Codex/Devin without their exact configs; best-of-n without stating the policy.
- **Note:** n=23 at ~50% pass gives ±20pp CIs — growing the frozen dataset matters post-launch (§9).
- **Alternative genre:** agent-pipeline evals (task → PR → tests pass + rubric) are a recognized 2026 genre (SWE family, SWE Atlas rubrics, Agentic Rubrics, ProdCodeBench) — NIKI's harness is the right shape.

### SQ7 — Marketing funnel & search angles

**Keyword landscape (verified Aug 2026):**
- "Turn a sentence into a pull request" — *forming category with direct rivals, no leader* (Ivan, Genie, Hive, baro all claim the phrase; none owns the SERP). NIKI can own it: landing H1 + README line 1 + blog + llms.txt (https://ariso.ai/ivan, https://github.com/automagik-dev/genie, https://hivecli.sh/).
- "AI pull request generator" — weak SERP (many 0–10-star tools, unoptimized pages) = classic content gap (https://github.com/Jeranguz/gitai, https://github.com/Tjhohn/Pr-Buildr).
- "open source Claude Code alternative" — saturated (Claurst 10.2K stars in 4 months; dozens claim it) — use only as secondary comparison term (https://github.com/Kuberwastaken/claurst).
- **"AI agent sandbox / run AI agent in Podman/container" — rising, low-competition, and NIKI's built-in sandbox beats the "bolted-on" wrapper crowd** (localdev, clampdown, agentbox, Sluice are all 2025–26 small projects) (https://github.com/gherlein/localdev, https://github.com/89luca89/clampdown, https://qwenlm.github.io/qwen-code-docs/en/users/features/sandbox/).
- Comparison pages are a ranked, page-1 genre in this exact niche (terminaltrove.com/compare/ai-coding-agents/ is a dedicated 48-agent comparison engine) — build `/compare/` pages with the eval harness as the differentiator (https://terminaltrove.com/compare/ai-coding-agents/, https://dev.to/kunal_d6a8fea2309e1571ee7/aider-vs-claude-code-vs-openhands-cli-ai-coding-tested-2026-5721). Note: the "comparison pages convert 3–5x" multiplier is *unverified* — what is verified is that they win high-intent SERPs.
- **GEO scarcity = early-mover window:** average site citability 23.1/100; only ~0.022% of sites "AI-ready"; 69.7% lack llms.txt; llms.txt serves third-party engines, NOT Google (Google's own docs) (https://searchscore.io/guides/generative-engine-optimisation/, https://developers.google.com/search/docs/fundamentals/ai-optimization-guide, https://arxiv.org/abs/2311.09735).

**Channel rulings:**
- **Directories that are exactly NIKI-shaped and open pre-launch:** TerminalTrove (free Post-a-Tool; has an LLM-crawlable llms-full.txt), opensourcealternative.to (Developer Tools, public submit), Console.dev Betas (30K devtools subscribers, accepts pre-1.0, no sponsored reviews) (https://terminaltrove.com/, https://opensourcealternative.to/, https://console.dev/selection-criteria).
- **This Week in Rust** — free, high-fit; submit via PR to rust-lang/this-week-in-rust drafts/ or tag @thisweekinrust.bsky.social; CFP rules: OSI license (Apache-2.0 ✓), public issue tracker, labeled good-first-issues (https://github.com/rust-lang/this-week-in-rust).
- **Paid newsletters** priced 2026: TLDR ~$15K primary placement; devtools CPCs $1.70–15 (Techpresso→Latent Space) — hold for post-Trending amplification, not launch day (https://www.beehiiv.com/blog/newsletter-sponsorship-cost, https://advertise.tldr.tech/case-studies/clerk-com-uncovers-newsletters-as-a-hidden-engine-behind-12x-traffic-and-30-of-signups/).
- **Reddit:** r/selfhosted, r/opensource, r/rust for story-framed posts; self-promo <10% rule, pre-build karma ~2 weeks prior; r/ExperiencedDevs promo-hostile (https://postinstantly.com/guides/how-to-grow-a-reddit-presence-as-a-self-hosted-software-project-from-zero, https://indexthread.com/newsletter/reddit-for-product-launches).
- **GitHub Trending (Rust filter)** est. 30–60 stars/day — the realistic #1 target; README must convert in 10–15 seconds (https://gingiris.tools/blog/2026/04/06/how-to-get-on-github-trending/).
- **Newsletter-driving docs SEO:** docs = 40–60% of dev-tool organic traffic; AI-referred traffic converts ~14.2% vs 2–5% organic (https://autoseobot.com/blog/seo-for-developer-tools.html).

---

## 4. Analysis & discussion

**Positioning synthesis (the "high-value, low-competition, high-reward" answer).** From the competitor's perspective, NIKI is attackable today because: (a) it's pre-traction (hard truth — every 2026 entrant has this problem); (b) chat-first UX is what users expect, and NIKI is pipeline-first; (c) parallel execution across repos/machines is where Foxl/baro/Cobalt push. From NIKI's perspective, the whitespace is real and defensible: **local-first + verified-before-PR + audit-trail + exact-cost + single-binary** remains an unoccupied combination, and the *category moment* helps — the loudest 2026 complaints (unverified agent PRs, review noise, spend runaway, data leaving your machine) are exactly the four pains NIKI's architecture addresses. The positioning sentence that survives verification: *"NIKI is the local-first way to turn a sentence into a reviewable PR — four sandboxed agents generate, test, and review before anything touches your repo, with a full audit trail and no code leaving your machine."*

**Strengths confirmed by code:** real pipeline, real sandbox hardening, real eval harness, honest doc drift warnings (rare), Apache-2.0, no ground-truth telemetry claims to walk back.

**Weaknesses confirmed by code:** warn-only spend cap (docs overclaim), chat viewer-only, egress not default-blocked, 0644 artifacts, demo.gif stale, winget command mismatch, token-system conflicts, docker-e2e test gap.

**Emergent patterns:** (1) "Verification" is the 2026 wedge — every credible player is moving toward it; (2) workflow-embedded beats standalone for retention — CI/PR-bot integration is the post-launch roadmap's highest-value item; (3) local-first is crowded *as messaging* but empty *above small-scope tools* — NIKI's pipeline+audit+spend-cap combo is the differentiator combination; (4) hype-backlash means understated, artifact-first, honest-limitations marketing is the winning register (zerobrew, cmux, agmsg, Ponytail-repair all confirm); (5) 2026 benchmarks punish claims — disclose everything, claim little.

---

## 5. Conclusions & implications

1. **Launch is engineering-ready; polish 6 items in the week left** (winget command, artifact perms, spend-cap doc honesty, token system conflicts, demo.gif refresh, SECURITY.md trust bar) — all cheap, all verified in code.
2. **Lead with the verification story, not "AI wrote your PR."** The Tester+Reviewer-in-the-loop is the product; the audit trail is the proof; understate everywhere it's honest.
3. **Run the launch as a 48h Rust-community-first event**: GitHub release + README-as-landing + Show HN (Tue) + This Week in Rust + Reddit story posts + PH as badge/backlink channel + pre-launch directory submissions (TerminalTrove/opensourcealternative/Console.dev Betas) + llms.txt/answer-first landing copy. Target Rust-filtered GitHub Trending (~30–60 stars/day) as the measurable #1.
4. **Publish benchmarks only with full disclosure manifests** (cost per solved case included; harness commit hashes; n≥3; "internal litmus, smoke-level" framing); never a SWE-bench number; no cross-tool comparisons without exact configs.
5. **Post-launch roadmap (already evidenced):** hard spend-kill (controller-level), egress default-block with per-run allowlist, interactive chat (retention), GitHub App/PR-bot touchpoint (the CodeRabbit retention loop), Docker Sandboxes re-eval after Linux GA (Oct 2026), gVisor path, expanded frozen eval set (n≈50+), G1-level star/activation instrumentation.

---

## 6. Disagreements & open questions (as flagged by verification)

1. **Show HN best time:** empirical dataset (Mon 00:00 UTC) vs 2026 playbooks (Tue–Thu 8–11am ET) — *carried*: recommend Tue, presence > timing.
2. **Product Hunt day:** 2026 sources (Tue, weekly-badge math) vs 2024 (Sunday) — *carried*: Tue.
3. **Docker Sandboxes perf:** vendor "near-instant" (unverified) vs independent "can be crippling" (Andrew Lock, single-but-credible) — *carried*: both reported; adopt-later decision does not hinge on it.
4. **E2B runtime:** gVisor (one source) vs Firecracker (E2B official docs + all others) — *resolved*: Firecracker.
5. **"PR-first lane narrow vs growing":** Sweep's exit vs 2026 entrant wave — *carried*: both true; narrow in vendor-viability terms, growing in user-demand terms.
6. **CodeRabbit actionable-comment figure:** the "~2.3% of 3,500 comments" figure could not be re-found during verification — *softened here*: "independent analyses put actionable comments in the low single digits" with the umurinan source.
7. **Devin cost/fix:** "~$8.50" (toolhalla) vs "~$10.92 unassisted / $4–6 assisted" (alatirok Feb 2026 per verification) — *carried*: $10.92 with source; range given.
8. **OpenHands stars:** 76K+ (Jul sources) vs 83,845 (verification pull) — *carried*: ~84K.
9. **Comparison-page conversion multiplier (3–5x):** unverified — *carried* as "unverified, but SERP dominance is verified."
10. **CI audit job's unpinned rust-toolchain** and Docker-sandbox e2e absence — code-level, carried into plan.
11. **measurement gaps:** true search volume for "sentence to pull request" (no keyword tool), CLI-specific retention curves (instrument from day 1), whether TerminalTrove category ingestion is curated, GitHub Trending Rust thresholds (practitioner estimate), Apple Containerization (too new).

---

## 7. Design tokens — status (secondary task)

`token.md` (the single source of truth) is now at **repo root** (`/home/shiva/projects/niki/token.md`). Provenance note: the pasted brand image could not be read in-session; the token file was reconstructed from the *shipped* palette (`src/display/theme.rs`) and `assets/logo.svg`, which is the real brand. **If the intended image differs, re-send it and tokens will be regenerated.** Code-vs-token verification found 5 of 6 documented conflicts still open (purple spinner vs teal brand; `text.muted` maps to the wrong primitive; missing `text_strong`/`autocomplete_bg`/`scrollbar_thumb`/`shimmer`; dead compound styles; `ThemeMode::Auto` without light-detection) — conflict #1 (logo blue→teal) is fixed. **Resolution executed this session:** see §10 plan items T1–T6; do not ship with the purple-spinner inconsistency.

---

## 8. Full source list

*Web sources (all fetched/verified 2026-08-13 unless dated):*
- https://rywalker.com/research/ai-coding-assistants ; https://datarekha.com/blog/coding-agents-2026/ ; https://codeables.dev/article/openhands-vs-devin-which-one-is-better-at-producing-pr-ready-diffs ; https://toolhalla.ai/blog/devin-vs-openhands-vs-swe-agent-2026 ; https://umurinan.com/pages/posts/ai-code-review-is-mostly-noise.html ; https://topaitracker.com/comparisons/2026-08-02-coderabbit-vs-greptile-ai-code-review-head-to-head/ ; https://dev.to/rahxuls/i-tested-13-ai-code-review-tools-so-you-dont-have-to-2026-ml1 ; https://www.coderabbit.ai/pricing ; https://ariso.ai/ivan ; https://specship.dev/ ; https://foxl.ai/code ; https://github.com/jigjoy-ai/baro ; https://github.com/Ascendral/codebot-ai ; https://github.com/ndburn/SITU-Agent ; https://privateer.pro/cli ; https://github.com/ankurCES/blumi-cli ; https://github.com/kstenerud/yoloai ; https://github.com/mattolson/agent-sandbox ; https://developers.openai.com/codex/concepts/sandboxing ; https://developers.openai.com/codex/agent-approvals-security ; https://tianpan.co/blog/2026-04-27-reviewing-agent-prs-different-not-faster ; https://tianpan.co/blog/2026-04-23-rubber-stamp-collapse-ai-authored-prs ; https://tianpan.co/blog/2026-03-09-agent-sandboxing-secure-code-execution ; https://news.ycombinator.com/item?id=46766961 ; https://news.ycombinator.com/item?id=47545748 ; https://news.ycombinator.com/item?id=47194611 ; https://news.ycombinator.com/item?id=49166230 ; https://news.ycombinator.com/item?id=49047839 ; https://news.ycombinator.com/item?id=49239751 ; https://news.ycombinator.com/item?id=43084121
- https://www.docker.com/blog/docker-sandboxes-run-claude-code-and-other-coding-agents-unsupervised-but-safely/ ; https://www.docker.com/blog/docker-vmm-public-beta/ ; https://www.docker.com/blog/why-microvms-the-architecture-behind-docker-sandboxes/ ; https://www.docker.com/blog/comparing-sandboxing-approaches-ai-agents/ ; https://www.docker.com/blog/untrusted-autonomous-workload-ai-sandboxes/ ; https://docs.docker.com/ai/sandboxes/release-notes/ ; https://docs.docker.com/ai/sandboxes/security/isolation/ ; https://github.com/docker/desktop-linux/issues/318 ; https://github.com/libkrun/libkrun ; https://andrewlock.net/running-ai-agents-safely-in-a-microvm-using-docker-sandbox/ ; https://rivet.dev/blog/2026-02-04-we-reverse-engineered-docker-sandbox-undocumented-microvm-api/ ; https://github.com/firecracker-microvm/firecracker ; https://github.com/kata-containers/kata-containers/releases ; https://safeguard.sh/resources/blog/firecracker-vs-cloud-hypervisor-vs-kata-buyer-guide-2026 ; https://gvisor.dev/docs/architecture_guide/security/ ; https://e2b.dev/docs/template/how-it-works ; https://www.daytona.io/docs/en/snapshots/ ; https://developers.cloudflare.com/sandbox/ ; https://upstash.com/docs/box/overall/quickstart ; https://www.bigiron.cc/guides/rootless-podman-where-it-works-where-it-bites ; https://grigio.org/docker-alternatives-for-ai-agents-podman-bwrap-and-firejail/ ; https://h5i.dev/blog/sandboxing-ai-agents-landscape/ ; https://securemachinery.com/2026/07/04/kata-containers-vs-gvisor-security-architecture-performance-full/ ; https://warski.org/blog/docker-sandboxes-sbx-vs-sandcat/ ; https://rywalker.com/research/container-vm-runtimes ; https://learn.arm.com/install-guides/sbx/ ; https://github.com/docker/sbx-releases
- https://userpilot.com/blog/time-to-value/ ; https://www.appcues.com/blog/time-to-value ; https://amplitude.com/blog/time-to-value-drives-user-retention ; https://www.saashero.net/strategy/devtools-saas-growth-marketing-strategies/ ; https://usertourkit.com/blog/onboarding-developer-tools-cli-dashboard-api ; https://jellyfish.co/blog/how-prevalent-is-autonomous-agent-use-heres-what-the-data-says/ ; https://jellyfish.co/blog/2025-ai-metrics-in-review/ ; https://arxiv.org/html/2607.01418 ; https://www.sonarsource.com/state-of-code-developer-survey-report.pdf ; https://danfking.github.io/blog/2026/04/23/show-hn-by-the-numbers/ ; https://hub.causo.ai/guides/show-hn-launch-playbook-technical-founders-2026 ; https://crossmind.io/blog/show-hn-what-gets-upvotes-and-what-gets-buried/ ; https://gingiris.tools/blog/2026/04/07/how-to-launch-on-hacker-news-show-hn-guide/ ; https://datadriven.partners/reach/channels/show-hn-for-data-infrastructure-tools/ ; https://www.launchpact.io/blog/product-hunt-launch-analysis ; https://gingiris.tools/blog/2026/03/18/product-hunt-launch-the-2026-playbook-for-winning-1/ ; https://fromscratch.dev/blog/product-hunt-launch-strategy ; https://dub.co/blog/product-hunt ; https://cmux.com/blog/show-hn-launch ; https://www.ycombinator.com/launches/PbB-cmux-the-open-source-terminal-built-for-coding-agents ; https://substack.lucasgelfond.online/p/reverse-engineering-a-viral-open ; https://rybbit.com/blog/5k-stars ; https://dev.to/fujibee/lessons-from-open-sourcing-a-cli-agent-messaging-layer-320-stars-in-a-week-1b75 ; https://www.coderabbit.ai/blog/coderabbit-series-b-60-million-quality-gates-for-code-reviews ; https://sacra.com/c/coderabbit/ ; https://neodrop.ai/post/fMwiOAWj53n ; https://pullflow.com/state-of-ai-code-review-2025 ; https://postinstantly.com/guides/how-to-grow-a-reddit-presence-as-a-self-hosted-software-project-from-zero ; https://indexthread.com/newsletter/reddit-for-product-launches ; https://github.com/anthony-chaudhary/fak/blob/main/docs/launch/landscape-research.md ; https://autoseobot.com/blog/seo-for-developer-tools.html ; https://devtune.ai/blog/aeo-vs-geo-vs-seo ; https://advertise.tldr.tech/case-studies/clerk-com-uncovers-newsletters-as-a-hidden-engine-behind-12x-traffic-and-30-of-signups/ ; https://thenewstack.io/vibe-coding-agentic-engineering/ ; https://www.coderabbit.ai/blog/a-semantic-history-how-the-term-vibe-coding-went-from-a-tweet-to-prod ; https://www.theverge.com/ai-artificial-intelligence/950844/vibe-coding-security-risks-apps
- https://arxiv.org/html/2509.22040v1 ; https://arxiv.org/html/2510.23675v3 ; https://arxiv.org/html/2509.05755 ; https://blog.trailofbits.com/2025/10/22/prompt-injection-to-rce-in-ai-agents/ ; https://blog.trailofbits.com/2025/08/06/prompt-injection-engineering-for-attackers-exploiting-github-copilot/ ; https://mindgard.ai/blog/cline-coding-agent-vulnerabilities ; https://mindgard.ai/blog/arbitrary-command-execution-in-ai-cli-tooling ; https://github.com/opencontainers/runc/security/advisories/GHSA-9493-h29p-rfm2 ; https://www.cncf.io/blog/2025/11/28/runc-container-breakout-vulnerabilities-a-technical-overview/ ; https://github.com/anthropics/claude-code/security/advisories/GHSA-jh7p-qr78-84p7 ; https://embracethered.com/blog/posts/2025/claude-code-exfiltration-via-dns-requests/ ; https://github.com/anthropics/claude-code/security/advisories/GHSA-x5gv-jw7f-j6xj ; https://github.com/anthropics/claude-code/issues/62156 ; https://github.com/anthropics/claude-code/issues/36727 ; https://www.theregister.com/security/2026/07/08/bug-in-top-ai-coding-agents-shows-that-unix-era-security-headaches-never-really-die/5268025 ; https://labs.ramp.com/research/coding-agents-ignore-spend/ ; https://www.kognita.co/blog/ai-agent-runaway-cost-no-kill-switch ; https://www.requesty.ai/blog/how-to-cap-runaway-agent-spend-2026 ; https://github.com/advisories/GHSA-69fq-xp46-6x23 ; https://kudelskisecurity.com/research/investigating-two-variants-of-the-trivy-supply-chain-compromise ; https://bmitch.net/blog/2025-08-22-ghrc-appears-malicious/ ; https://safeguard.sh/resources/blog/container-image-supply-chain-security-deep-dive-2026 ; https://github.blog/ai-and-ml/github-copilot/how-githubs-agentic-security-principles-make-our-ai-agents-as-secure-as-possible/ ; https://speedscale.com/blog/peeking-under-the-hood-with-claude-code/ ; https://github.com/anthropics/claude-code/issues/11057 ; https://cheatsheetseries.owasp.org/cheatsheets/Secure_Coding_with_AI_Cheat_Sheet.html ; https://www.anthropic.com/engineering/claude-code-sandboxing ; https://www.anthropic.com/engineering/how-we-contain-claude
- https://openai.com/index/why-we-no-longer-evaluate-swe-bench-verified/ ; https://openai.com/index/separating-signal-from-noise-coding-evaluations/ ; https://decrypt.co/359012/openai-benchmark-measure-ai-coding-supremacy-contaminated ; https://www.latent.space/p/swe-bench-dead ; https://github.com/swe-bench/experiments ; https://debugml.github.io/cheating-agents/ ; https://arxiv.org/html/2607.22585 ; https://arxiv.org/html/2605.23950 ; https://arxiv.org/html/2606.17799 ; https://arxiv.org/html/2605.21404v1 ; https://arxiv.org/html/2601.11868v1 ; https://liveswebench.ai/ ; https://swe-bench-live.github.io/ ; https://artificialanalysis.ai/evaluations/livecodebench ; https://scale.com/blog/advancing-agents ; https://scale.com/blog/swe-atlas ; https://aiweekly.co/alerts/openai-drops-swe-bench-verified-backs-pro-after-flaw-audit ; https://github.com/Aider-AI/aider/blob/main/benchmark/README.md ; https://aider.chat/2024/12/21/polyglot.html ; https://github.com/DietrichGebert/ponytail/issues/126 ; https://blog.scottlogic.com/2026/06/16/ponytail-yagni-and-the-problem-with-prompt-benchmarks.html ; https://www.infoq.com/news/2026/08/ponytail-agent-skill-benchmark/ ; https://github.com/JuliusBrussee/caveman/issues/234 ; https://github.com/dmtrKovalenko/fff/issues/540 ; https://github.com/rtk-ai/rtk/issues/839 ; https://software-lab.org/publications/icse2026_SWE-bench-correctness.pdf ; https://aclanthology.org/2026.acl-long.697/ ; https://arxiv.org/html/2604.01527v1 ; https://cs.stanford.edu/people/brando9/professional_documents/papers/NeurIPS_2026_VeriBench.pdf ; https://link.springer.com/article/10.1007/s10462-026-11571-0 ; https://github.com/OpenHands/benchmarks ; https://github.com/OpenHands/openhands-index-results ; https://www.openhands.dev/blog/sota-on-swe-bench-verified-with-inference-time-scaling-and-critic-model
- https://terminaltrove.com/ ; https://opensourcealternative.to/ ; https://console.dev/selection-criteria ; https://github.com/rust-lang/this-week-in-rust ; https://this-week-in-rust.org/blog/2026/07/22/this-week-in-rust-661/ ; https://www.beehiiv.com/blog/newsletter-sponsorship-cost ; https://dupple.com/learn/best-tech-newsletters-to-advertise-in ; https://s2p.dev/blog/how-to-launch-on-hacker-news ; https://www.youngju.dev/blog/culture/2026-05-14-side-project-launch-strategy-2026-product-hunt-hacker-news-twitter-x-indie-hackers-deep-dive.en ; https://gtm-labs.co/how-to-launch-a-developer-tool ; https://pristren.com/blog/product-hunt-launch-guide-developer-tools/ ; https://gingiris.tools/blog/2026/04/06/how-to-get-on-github-trending/ ; https://top10.dev/story/github-trending-is-a-marketing-surface-heres-how-to-read-it-1363 ; https://github.blog/news-insights/company-news/explore-what-is-trending-on-github/ ; https://searchscore.io/guides/generative-engine-optimisation/ ; https://developers.google.com/search/docs/fundamentals/ai-optimization-guide ; https://arxiv.org/abs/2311.09735 ; https://github.com/Kuberwastaken/claurst ; https://github.com/automagik-dev/genie ; https://hivecli.sh/ ; https://github.com/gherlein/localdev ; https://github.com/89luca89/clampdown ; https://qwenlm.github.io/qwen-code-docs/en/users/features/sandbox/

*Code-level evidence:* `/home/shiva/projects/niki` — audit matrix with file:line in §3; `src/cli/run.rs:221,457` (spend cap warn-only, verified this session); `winget/RavaniRoshan.niki*.yaml` vs `README.md:151` (winget mismatch); `README.md:10` (external hero URL); `src/util.rs:18` vs `src/cli/run.rs:435,542` (artifact perms).

---

## 9. Implementation plan (actionable, with verification)

*Priority P0 = must ship before Aug 18; P1 = first week after; P2 = roadmap. Every item maps to findings above.*

| # | Item | Source finding | Verify |
|---|---|---|---|
| P0-1 | Fix README winget command → `winget install RavaniRoshan.niki` | SQ1 gap 1 | `grep winget README.md` |
| P0-2 | Right-size artifact perms: `run.rs:435,542` → `write_restricted(0o600)` | SQ1 gap 4 / S11 | `cargo check` + code read |
| P0-3 | Honest spend-cap docs: `niki.example.toml` + README config text say "warn-only; hard enforcement post-launch" | SQ1 gap 2 / SQ5 | grep |
| P0-4 | Design tokens: resolve 5 open conflicts in `src/display/theme.rs` (spinner→accent.primary, `text.muted`, add missing tokens, dead styles, Auto light-detection) so `token.md` is true; update `token.md` conflict list | SQ1 gap 6 / §7 | `cargo check`, unit tests, `cargo test` |
| P0-5 | SECURITY.md: add 2026 trust-bar section (threat model, no-telemetry statement, key hygiene policy, digest-pinning guidance, spend-cap limitation, disclosure path already present) | SQ5 | review diff |
| P0-6 | Regenerate `assets/demo.gif` with the updated VHS script post-TUI-fixes | SQ1 gap 8 | file mtime + visual spot |
| P0-7 | `docs/marketing/funnel-plan.md`: launch-week funnel (48h window, Show HN Tue, TWiR, Reddit, PH badge, directories, llms.txt, comparison pages, GEO copy rules) | SQ2/SQ4/SQ7 | file exists, consistent with run-sheet |
| P0-8 | `docs/benchmarks.md`: add disclosure-manifest template (harness/dataset hashes, per-stage model+date, seed, n≥3, cost/run, cost/solved), "run it yourself", forbidden-claims list | SQ6 | file diff |
| P0-9 | README: add "no telemetry" + security-posture blurb; verify zero hidden outbound (code read) | SQ5 | grep for http/telemetry calls |
| P1-1 | Hard spend-kill (controller-level, mid-pipeline abort + salvage) | SQ5 / Ramp | test: cap below cost → aborts |
| P1-2 | Egress default-block in container backend with per-run allowlist | SQ5 | integration test |
| P1-3 | GitHub App / PR-comment bot (workflow touchpoint; CodeRabbit loop) | SQ4 | workshop + pilot repo |
| P1-4 | Interactive chat input (wire `display/input.rs` → engine) | SQ1 gap 3 / SQ4 | unit tests |
| P1-5 | Docker-sandbox e2e CI tests + enable parallel/red-blue in CI matrix | SQ1 gap 5 | CI green |
| P1-6 | Sigstore/cosign sign sandbox image + enforce digest pin in code; SBOM artifact | SQ5 supply chain | `cosign verify` |
| P1-7 | Grow frozen eval set n=23 → ~50 with per-case YAML records + cost disclosure | SQ6 | harness run |
| P2-1 | Docker Sandboxes (`sbx`) / Docker VMM re-eval after Linux GA (Oct 2026); gVisor path doc | SQ3 | revisit triggers |
| P2-2 | Session persistence + resume for retention | SQ4 | UX test |
| P2-3 | "/compare" page suite + llms.txt + GEO-optimized landing copy (landing repo) | SQ7 | SERP spot |

*Ground rule from research: every public claim in README/landing/PH must be reproducible from the repo (harness, commands, numbers). 2026 rewards self-restraint; one overclaim derails the launch.*

---

*Report produced 2026-08-13 by the deep-research workflow (7 parallel streams + adversarial verification + code-level adjudication). All URLs verified reachable on 2026-08-13; findings marked as carried-forward disagreements in §6.*