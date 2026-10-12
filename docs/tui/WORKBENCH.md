# NikiCode Workbench — layout spec (docs/tui/WORKBENCH.md)

Status: **U1+U2 built and tested (2026-10-12); U0 approval items in
§10 remain open** — the shell is selectable via
`ui.layout = "workbench"` while the default stays `classic` until
the owner's call. Build status: §11.

Sources read for this spec: `AGENTS.md`, `docs/PACK.md`,
`docs/CAPABILITY_PACK.md`, `docs/DESIGN_V2.md`, `docs/ui/CLASSIC.md`
(the current Classic Flow spec), plus the live seam
(`internal/protocol/event.go`) and the current TUI
(`internal/tui/app.go`, `state.go`, `keys.go`, `commands.go`).

Pack-name note: the mission named `docs/FINAL_PACK_V2.md` and
`docs/TUI_PACK.md`; neither exists in the repo. The equivalent
content is `docs/DESIGN_V2.md` (G-design) + `docs/CAPABILITY_PACK.md`
(final capability pack) + the visual_spec in `docs/PACK.md` +
`docs/ui/CLASSIC.md` (the current TUI spec). This file supersedes the
layout half of CLASSIC.md when approved; CLASSIC.md stays as the
spec of the Stream view (unchanged).

## 1. What this is

Replace the chat-style UI with the **Workbench**: a full-screen,
three-rail, task-first interface. The primary object is a Run grouped
by plan Step, not a message. Every visible element is driven by a
real Op/Event from the existing seam; an element with no event is
absent (P20/P21), never faked, never zeroed. Nib (the existing orb)
becomes the header/status mascot, driven only by the run state
machine.

Rule of proof: each element ships with a test asserting it is
**absent before its event fires and present after**.

## 2. The layout (100 columns)

```
┌ header ────────────────────────────────────────────────────────────────┐
│ [Nib] NikiCode  v0.11.0 · <model> · <mode>          <cwd> · <branch>  │
├─ rail ─┬─ run trace ────────────────────────────────────┬─ rail ───────┤
│ PLAN   │ > the request                                   │ CONTEXT     │
│ ✓ 1 …  │   ── step 1 · <label> ───────────────────────  │  <used>/…   │
│ ▶ 2 …  │   ● Read(<file>)   └ <result>                  │ CHECKPOINTS │
│ ○ 3 …  │   ● Grep("<q>")    └ <result>                  │  ● <label>  │
│        │   ● Edit(<file>)   └ +a −d                     │ COST        │
│ AGENTS │     ┌ diff ──────────────────────────┐          │  turn <$>   │
│ ● <id> │     │ + …                             │          │  session <$>│
│        │     └─────────────────────────────────┘          │             │
│        │   * <live action>  (esc to interrupt)            │             │
├────────┴──────────────────────────────────────────────────┴─────────────┤
│ > <input>                              mode <mode> · ctrl+k             │
│ [Nib] <state> · <model>        <agents> · <checkpoints> · esc interrupt │
└──────────────────────────────────────────────────────────────────────────┘
```

Regions: header (mascot, product/version, model, mode, cwd+branch);
left rail (Plan checklist, Agents list); centre (Run trace: request,
steps with tool rows, diffs, prose, live activity line); right rail
(Context meter, Checkpoints, Cost); composer (input, mode chip,
palette hint); status bar (mascot + run state, live agent/checkpoint
counts, interrupt hint). Panels open over the centre: palette
(ctrl+k), approvals, diff viewer, agent detail (deferred, §7),
transcript pager.

Density: wide gaps between regions, tight rows inside them. No
decorative borders, no empty panels, no panel without an event.

## 3. The wiring table (the spine)

Every element → the real seam in `internal/protocol`. Status:
**HAVE** (field/event exists today), **EXTEND** (needs one of the
minimal seam additions in §6, proposed, not yet built), **DEFER**
(no emitter — element stays absent, §7).

### Header
| Element | Driver | States | Absent rule | Status |
|---|---|---|---|---|
| product/version | build info via `SetVersion` (`state.Version`) | always | never | HAVE |
| model | config at boot (`SetModel`, main.go) + Op echo after `/model`, key connect, Ollama (`modelPickedMsg`, `keyConnectedMsg`) | shows active model | shows "—" until known | HAVE (echo, not event; see E5) |
| mode | `SetMode`/`SetPermissionMode` + Op echo (`CmdSetPlanMode`/`CmdSetPermissionMode`) | plan · act · yolo | always | HAVE (echo; E5) |
| cwd + branch | real environment at boot (`SetDirectory`, `SetGitBranch`) | always | never | HAVE |
| mascot (Nib) | run state machine: `Busy`, `TurnOutcome`, `Gate.Active`, `Verdict.Active` | idle/working/done/error/interrupted | never (idle state) | HAVE |

### Left rail
| Element | Driver | States | Absent rule | Status |
|---|---|---|---|---|
| Plan list | `plan_updated` (`Plan []PlanStep`) | pending · in_progress · completed · failed | ABSENT when no plan event has arrived | HAVE |
| Agents list | `subagent_started` / `subagent_completed` / `subagent_failed` (`Text` = agent id) | running · done · failed | ABSENT when no subagent event | HAVE (id only; E4 for names) |

### Centre — Run trace
| Element | Driver | States | Absent rule | Status |
|---|---|---|---|---|
| request line | the user's actual submit (Op `CmdSubmitPrompt` + local append) | always once sent | ABSENT before first submit | HAVE |
| Step headers | `plan_updated` + tool→step attribution (`EngineEvent.Step`, E1) | one per step that owns a tool | ABSENT until a tool is attributed to a step | HAVE (U2; E1 built) |
| Tool rows | `tool_started` / `tool_completed` / `tool_failed` with `Target`, `Result`, `Excerpt`, `Duration` | running · done · failed | ABSENT when no tool ran | HAVE |
| Diff block | `EngineEvent.Diff` on the patch tool's completion (E2) | shown under the Edit row | ABSENT when no patch applied | HAVE (U2; E2 built) |
| Reasoning line | `reasoning_delta` (`Text`); no provider emits one yet, so live runs show no line until a provider gains a reasoning channel | collapsed · expanded (ctrl+o) | ABSENT when the model produced none — which, with no producer, is always | HAVE (U2; producer pending) |
| Activity line | the in-flight op from real events (`state.Activity` is set only by events: turn_started, tool_started, subagent_*, mcp_*, steer, budget_*) | live action only | ABSENT when idle | HAVE |
| Prose | `assistant_text_delta` (streamed) → `assistant_message_done` (settled) | streaming · done | ABSENT before the first delta | HAVE |
| Esc-interrupt hint | `Busy` + registry key | only while running | ABSENT when idle | HAVE |

### Composer
| Element | Driver | States | Absent rule | Status |
|---|---|---|---|---|
| input | local editor state (the real textarea) | always | never | HAVE |
| queue | no queue state exists in the TUI and no `QueueChanged` event exists; the classic "queue with ctrl+enter" hint was aspirational | queued · sent | ABSENT (the row does not exist) | DEFER |
| mode chip | `state.Mode`/`PermissionMode` + the registry's palette hint | always | never | HAVE |

### Popups (over the centre)
| Element | Driver | States | Absent rule | Status |
|---|---|---|---|---|
| Approvals (gate) | `question_prompted` (`Questions`, `Safe` index) | pending · answered | ABSENT when nothing pending | HAVE (C3; safest option focused) |
| Command palette | one registry: `DefaultPaletteCatalog` + `CoreSlashCommands` + `KeyBindings` | open · closed | ABSENT when closed | HAVE |
| Diff viewer | `EngineEvent.Diff` accumulation (E2); `/diff` keeps its labeled working-tree read | per patch | ABSENT when no patch exists | EXTEND (E2) |
| Agent detail | subagent child stream — intentionally not forwarded to the parent (`engine/subagent.go`) | — | ABSENT (cannot fabricate a stream) | DEFER |
| Transcript pager | committed history cells (real rows) | open · closed | ABSENT when closed | HAVE |

### Right rail
| Element | Driver | States | Absent rule | Status |
|---|---|---|---|---|
| Context meter | `Usage` on `assistant_message_done` (+ window from `SetModel`) | real used/window | shows "—" until the first usage event; never a fake 0 | HAVE (P22 change from the current 0% footer) |
| Checkpoints | none (`checkpoint` package exists; no event) | — | ABSENT until an event exists | EXTEND (E3) |
| Cost | `Usage` → `routing.CalculateCost` (turn and session accumulators) | real $ | "—" until the provider reports usage | HAVE |

### Status bar
| Element | Driver | States | Absent rule | Status |
|---|---|---|---|---|
| mascot + run state | run state machine | idle/working/done/error/interrupted | never | HAVE |
| agent count / checkpoint count | live agent list / checkpoint list | counts | ABSENT when zero | HAVE (agents) / EXTEND (checkpoints, E3) |
| interrupt hint | registry key + `Busy` | only while running | ABSENT when idle | HAVE |

### Notifications / errors
| Element | Driver | States | Absent rule | Status |
|---|---|---|---|---|
| notice row | `warning`, `mcp_server_ready`/`failed`, `config_reloaded`, `context_compacted`, `skill_discovered`, `steer` | transient | ABSENT when none fired | HAVE (renders as a trace row, U1 — one event, one row; no duplicate widget) |
| error banner | `error`, `turn_failed`, `turn_not_verified` | persistent | ABSENT when no error | HAVE (renders as a trace row, U1 — same reason) |
| contract strip + verdict card | `turn_started.Contract` → `verification` / `turn_not_verified` (C3: "verified" only at T2+ with all required evidence; tamper test exists) | during run · at end | contract ABSENT before turn_started; verdict ABSENT until the verification event | HAVE (carried from Classic unchanged) |

## 4. Nib (the mascot)

Nib is the existing orb, promoted to header + status bar. No new art:
the current `renderMascot` body and the CLASSIC.md eye table carry
over, with the mission's eye semantics:

- idle: accent body, half-lit eye. working: accent body, flipped
  half-lit eye (static — the sweep belongs to the activity line, so
  there is never a double spinner). done: success body, filled eye.
  error: error body, hollow eye. interrupted: muted body, half-lit
  eye.
- ASCII fallback eyes: `o` (idle/working), `*` (done), `x` (error).
- Width tiers: full 3-row art wide, compact one-row form mid, none
  below the minimum. Constant width in every state (no jitter);
  art lives in one module (`wordmark.go` + `theme.go`).
- It never schedules a redraw of its own; it changes only when a
  real event changes the run state (pure `View()`).
- Contrast: body-vs-background passes 3:1 in every theme variant —
  the existing `contrast_test.go` gate covers any palette it gets.

## 5. One core, three views

The same Op/Event stream, three renderers:

| View | What it is | Config |
|---|---|---|
| **Workbench** | the full-screen three-rail layout in §2 (new, default) | `ui.layout = "workbench"` |
| **Stream** | the current Classic Flow: inline, settled rows committed to scrollback with `tea.Println`, live area at the bottom | `ui.layout = "stream"` (renames `"classic"`; the old value stays accepted as an alias) |
| **Headless** | `nikicode exec` / `--json`: no TUI, the same events as JSONL | unchanged |

`ledger` remains behind `ui.layout = "ledger"` (experimental fossil).
The core (`internal/engine`, `internal/protocol`) imports none of the
views — unchanged. Full-screen means the Workbench runs alt-screen;
scrollback behavior lives in Stream (inline). This is the one taste
call in §10.

## 6. Proposed minimal seam extensions (approve or reject each)

Extend, never fork. Each is additive and optional-field only; every
reader tolerates their absence (schema version stays 1).

- **E1 — Step attribution.** `EngineEvent.Step string`: the
  in_progress plan step's label, set by the engine on
  `tool_started`/`tool_completed`/`tool_failed`. The TUI groups tool
  rows under a step header only when the event names the step. No
  field, no header. **BUILT (U2)**: the engine reads the plan from
  the `update_plan` tool's own steps (its single source of
  truth) and emits `plan_updated` after a successful
  `update_plan` call.
- **E2 — Patch diff.** `EngineEvent.Diff string`: the unified diff
  attached by `apply_patch`/`edit` at `tool_completed`. Feeds the
  centre diff block, the diff viewer, and the verdict `d` key.
  `/diff` keeps its working-tree read, labeled as the working tree.
  **BUILT (U2)** for the tools whose paths the engine tracks
  (`edit_file`, `write_file`, `notebook_edit`): the diff is
  computed from the file's real before/after bytes by the
  engine's own unified-diff writer (common prefix/suffix
  trimmed, one honest hunk with correct line counts).
  `apply_patch` does not yet fire the event — its args are a
  patch, not a path, so the engine cannot attribute its
  changes without parsing the patch grammar; it shows no
  diff block until then.
- **E3 — Checkpoints.** `EventCheckpointCreated` with `Text` = the
  checkpoint label, emitted where `internal/checkpoint` snapshots
  are taken (engine undo/rewind path already has the call sites).
  Feeds the right rail and the status-bar count.
- **E4 — Subagent identity (optional).** `EngineEvent.AgentName
  string` on the subagent events; without it the Agents rail renders
  the real id only. E4 also lists what the child stream would need
  for the Agent detail panel — out of scope for U1–U4, DEFER.
- **E5 — Model/mode events (optional, later).** `ModelChanged` /
  `ModeChanged` would replace today's Op-echo header updates. Not
  needed to build; recorded so the echo is not mistaken for an
  event.
- **E6 — Reasoning deltas (new, U2).** `EventReasoningDelta` with
  `Text` carries provider reasoning summaries. The event exists and
  the TUI renders it (collapsed by default, ctrl+o expands), but NO
  provider emits one — the provider layer has no reasoning channel
  at all. The line therefore stays absent in every live run until
  a provider gains one; the renderer and its tests are real, and
  the absence is the honest state, not a stub.

## 7. Deferrals (no event source — the element is absent, not faked)

- Reasoning line: the event exists (E6) and the renderer is built,
  but no provider emits a reasoning delta, so the line is absent in
  live runs until a provider gains a reasoning channel.
- `apply_patch` diff block: the engine cannot attribute a patch
  tool's changed files without parsing the patch grammar; the diff
  block shows for `edit_file`/`write_file`/`notebook_edit` only.
- Agent detail panel: the child's stream is intentionally not
  forwarded; forwarding it is a core design change, not a UI one.
- Checkpoints rail: until E3 is approved.
- Queue: no queue state exists in the TUI at all (the classic
  "queue with ctrl+enter" hint was aspirational); the element is
  absent until a queue exists. No engine event is proposed.
- Context elapsed time: `View()` reads no clock (pure render); the
  activity line shows no duration unless an event carries one.
  (`EngineEvent.Duration` already exists — a tool row may show the
  engine-set duration when present.)

## 8. Widths, modes, accessibility

- 100+: as §2. 80–99: rails narrow, results kept right-aligned.
  60–79: right rail collapses into the status bar; results on a
  second indented line. Under 50: one line, state preserved, resize
  hint. Never panic at any size (existing width tests).
- Plain mode, `NIKI_ASCII=1`, `NO_COLOR`, 16-color, reduced motion:
  every glyph has a word and an ASCII fallback (CLASSIC.md §5
  carries over; the ASCII/NO_COLOR matrix test covers the
  Workbench goldens).
- Footer hints come from `KeyBindings` only; the existing test
  asserts every shown hint dispatches in that state. Palette,
  footer and help stay one registry.
- Originality: the C6 lint guard (banned glyphs/strings from other
  products' interfaces) extends over all Workbench golden frames.
- Motion: streaming cursor, state change, panel open only; nothing
  while idle (B5 zero-redraw gate unaffected).

## 9. Build order and gates

- **U1 shell + regions**: header, three rails, composer, status bar;
  every region rendered from the event bus with the absent rules
  from §3. Tests: per-element absent-before/present-after; PTY
  smoke (resize, exit, terminal restore); perf rig rerun.
- **U2 run trace**: request, step grouping (E1), tool rows, prose,
  activity line; dogfood starts here.
- **U3 rails**: plan checklist, agents list, context meter ("—"),
  cost, checkpoints (E3 if approved), notice/error rows.
- **U4 mascot**: Nib states, width tiers, ASCII, contrast test.
- **U5 modes/approvals/palette**: one registry; gate defaults to
  the constraint-keeping option (test carried from C3).
- **U6 import**: `nikicode import --from claude-code|codex`,
  preview + confirm, map-never-copy, reversible, fixture tests.
  Implementation in `internal/import`, subcommand in
  `cmd/nikicode`. Reads the project's `CLAUDE.md` /
  `.claude/` settings+skills+MCP config, or the AGENTS.md chain +
  `CODEX_HOME` config; writes NikiCode's shape (AGENTS.md chain,
  skills, MCP config) with backups; offers on first run when a
  Codex/Claude project is detected; records every import for undo;
  session-history import optional and off by default.
- **U7 wiring audit**: a test walks the §3 table — every rendered
  element id is registered there, every registered id renders (or
  is listed in §7 as DEFER with a linked reason). No second list
  anywhere.

After each phase: PTY run, ANSI frame dumps into
`docs/tui/review/`, perf rig; a regression over 10% blocks the
slice (PACK rule). Goldens at 100x27, 80x24, 60x20 + plain mode,
updated only after printing the diff.

## 10. Approval requested

1. Approve the layout and the wiring table (§2–§3) as written.
2. Approve/reject each seam extension: E1 step attribution,
   E2 patch diff, E3 checkpoints, E4 subagent identity (optional),
   E5 model/mode events (optional, later).
3. Taste call: Workbench is full-screen alt-screen by construction
   (that is what a three-rail fixed layout means); Stream stays the
   inline/scrollback view. Confirm that split, or restrict the
   Workbench to inline with a collapsing rail.
4. Confirm `ui.layout` values: `workbench` (default) · `stream`
   (classic alias) · `ledger` (experimental).

On approval: U1 starts. `docs/tui/PROGRESS.md` is the ledger.

## 11. Build status (2026-10-12)

U1 (shell + regions) is **built and tested**; `ui.layout` accepts
`"workbench"` while the default stays `classic` until the owner's
§10 call — the default flip is one constant, deliberately not
taken unilaterally.

Shipped in U1 (proven by tests, goldens, dumps — evidence in
docs/tui/PROGRESS.md):
- The six regions: header (full orb ≥80 columns, compact one-row
  form below), left rail (PLAN, AGENTS), centre (viewport + live
  block), right rail (CONTEXT, COST), composer, status bar.
- Left rail AGENTS is now event-wired: `subagent_started/
  completed/failed` maintain `state.Agents` (id + running/done/
  failed). This is the real source the previously dead
  `SwarmAgents` field never got; `SwarmAgents` itself stays
  unused and is listed for removal, not silently deleted.
- Instruments follow P22: context, turn cost and session cost
  render the em dash (ASCII: `--`) until the first usage event;
  the activity line is ABSENT when idle (P21) — the status bar's
  state word carries idle.
- Widths: 100/80 full rails; 60–79 compact header, no right rail
  (instruments move to the status bar); <50 (or height <6) a
  one-line notice with state preserved. No overflow or clipping
  at any tested size (asserted per line, per width).
- ANSI-aware truncation added (`TruncateStyledTo`, width.go): the
  rune-based path over-measured styled strings and clipped the
  rails mid-escape; rails and columns now clip correctly.
- The originality guard covers the workbench goldens too (classic
  + workbench frames; the ledger fossil's boxed chrome is out of
  scope by design, noted in the test).
- Notices and error rows render as trace rows (one event, one
  row) rather than as separate widgets sharing the same driver.
- The composer's palette hint and the status-bar interrupt hint
  are width-prioritized: nothing real is dropped for decoration
  (the registry hints yield below 80, where the composer row
  already shows the palette hint and "?" lists every key).

U2 (run trace) is **built and tested** on top of U1:
- Step headers: tool rows group under the plan step the engine
  attributed them to (`EngineEvent.Step`, E1 built). The engine
  emits `plan_updated` from the `update_plan` tool's own steps -
  that event had no emitter before; the plan rail and the
  headers now share one real source.
- Diff block: the patching tool's completion carries the real
  unified diff (`EngineEvent.Diff`, E2 built), computed by the
  engine from the file's own before/after bytes (prefix/suffix
  trimmed, correct hunk line counts, tested for insert,
  delete, multi-change and created-file). Tabs expand to four
  spaces explicitly in the block renderer.
- Reasoning line: the `reasoning_delta` event and the collapsed
  renderer (ctrl+o expands) are built (E6); no provider emits
  one yet, so live runs show no line - proven absent before and
  present after through the same event path.
- Plain-mode fix found by the U2 tests: the narrow contract
  strip's aggregate glyph now degrades to its word (two classic
  plain goldens updated, diff printed: `○` -> `queued`).
- Tool-row width fix: under 80 columns the target no longer
  reserves the result's width when the result moved to its own
  line (frame6 and the workbench frames keep full targets;
  classic goldens unaffected).

Not yet built (later slices or deferrals): the diff VIEWER fed
from patch events (E2's diff block is built; the viewer still
reads the working tree via /diff), checkpoints (E3), subagent
names (E4), the Agent detail panel (§7), and the default-layout
flip (§10).
