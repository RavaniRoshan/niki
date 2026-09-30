# What does not work yet

This file is the one most projects would not write. It is here because you
should know what you are getting before you spend an evening on it, and because
the rest of this repository makes a point of not overselling — a claim you
cannot check is a claim you should not repeat.

Everything below is measured, not guessed. Where there is a number, it came from
running it.

---

## 1 · A small local model will often stop at the Coder stage

**This is the most likely thing to happen to you, and it is not your fault.**

Every stage of the pipeline has to return a JSON artifact matching a strict
schema. The Coder's artifact, in particular, is a unified diff expressed as
search-and-replace pairs. This is a harder thing to produce on demand than
prose, and small models are bad at it.

Measured on this exercise, with the model named in `niki.toml`:

| Model | Outcome |
|---|---|
| `qwen2.5-coder:3b` | Fails at the Coder stage on ordinary tasks. Do not start here. |
| `qwen2.5-coder:7b` | Works sometimes. Retry, or use a larger model. |
| A hosted model with a real key | Works. This is the reliable path. |

**What a failure looks like:** the run stops, names the stage, and tells you
there is nothing to review. No branch is created. Nothing on your machine has
changed beyond the artifacts under `.niki/`.

**What to do:**

```bash
ollama pull qwen2.5-coder:14b     # or a hosted model
```

and change the four `model =` lines in `niki.toml`. Then run again.

**How to check where your own model stops**, rather than guessing:

```bash
niki smoke
```

That runs a trivial task end to end and reports the stage that failed. It is the
fastest way to find out whether your model is capable before you spend twenty
minutes on a real run.

---

## 2 · The twenty-two tools are not what runs your task

NIKI's runtime has twenty-two tools — file reads, search, shell, web fetch, task
control. They are real, they are tested, and they are **not** what a default run
uses.

By default the four agents receive a deterministic context pack built from your
repository and each return a single schema-valid JSON artifact. The tool loop
exists behind `[tools] experimental_tool_loop = true`, where it runs as a
bounded research pass before the Planner.

This is a deliberate trade: determinism and bounded cost in exchange for an agent
that cannot go and look at something for itself. It is a good trade for a
pipeline whose selling point is that you can read what every stage did. It is
worth knowing which one you are buying.

---

## 3 · The goal loop cannot retry

`niki goal` runs a task list forward. It does not retry a task that blocked,
and it does not carry what it learned into the next iteration, so a second
iteration can repeat a first iteration's mistake. It is a single forward pass
with persistence, not an autonomous agent that improves.

Use `niki run`, which is what this exercise uses.

---

## 4 · Visual regression is not runnable on your machine

If you ever touch the TUI: the visual gate compares rendered frames against
references captured on the CI runner, and a local render differs from that
reference by 6–9% of pixels. It is not broken; it is environment-dependent.
Recovery is `gh workflow run ci.yml -f regen=true` plus a human reviewing the
frames.

---

## 5 · The worktree backend has no isolation

`niki.toml` here sets `backend = "worktree"`, which is what lets this run
without a container. Agent commands execute as **local processes with your own
privileges**.

That is fine for a project you just read end to end, like this one. It is not
fine for a repository you did not write. If you have Podman or Docker, change
`backend` to `"docker"`, build the sandbox image, and get real isolation.

---

## 6 · What is not covered by any test

Prose claims about behaviour. The numbers in the NIKI README — test counts, cost
per task — are the clearest examples: they were true when written and the
project can no longer falsify them automatically. The claims that *can* be
checked are checked, by `tests/claims.rs` and `tests/docs_consistency.rs`, and
those gates are themselves proven able to fail. The rest is maintained by
attention, and attention runs out.

---

## The short version

The parts of NIKI that make it trustworthy — the audit trail, the independent
review, the permission model, the branch you can read — are real and are tested.
The parts that would make it more capable — a real tool loop, a goal runner that
retries — are real code that is not on by default.

If a document tells you otherwise, including this one, believe the code.
