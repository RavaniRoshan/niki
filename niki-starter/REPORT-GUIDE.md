# Reading `report.md`

NIKI's best artifact is `report.md`, and nobody had ever written the guide that
makes it obvious. This is that guide.

```bash
niki report              # the most recent run
niki report <id>         # a specific one
niki report <id> --json  # the same thing, structured
```

Every run writes one. After a successful run you will find it at
`.niki/tasks/<id>/report.md`.

---

## Read these four sections first

### `## Pipeline Result`

The verdict, the branch, and how many revision rounds it took.

The word to look at is **how the verdict was reached**, not just the verdict. A
run where the Reviewer approved is different from a run that completed with
nothing independently reviewing it, and NIKI distinguishes them: an approval
nobody granted is not reported as an approval. If a section is missing, that is
the answer — it means no independent stage looked at the change.

### `## Final Diff`

The change itself, as a diff. Same content as `changes.patch`, inline.

### `## Verification — Test Suite (executed in sandbox)`

What the Tester actually ran, and what it actually output. Not a summary of a
summary — the command, the exit code, the pass and fail counts, and the tail of
the real output.

**This is the section to distrust least.** Everything else in the report is one
model's account of something; this is a command that ran.

### `## Audit Trail`

The chain, in order: who decided what, and what each stage was given. It is how
you answer "did the Reviewer see the Coder's reasoning?" — it did not, and this
is where you can see that it did not.

---

## Then, if you want the interesting parts

### `## Agent Isolation`

What each stage could see. The Coder saw a task and a context pack. The Reviewer
saw a diff and a test report, and *not* the Coder's reasoning. That is the
central design claim of the whole product, and this is the section that shows
it rather than asserts it.

### `## Hermetic Safety Proof`

What containment was in force: backend, network posture, permission mode,
whether egress was allowed. Read this before running NIKI on a repository that
is not yours.

### `## Cost & Performance`

Tokens and money, per agent, plus a comparison against what a single
autonomous agent would have cost for the same task.

Two things to know. An unpriced model is **warned about, not silently billed at
$0.00** — if this section shows a cost of nothing, check whether your provider
is in the price table before believing it. And a repair retry's tokens are
billed: a stage that had to be asked twice cost twice as much, and the number
here includes that.

### `## Watch items`

Things NIKI is not sure about. Uncertainty the run chose to surface rather than
round off. **This is the most valuable section in the file and the one nobody
reads.** If something is listed here, it is a thing the pipeline noticed and
could not resolve — check it yourself.

---

## When the run stopped early

You still get a report. Two things to look at:

- The **stage** that stopped, named at the top.
- `.niki/tasks/<id>/artifacts/<stage>.json` — the raw structured output of that
  stage. When a stage fails, this is where the model's actual words are, and it
  is usually more informative than the error message.

Every stage has an artifact: `planner.json`, `coder.json`, `tester.json`,
`reviewer.json`, and more if the run reached them.

---

## A worked reading

For this exercise, a good report says roughly:

- **Pipeline Result** — Approved, on `niki/<id>`, 0 or 1 revision rounds, with
  a verdict the Reviewer granted.
- **Verification** — 5 tests, 5 passed. If this says 4 passed, the change did
  not finish the job regardless of what the verdict says.
- **Agent Isolation** — the Reviewer's inputs listed, and the Coder's reasoning
  absent from them.
- **Watch items** — probably empty. If it is not, read it.

Then run `node --test test/` yourself. That is the check nothing in the report
can substitute for.
