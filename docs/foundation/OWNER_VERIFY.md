# NIKI foundation — OWNER VERIFY

Exact manual steps for the rows that cannot be proven by a test in this environment. Each one
names the terminal, the command, and what to look for. A row only appears here when an automated
check would have to fake the thing it claims to check.

Last updated: 2026-10-04.

---

## D2 — Ctrl+Z suspend and resume

**Why it is here:** suspension needs a *controlling terminal* — the shell has to be in the
foreground process group to receive `SIGTSTP`, and to be re-foregrounded to receive `SIGCONT`. The
PTY harness owns a pseudo-terminal but not the job control that goes with it, so a test would have
to fake the very thing it claims to check.

**Steps (any of: GNOME Terminal, Kitty, WezTerm, iTerm2, Windows Terminal):**

1. `cd shell && bun install && cargo build -j 2 --manifest-path ../Cargo.toml` from the repo root
   if the binary is not already built.
2. Start the shell in a **real** terminal, not through `script`:
   `cd shell && node --import tsx -e "import('./src/cli.tsx').then(m => m.main(['--engine','../target/debug/niki']))"`
3. Type a few characters in the composer. Press **Ctrl+Z**.
4. **Look for:** the shell disappears, your shell prompt returns, and `ps` shows the node process in
   state `T` (stopped).
5. Run `jobs` — the job is listed as `Stopped`.
6. Type `fg` and press Enter.
7. **Look for:** the shell comes back, **fully redrawn** — the mascot, the header, the transcript
   and the composer are all present, and the characters you typed before suspending are still in the
   composer.
8. Press Enter and confirm the turn still runs.

**Fails if:** the terminal is left without a cursor, with the alternate screen still active, or
with the composer empty after resume.

---

## Mascot — the five states, all three tiers

The frames are already in `docs/foundation/review/mascot-tiers-and-states.txt`; this is the
judgement call that a test cannot make.

1. Open `docs/foundation/review/mascot-tiers-and-states.txt`.
2. **Look for:** one eye, no legs, no antennae, no crab or bug silhouette.
3. Confirm the eye differs between `idle` (◐) and `working` (◑), and that `done` (●) and `error`
   (○) differ from both — colour must never be the only difference.
4. Confirm the body colour changes for `done` (success) and `error` (error) as well as the eye.
5. Confirm the `working` state is **static**. It must not animate: the sweep belongs to the
   activity line, and two spinners at once is the defect.
6. Confirm every row is seven columns wide in the full tier, so the header does not jitter between
   states.

---

## The review frames — taste

`docs/foundation/review/` holds eleven key chat states at 80x24 and 120x38, the mascot sheet, and
the palette reference.

1. Start at `01-idle_80x24.txt` and `02-thinking_80x24.txt`.
2. **Look for:** the composer pinned at the bottom, the header scrolling away with the transcript,
   one activity line directly above the composer.
3. `03-parallel-tools_80x24.txt` — three tools, two finished with result lines, one still running.
4. `04-failed-tool_80x24.txt` — the failed row in the error colour with the engine's own excerpt,
   and the run continuing afterwards.
5. `05-approval_80x24.txt` — the prompt takes the activity line's place, and the **Deny** option
   carries the default marker even though Allow is listed first.
6. `06-stages_80x24.txt` — stage rows using the same grammar as tool rows, with provenance labels.
7. `09-end-of-turn_80x24.txt` — the summary line with real numbers.
8. Compare the 80x24 and 120x38 versions of the same state: the composer and the footer must be
   identical in shape, and the extra width must not squeeze anything.

**Taste questions for the owner**, answered in the owner's own words:

1. Does the orb read as *yours*, or as decoration? You asked for one eye; is one eye enough
   character at 7x3?
2. Is the header too loud at three lines, or is it the right amount of identity before it scrolls
   away?
3. Do you want the rule-based composer chrome, or Kimi's bordered cards?
4. Is the rule span the full width, or should it stop short at wide terminals?
5. Should the footer keep the model when narrow, or drop the cwd first and keep the model longer?

---

## Colour on a real terminal

The contrast checks are automated and measured; what they cannot check is whether the palette is
*tasteful* on hardware you own.

1. Run the shell in your normal terminal.
2. Switch through `/theme` (or the theme picker) across `niki`, `niki-light`, `niki-contrast`,
   `niki-dim`.
3. **Look for:** flat greys reading as grey rather than blue-ish; the accent used only for focus
   and activity; error the only hue that means removal.
4. Run with `NO_COLOR=1` and confirm the interface is still fully usable in monochrome — every
   state has a glyph and a label, not only a colour.

---

## Performance, on this machine

The automated probes record numbers. What only you can judge is whether they are *fast enough*.

1. With a real project loaded, start a turn that streams for a minute.
2. **Look for:** no perceptible lag while typing, no layout shift as output arrives.
3. Leave the shell idle for a minute and check CPU:
   `ps -o %cpu,rss,comm -p $(pgrep -f "tsx src/cli.tsx" | head -1)` sampled a few times.
   Idle should be effectively zero, because the sweep timer only runs while something is in flight.