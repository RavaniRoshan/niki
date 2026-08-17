# NIKI — COMPLETE PHASE-BY-PHASE OPEN-SOURCE LAUNCH PLAN

> **Started:** 2026-08-17
> **Optimization order:** CLARITY → FIRST EXPERIENCE → CORE RELIABILITY → EXTENSIBILITY → DISTRIBUTION → COMMUNITY
> **Launch target:** 2026-08-18 (Product Hunt)
> **Last updated:** 2026-08-17

## Mission

Transform Niki from a technically capable AI-agent project into a polished, clearly positioned, highly adoptable open-source product.

Do not optimize for feature count.
Do not copy competitors feature-for-feature.
Do not add infrastructure simply because another agent project has it.

The final product should make a new developer understand:

1. What Niki is.
2. Why Niki exists.
3. What Niki can actually do.
4. Why Niki is different.
5. How to install it.
6. How to extend it.
7. Why they should contribute.

---

## Phase 0 — REPOSITORY AUDIT ❌ undone

**Objective:** Understand the current Niki implementation before modifying anything.

| Task | Status |
|---|---|
| Audit agent runtime, TUI, CLI, tool system, skill system | ❌ undone |
| Audit provider/model abstraction, config, permissions, memory/state | ❌ undone |
| Audit execution lifecycle, error handling, logging | ❌ undone |
| Audit installation, build system, release process | ❌ undone |
| Audit repo structure, docs, examples, tests, CI/CD | ❌ undone |
| Audit current landing page, README | ❌ undone |
| Write `research/launch-audit.md` (internal audit doc) | ❌ undone |
| Exit criteria: can explain User→TUI→Runtime→Model→Planning→Tools→Env→Result | ❌ undone |

---

## Phase 1 — PRODUCT POSITIONING ❌ undone

**Objective:** Determine what Niki actually is before redesigning the marketing.

| Task | Status |
|---|---|
| Complete "Niki is the open-source ___ that ___" statement | ❌ undone |
| Competitive mental-category matrix vs Claude Code, OpenManus, OpenMausBot, Cursor, Codex, Grok | ❌ undone |
| Write `research/positioning.md` | ❌ undone |
| Core exercise: position Niki with an actual capability, not empty phrases | ❌ undone |

---

## Phase 2 — CLARITY (docs + README) ❌ undone

**Objective:** Make every page — README, docs, landing — tell one consistent story.

| Task | Status |
|---|---|
| Restructure README: positioning first, then how it works, then install | ❌ undone |
| Refresh claims-audit.md against HEAD | ❌ undone |
| One-line story consistency across README/docs/landing | ❌ undone |
| Docs site (Blume) structure review | ❌ undone |
| Logo/demo QA (visual) | ❌ undone |

---

## Phase 3 — FIRST EXPERIENCE ❌ undone

**Objective:** A new developer should get a verified branch in under 5 minutes.

| Task | Status |
|---|---|
| Quickstart rewrite — clear prerequisites, copy-paste steps | ❌ undone |
| `niki doctor` — verify all diagnostics work | ❌ undone |
| Install.sh + Homebrew verify on clean machine | ❌ undone |
| Onboarding flow — welcome guidance on first launch | ❌ undone |
| Fresh demo gif / screenshots (visual) | ❌ undone |

---

## Phase 4 — CORE RELIABILITY (fix credibility hazards) ❌ undone

**Objective:** Fix every claim a skeptical HN commenter would catch.

| Hazard | Status |
|---|---|
| Spend-cap doc consistency (SECURITY.md vs pipeline.rs behavior) | ❌ undone |
| SHA-pinned CI claim vs actual tag-pinned actions in release.yml | ❌ undone |
| MCP copy: wired in code vs "not wired" in some docs | ❌ undone |
| Stale scoop/winget v0.3.1 Windows stubs — release has no Windows assets | ❌ undone |
| Landing page: dead links, OG/SEO completeness | ❌ undone |

---

## Phase 5 — EXTENSIBILITY ❌ undone

**Objective:** Make it clear how to extend Niki — prompts, pipelines, tools, skills.

| Task | Status |
|---|---|
| Docs: how to customize `[pipeline]` topologies | ❌ undone |
| Docs: how to write prompts (`prompts/*.md`) | ❌ undone |
| Docs: how tools work (Tool trait, ToolRegistry) | ❌ undone |
| Docs: how skills integrate | ❌ undone |
| CONTRIBUTING.md — contributor workflow | ❌ undone |

---

## Phase 6 — DISTRIBUTION ❌ undone

**Objective:** Installers and release process work reliably on every platform.

| Task | Status |
|---|---|
| Verify cargo-dist / release workflow produces correct archives | ❌ undone |
| Verify install.sh downloads correct target + SHA check | ❌ undone |
| Verify Homebrew formula resolves latest version | ❌ undone |
| Remove or mark scoop/winget as Windows not yet available | ❌ undone |
| `niki --version` prints correct version on all targets | ❌ undone |

---

## Phase 7 — COMMUNITY ❌ undone

**Objective:** A contributor knows exactly what to do and feels welcome.

| Task | Status |
|---|---|
| Issue templates (bug_report, feature_request) — verify they work | ❌ undone |
| SECURITY.md — accurate threat model + responsible disclosure | ❌ undone |
| FUNDING.yml — configured | ❌ undone |
| Roadmap visibility — clear next steps | ❌ undone |
| Code of Conduct — present and referenced | ❌ undone |

---

## Visual vs Non-Visual Split

| Work | Model required |
|---|---|
| Phase 0 audit doc, positioning analysis, progress.md, README/docs copy, CI fixes, manifests cleanup, claims-audit refresh | Non-visual (text model) |
| Phase 2 logo/demo QA, Phase 3 demo gif/screenshots, landing page visual review | Visual-capable model |
| All other phases | Non-visual (text model) |

---

## Commits (one per phase, on master)

| # | Commit message |
|---|---|
| C1 | feat: unified TUI mission control, tool runtime & activity grammar *(done — pending work)* |
| C2 | docs: fresh launch-plan progress + Phase 0 repo audit |
| C3 | docs: Phase 1 product positioning |
| C4 | docs: Phase 2 clarity — README restructure + claims refresh |
| C5 | docs: Phase 3 first experience — quickstart + onboarding |
| C6 | fix: Phase 4 credibility hazard fixes |
| C7 | docs: Phase 5 extensibility docs |
| C8 | fix: Phase 6 distribution — installer/release verification |
| C9 | docs: Phase 7 community — templates + security + funding |

Landing page changes (niki-site repo): separate commits in `research/refs/niki-site`.

---

## Changelog

- **2026-08-17:** Created fresh launch-plan progress.md. Committed previous goal work (C1). Starting Phases 0–7.
