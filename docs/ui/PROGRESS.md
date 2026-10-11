# NIKI UI progress ledger (docs/ui/PROGRESS.md)

Chronological log for the Classic Flow work (docs/ui/CLASSIC.md).
Post-compact re-read file. Each entry states what was verified and
what remains.

## 2026-10-11 — Slice V: Charm stack v1 -> v2 (prerequisite)

- go.mod: bubbletea v1.3.10 -> charm.land/bubbletea/v2 v2.1.0,
  bubbles v1.0.0 -> charm.land/bubbles/v2 v2.2.1,
  lipgloss v1.1.0 -> charm.land/lipgloss/v2 v2.0.6. APIs verified
  against the downloaded module source (tea.View/AltScreen,
  tea.KeyPressMsg + Key.String() matching, tea.Quit/Suspend as Msg
  values, textinput v2 Styles/SetStyles, viewport v2 options,
  lipgloss v2 renderer removal -> colorprofile.Writer at the output
  boundary).
- internal/tui fully migrated (key handling via msg.String(),
  viewport via options, View() returns tea.View); cmd/nikicode
  main.go drops tea.WithAltScreen.
- Purity: View() reads no clock and mutates nothing; B9 debug
  overlay narrowed to event counters + boot phases; render-cost
  timing removed with the mutation that collected it.
- Gates printed: build/vet/race/full suite green; golangci-lint
  clean on changed packages (1 pre-existing QF1011 in an untouched
  file). Detail in docs/PROGRESS.md.

## 2026-10-11 — Slice C0: layout switch + pure View

- docs/ui/CLASSIC.md materialized from IMPf/niki-classic-flow-design.md
  (the approved mockup set); §7 lists known deferrals where no event
  source exists (sandbox state, thought line, git ahead count,
  verdict `f` key, `[d]` demo).
- config.UIConfig.Layout (`ui.layout = "classic" | "ledger"`),
  Default() = classic, unknown values fail closed to classic
  (config.NormalizeLayout). Wired through main.go -> app.SetLayout.
- View() dispatches: classic (default) / ledger (previous renderer,
  unchanged) / overlays take the ledger chrome in either layout.
- Tests: layout dispatch incl. fail-closed, View purity (deterministic,
  no mutation), both layouts deterministic at 4 sizes.

## 2026-10-11 — Slice C1: header, setup block, composer, footer

- internal/tui/classic.go: orb header (eye driven only by turn
  events: working/done/error/idle), identity rows (model,
  approvals, dir, branch — sandbox and ↑n deferred per CLASSIC.md §7),
  tip line, SETUP block (only when unconfigured; [k] opens the
  connect flow, [o] writes the local Ollama endpoint; both type
  while the composer has text), single-rule composer with
  state-dependent placeholder, registry-driven footer (facts left,
  hints right, ctx N% only with usage, hints-only under 80 columns).
- Key registry: 12 entries; added ? (keys from registry), ctrl+k
  (palette — repurposed from kill-line-forward by design), state
  filters for footer hints.
- Golden frames 1-2 at 100x27/80x24/60x20 + plain mode
  (internal/tui/testdata/classic_frame{1,2}_*.golden), generated
  with diffs printed (viewport transcript region, sweep glyph ◐,
  hints-only footer at 60).
- Perf guard caught a real flaw during the slice: classic first
  rendered all transcript cells per frame (51x growth); the
  transcript now scrolls through the viewport (2.7x, flat).
- Tests: TestLayoutDispatch, TestViewPure, TestSetupBlockOnlyWhenUnconfigured,
  TestSetupChoicesDispatch, TestFooterHintsDispatch (every hint
  dispatches in its state), TestClassicRows, TestClassicPlainMode,
  TestClassicGoldenFrames (12 goldens).
- Gates printed: go build ./..., go vet ./..., go test ./internal/tui/
  (ok), go test -race ./internal/tui/ (ok), go test ./... (all ok,
  tmux P2 test adapted to accept both layouts' identity),
  golangci-lint clean on changed packages (2 pre-existing issues in
  untouched files).
- Deferred to later slices: C2 tool rows/loop/excerpts, C3 contract
  strip/gates/verdict card + tamper test, C4 widths/plain/resize,
  C5 ANSI dumps + originality guard.

## 2026-10-11 — Slice C2: tool rows, activity line, loop rows, excerpts

- Minimal event interface (additive, zero behavior change):
  protocol.EngineEvent gains Target (the tool's real argument),
  Result (the real outcome summary), Excerpt (up to two lines of
  failing output). Filled by the engine at the three tool emission
  sites (new internal/engine/toolrows.go: toolTarget, toolRowResult,
  failureExcerpt — counts come from real output, nothing estimated).
- TUI: HistoryCell gains Kind ("tool"/"tool_excerpt"/"loop"),
  CallID, State, Verb, Target, Result, Excerpt. applyEvent opens a
  tool row at tool_started and closes it only at that call's
  completed/failed event (OpenToolCell by call id); failures append
  the excerpt row from the event; retry/repetition warnings become
  loop rows (and only those).
- classic rows: state glyphs ✓/✗/◐/○ with ASCII (+/x/*/o) and
  plain-mode words (done/failed/running/queued); verbs from the fixed
  list (read/search/edit/shell/plan/fetch/mcp:<server>); results
  right-aligned at 80+, second indented line at 50-79; targets
  middle-truncated to fit; failure excerpts indented under the verb.
- Activity line: sweep glyph, real action phrase, elapsed hint, plus
  the optional `next` line rendered only from a real plan event and
  only for pending steps.
- Five-frame fixture (internal/tui/classic_fixture.go): scripted
  engine events producing the mockup frames; the replay test drives
  them through Update and asserts every promised row renders.
- Goldens: classic_frame3_{100x27,80x24,60x20}_{normal,plain}
  generated with diffs printed (say text now truncates with an
  ellipsis per line; earlier clip-without-ellipsis and double-ellipsis
  bugs found and fixed in the diff review).
- Gates printed: go build ./..., go test ./internal/engine/ (ok),
  go test -race ./internal/tui/ (ok), go test ./... (all ok),
  golangci-lint clean on changed packages (3 pre-existing staticcheck
  QF1001 in untouched engine files; 1 pre-existing budget_test flake
  under -race — verified independent of this slice by re-running the
  wall_time_wrap scenario with the new emission stubbed out: same
  failure, wall=1.08 vs 1.07).
- Known deferral recorded in docs/ui/CLASSIC.md §7: the queued tool
  row (○ ... queued) has no engine event source yet, so frame 3
  renders only real states.

## 2026-10-11 — Slice C3: contract strip, gates, verdict card, tamper test

- Tamper test FIRST: a real tampered run (the check runner deletes the
  test file mid-run) goes through internal/verifier; the events it
  produces render "not verified" and never a bare "verified". Written
  before the card existed (it could not compile until the renderer
  landed). Boundary tests: T2 with every required evidenced renders
  "verified"; T2 with an unevidenced required requirement stays "not
  verified" and names the requirement; T1 never renders "verified".
- Minimal event interface (additive): EngineEvent gains Target/Result/
  Excerpt (C2) and Contract (turn-start requirement extraction);
  VerificationInfo gains Tier/Requirements/NotVerified/Contract;
  UserQuestion gains Safe (the constraint-keeping option index).
  verifier.Verdict gains Tier + []ReqEvidence, filled in
  Standard.Verify; the engine copies them into the verification event.
- Engine: the ask_user_question tool's prompt handler now emits
  EventQuestionPrompted and blocks for CmdAnswerQuestion (the event
  was defined but never emitted); turn-start contract extraction
  added. Tool-row fields filled at the three tool emission sites.
- TUI: verdictFromEvent applies the tier rule; the contract strip
  (counts-only under 80), the gate block (n = always the safe option,
  y/v in order; one state with the question modal), and the verdict
  card (verified headline only at T2+ with every required requirement
  evidenced; non-required notes still listed) render from those
  events. Gate and verdict keys registered with footer hints; the
  recorded-demo choice replays the five-frame fixture.
- Goldens: classic_frame4_* and classic_frame5_* at 100x27/80x24/60x20
  + plain mode (12 more), frame1 regenerated after printing the diff
  (the [d] choice appeared). Frame 5's card matches the mockup:
  "verified 5 of 5 required · trust T2 baseline-validated", five
  evidence rows, the honest not-verified line, branch + cost.
- Gates printed: go build ./..., go vet ./..., go test ./... (all ok),
  go test -race ./internal/tui/ (ok), golangci-lint clean on changed
  packages (pre-existing issues in untouched files listed in C2).
- Deferrals recorded in docs/ui/CLASSIC.md §7 (elapsed time, card file
  counts/duration, t trace, queued tool rows, gate v-as-answer).

## 2026-10-11 — Split C4: widths, plain mode, accessibility, resize

- Under 50 columns: one-line notice ("resize to 50 or wider. Your run
  continues."), truncated to the width; a pending gate stays visible
  and answerable (a decision is never hidden by a resize).
- Tip line and gate hint truncate to the width; gate options stack
  one per line under 80 columns.
- Plain mode (NIKI_PLAIN=1): every glyph has a word (state glyphs,
  orb bracket art, rule, prompt, loop, gate marks) and the frame is
  pure ASCII — engine-provided strings (results, excerpts, evidence,
  warnings, system notes) are transliterated (· → -, — → --,
  − → -, ✓ → "done ").
- ASCII mode (NIKI_ASCII=1, no plain): glyph fallbacks (+ x * o ! >),
  no unicode glyphs.
- NO_COLOR: the output boundary strips all sequences (unchanged v2
  mechanism, tested).
- Resize: focus (composer position), scroll offset, gate state, run
  state, and the gate's safe-option default all survive WindowSizeMsg.
- Tests: TestClassicWidths (10..200, no line exceeds the terminal,
  notice under 50), TestNarrowModePreservesState,
  TestResizePreservesState, TestClassicPlainModeGlyphWords,
  TestClassicAsciiMode, TestClassicNoColorMode,
  TestGateDefaultSurvivesResize.
- Goldens regenerated after printing diffs: plain-mode frames now
  carry the transliterated separators; frame1/2/3 at 60 truncate the
  tip line.
- Gates printed: go build ./..., go vet ./..., go test ./... (all ok),
  go test -race ./internal/tui/ (ok), golangci-lint clean on changed
  packages (pre-existing issues in untouched files only).

## 2026-10-11 — Slice C5: owner review artifacts, originality guard, STOP

- ANSI frame dumps: docs/ui/review/frame{1..5}_{100x27,80x24,60x20}.ansi
  (truecolor, unicode glyphs) plus .plain.txt variants (ASCII words,
  no color) — 30 files, written by NIKI_WRITE_DUMPS=1
  TestWriteClassicFrameDumps. README.md there lists the known
  mockup differences and the open owner decisions.
- Originality guard (design §8) over ALL 30 classic golden frames:
  forbidden glyphs (⏺ ⎿ ∴ ✱), exact strings ("esc to interrupt",
  "(ctrl+o to expand)", "Thought for", "Welcome to"), whimsical
  spinner verbs, border edges (welcome card / boxed composer), and a
  two-rule composer sandwich (adjacent rule lines) — all clean. The
  guard's own detection is proven by TestOriginalityGuardCatchesInjectedSignature.
  Two guard bugs were found and fixed while writing it (corner
  matching hit "+" in "ctrl+p"; substring rule counting double-counted
  one wide rule) — the checks are line- and edge-based now.
- Final gates printed: go build ./..., go vet ./..., gofmt clean,
  go test ./... (all ok), go test -race ./internal/tui/ (ok),
  golangci-lint clean on changed packages; pre-existing issues in
  untouched files only (QF1011, QF1001, S1000, S1008, and the
  -race-only budget_test wall_time flake verified independent of this
  work in C2).
- STOP at OWNER REVIEW per the mission. Nothing below this line
  claims the design is good; the dumps are for the owner to judge.

## 2026-10-11 — Owner decisions recorded; validation harness built

- Layout: accept Classic as default. Uniqueness stays where it is
  earned — the contract strip + verdict card, the orb, the
  registry-proven footer hints, plain-mode glyph words; everything
  Claude Code / OpenCode users already know (header identity rows,
  bottom composer, right-side hints, esc interrupt, ctrl+c ladder)
  stays conventional. No new chrome; frame 1 is 12 of 27 rows.
- Mockup-only elements stay mockup-only: production renders only real
  event data (the §7 deferral list is the contract; nothing mock
  enters production as feature or data).
- Ctrl+K: palette stays on ctrl+k (the tip line and registry hint
  advertise it; kill-line-forward is a readline habit, not an
  agent-CLI convention, and restoring it would make an advertised
  hint dead). Recorded as a deliberate keymap change.
- Contract strip / verdict card: DECIDE AFTER VALIDATION (owner's
  call). Harness built: State.HideContractUI renders the
  plain-transcript variant; docs/ui/review/validation/ holds the
  with/without frame pairs for frames 3 and 5 at three widths plus
  PROTOCOL.md (the 6-8 developer script and pass criteria).
- Gates re-run: build, vet, gofmt, full suite, race (tui), lint — all
  clean.
