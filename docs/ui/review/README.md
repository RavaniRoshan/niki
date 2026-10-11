# Classic Flow — owner review (docs/ui/review/)

Rendered frames for owner review (C5). Nothing here is a claim that
the design is good — resemblance and taste are the owner's call
(design §8).

## Files

- `frame{1..5}_{100x27,80x24,60x20}.ansi` — full-color frame dumps
  (truecolor). View with `cat` (or `less -R`) in a truecolor terminal.
- `frame{1..5}_{100x27,60x20,80x24}.plain.txt` — the same frames in
  plain mode (NIKI_PLAIN=1): ASCII glyphs with words, no color, for
  screen readers and logs.

## The five frames

| # | Shows |
|---|---|
| 1 | First launch: orb header, tip line, SETUP block, idle composer |
| 2 | Submitted: raised user row, say text, activity line |
| 3 | Tools in flight: tool rows with right-aligned results, contract strip, next line |
| 4 | Failure and gate: failed tool with excerpt, loop row, gate naming the conflict |
| 5 | Verdict: the card with one honest not-verified line |

## Regenerate

```
NIKI_WRITE_DUMPS=1 go test ./internal/tui/ -run TestWriteClassicFrameDumps
```

Compare against the approved mockups (`IMPf/niki-classic-flow-sheet.png`,
`IMPf/niki-classic-flow-gate.png`).

## Validation study (contract strip + verdict card)

`validation/` holds the with/without frame pairs for the design §9
study, plus `validation/PROTOCOL.md` (the 6-8 developer script and
pass criteria). The strip and card stay on by default until that
study runs.

## Known differences from the mockups (all recorded in ../CLASSIC.md §7)

- No sandbox segment, ↑n ahead count, elapsed time, or thought line —
  no engine events report them; nothing is invented.
- The header omits the footer-context meter when usage is unreported.
- Gate options stack one per line under 80 columns.

## Open decisions for the owner (design §11, still open)

1. Accept Classic as the default and Ledger as an optional view.
2. Accept the one-line tool rows with right-aligned results.
3. Accept the gutter glyph set and the plain activity phrases.
4. Keep the contract strip visible during runs, or on request only.
5. Run the validation test (design §9) before enabling the contract
   strip and verdict card for everyone.
