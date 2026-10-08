# NikiCode Claims Ledger

Spine of the final pack. Rule: no claim ships without a passing probe.
Status keys: UNPROVEN · PROVEN · OWNER-VERIFY · CUT.

| # | Claim (exact user-facing text) | Phase | Probe command | Result |
|---|---|---|---|---|
| C1 | NikiCode is an agentic coding tool that lives in your terminal | G1 | `NIKI_PTY_TESTS=1 go test ./cmd/nikicode/ -run TestPTY -count=1` + `nikicode exec` fixture run | PROVEN 2026-10-08: PTY coding loop passes; `nikicode exec` fixture returns mock turn |
| C2 | NikiCode understands your codebase: codebase Q&A cites real file:line locations and refuses unknown symbols | G2 | `go test ./internal/explain/ -count=1` (citation-exists + refusal tests) | PROVEN 2026-10-08: 4/4 pass; every citation stat-checked against disk |
| C3 | NikiCode executes routine tasks: test, lint/format, build, commit, scaffold, multi-file refactor, docs — each runnable from natural language behind the permission gate | G2 | `go test ./internal/recipes/ -count=1` (7 acceptance + match + gate tests) | PROVEN 2026-10-08: 10/10 pass; `nikicode do` demos ran live |
| C4 | NikiCode explains complex code with file:line citations, plus a blame/history view | G2 | `go test ./internal/tui/ -run TestSlashExplainCites` + `go test ./internal/git/ -run TestChangelogAndBlameAndReview` | PROVEN 2026-10-08: cited answer + refusal shown; blame verified against throwaway repo |
| C5 | NikiCode handles git workflows: status, commit from the staged diff, branch, rebase, conflict cycle, PR draft, changelog, staged-diff review, blame-explore | G2 | `go test ./internal/git/ ./internal/tools/ -run Git -count=1` + `nikicode git` subcommands | PROVEN 2026-10-08: 7/7 package incl. conflict resolve/continue; registry + CLI dispatch pass |
| C6 | NikiCode acts through natural-language instructions routed to recipes, explanations, and git ops | G2 | `go test ./internal/intent/ -count=1` + live `nikicode do` set | PROVEN 2026-10-08: 5/5 routing incl. boundary cases; NL demos ran live |
| C18 | NikiCode holds conversational ergonomics: follow-ups keep context, @-mentions resolve, multi-step previews before running, "X then Y" runs in order, corrections re-route, undo/redo revert and re-apply | G3 | `go test ./internal/mention/ ./cmd/nikicode/ -run TestDo -count=1` + live `do --plan`/correction/undo/redo demos | PROVEN 2026-10-08: mentions 3/3; plan-first output; multistep order; var-keyed correction; undo removes + redo restores |
| C7 | NikiCode prints `--version` faster than Codex (scoped: this machine, hyperfine N=30) | G1/G4 | `nikicode bench --metric version --n 30 [--bin ...]` raw docs/bench/raw/version-*.json | PROVEN 2026-10-08: NikiCode p50 6.21/p95 6.90 vs Codex p50 20.20/p95 21.34 (-69%) |
| C8 | NikiCode paints first terminal bytes faster than Codex (scoped: tools/ttff, same PTY) | G1/G4 | `nikicode bench --metric ttff --n 30 --keyword ...` raw docs/bench/raw/ttff-*.json | PROVEN 2026-10-08: first bytes p50 5.38 vs 17.56; content paint p50 16.89 vs 170.72 (-90%) |
| C9 | NikiCode is lighter at idle and smaller on disk than Codex (scoped: ttff RSS + binary bytes) | G1/G4 | `nikicode bench --metric ttff/peak/footprint` raw docs/bench/raw/ | PROVEN 2026-10-08: idle RSS p50 16.12 vs 69.32 (-77%); peak 10.38MB; binary 19.70 vs 243.67MB (-92%) |
| C10 | End-to-end task time is model-bound and identical on the same model (NikiCode does not win here) | G4 | same-model timed turns, both harnesses (OWNER-VERIFY: spend approval) | UNPROVEN; BENCH.md declares the tie by construction |
| C11 | NikiCode survives a full working day and never loses a session | G5 | `nikicode soak --turns 200 --mcp fake --hook /bin/true` + kill-9 test + degradation demos | PROVEN 2026-10-08: 200 turns 0 crashes RSS +4.9MB plateau heap 2.7MB; kill-9 recovered 16 gap-free events; no-net/MCP-down/5xx each show what+hints |
| C12 | One-command local install works on this machine | G6 | `make install` on a clean checkout path | PROVEN 2026-10-08: local `git clone` to /tmp/cleanclone + `make install PREFIX=/tmp/cleanbin` → nikicode/nc/niki all print the version |
| C16 | BLANKET "faster than Claude Code" summary | G4 | BLOCKED: may ship only as a summary citing C7–C9 deltas alongside C10 | BLOCKED (no blanket appears in README/--help/TUI until G4 proves it) |
| C17 | Foundation harness capabilities behind sessions, providers, MCP, skills, sandbox, and the TUI (grandfathered pre-CLAIMS surfaces) | F1–F9 | `go test ./... -race` + docs/PARITY.md VERIFIED rows | PROVEN 2026-10-08: 24/24 packages; 9/9 parity rows VERIFIED |

Notes: C7–C9 name Codex only where measured. No blanket "faster than Claude
Code" ships except as a summary citing C7–C9 deltas alongside C10.
