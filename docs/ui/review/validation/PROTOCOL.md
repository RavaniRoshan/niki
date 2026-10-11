# Validation study — contract strip and verdict card (design §9)

Purpose: decide whether the contract strip and verdict card raise
correct verdict readings before they stay on by default. Until this
runs, they ship but remain reviewable — the plain-transcript variant
exists so the same recorded run can be shown both ways.

## Materials (in this directory)

- `frame{3,5}_{100x27,80x24,60x20}_with.ansi` — Classic with the
  contract strip and verdict card (current default).
- `frame{3,5}_{100x27,80x24,60x20}_without.ansi` — the same recorded
  run as a plain transcript (no strip, no card).

The frames are renders of the same scripted run
(`internal/tui/classic_fixture.go`, frame 3 mid-run, frame 5 the
verdict). The with/without pair differs only in those two elements.

Regenerate with:

```
go test ./internal/tui/ -run TestPlainTranscriptVariant
```

## Protocol

1. Recruit 6-8 developers who use Claude Code, OpenCode, or a
   similar agent CLI regularly.
2. For each participant, in random order, show the frame 5 pair from
   one size (100x27 for most; 60x20 for anyone on a small screen):
   one version WITH the strip and card, one WITHOUT. Don't explain
   the elements first.
3. After each version, ask exactly:
   - "Is this done?"
   - "What is not verified?"
   - "Why did it loop?"
4. Record answers verbatim. The frame-4 pair is the follow-up probe:
   show it when a participant is confident about frame 5, to test
   whether the gate's reason line lands.

## Pass criteria (ship the additions only if...)

- Correct verdict readings go UP: participants shown the WITH version
  answer "what is not verified" correctly more often (the honest
  `not verified  load behavior: no check is configured for it` line
  is the target answer) than participants shown the WITHOUT version.
- AND no participant accepts a tampered run as done. The mechanical
  half of this is already proven (`TestTamperedRunRendersNotVerified`
  guarantees a planted tampered run never renders "verified"); the
  human half is whether the card's wording convinces a reader.

## Status

NOT YET RUN — owner to schedule. Until then: the default stays as
built (strip and card visible), the toggle exists
(`State.HideContractUI`, no key bound), and nothing here is a claim
that the design is good.
