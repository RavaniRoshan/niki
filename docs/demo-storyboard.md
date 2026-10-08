# Demo storyboard: the agent loop (TUI glyph/state tour)

Method: real `vhs` footage of the real binary on the mock provider with
`NIKICODE_DEMO_TOUR`… — correction: `NIKICODE_DEMO_TOUR=1`, which arms
scripted tool calls (zero model spend). Nothing is composited or faked:
every frame is terminal output. Pacing borrows only generic craft
(single play, holds on settled states, 10fps, decode-validation).

## Beat grid (order normative, durations approximate)

| # | Beat | Screen | Hold |
|---|---|---|---|
| 1 | Cold open | Welcome card (orb mascot, "Welcome to NikiCode!"), announce line, `○ NikiCode is ready` idle, bordered composer, footer (model, dir, context meter) | 1.0s |
| 2 | Prompt | `read the main file` typed fast into the composer, submitted | — |
| 3 | thinking | Live line: sweep glyph + `thinking…` (TurnStarted). One sweep frame every 120ms while busy. | — |
| 4 | running + tool cell | Live line: sweep + `running read_file`; transcript commits `● read_file` cell with the file content; assistant streams `Read complete …` (`streaming…`) | — |
| 5 | Settle | Idle line back, footer cost/context ticked up. Ledger readable | 1.0s |
| 6 | Loop again (grep, then git status) | Same sweep: thinking → running → committed cell → streaming → idle, footer ticking each turn | — |
| 7 | End card | Full ledger: 3 prompts, 3 tool cells, 3 replies, idle line, footer. NO loop cut — the GIF itself loops | 2.5s |

Shipped cut: 214 frames @10fps, 21.4s, 960px, 317KB, looping.
Sweep frames verified in the encode (◑ streaming mid-turn).

## Glyph/state key (what the viewer learns)

- `○` muted = idle/ready. Sweep `◐◓◑◒` (120ms) + verb = live state;
  static `◐` under reduced motion; `-\\|/` on dumb terminals.
- `thinking…` = turn accepted, model working.
- `running <tool>` = tool executing; `● <tool>` cell = committed result.
- `streaming…` = text deltas landing.
- `●` bullets = assistant/tool transcript; footer `context: x%` + `cost: $y`
  tick every turn — the loop made visible.

## Validate by decoding

- Canvas 720px wide; text legible at 100%.
- Cold open shows welcome + idle (no prompt yet).
- End card shows 2 tool cells with real file content.
- Single play (no NETSCAPE loop extension).
- `NIKICODE_DEMO_TOUR` does not appear; env is `NIKICODE_DEMO_TOUR=1`.
