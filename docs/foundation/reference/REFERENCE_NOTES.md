# Niki TUI — Reference Notes (frame-by-frame)

Sources, both in the niki-agent repo root (branch `niki-agent`):

| File | Product | Size | Frames | Length |
|---|---|---|---|---|
| `intro.gif` | Kimi Code CLI 0.1.0 | 800x653 | 325 | 23.1 s |
| `demo.gif` | Claude Code v2.0.0 | 1552x992 | 414 | 42.4 s |

These notes describe **only the 12 frames in `frames/`**, extracted at scene changes. They are observations, not an exhaustive review. Inspect the PNGs yourself. If you can run Python with Pillow, you may extract more frames from the GIFs.

**Use these as a study of structure and behavior. Do not copy** either product's mascot, wording, spinner verbs, glyph set, or colors.

## Kimi Code CLI (`frames/kimi_intro/`)

- **K00 (0.0 s, startup idle).** A bordered welcome card: small pixel mascot, product name, a dim "Send /help for help information." line, then four aligned fields (Directory, Session, Model, Version). Below it, a bordered composer box with a `>` prompt and a block cursor. Under the composer, a one-line footer. Left side: `<model> thinking`, short cwd, git branch with ahead count (`main [↑1]`). Right side: contextual hints (`/yolo: toggle yolo | ctrl+c: cancel`). A second right-aligned line shows a context meter: `context: 0.0% (0/262.1k)`. The terminal window title is the product name.
- **K01 (8.4 s, turn begins).** The user's message is echoed as a single accent-colored line with a sparkle marker. The model's reasoning appears in dim italic with a bullet, fully visible. The assistant answer starts with a bullet. A spinner line (`working...`) is pinned directly above the composer. The composer never moves. The footer hints changed to `ctrl+c: cancel | /help: show commands`. The window title changed to the user's prompt text, and a thin progress line appears under the title bar while working.
- **K02-K04 (12.7-21.9 s, streaming).** The answer streams as Markdown: bold section headings, bullet lists, blue links (including an inline path rendered as a link). The transcript scrolls up as content grows. The spinner line, composer, and footer stay fixed at the bottom. The context meter stays visible throughout.

## Claude Code v2.0.0 (`frames/claude_demo/`)

- **C00 (0.0 s, startup + typing).** A header block, not a box: pixel mascot, `Claude Code v2.0.0`, a dim `model · plan` line, and the cwd. Then a composer drawn as **two thin horizontal rules** (one above, one below) with a `>` prompt between them. Under the lower rule, a dim footer: project name left, a contextual hint right (`○ /ide for Cursor`). The window title is a session topic (`Test Coverage`).
- **C01 (5.1 s, submitted).** The user's message is echoed as a **raised/highlighted row** with a `>` prefix. Below it, a dim `Thinking…` line, then an **activity line** in the accent color with a spinner glyph, a whimsical verb, and `(esc to interrupt)`. The composer is immediately live again below the rules, so the user can keep typing.
- **C02 (14.6 s, first tool).** Assistant text with a white bullet. A tool call as one line: a state-colored dot, **bold tool name**, dim argument in parentheses (`Read(package.json)`). A dim `Thought for 1s (ctrl+o to show thinking)` line (reasoning collapsed). The activity line now names the **current task** (`Identifying testing framework…`) and adds a dim sub-line `└ Next: …`.
- **C03 (15.9 s, parallel tools).** Finished tools show a **green** dot and a one-line dim result under a corner connector: `Read 46 lines (ctrl+o to expand)`. In-flight tools show a **gray** dot and no result yet. Several tools can be in flight at once.
- **C04 (26.7 s).** Same grammar repeated: `Search(pattern: …)` rows with `Found 100 files (ctrl+o to expand)`. Counts are emphasized in bold inside the dim result line. Transcript scrolls; composer and footer stay fixed.
- **C05 (34.4 s, shell tool).** A `Bash(cd … && npm list …)` row in flight shows a dim `Waiting…` result line. The activity line's hint grows: `esc to interrupt · ctrl+t to show todos`.
- **C06 (41.6 s, failed result).** A tool failure is shown **inline** in the error color with the message, under the same connector, with a dim `(empty)` sub-line. The session continues. The composer and footer do not change.

## What the GIFs do NOT show

Approval prompts, diffs, the slash menu, a todo/plan panel, an end-of-turn summary, narrow-terminal layouts, mouse behavior, and anything after the last frame. Do not infer these from the GIFs. For them, use the behaviors in niki-agent (`libs/code/deepagents_code/tui/widgets/`) and the Niki design rules in the main prompt.

## Structural grammar to take (common to both)

1. The **composer is the anchor.** It never moves, stays focused during a run, and accepts input while output streams above it.
2. One **live activity line** directly above the composer: spinner, what is happening now, elapsed, and the interrupt key.
3. **Tool calls are one compact line each**, with state encoded in the glyph color and a dim one-line result beneath.
4. **Reasoning is dimmed** and collapsible.
5. A **footer** splits ambient facts (left) from contextual hints and a context meter (right), and the hints change with state.
6. The **user's message is visually distinct** from the assistant's.
7. The **window title tracks the session topic.**

## Niki mascot previews (`mascot/`)

Rendered with the Niki dark theme tokens (accent body, eye glyph in background color). These are PROPOSALS for the owner to approve, not final art.

- `orb_states_and_tiers.png`: the recommended 7x3 "orb" in five states, plus the compact one-row and tiny/ASCII headers.
- `orb_silhouette_variants.png`: six silhouette studies. V1 (7x3) is the header art; V6 (9x4) is the optional first-run welcome size.
- `alternates_and_rejected.png`: a half-moon badge and a block-N monogram as alternates, and the two silhouettes to avoid.
