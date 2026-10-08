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
| 3 | thinking | Live line: `✱ thinking…` (TurnStarted). Flashes in real time. | — |
| 4 | running + tool cell | Live line: `✱ running read_file`; transcript commits `● read_file` cell with the file content; assistant streams `Read complete …` (`streaming…`) | — |
| 5 | Settle | Idle line back, footer cost/context ticked up. Ledger readable | 1.0s |
| 6 | Loop again | Prompt 2: `search for worker` → thinking → `running grep` → grep cell → streaming text → idle | — |
| 7 | End card | Full ledger: 2 prompts, 2 tool cells, 2 replies, idle line, footer. NO loop | 2.5s |

Total ~20s @10fps ≈ 200 frames, 720px, ~200KB.

## Glyph/state key (what the viewer learns)

- `○` muted = idle/ready. `✱` orange + verb = live state.
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
