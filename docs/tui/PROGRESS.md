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

## 2026-10-12 — U2: run trace (steps, diffs, reasoning) — built and tested

Seam extensions (all additive, optional fields; schema version
unchanged): `EngineEvent.Step` (E1), `EngineEvent.Diff` (E2),
`EventReasoningDelta` (E6) in internal/protocol/event.go.

Engine (internal/engine/trace_u2.go + agent.go wiring):
- `plan_updated` finally has an emitter: after a successful
  update_plan call the engine emits the tool's own steps (its
  single source of truth, read through the registry). Status map:
  in_progress -> active; pending/completed/cancelled pass through.
- Tool events carry `Step` = the in_progress step's label at emit
  time; no plan means no attribution (never invented).
- `edit_file`/`write_file`/`notebook_edit` completions carry
  `Diff` = the unified diff computed from the file's real
  before/after bytes (engine's own writer: common prefix/suffix
  trimmed, one hunk, correct line counts). apply_patch does not
  fire it yet (its args are a patch, not a path) - documented.
- New engine tests (trace_u2_test.go): plan event contents, step
  attribution sequence (incl. the no-plan case), real diff on
  edit, no diff on a no-op edit, and unifiedDiff unit cases
  (unchanged/insert/delete/multi-change/created-file). All printed
  green.

TUI (history.go cells, classic.go renderers, app.go wiring):
- Step headers: tool rows group under `-- step N - label`, N from
  trace order, stable per frame; unattributed calls render with no
  header.
- Diff block: real +/- lines with a 40-line cap and a labeled
  remainder; tabs expand to four spaces explicitly (Lip Gloss's
  render rule, now owned by the renderer).
- Reasoning line: collapsed to one row (`thought - ctrl+o expand`),
  ctrl+o expands (6-line cap, labeled remainder); absent when no
  reasoning event fired.
- Tool-row width fix: under 80 columns the target no longer
  reserves the result's width when the result moved to its own
  line (classic goldens unaffected; workbench frames keep full
  targets).
- Plain-mode fix found by the new tests: the narrow contract
  strip's aggregate glyph now degrades to its word; two classic
  60x20 plain goldens changed (`○` -> `queued`), diff printed
  before acceptance.

Verification (all printed):
- Per-element absent-before/present-after: request line, result
  lines, step headers (incl. grouping/order), diff block (incl.
  no-patch-run absence and placement under its row), reasoning
  line (absent/collapsed/expanded), activity line (absent idle,
  present in flight, absent after the run ends).
- Full-run fixture (frame6): plan, three attributed steps, the
  real diff, the reasoning line, the failing test - fits 100x27,
  80x24, 60x20 with no overflow; same events render the same
  trace in the Stream view.
- Goldens: 6 new workbench_frame6 frames (3 sizes x normal/plain)
  printed then accepted; 22 workbench frames 1-5 and 2 classic
  plain frames regenerated for the two reviewed fixes (diffs
  printed).
- Gates: build, vet, full suite, race (tui+engine), golangci-lint
  (0 issues), PTY e2e all green.
- Perf (ttff, 3-run medians): first paint 6.61 ms (U1: 6.34,
  baseline 5.7), idle RSS 23.25 MB (U1: 22.84) - inside the rig's
  noise band, no regression over 10%; render flatness unchanged
  (170us @100 rows vs 200us @5000).
- ANSI dumps: docs/tui/review/workbench_frame6_* (36 files total
  in the review dir).

Open: E3 checkpoints, E4 subagent names, the Agent detail panel
(no producer), the apply_patch diff, the default-layout flip
(SECTION 10).

## 2026-10-12 — U3: checkpoints rail (E3) + the mascot proof set

Seam: `EventCheckpointCreated` (protocol) with `Text` = the
checkpoint's own turn number. Engine: emitted where the turn's
checkpoint is created (agent.go); no manager means no event
(engine test proves both directions).

TUI: state.Checkpoints filled only from the event; the right
rail's CHECKPOINTS section and the status-bar count are absent
when none exist; the full-run fixture now carries a real
checkpoint event, so frame6 shows the rail (6 goldens
regenerated, diff printed: CHECKPOINTS/`turn 1` section added,
COST shifted down).

Mascot proof set (rails_u3_test.go):
- All five run states render in the header orb AND the status bar
  with the state word; the eye shape itself distinguishes
  idle/working (half-lit), done (filled), error (hollow) - color
  is never the only signal.
- Constant width: the header orb's total row width and its own
  7-cell width are identical across all five states.
- ASCII eyes o (idle/working), * (done), x (error); NO_COLOR
  frames carry no escapes.
- No self-scheduled redraw: Init starts no mascot tick; the
  frame is byte-identical between events; a same-size resize
  changes nothing.
- Contrast gate per theme variant found and fixed a REAL bug:
  SelectTheme("monochrome") stripped the foreground, leaving the
  idle/working body at 1.16:1. Monochrome now keeps a visible
  neutral foreground (#f1f5f9); all four mascot body colors
  clear 3:1 in default/dark/light/monochrome. (Note for the
  record: "light" is still a no-op alias of the dark palette -
  the gate measures what SelectTheme actually returns.)

Rails re-proven in one place (U3 table test): Plan (absent then
present), Agents (absent then present), Context (em dash then
real, never a fake zero), Checkpoints (absent then present, plus
absence in a run that created none), Cost (em dash then real).

Verification: build, vet, full suite, race (tui+engine), lint
(0 issues), PTY e2e all green; perf ttff medians first paint
5.79 ms (baseline 5.7), idle RSS 23.1 MB (22.9) - no regression;
render flatness unchanged (188us @100 vs 204us @5000); 36 dumps
in docs/tui/review/.
