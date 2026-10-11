# NIKI — Classic Flow Terminal UI (design)

**Replaces:** the Ledger layout as the **default**. Ledger's best parts stay inside this design (the contract line, the verdict card, gates). The rest of Ledger becomes an optional view.
**Visuals:** `niki-classic-flow.gif` (five frames, like demo.gif), `niki-classic-flow-sheet.png` (the same frames stacked), `niki-classic-flow-gate.png` (the key frame at full size).

**Status:** a mockup and a proposal. The frames are rendered from Niki's tokens with a monospace font, not captured from a running program. The window chrome (title bar, three dots) belongs to the mockup, not the product.

---

## 1. Why this is the better default

You asked for the same architectural flow and visual flow as Kimi Code and Claude Code, without an exact copy. That is the right call, and it corrects my last design, which went much further than you asked.

- **The flow is a convention, and a good one.** The two GIFs you uploaded share it: a header block with a mascot and identity lines, a transcript where each tool call is one compact row with a state and a one-line result, one live status line while working, a composer anchored at the bottom, and a footer with ambient facts on the left and hints on the right. Users already know where to look.
- **Ledger was a hypothesis.** It has never been tested with a user. A familiar layout carries no such risk.
- **Familiar structure and original identity are compatible.** Structure is not what makes a product a copy. The mascot, wording, glyphs, colors, and the exact row format are. This design keeps the structure and changes all of those.

## 2. What is shared, and what is ours

| Element | The references do | Niki does |
|---|---|---|
| Header | A block with a mascot, name and version, model, and cwd. Claude Code is borderless; Kimi is boxed | Same structure. Three rows: **the one-eyed orb**, name and version, model and approval mode and sandbox, cwd and branch. Borderless |
| User message | Echoed as a highlighted row | A raised row with a `›` prefix, in Niki's surface tone |
| Assistant text | A bullet before the text | **No bullet.** Indented two columns, and capped at two lines per statement |
| Tool calls | Two lines each: a dot, a bold name with arguments in parentheses, then a result line below with an expand hint repeated on every row | **One line each.** A state glyph in the gutter, a lowercase verb, the target, and the **result right-aligned** on the same line. No repeated expand hint; the footer shows `ctrl+o expand` |
| Failures | Inline, in the error color | Inline, plus a two-line excerpt beneath the row, in a muted red |
| Reasoning | A collapsed line with a duration | `thought 1s · ctrl+o`, dim, collapsed by default |
| Activity line | A spinner, a whimsical verb, "(esc to interrupt)" | A sweep glyph, a **plain phrase built from the real current action**, elapsed time, `esc pause`. An optional `next` line only when the plan supplies it |
| Composer | Two rules around it, or a box | **One rule above** the composer; the footer sits directly beneath. No bottom rule |
| Footer | Project and hints; Kimi adds a context meter | Left: mode, branch, cwd. Right: hints that change with state, plus `ctx N%`. Only values the engine reported |
| Run state | Not shown beyond the spinner | **A one-line contract strip** (R1 to R5 with states) while a run is active; **a verdict card** at the end |
| Gate | A permission prompt | An amber `needs you` block with the reason and the constraint it conflicts with, the **safe option as the default** |

The last two rows are what Niki adds: the contract strip and the verdict card turn "the model says it's done" into "these requirements have this evidence." They live inside the familiar flow and don't change it.

## 3. Element specification

### 3.1 Header (first launch and idle)
Three rows, no border:
```
 ▄███▄    niki  0.1.0
███◐███   claude-sonnet · approvals manual · sandbox on
 ▀███▀    ~/projects/api · main ↑1
```
Below it, one dim tip line: `Type a request · / for commands · ctrl+k for everything · ? for help`. The header scrolls away as the transcript grows (the Claude and Kimi behavior). The footer carries the ambient facts afterward.

If setup is incomplete (no model, no sandbox), a `SETUP` block follows the tip, listing only what is missing, with the choices `[k]` add a key, `[o]` use Ollama, `[d]` recorded demo. Niki never picks a provider on its own.

### 3.2 Transcript rows
- **User row:** `› text`, full-width raised background, never truncated; wraps.
- **Say text:** plain, indented two columns, at most two lines. States an observed fact or a decision with its reason. Never claims success.
- **Tool row:** `<glyph> <verb> <target>` on the left, `<result>` right-aligned. Verbs are lowercase and from a fixed list: `read`, `search`, `edit`, `shell`, `plan`, `fetch`, `mcp:<server>`.
- **State glyphs** (each with an ASCII fallback and a word in plain mode): `✓` done (+), `✗` failed (x), `◐` running (*), `○` queued (o), `■` needs you (!).
- **Failure excerpt:** up to two lines below the row, indented under the verb.
- **Loop row:** `↺ loop 1 of 4  R2 failed: <reason> · re-planning`, amber.
- **Thought line:** `thought 3s · ctrl+o`. Provider-supplied reasoning summaries only; raw reasoning is never shown.

### 3.3 The live area (bottom of the transcript)
1. The **contract strip** (only during a run): `contract  R1 ✓  R2 ✗  R3 ◐  R4 ○  R5 ■   1 of 5`. It updates in place.
2. The **activity line:** `◐ writing the limiter · 14s · esc pause`, then an optional `next  …` line.
3. A **gate block** replaces the activity line when a decision is needed:
```
■ needs you  add golang.org/x/time?
  R3 would use it. R5 says no new dependency.
  [n] write the limiter without it (default)    [y] allow once    [v] view diff
```
The reason must name the conflicting requirement. The default focused option is always the one that keeps the constraint.

### 3.4 Composer and footer
```
────────────────────────────────────────────────────────────────────────────────
›  steer with enter · queue with ctrl+enter
manual · main ↑1 · ~/projects/api                esc pause · ctrl+o expand · ctx 11%
```
- Placeholder text changes with state: empty, "Describe a change, or ask a question"; running, "steer with enter · queue with ctrl+enter"; gate, "answer the question above, or steer".
- Footer hints come from the key registry; every hint shown must work in that state (a test enforces it).
- `ctx N%` appears only when the provider reported usage.

### 3.5 Verdict card (end of a run)
```
verified  5 of 5 required  · trust T2 baseline-validated
────────────────────────────────────────────────────────────────────────────────
✓ R1  Expired tokens are rejected              token_test.go:44 · tests, base vs patch
✓ R2  Refresh keeps the session id             token_test.go:81 · tests, base vs patch
…
────────────────────────────────────────────────────────────────────────────────
not verified  load behavior: no check is configured for it
branch  niki/run-7f3a  · 3 files · +84 −4 · $0.07 · 6m12s
```
Rules: the word `verified` appears only at trust T2 or higher, with passing evidence for every required requirement. A card that lists nothing under `not verified` is treated as suspicious and is logged. When something fails, the headline reads `not verified` and the card says which requirement and why.

## 4. The five frames

| # | Frame | Shows |
|---|---|---|
| 1 | First launch | Header with the orb, the tip line, the request being typed; composer anchored at the bottom |
| 2 | Submitted | The raised user row, the collapsed thought line, the activity line; header still visible |
| 3 | Tools in flight | Say text, tool rows with results aligned right, the contract strip, the activity line with a `next` |
| 4 | Failure and gate | A failed test with its excerpt, a loop row with its reason, a gate that names R3 versus R5 |
| 5 | Verdict | Final checks, a short say, the verdict card with one honest `not verified` line |

## 5. Widths

- **100 columns and up:** as in the frames. Results align to the right edge.
- **80 to 99:** same layout; targets truncate in the middle (`internal/…/limit.go`), and the right-aligned result stays.
- **50 to 79:** results move to a second, indented line; the contract strip shows counts only (`3 of 5`); the footer shows hints only.
- **Under 50:** one line, "resize to 50 or wider. Your run continues." State is preserved.

## 6. Keys

| Key | Action |
|---|---|
| Enter | Send. During a run, a steer at the next safe point |
| Ctrl+Enter | Queue a message for after the run |
| Esc | Pause at the next safe point |
| Esc Esc | Stop; keep every edit; verdict reads "not verified: stopped" |
| Ctrl+C | Clear input, then interrupt, then arm exit |
| Ctrl+D | Exit on an empty input only |
| Ctrl+O | Expand or collapse the focused row's output |
| Ctrl+K | Actions palette |
| n / y / v | At a gate: default, allow once, view diff |
| d / t / b / f | At a verdict: diff, trace, branch, edit a requirement |
| Up / Down | History at buffer boundaries; row focus when the composer is empty |
| ? | Keys, generated from the registry |

## 7. Wording and glyph rules

- Sentence case. No exclamation marks. No whimsical status words. Activity phrases are built from the real current action: `reading the project`, `writing the limiter`, `running go test ./internal/auth`.
- Numbers come from counters, never estimates.
- Every glyph has a word in plain mode and an ASCII fallback. Color is never the only signal.
- Palette: teal means verified and focus, blue means running, amber means needs you or loop, rose means failed, slate means queued. Muted gray is metadata.

## 8. Originality guard (what the lint test checks)

It fails on exact signatures from other products' interfaces: the glyphs `⏺`, `⎿`, `∴`, `✱`; the strings "esc to interrupt", "(ctrl+o to expand)", "Thought for", and "Welcome to"; whimsical spinner verbs; a bordered welcome card; a two-rule composer sandwich. It checks text only, so it catches accidental copying. It cannot judge overall resemblance. Judging resemblance is a human review, and it belongs to the owner.

## 9. How Ledger fits

- **Kept inside Classic:** the contract strip, the loop reasons, the gates with the conflicting requirement, and the verdict card.
- **Optional:** the full two-column Ledger layout becomes `ui.layout = "ledger"`, switched with `l` on terminals 120 columns or wider. It stays experimental until users have tested it.
- **Validation (replaces the earlier plan):** show 6 to 8 developers the same recorded runs in two versions: Classic with the contract strip and verdict card, and Classic without them (a plain transcript). Ask: "Is this done? What is not verified? Why did it loop?" Ship the additions only if they raise correct verdict readings and no tampered run is accepted. Be clear that this is a small qualitative test.

## 10. Build brief for Kimi Code

Builds on the Go foundation (Bubble Tea v2, inline mode). Replaces the earlier Ledger-first brief.

````
<role>
You are the lead TUI engineer for NIKI (Go, Bubble Tea v2, Bubbles v2, Lip Gloss v2). Repo root = current directory. Read AGENTS.md and docs/PACK.md first. Verify every Bubble Tea and Lip Gloss API against its installed source. Never rely on memory.
</role>

<mission>
Build the Classic flow in docs/ui/CLASSIC.md as the default layout: the orb header, raised user rows, one-line tool rows with right-aligned results, an activity line, the contract strip, gates, the verdict card, a single-rule composer, and the footer. Keep layouts behind ui.layout = "classic" | "ledger". Default "classic".
</mission>

<rules>
- Render is a pure function of AppState and terminal size. No I/O in View or Update; all I/O in tea.Cmd.
- Inline mode: commit settled rows to scrollback with tea.Println; keep only the live area in View().
- Rows come only from real engine events. Never show a state without its event.
- The word "verified" appears only at trust T2 or higher with passing evidence for every required requirement. A planted tampered run must render "not verified"; add that test first.
- Gates default focus to the option that keeps the constraint; test it.
- Every glyph has a word in plain mode and an ASCII fallback; test with NO_COLOR and NIKI_ASCII=1.
- Footer hints come from the key registry; a test asserts every hint dispatches in that state.
- Originality guard from section 8 runs over all golden frames.
- Golden frames at 100x27, 80x24, 60x20, plus plain mode. Update goldens only after printing the diff.
</rules>

<limits>
- Work only in internal/tui and tests, plus the minimal event interface. Never publish or push.
- Dependencies outside the pre-approved list: ask first.
- Follow AGENTS.md build hygiene. After any /compact, re-read docs/PACK.md and docs/ui/PROGRESS.md.
</limits>

<phases>
C1 Header, first-launch setup block, composer, footer; golden frames 1 and 2.
C2 Tool rows, thought line, activity line, loop rows, failure excerpts; golden frame 3.
C3 Contract strip, gates, verdict card, tamper test; golden frames 4 and 5.
C4 Widths (100, 80, 60), plain mode, accessibility modes, resize preservation.
C5 Owner review: ANSI frame dumps in docs/ui/review/. Stop. Do not claim the design is good.
</phases>

Begin with C1 after I approve.
````

**Goal (after approval, `/auto`):**

```
/goal Classic flow phases C1-C4 are done, proven in this conversation: (1) golden frames exist at 100x27, 80x24, 60x20 and in plain mode and match the approved mockup content; (2) tool rows render from engine events only, shown by a replay test of the five-frame fixture; (3) a planted tampered run renders "not verified" and never "verified", shown by test; (4) every gate defaults focus to the option that keeps the constraint, shown by test; (5) every footer hint dispatches in its state, shown by test; (6) every state renders with NO_COLOR and NIKI_ASCII=1 with a word for each glyph, shown by test; (7) the originality guard passes on all golden frames and fails on a deliberately injected forbidden signature; (8) the resize test preserves focus, scroll, and gate state; (9) ANSI frame dumps are in docs/ui/review/ for owner review; (10) golangci-lint and tests ran clean with output printed; (11) no golden was updated without a printed diff; (12) git status is clean. OR stop at OWNER REVIEW and print what I must decide.
```

## 11. Owner decisions

1. Accept Classic as the default and Ledger as an optional view.
2. Accept the one-line tool rows with right-aligned results (a deliberate departure from the two-line rows).
3. Accept the gutter glyph set and the plain activity phrases.
4. Keep the contract strip visible during runs (recommended) or show it only on request.
5. Run the validation test in section 9 before turning the contract strip and verdict card on by default for everyone.
