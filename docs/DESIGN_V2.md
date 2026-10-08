# NikiCode Final Pack — Design (G0, for owner approval)

Status: APPROVED by owner (2026-10-08). No code written. Rule of proof holds.

## 1. Name plan (G1)
- Canonical binary `nikicode` (`cmd/nikicode`); module path unchanged
  (`github.com/RavaniRoshan/niki`) to avoid breaking go tooling.
- `nc` alias: symlink created by `make install`, not a second binary.
- `niki` compat: symlink kept; old name survives only there + migration code.
- Config home: new `internal/paths` package is the single choke point.
  Canonical `~/.nikicode`; first boot copies `~/.niki` → `~/.nikicode` once
  (copy, never move; writes `MIGRATED_FROM`; verifies file count + bytes).
- Env: canonical `NIKICODE_*`, legacy `NIKI_*` honored as fallback, `doctor`
  reports which fired. ~10 scattered `"~/.niki"` literals collapse into paths.
- Strings: boot/help/version/TUI say `NikiCode`; `⚡` emoji removed (glyph
  rule: no emoji). Original wordmark: styled `NikiCode` text treatment +
  orb motif, Lip Gloss when styled, pure-ASCII fallback; snapshot-tested at
  40 / 60 / 80+ columns. Nothing copied: no competitor name, tagline, asset.
- Gate: `nikicode --version` and `nc --version` both print version via
  the B0 fast path; `grep -ri old-name` hits only compat alias + migration.
- G1 grep-gate scoping (decided): ALLOWED old-name hits are (1) the module
  path `github.com/RavaniRoshan/niki` (unchanged by design), (2) the
  `niki` compat symlink + `paths.Compat`, legacy `NIKI_*` env fallback,
  legacy filenames (`niki.toml`, `.niki/`, `NIKI.md`) in fallback code and
  in tests that prove the fallback, and the one-time migration; (3) frozen
  build-evidence docs (PACK/CAPABILITY/foundation-pack/DESIGN/PARITY/
  ATLAS/ARCHITECTURE/SECURITY/DOGFOOD histories, PROGRESS history rows,
  docs/review frames, perf/*.txt) — renaming those would falsify history.
  Everything live (product strings, help, TUI, live docs, temp-file names,
  MCP/ACP names, URIs, system prompt) says NikiCode.

## 2. Claim audit (positioning line, clause by clause)
| Clause | Verdict | Proving probe |
|---|---|---|
| agentic coding tool that lives in your terminal | BUILT | PTY boot + `exec` fixture run |
| understands your codebase | TO-BUILD (G2) | Q&A answers cite real file:line; invented symbol refused |
| executes routine tasks | TO-BUILD (G2) | 7 recipes run from NL, each with acceptance test |
| explaining complex code | TO-BUILD (G2) | `/explain` with citations + blame view |
| handling git workflows | TO-BUILD (G2) | 8 git ops against throwaway repos |
| all through natural language commands | TO-BUILD (G3) | scripted NL tasks, no tool names; correction re-routes; undo reverts |
| faster than Claude Code | TO-BUILD, SCOPED (G4) | per-metric table only; bare comparative never ships |

## 3. The three promises (G2)
- Recipes: `recipes/*.md` (frontmatter name/desc/steps), loader reuses the
  skills frontmatter pattern; each recipe maps to real tools behind the
  permission gate; test/lint/format/build/commit/scaffold/refactor/docs.
- Explain: `/explain <symbol|file>` + codebase Q&A; resolver = grep + ranged
  read (line numbers mandatory); unresolved symbol → explicit refusal + what
  was searched. Blame view shells to `git blame` + history walk.
- Git: shell out to system `git`, parse porcelain; commit(branch/rebase/
  conflicts/pr-draft/changelog/review/blame) each tested in `t.TempDir()`
  repos (skip if `git` absent). PR draft writes markdown locally (no net).
- Claims-as-code: `internal/claimcheck` test fails if any README/`--help`/
  TUI sentence lacks a passing CLAIMS.md row.

## 4. Benchmark protocol (G4)
- `nikicode bench`: stdlib os/exec+time; CSV/JSON to `docs/bench/raw/`;
  itself unit-tested; numbers flow to BENCH.md, never hand-edited.
- Metrics: TTFF, input-ready, `--version` cold, echo p95, idle redraws,
  idle RSS, peak RSS, footprint, MCP+skills warm-up, per-turn overhead
  (model time excluded). Same machine, cold+warm, N>=30, median+p95.
- References: Codex CLI 0.152.1 installed and measurable. Claude Code is
  NOT installed: no measured Claude numbers until OWNER-VERIFY installs it;
  until then only Codex-measured deltas + public-writeup shape.
- Summary states win/tie/loss per metric; end-to-end task time declared
  model-bound and identical on the same model.

## 5. Packaging, trust, docs (G5–G7)
- Personal-only; `make install` = one command (build + `nikicode` + `nc` /
  `niki` symlinks + migration note). Distribution section in README marked
  OUT/unbuilt. Soak: scripted mock-provider day (RSS-over-time CSV),
  kill-mid-turn resume (extends existing test), 5xx/offline/MCP-down paths
  assert message text. VERDICT traces every clause to a probe and names
  losses and unbuilt items. THIRD_PARTY.md versions fixed to match go.mod.
- Carried-over owner calls (DECIDED 2026-10-08): FUND THE TRIMMING — a
  pre-G1 slice brings warm-start to <=25 ms and idle CPU to <1% before the
  rename lands. Positioning line is FIXED: every clause must be built, none
  may be cut. Claude Code column: OWNER-VERIFY install on this machine,
  then measure both. Env: dual NIKICODE_*/NIKI_* prefix approved.
