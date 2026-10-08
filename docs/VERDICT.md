# NikiCode Verdict (G7/G6)

## Final positioning statement

> NikiCode is an agentic coding tool that lives in your terminal,
> understands your codebase, and helps you code faster by executing
> routine tasks, explaining complex code, and handling git workflows —
> all through natural language commands.

Every clause traces to a passing probe (docs/CLAIMS.md):

| Clause | Claim | Probe (ran 2026-10-08) |
|---|---|---|
| agentic coding tool in your terminal | C1 | PTY coding loop passes; `nikicode exec` fixture returns |
| understands your codebase | C2 | explain citations stat-verified; unknown symbols refused |
| executes routine tasks | C3 | 7 recipes end-to-end (test/lint/build/commit/scaffold/refactor/docs) |
| explaining complex code | C4 | `/explain` cited answers + refusal; blame view on throwaway repo |
| handles git workflows | C5 | 8 ops on throwaway repos incl. conflict resolve/continue |
| natural-language commands | C6 + C18 | routing incl. boundaries; plan-first multistep; corrections; undo/redo |

The speed half lives in docs/BENCH.md, per metric below. The blanket
"faster than Claude Code" is NOT claimed (C16 BLOCKED): Claude Code is
not installed here, and end-to-end time is model-bound by construction.

## Speed verdict (same machine, N>=30, medians)

| Metric | NikiCode | Codex | Verdict |
|---|---|---|---|
| first terminal bytes | 5.38ms | 17.56ms | WIN (-69%) |
| content paint (input-ready) | 16.89ms | 170.72ms* | WIN on paint (-90%); *Codex sits behind an update modal that blocks input |
| `--version` cold | 6.21ms | 20.20ms | WIN (-69%) |
| keystroke echo p95 | 5.63ms | unmeasurable here | NOT CLAIMED (reference gated) |
| idle redraws | 0 | 0 | TIE |
| idle RSS | 16.12MB (9.1 real-home) | 69.32MB | WIN (-77%) |
| peak RSS (mock turn) | 10.38MB | unmeasured (needs model) | NOT CLAIMED |
| binary footprint | 19.70MB | 243.67MB | WIN (-92%) |
| MCP+skills warm-up | 1.64ms index; 16ms boot w/ 5 MCP + 50 skills | n/a | informational |
| per-turn overhead (no model) | 2.42ms | n/a (black box) | informational |
| end-to-end task time | model-bound | model-bound | DECLARED TIE — NikiCode does not win here |

## Where NikiCode does not win (explicit)

1. End-to-end task time: identical on the same model by construction.
2. Anything vs Claude Code: uninstalled; no number claimed.
3. Keystroke echo vs Codex and input-usability vs Codex: unmeasurable in
   this environment (update-modal gate); not claimed.
4. Absolute idle CPU: production reads 1.8% on this WSL2 VM (a bare 120Hz
   ticker alone reads 2.5%); the <1% gate holds on the deterministic unit
   test. Bare metal reads lower.

## Deliberately not built (and why)

- Public distribution (Homebrew, website, launch, signing, auto-update):
  personal tool by decision; see README packaging section.
- Release-channel QA: `.goreleaser.yaml`/`release.yml` dormant.
- `ACP/IDE polish beyond adapters, web dashboard, telemetry: out of
  scope for a personal harness (adapters exist and are tested).
- Model-driven intent parsing beyond deterministic routing: `do` routing is
  phrase-based on purpose (no spend, testable); model-driven planning
  stays in-session.
- Live-turn benchmark vs references: needs owner-approved provider spend
  (C10 method documented).
- A week-long TUI dogfood log: this pack was dogfooded per-slice (see
  docs/DOGFOOD.md); a full week in-terminal is owner territory.
- govulncheck in CI: unrunnable offline here; `go vet` + staticcheck ran
  clean. OWNER-VERIFY with network.
