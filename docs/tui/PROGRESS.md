# NikiCode Workbench progress ledger (docs/tui/PROGRESS.md)

Chronological log for the Workbench work (docs/tui/WORKBENCH.md).
Post-compact re-read file. Each entry states what was verified and
what remains. Layout of the Classic Flow (the Stream view) is
spec'd in docs/ui/CLASSIC.md and logged in docs/ui/PROGRESS.md.

## 2026-10-12 — Phase U0: WORKBENCH.md drafted, awaiting approval

- Read the packs (PACK.md, CAPABILITY_PACK.md), DESIGN_V2.md, the
  live seam (internal/protocol/event.go) and the current TUI
  (internal/tui/app.go, state.go, keys.go, commands.go) before
  writing. Pack-name mismatch recorded: the mission named
  docs/FINAL_PACK_V2.md and docs/TUI_PACK.md; neither exists —
  DESIGN_V2.md + CAPABILITY_PACK.md + PACK.md visual_spec +
  docs/ui/CLASSIC.md are the real sources.
- Wrote docs/tui/WORKBENCH.md: the layout, the region list, and
  the completed wiring table mapped to the REAL event seam (every
  row marked HAVE / EXTEND / DEFER against protocol.EventType and
  the EngineEvent fields). No element is listed without its driver.
- Seam audit result: 24 of the mission's elements are HAVE today
  (tool rows, plan, prose, activity, gates/approvals, verdict +
  contract strip, context meter via Usage, cost, notices, errors,
  palette/one registry, queue local+steer, header/mode via config +
  Op echo). 4 need the minimal additive extensions proposed in
  §6 (E1 step attribution, E2 patch diff, E3 checkpoints, E4
  subagent identity — optional). 3 are DEFER with reasons: no
  reasoning-summary event; the subagent child stream is
  intentionally not forwarded (engine/subagent.go), so the Agent
  detail panel cannot be event-driven; checkpoints until E3.
- No UI code written (U0 stops for approval, per process rule).
  Nothing to verify by build yet; the doc is the deliverable.
- Open: the four approval questions in WORKBENCH.md §10 (layout +
  table, each seam extension, alt-screen vs inline split, the
  ui.layout value names).

## 2026-10-12 — U1: workbench shell + regions (built, tested)

Owner answer received as the U1 goal statement; the shell was
built under it. The default layout stays "classic" until the
§10 call — `ui.layout = "workbench"` selects the new shell.

Shipped (`internal/tui/workbench.go`, wiring in `app.go` /
`state.go` / `classic.go`, config in `internal/config/model.go`):
- Six regions: header (full orb >=80 cols, compact row below;
  model shows the em dash until configured), left rail (PLAN from
  plan_updated, AGENTS from the subagent lifecycle events — the
  rail's real event source), centre (viewport + live block: gate
  inline replaces the activity line, verdict card, contract
  strip), right rail (CONTEXT, COST — em dash until usage),
  composer, status bar (mascot state word, model, agent count,
  interrupt hint while busy, registry hints).
- `state.Agents` + `state.TurnCost` added; subagent events and
  usage events now fill them. `SwarmAgents` remains unused —
  flagged for removal, not silently deleted.
- Activity line ABSENT when idle (P21); instruments never fake a
  zero (P22). Below 80 columns the right rail collapses and the
  instruments move to the status bar (registry hints yield; the
  composer row carries the palette hint).
- ANSI-aware truncation (`TruncateStyledTo`, width.go): the
  rune-based truncator over-measured styled strings and clipped
  rails mid-escape — found by the first test run, fixed, and
  now the single truncation path for styled text.

Verification (all printed this session):
- `go build ./...`, `go vet ./...`, `go test ./...` all clean;
  `go test -race ./internal/tui/` clean; golangci-lint 0 issues
  on internal/tui + internal/config.
- PTY e2e: `NIKI_PTY_TESTS=1 go test ./cmd/nikicode/ -run
  'PTY|NonTTY'` ok.
- Per-element absent-before/present-after: table test over plan,
  agents, context, cost, activity, interrupt hint, agent count,
  contract strip, gate (safe option focused), error row, verdict
  card — all proven both directions (workbench_test.go).
  CHECKPOINTS proven absent after a full run (no event exists).
- Widths 100x27 / 80x24 / 60x20: every line <= width, frame <=
  height, status bar last, trace visible; resize preserves state.
- Stream parity: the same events render the same facts in the
  workbench and the classic (stream) view.
- Plain (NIKI_PLAIN=1) and ASCII (NIKI_ASCII=1) frames are pure
  ASCII with words; plain mode has no escape sequences.
- No-mock check: a test greps the UI package's non-test files for
  placeholder literals (clean).
- Goldens: 30 new workbench frames (5 stages x 3 sizes x
  normal/plain) generated and printed before acceptance; the
  originality guard now covers them (scoped to classic+workbench
  frames; the ledger fossil's boxed chrome is excluded by
  design).
- Perf rig (tools/ttff -json, 3 runs, medians): first paint
  6.34 ms (recorded baseline 5.7), input-ready 6.34 ms (5.7),
  idle RSS 22.84 MB (22.9 — flat). The +0.6 ms is inside the
  rig's own jitter band (the three runs spanned 6.09-6.86 ms)
  and the workbench is not in the default boot path. Render
  flatness holds via the shared viewport: 169 us at 100 rows vs
  203 us at 5000 (BenchmarkViewFlatness100vs5000).
- ANSI dumps: docs/tui/review/workbench_frame{1..5}_{100x27,
  80x24,60x20}.ansi (+ .plain.txt) for owner review.

Open: §10 owner calls (default flip, E1-E5). U2 (step grouping
via E1, diff block via E2) is blocked on the E1/E2 answers.
