# NikiCode Benchmark Ledger

Protocol: same machine (AMD Ryzen 7 4800H, WSL2 — 8C/16T), fresh processes,
N>=30 with median + p95 unless noted. Every number comes from
`nikicode bench`; raw samples live under `docs/bench/raw/` (file per
metric per run). Nothing hand-edited. PTY runs use an 80x24 window with
terminal queries answered like a real terminal.

Reference versions (pinned): Codex CLI `codex-cli 0.152.1` (installed,
measured where the black box allows). Claude Code: NOT installed — every
cell is UNMEASURED until OWNER-VERIFY installs it (steps below).

## Results (2026-10-08)

| Metric | NikiCode | Codex | Claude Code | Delta (NikiCode vs Codex) | Verdict |
|---|---|---|---|---|---|
| time-to-first-terminal-bytes (N=30) | p50 5.38 / p95 5.96 ms | p50 17.56 / p95 18.48 ms | unmeasured | -12.2ms (-69%) | WIN |
| time-to-input-ready content paint (N=30) | p50 16.89 / p95 17.90 ms (`NikiCode` header) | p50 170.72 / p95 176.05 ms (`Ask Codex` header; input itself is modal-blocked, see notes) | unmeasured | -154ms (-90%) | WIN on paint; input-usability unmeasured for Codex |
| `--version` cold start (N=30) | p50 6.21 / p95 6.90 ms | p50 20.20 / p95 21.34 ms | unmeasured | -14.0ms (-69%) | WIN |
| keystroke-echo p95 (N=30 keys) | p50 4.62 / p95 5.63 ms | unmeasurable (update modal eats keys; Esc no-op; forcing a choice risks triggering the auto-update installer) | unmeasured | — | reference unmeasurable here |
| idle redraws (2 s window, N=30) | 0 bytes p50/p95 | 0 bytes p50/p95 (N=30, modal-gated idle) | unmeasured | 0 / 0 | TIE |
| idle RSS (N=30) | p50 16.12 MB fresh-HOME (9.1 MB real-HOME per tools/ttff) | p50 69.32 MB | unmeasured | -53MB (-77%) | WIN |
| peak RSS (mock exec turn, N=5) | p50 10.38 MB | unmeasured (needs a model turn) | unmeasured | — | nikicode-only |
| binary footprint | 19.70 MB | 243.67 MB binary (331 MB install with host) | unmeasured | -224MB (-92%) | WIN |
| MCP+skills warm-up | skills index p50 1.64ms; boot with 5 MCP + 50 skills paints in 16ms (test) | n/a (different architecture) | unmeasured | — | informational |
| per-turn harness overhead (mock, N=10, model time excluded) | p50 2.42 / p95 2.52 ms | n/a (black box) | unmeasured | — | informational |
| end-to-end task time | model-bound | model-bound | model-bound | identical by construction | DECLARED TIE (not a win; C10) |

Raw logs: `docs/bench/raw/` — `version-*.json` (both binaries),
`ttff-*.json` (both), `echo-*.json` (nikicode), `peak-*.json`,
`footprint-*.json` (both), `skills-*.json`, `turn-*.json`.
Reproduce, e.g.: `nikicode bench --metric ttff --n 30 --keyword NikiCode
--out docs/bench/raw`; `nikicode bench --metric version --n 30 --bin
~/.local/bin/codex --out docs/bench/raw`.

## Notes (read before quoting)

- First-bytes vs content paint: both binaries write capability queries
  immediately; the content-paint rows time the real header. Codex's header
  says `model: loading` and sits behind an "Update available" modal that
  blocks all input; dismissing it risks running its auto-update installer,
  so input-ready means *painted*, and echo/input-usability are recorded
  unmeasurable rather than guessed.
- RSS: fresh-HOME runs (16.1MB) pay first-run allocation; the real-HOME
  number (9.1MB) is the daily-driver figure. Codex was measured with its
  real HOME (it needs its auth/config to paint at all).
- Cold vs warm: every run is a fresh process (cold); the page cache is
  warm after the first run. No cache-dropping was performed (no root).
- End-to-end task time is model-bound: with the same provider and model,
  both harnesses spend their time waiting on tokens. No live-turn
  comparison was run (needs owner-approved spend); C10 stays UNPROVEN
  with this method attached.

## Summary (scoped, no blanket)

- NikiCode wins, measured: first bytes (-69%), content paint (-90%),
  `--version` (-69%), idle RSS (-77%), binary footprint (-92%), idle
  redraws tie at zero.
- NikiCode does NOT win: end-to-end task time (model-bound, declared
  tie); keystroke echo and input-usability vs Codex (unmeasurable here,
  not claimed); anything vs Claude Code (uninstalled, not claimed).
- No blanket "faster than Claude Code" appears in README/--help/TUI
  (C16 stays BLOCKED).

## OWNER-VERIFY (needs you)

1. Install Claude Code on this machine, then:
   `nikicode bench --metric version --n 30 --bin <claude-bin> --out docs/bench/raw`
   plus ttff with its ready keyword. Until then its column stays UNMEASURED.
2. Live-turn spend: approve a provider + model + USD cap for same-model
   timed turns in both harnesses (closes C10).
