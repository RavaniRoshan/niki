# PARITY — what the reference CLIs have that NIKI does not, and what to do about each

**This file is a decision record, not a work order. Nothing in it is authorised to be built.**

The owner's rule, restated so it cannot be misread later: *none of these are built until every P0
and P1 row in `CHECKLIST.md` is WORKS and the owner approves.* `CHECKLIST.md` does not meet that
bar today — the summary table at its foot shows groups F, G and H entirely unbuilt, so the gate is
not open for any row below, however small. Writing the recommendation now, while the work is
blocked, is what makes the deferral a decision rather than an omission.

Last verified: 2026-10-04, against `shell/src/` as it stood at commit `1ef4b15` plus uncommitted
work in progress under `shell/src/surfaces/`. Line numbers are from that read; the symbols and the
files are the durable part of each citation, the line numbers are the fragile part.

---

## How to read the table

| Column | Meaning |
| --- | --- |
| **NIKI today** | what the code does, with a `path:line` a reader can check. "Absent" means a search of `shell/src/` found nothing, not that the author forgot. |
| **Recommendation** | build / defer / never. |
| **If deferred** | what it costs to wait — the honest consequence, not "no consequence". |

---

## 1. Configurable keymap — remap every binding

| | |
| --- | --- |
| **NIKI today** | **Absent.** Every binding is a literal comparison inside `handleKey` (`shell/src/dispatch.ts`) and there is no configuration file to remap it from. The shell's only CLI flags are `--engine`, `--engine-arg` and `--theme` (`shell/src/cli.tsx:84`, `:92`), and the only environment variables it reads are `HOME` (`shell/src/components/footer.tsx:128`) plus the motion/charset/no-colour variables the reducer reads (`shell/src/cli.tsx`). No keymap is loaded from disk. |
| **Recommendation** | **Defer.** Not until the keymap is generated rather than hand-written — `KEYMAP.md` and `HELP` are generated from the registry now, so the next step is making the *dispatcher itself* read a table instead of a chain of `if`s. A remapper over a chain of `if`s is a rewrite wearing a config file's clothes. |
| **If deferred** | A user on a non-QWERTY layout, an AZERTY user, or anyone whose terminal sends `Ctrl+K` for something else cannot change a binding. The cost is bounded and honest: every binding is documented in `KEYMAP.md`, so the workaround is "read the doc and use the real key". The cost that is *not* bounded is if a binding later changes without the docs being regenerated — which is why the docs are generated and tested. |

## 2. Vim composer mode

| | |
| --- | --- |
| **NIKI today** | **Absent, except for three narrow chords.** `g g` and `g G` jump to the top and end of the transcript (`shell/src/dispatch.ts`, the `pendingChord === 'g'` arms), and `j`/`k` move the approval focus while a prompt owns the keyboard (`shell/src/dispatch.ts:128` and the `key.downArrow` arm beside it). There is no normal mode, no word motion, no `dd`/`cw`, no counts, no `.` repeat. |
| **Recommendation** | **Defer.** A vim mode is a second input language layered over a composer that is still settling: caret motion is currently the terminal's job, not the shell's. Adding normal mode first means owning the caret, and owning the caret is a real change to `AppState`. |
| **If deferred** | Vim users keep typing prose into a linear composer. Nothing breaks; the shell does not *detect* and help. If the composer's editing model changes later (see the caret note above), a vim mode designed against the old one would be thrown away, so building early buys nothing. |

## 3. `@` file mention that honours `.gitignore`

| | |
| --- | --- |
| **NIKI today** | **Absent.** The composer renders `state.composer` as text and nothing else (`shell/src/components/composer.tsx:50`); an ordinary character is inserted verbatim (`shell/src/dispatch.ts`, the `key.input` catch-all). The shell resolves no path a user typed: the only `node:fs` import under `shell/src/` is `shell/src/cli.tsx:17`, and it exists to append the engine's stderr to a log file, not to read the user's repository. `.gitignore` appears nowhere in the shell. |
| **Recommendation** | **Defer.** Mentioning a file means the shell reads the user's repository, which is a capability it currently does not have and should acquire deliberately, not as a side effect of a completer. It also needs a protocol question answered first: does the engine resolve `@src/x.rs` to content, or does the shell paste the path and let the engine do it? |
| **If deferred** | A user types the path. `@` in a prompt is literal text, which is what every non-supporting tool does. The real cost is discoverability: users assume `@` works because they have used it elsewhere, and NIKI will not say so. That is a documentation cost, and `KEYMAP.md` plus the README are where it should be paid. |

## 4. Image paste

| | |
| --- | --- |
| **NIKI today** | **Absent.** A bracketed paste is parsed into one key event carrying the whole payload (`shell/src/input.ts`, `InputParser`), and anything over 4 000 characters is deliberately collapsed to a `[pasted N lines, M chars] …` placeholder (`shell/src/input.ts:138`) so a 40 000-line file cannot land in the composer. There is no kitty-graphics, iTerm2 inline-image or Sixel handling anywhere in `shell/src/`. |
| **Recommendation** | **Defer**, and treat the *paste limit* as the thing to revisit first. Image paste is a protocol negotiation with the terminal emulator, and no two emulators agree. The current placeholder behaviour is a deliberate, tested choice; changing it is a separate decision from adding images. |
| **If deferred** | A screenshot cannot be shown to the model. Pasting one currently produces either its filename or a `[pasted …]` placeholder in the prompt, which is a confusing thing to send. The mitigation is honest phrasing in the docs, not a silent fallback. |

## 5. Transcript search

| | |
| --- | --- |
| **NIKI today** | **Absent.** `Transcript` takes state and renders a window of rows (`shell/src/components/transcript.tsx`); there is no query, no match index and no filter. Scrolling is an integer offset (`scrollOffset` on `AppState`, `shell/src/state.ts`) moved by PgUp/PgDn/Home/End and the line keys. `/prompts` searches *prompt* history — what the user sent — not the transcript. The overlay machinery that a search box would need (`overlayQuery` on `AppState`) exists and is used by the palette and pickers, so the missing piece is the search itself, not a place to put it. |
| **Recommendation** | **Defer.** A transcript search needs a decision the checklist has not made yet: whether search is a *view* of the same message list or a separate index, and what "next match" means when the transcript is being rewritten by a streaming turn. |
| **If deferred** | Finding something in a long session means scrolling and re-reading. For the session lengths NIKI is aimed at this is tolerable; for a long session it is the first thing users miss. Note the interaction with the known `reduce` cost in `GAPS.md` §G6: search over a transcript built by streaming is exactly where that cost would be felt. |

## 6. Fork and resume

| | |
| --- | --- |
| **NIKI today** | **Split. Resume: present. Fork: absent.** Resume is wired end to end — the reducer emits a `session.load` request carrying a `session_id` (`shell/src/state.ts`, the `session.resume` arm), the engine answers with `session.ready` carrying `resumed_messages`, and the sessions picker offers the sessions the engine reported (`shell/src/surfaces/items.ts`). Fork is absent: the string `fork` appears nowhere in `shell/src/`, and the protocol's only session message is `session.load` (`crates/niki-protocol/src/messages.rs`) — a load *replaces* the session rather than branching it. |
| **Recommendation** | **Build fork, later, and only after resume is proven end to end.** Fork is a genuine product capability rather than a parity checkbox: it is how a user branches one session into two without losing either. But it is a protocol change (a `session.fork` request and the checkpoint semantics behind it), and protocol changes are Phase-2 work by the checklist's own ordering. |
| **If deferred** | A user who wants to try a different approach from message 12 restarts or re-asks. Since resume exists, the cheap version — copy the prompt, start a session — is available and is what a user will do. The cost is that any state built up in the old session (a chosen model, a permission mode, accumulated context) is not carried over, which is exactly the friction fork removes. |

## 7. OSC 9;4 progress reporting

| | |
| --- | --- |
| **NIKI today** | **Absent.** The shell's VT reader *parses* OSC from the engine's output and discards it — an OSC sequence is consumed and dropped (`shell/src/vt.ts`, the `void osc` arm). At the other end, no NIKI code writes an escape sequence: the module contract for the shell root states that "nothing is written to stdout except what Ink itself draws" (`shell/src/cli.tsx:11`), and neither `app.tsx` nor `cli.tsx` contains a `\x1b` write. Ink emits its own cursor and erase sequences; what is absent is any NIKI-level OSC write. So the capability is absent in both directions: NIKI does not read progress from a terminal, and does not report progress to one. |
| **Recommendation** | **Never, as a hand-rolled write.** Concretely: do not add an `ESC ] 9 ; 4 ; st = … BEL` write to `cli.tsx`. Ink renders the frame and the escape would have to bypass it, and a sequence that a resize redraw can interleave with produces visible garbage in the one place the user is looking. If progress reporting is wanted, it belongs to the render layer as a frame-level concern. |
| **If deferred** | The terminal's own progress indicator (the one a shell shows for a background job) does not light up while NIKI runs. Users who rely on it lose a peripheral cue. The cost is small and purely cosmetic; nothing in NIKI's correctness depends on it. |

## 8. Terminal window title tracks the session topic

*Found while reading `reference/REFERENCE_NOTES.md` for this file: both products set the terminal
title — Kimi to the product name at rest and to the user's prompt while working (K00, K01), Claude
Code to the session topic (C00) — and `REFERENCE_NOTES.md` lists "the window title tracks the
session topic" as grammar to take.*

| | |
| --- | --- |
| **NIKI today** | **Absent, for the same reason as row 7 and for a stronger one.** No NIKI code writes an escape sequence (see the row above), so no title is ever set. The VT reader already treats an OSC title as hostile-looking input: the sanitiser strips OSC bodies because a fixture carrying an OSC title-spoof must never reach the terminal (`shell/src/sanitize.ts`, the OSC arm). The machinery to *refuse* a spoofed title is in place; the machinery to *set* a legitimate one is not. |
| **Recommendation** | **Defer**, and if it is ever built, build it behind the sanitiser rather than beside it. A title write is the one escape sequence NIKI would emit, and it is exactly the sequence a hostile engine output would try to spoof. The sanitiser already knows how to tell them apart by direction; the check has to be written once, not twice. |
| **If deferred** | A user running NIKI in a tabbed terminal sees the tab labelled by their shell, not by the NIKI session, so parallel NIKI sessions are indistinguishable in the tab strip. This is the same class of cost as row 7: peripheral, cosmetic, and annoying rather than wrong. |

---

## What is *not* on this list

Two reference behaviours were checked and are **not** parity gaps, so they are recorded here to
stop a later reader re-deriving them:

- **The `└ Next: …` sub-line under the activity line** (Claude Code C02). Present:
  `shell/src/components/transcript.tsx:135` renders the connector and `state.activity.next` when,
  and only when, the engine supplied one.
- **Collapsed, expandable reasoning** (Claude Code C02, `Thought for 1s (ctrl+o to show thinking)`).
  Present: `ctrl+o` toggles `details.toggle` and the transcript has a details mode.

Two reference items are **not decidable from the material on this machine**, and are recorded in
`GAPS.md` §G1 rather than here: `niki-agent-ref/intro.gif` was never supplied, so frame-accurate
claims about the Kimi CLI's motion are permanently unavailable, and the GIFs explicitly do not
show approval prompts, diffs, the slash menu, a todo panel, an end-of-turn summary, narrow-terminal
layouts or mouse behaviour.

## The gate, restated

Every row above says *defer* or *never* except fork, and fork is itself blocked. The precondition
is unchanged and is not this file's to move: **every P0 and P1 row in `CHECKLIST.md` is WORKS, and
the owner approves.** When that gate opens, this file is the list to work from — and the
recommendations here are recommendations, not approvals.
