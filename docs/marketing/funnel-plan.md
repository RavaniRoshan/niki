# NIKI Launch Funnel Plan — Aug 18, 2026

Source: `research/niki-launch-aug18-2026-wide-research.md` (SQ4/SQ7 + adversarial verification).
Ground rule: every public claim must be reproducible from the repo (harness, commands, numbers).
One overclaim derails the launch; 2026 punishes hype and rewards self-restraint.

## Positioning (one sentence, use everywhere)

> NIKI is the local-first way to turn a sentence into a reviewable PR — Planner, Coder,
> Tester and Reviewer agents run sandboxed on your machine, nothing touches your repo until
> you review the branch, and a full audit trail ships with every diff. No code ever leaves
> your machine.

Secondary angles (never in the headline): exact cost control (BYOK + spend visibility) ·
open source (Apache-2.0) · anti-vibe-coding / "agentic engineering" · "verified before review".

## The 48-hour window (the whole launch)

Target: **Rust-filtered GitHub Trending (~30–60 stars/day threshold, practitioner estimate) + HN front page.**
Compress everything into Tue–Thu (Aug 18–20). Friday = newsletter wave with "we hit Trending".

Hard dependency before the window opens:
- [ ] Release v0.3.0 cut (tag, binaries, manifests)
- [ ] `assets/demo.gif` regenerated from current TUI (vhs script)
- [ ] README converts in 10–15s: line 1 = the positioning sentence; GIF above the fold;
      5-command quickstart; security-posture block; badges
- [ ] Pre-submit directories (below) so listings index before day 1
- [ ] Reddit accounts: 2 weeks of organic participation + karma built

## Channel-by-channel (with evidence)

### 0. Pre-launch (now → Aug 17)
- **Directories (free, early-indexing, listed pre-launch):**
  - TerminalTrove — "Post a Tool" (https://terminaltrove.com/) — also feeds their LLM-readable index
  - opensourcealternative.to — submit under Developer Tools (https://opensourcealternative.to/)
  - Console.dev — pitch **Betas** section via hello@console.dev; criteria: dev-primary, self-service, actively maintained, good docs, no privacy negatives (https://console.dev/selection-criteria)
- **This Week in Rust:** PR to `rust-lang/this-week-in-rust` drafts/ (or tag @thisweekinrust.bsky.social). CFP rules: OSI license ✓ (Apache-2.0), public issue tracker ✓, labeled good-first-issues (add a few).
- **GEO baseline on landing page (landing repo):** answer-first paragraphs, question-phrased H2s, FAQ JSON-LD + SoftwareApplication schema, `llms.txt`, visible dates. Average site citability score is ~23/100 — baseline structure already wins.
- **Comparison pages draft** (docs or landing repo): NIKI vs OpenHands · vs Aider+gh workflow · vs Claude Code · vs Codex CLI · vs Devin · vs sentence-PR peers (Ivan/Genie/Hive/baro). Answer-in-first-paragraph, table-first, eval-harness numbers with disclosure manifests (§benchmarks.md). Publish post-launch week, not before.

### 1. Day 0 — Monday Aug 17, 21:00 PT (Tue 00:00 ET+)
- **Product Hunt** live 12:01am PT Tue: tagline "One sentence to a reviewed pull request",
  gallery = terminal recording/GIF, maker comment 180+ words (problem → why → build →
  honest limitation → question). Rank mechanics: 120–180 quality upvotes in first 3h;
  first-hour ~4x weight; **comments 40–50x upvotes** — reply <8 min for 4h; >100 upvotes/h
  spikes get cleared. PH is a badge/backlink/SEO channel, not the traffic engine (realistic:
  top-5 = 500–2,000 signups; badge ≈ +17% ongoing signups). LaunchPact pacts 8–15.

### 2. Day 1 — Tue Aug 18 (release day, core)
- **GitHub release** early morning ET: v0.3.0 tag, binaries, changelog, release notes with the
  eval-harness disclosure manifest linked.
- **Show HN** ~9–11am ET, within 2–4h of release: plain factual title —
  `Show HN: NIKI – sentence to reviewable PR, four sandboxed agents, BYOK` —
  top comment within 60s naming the obvious objection ("why not just Claude Code?" + honest
  limitation, e.g. first-run image pull), reply to EVERY comment 2h, stay 6h. Never ask for upvotes.
  Empirical: ~1.4 stars per HN point; 92% of star impact in 48h; median post = 2 points, front
  page = 10k–50k views. (Timing dispute: Mon 00:00 UTC empirical best vs Tue–Thu playbook
  consensus — we pick Tue with presence; data says author presence beats slot.)
- **This Week in Rust** mention targets Tue issue.
- **Reddit** (story-framed, one post per sub, no crossposting identical text):
  r/selfhosted, r/opensource, r/rust + r/SideProject; "I built X because Y" narrative,
  embedded GIF, honest tradeoffs, single-command install; reply 6h, keep a week. Self-promo
  <10% rule — karma pre-built.

### 3. Day 1–2 — Wed Aug 19 (amplification)
- **X**: ONE concrete artifact post (demo clip + a real number, e.g. cost per PR or
  time-to-PR) + reply-thread presence; pin launch thread. (zerobrew: one tweet = 380k
  impressions; cmux: single viral post.)
- **Bluesky** mirror; tag @thisweekinrust.
- **Lobsters** backup post (plain title, no blast smell).

### 4. Day 5 — Fri Aug 21 (newsletter wave)
- Pitch 5–10 dev newsletters with **"NIKI hit GitHub Trending / HN front page" as social
  proof** + runnable demo. Free editorial first (Console.dev full review, Hacker Newsletter).
- Hold paid sponsorships (TLDR ≈ $15k primary; devtools CPCs $1.70–15) until conversion data exists.

### 5. Post-launch week (compounding flywheel)
- Publish `/compare/` pages + "one sentence becomes a reviewed PR" blog (docs-driven SEO:
  40–60% of dev-tool organic traffic; AI-citation: answer-in-first-paragraph).
- Keyword ownership: "AI agent sandbox / run agent in Podman container" (low-competition,
  NIKI's built-in sandbox beats wrappers) · "AI pull request generator" (weak SERP, real
  queries) · "sentence to pull request" (no category leader yet).
- GitHub topics: rust, ai-agent, cli, byok, podman, sandbox, coding-agent, pr.
- 72h follow-through (~40% of total traffic): thank-you comments, reviews request on PH.

## Metrics (instrument from day 1)
- **Activation:** Time-to-First-PR (target <5 min; initiative smoke task) — `niki init` completion rate.
- **Retention:** weekly cohort of 5-of-14-days users (Microsoft finding: habit forms in 2 weeks; retained users merge +24% more PRs). Signup→star conversion on GitHub.
- **Growth:** stars/day velocity against the 30–60/day Trending threshold; PH upvote/comment ratios; HN score→stars correlation.

## Disputes to be aware of (don't over-optimize)
- Show HN best day/time: empirical dataset (Mon 00:00 UTC) vs 2026 playbooks (Tue–Thu 8–11am ET) — presence > slot.
- PH day: 2026 sources say Tue for weekly-badge math; 2024 sources said Sunday — Tue chosen.
- Comparison-page "3–5x conversion" is unverified; the SERP-ranking dominance is verified.
- GitHub Trending thresholds are practitioner estimates — calibrate against github.com/trending/rust the week before.

## Copy guardrails (from security + sentiment research)
- Never write: "agents can't run away on cost" (cap is warn-only), "SWE-bench X%", "100%", "fully secure/AI-safe".
- Do say: sandboxed, audit trail, BYOK, no telemetry, warn-only spend cap (honesty is the trust wedge).
- Terminology: "agentic engineering", "verified before review" — not "vibe coding".