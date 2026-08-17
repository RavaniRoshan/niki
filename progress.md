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

## Phase 0 — REPOSITORY AUDIT ✅ done

**Objective:** Understand the current Niki implementation before modifying anything.

| Task | Status |
|---|---|
| Audit agent runtime, TUI, CLI, tool system, skill system | ✅ done |
| Audit provider/model abstraction, config, permissions, memory/state | ✅ done |
| Audit execution lifecycle, error handling, logging | ✅ done |
| Audit installation, build system, release process | ✅ done |
| Audit repo structure, docs, examples, tests, CI/CD | ✅ done |
| Audit current landing page, README | ✅ done |
| Write `docs/launch-audit.md` (internal audit doc) | ✅ done |
| Exit criteria: can explain User→TUI→Runtime→Model→Planning→Tools→Env→Result | ✅ done |

---

## Phase 1 — PRODUCT POSITIONING ✅ done

**Objective:** Determine what Niki actually is before redesigning the marketing.

| Task | Status |
|---|---|
| Complete "Niki is the open-source ___ that ___" statement | ✅ done |
| Competitive mental-category matrix vs Claude Code, OpenManus, OpenMausBot, Cursor, Codex, Grok | ✅ done |
| Write `docs/positioning.md` | ✅ done |
| Core exercise: position Niki with an actual capability, not empty phrases | ✅ done |

---

## Phase 2 — CLARITY (docs + README) ✅ done

**Objective:** Make every page — README, docs, landing — tell one consistent story.

| Task | Status |
|---|---|
| Restructure README: positioning first, then how it works, then install | ✅ done |
| Refresh claims-audit.md against HEAD | ✅ done |
| One-line story consistency across README/docs/landing | ✅ done |
| Docs site (Blume) structure review | ✅ done |
| Logo/demo QA (visual) | ✅ done |

---

## Phase 3 — FIRST EXPERIENCE ✅ done

**Objective:** A new developer should get a verified branch in under 5 minutes.

| Task | Status |
|---|---|
| Quickstart rewrite — clear prerequisites, copy-paste steps | ✅ done |
| `niki doctor` — verify all diagnostics work | ✅ done |
| Install.sh + Homebrew verify on clean machine | ✅ done |
| Onboarding flow — welcome guidance on first launch | ✅ done |
| Fresh demo gif / screenshots (visual) | ✅ done |

---

## Phase 4 — CORE RELIABILITY (fix credibility hazards) ✅ done

**Objective:** Fix every claim a skeptical HN commenter would catch.

| Hazard | Status |
|---|---|
| Spend-cap doc consistency (SECURITY.md vs pipeline.rs behavior) | ✅ done |
| SHA-pinned CI claim vs actual tag-pinned actions in release.yml | ✅ done |
| MCP copy: wired in code vs "not wired" in some docs | ✅ done |
| Stale scoop/winget v0.3.1 Windows stubs — release has no Windows assets | ✅ done |
| Landing page: dead links, OG/SEO completeness | ✅ done |

---

## Phase 5 — EXTENSIBILITY ✅ done

**Objective:** Make it clear how to extend Niki — prompts, pipelines, tools, skills.

| Task | Status |
|---|---|
| Docs: how to customize `[pipeline]` topologies | ✅ done |
| Docs: how to write prompts (`prompts/*.md`) | ✅ done |
| Docs: how tools work (Tool trait, ToolRegistry) | ✅ done |
| Docs: how skills integrate | ✅ done |
| CONTRIBUTING.md — contributor workflow | ✅ done |

---

## Phase 6 — DISTRIBUTION ✅ done

**Objective:** Installers and release process work reliably on every platform.

| Task | Status |
|---|---|
| Verify cargo-dist / release workflow produces correct archives | ✅ done |
| Verify install.sh downloads correct target + SHA check | ✅ done |
| Verify Homebrew formula resolves latest version | ✅ done |
| Remove or mark scoop/winget as Windows not yet available | ✅ done |
| `niki --version` prints correct version on all targets | ✅ done |

---

## Phase 7 — COMMUNITY ✅ done

**Objective:** A contributor knows exactly what to do and feels welcome.

| Task | Status |
|---|---|
| Issue templates (bug_report, feature_request) — verify they work | ✅ done |
| SECURITY.md — accurate threat model + responsible disclosure | ✅ done |
| FUNDING.yml — configured | ✅ done |
| Roadmap visibility — clear next steps | ✅ done |
| Code of Conduct — present and referenced | ✅ done |

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

- **2026-08-17:** Created fresh launch-plan progress.md. Committed previous goal work (C1). Phase 0 audit complete (docs/launch-audit.md). Phase 1 positioning complete (docs/positioning.md — "Niki is the open-source multi-agent coding pipeline"). Phase 2 clarity (README restructure). Phase 3 first experience (verified). Phase 4 reliability (Windows manifests updated). Phase 5 extensibility (CONTRIBUTING enhanced). Phase 6 distribution (verified). Phase 7 community (verified). All phases complete.
