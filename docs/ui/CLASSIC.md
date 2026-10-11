# NIKI Classic Flow — layout spec (docs/ui/CLASSIC.md)

Source: `IMPf/niki-classic-flow-design.md` (approved mockup set:
`niki-classic-flow-sheet.png` frames 1-5, `niki-classic-flow-gate.png`).
This file is the implementation spec; the mockups are renders from Niki's
tokens, not captures from a running program, and their window chrome is not
part of the product.

Classic is the **default** layout (`ui.layout = "classic"`). Ledger is the
previous full-screen layout, kept behind `ui.layout = "ledger"` and
experimental.

## 1. Element specification

### 1.1 Header (first launch and idle)

Three rows, no border, then one dim tip line:

```
 ▄███▄    niki  0.1.0
███◐███   claude-sonnet · approvals manual
 ▀███▀    ~/projects/api · main
```

- Orb eye by real state: idle ◐ (accent), working ◑, done ● (green),
  error ○ (red), interrupted ◐ (muted). ASCII eyes: o, *, x.
- Row 2: model · approvals <mode>. A sandbox segment renders only when an
  engine event reports sandbox state; none exists yet (see §7 deferrals).
- Row 3: cwd · branch. The ahead/behind count (↑n) needs a git-ahead event
  source; branch-only until then.
- Tip: `Type a request · / for commands · ctrl+k for everything · ? for help`.
- In inline mode the header scrolls away into native scrollback once the
  transcript grows; the footer carries ambient facts afterward.

**SETUP block** (only when no model is configured): lists what is missing
with choices `[k] add a key`, `[o] use Ollama`, `[d] recorded demo`. Every
choice dispatches a real action; a choice with no real backing is omitted,
never shown dead.

### 1.2 Transcript rows

- **User row:** `› text`, full-width raised background, never truncated; wraps.
- **Say text:** plain, indented two columns, at most two lines. States an
  observed fact or a decision with its reason. Never claims success.
- **Tool row:** `<glyph> <verb> <target>` left, `<result>` right-aligned.
  Verbs lowercase, from a fixed list: `read search edit shell plan fetch
  mcp:<server>`.
- **State glyphs** (word in plain mode, ASCII fallback): ✓ done (+),
  ✗ failed (x), ◐ running (*), ○ queued (o), ■ needs you (!).
- **Failure excerpt:** up to two lines below the row, indented under the verb.
- **Loop row:** `↺ loop 1 of 4  R2 failed: <reason> · re-planning`, amber.
- **Thought line:** `thought 3s · ctrl+o`. Provider-supplied reasoning
  summaries only; no event carries them yet (see §7).

### 1.3 The live area (bottom of the transcript)

1. **Contract strip** (only during a run, only from contract events):
   `contract  R1 ✓  R2 ✗  R3 ◐  R4 ○  R5 ■   1 of 5`. Updates in place.
2. **Activity line:** sweep glyph + a phrase built from the real current
   action + elapsed + `esc pause`; optional `next <step>` line only when the
   plan supplies it.
3. **Gate block** replaces the activity line when a decision is needed:

```
■ needs you  add golang.org/x/time?
  R3 would use it. R5 says no new dependency.
  [n] write the limiter without it (default)    [y] allow once    [v] view diff
```

The reason names the conflicting requirement. The default focused option is
always the one that keeps the constraint.

### 1.4 Composer and footer

```
────────────────────────────────────────────────────────────────
›  steer with enter · queue with ctrl+enter
manual · main · ~/projects/api          esc pause · ctrl+o expand · ctx 11%
```

- One rule above the composer, none below; footer sits directly beneath.
- Placeholder by state: idle "Describe a change, or ask a question";
  running "steer with enter · queue with ctrl+enter";
  gate "answer the question above, or steer".
- Footer hints are generated from the key registry, filtered by state; a
  test asserts every shown hint dispatches in that state.
- `ctx N%` appears only when the provider reported usage.

### 1.5 Verdict card (end of a run)

```
verified  5 of 5 required  · trust T2 baseline-validated
✓ R1  Expired tokens are rejected        token_test.go:44 · tests, base vs patch
…
not verified  load behavior: no check is configured for it
branch  niki/run-7f3a  · 3 files · +84 −4 · $0.07 · 6m12s
```

The word `verified` appears only at trust T2 or higher, with passing
evidence for every required requirement. A planted tampered run renders
`not verified`. A failed card that lists nothing under `not verified` is
logged as suspicious.

## 2. The five frames (golden fixtures)

| # | Frame | Shows |
|---|---|---|
| 1 | First launch | Orb header, tip line, empty transcript, idle composer |
| 2 | Submitted | Raised user row, activity line, running placeholder |
| 3 | Tools in flight | Tool rows with right-aligned results, contract strip, next line |
| 4 | Failure and gate | Failed tool with excerpt, loop row with reason, gate (R3 vs R5) |
| 5 | Verdict | Verdict card with one honest `not verified` line |

Golden frames at 100x27, 80x24, 60x20, plus plain mode. Goldens are updated
only after printing the diff.

## 3. Widths

- 100+: as in the frames, results right-aligned at the edge.
- 80-99: same layout, targets middle-truncated, right-aligned result kept.
- 50-79: results on a second indented line, contract strip counts-only
  (`3 of 5`), footer hints only.
- Under 50: one line — `resize to 50 or wider. Your run continues.` — with
  state preserved.

## 4. Keys

Footer-relevant registry keys: Enter (send / steer at safe point), Ctrl+Enter
(queue), Esc (pause; Esc Esc stops), Ctrl+C (clear input, interrupt, arm
exit), Ctrl+D (exit on empty input), Ctrl+O (expand/collapse row), Ctrl+K
(actions palette), ? (keys from the registry). Gate: n / y / v (default,
allow once, view diff). Verdict: Enter accept, d diff, t trace (event trace
export), b branch (ensure run branch).

## 5. Wording and glyph rules

Sentence case, no exclamation marks, no whimsical status words. Activity
phrases are built from the real current action. Numbers come from counters.
Every glyph has a word in plain mode and an ASCII fallback; color is never
the only signal. Palette: teal = verified/focus, blue = running,
amber = needs you/loop, rose = failed, slate = queued, muted gray = metadata.

## 6. Originality guard (lint test)

Fails on exact signatures from other products' interfaces: glyphs
`⏺ ⎿ ∴ ✱`; strings "esc to interrupt", "(ctrl+o to expand)", "Thought for",
"Welcome to"; whimsical spinner verbs; a bordered welcome card; a two-rule
composer sandwich. Text-only check over all golden frames; resemblance
judgment is human and belongs to the owner.

## 7. Known deferrals (no event source — state is never invented)

- Sandbox segment in the header: no engine event reports sandbox state.
- Thought line: no reasoning-summary event; renders when one exists.
- `↑n` ahead count: no git-ahead data in the current git status path.
- Verdict `f edit a requirement`: no requirement store to edit; hint omitted.
- Activity-line elapsed (`· 14s`): no event reports elapsed time.
- Verdict-card file counts and duration: no event carries them; the
  card shows the branch and cost, which are real.
- Verdict `t trace`: needs the session's event list, which the TUI does
  not hold; ships when a trace command exists.
- Queued tool row (`○ ... queued`): no engine event reports a queued
  tool call; frame 3 renders only real states.
- The gate's `v` answers the question's third option (the generic
  ask_user_question mechanism); a dedicated diff preview needs its own
  event.

## 8. Implementation notes (2026-10-11)

- Contract states are extracted from the prompt at turn start
  (EventTurnStarted.Contract) and re-stated by the verification event;
  mid-run states stay queued until the verifier grades them.
- Gates render inline in classic from the ask_user_question event,
  which the engine now actually emits (the event was defined but had no
  emitter; the prompt handler blocks the turn for the answer).
- The verdict card's evidence comes from verifier.Verdict's structured
  fields; the engine copies them into the verification event.
- The `[d]` recorded demo replays the five-frame fixture through the
  same event path a live run uses (no model call).
