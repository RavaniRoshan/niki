# Phase B — First-principles failure map ("the margin map")

Built without consulting other tools. Categories are derived from the question
*what must be true for a run to be right*, not from published taxonomies.

Scoring: **Frequency** (F) and **Harness-fixable** (H) on 1–5. "H" means a
harness can fix it *without a better model*; 5 = purely a harness decision,
1 = only a better model helps.

| # | Failure | F | H | F×H | Note |
|---|---|---|---|---|---|
| 1 | Premature done / false approval | 5 | 5 | **25** | "Done" is a *harness* decision. Nobody else can take it from the model. |
| 2 | Verification that doesn't verify | 5 | 5 | **25** | The harness can run the tests itself instead of reading a claim. |
| 3 | Silent degradation | 4 | 5 | **20** | Error hit, retried, omitted from the final answer. |
| 4 | Context rot (long-horizon) | 5 | 4 | **20** | Typed state outside the transcript beats replaying history. |
| 5 | Wasted budget | 5 | 4 | **20** | Allocation is bookkeeping, not intelligence. |
| 6 | Loops / thrashing | 4 | 5 | **20** | Pure detection. |
| 7 | Broken / wrong edits | 4 | 5 | **20** | The harness owns the edit primitive. |
| 8 | Environment breakage unnoticed | 3 | 4 | 12 | Hermetic sandbox + a build check the agent cannot skip. |
| 9 | Overclaiming provenance | 4 | 5 | **20** | "I tested this" is checkable against a recorded run. |
| 10 | Flaky verification | 3 | 3 | 9 | Detectable by re-running, but detection costs as much as the flake. |
| 11 | Bad localization (right fix, wrong place) | 4 | 2 | 8 | Tests passing does not prove you edited the right place. |
| 12 | Misreading the spec | 4 | 1 | **4** | **The ceiling.** An oracle can only check what the spec makes checkable. |

## The margin map

**Rows 1–9 are where a large margin can exist.** Eight of the top nine are
harness-owned, and three of them (1, 3, 9) are *purely* harness-owned: no
model improvement fixes "done" being asserted without evidence, a swallowed
error, or a claim of work never performed.

**Row 12 is the hard ceiling, and it bounds the ambition.** If the model builds
the wrong thing perfectly, every harness mechanism still scores zero. That is the
argument that no harness wins on raw pass rate: the residual is a model property.

## Which TWO axes to own

**Axis 1 — false-done rate.** The share of runs reporting success that did not
pass their own acceptance criteria. It is the top of the map, it is the one a
vendor cannot optimise without admitting their product lies, and — decisively —
it is the axis where NIKI is *already structurally built*: the repo's own eval
separates `verdict` from `red_reconciliation` and grades maintainer
merge-worthiness over test-passing. [repo]

**Axis 2 — cost per accepted change.** Spend divided by a change a human
actually took. It rewards transactions, rollback and budget allocation together
(rows 2, 5, 7), and it is the axis a solo user feels.

**Why not pass^k.** It is the obvious third. The objection is strong: if one
attempt succeeds at rate p, running k of them succeeds at 1−(1−p)^k, and *that
is arithmetic, not a harness*. It only becomes a harness claim if attempts share
work (snapshots, cached prefixes, warm context) — which is a real mechanism, but
it is the same mechanism as axis 2 wearing a different name. One axis, not two.

**Why not human-minutes.** It is what the user feels, but it is measured with a
stopwatch by a person, has no ground truth, and is unreproducible for a grader.
Axis 2 is its objective shadow.

## Consequence, and it is uncomfortable

Both axes require **provenance the repo does not record**. Phase A measured it:
the eval fixtures carry no model, no prompt, no tool trace. Axis 1 cannot be
computed from an artifact that says `"approved"`. **Provenance is a prerequisite,
not a reporting feature** — and it is cheaper than either axis.
