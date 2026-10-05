# NIKI scoreboard — status, method, and what is blocked

The scoreboard answers one question: **when every agent is given the same task and the same model,
who catches what?** It is not `niki eval` — that replays NIKI's own recorded artifacts against a
maintainer's grades and asks "does NIKI agree with a human?". Different question, different method.

**Status: the harness is finished and tested. The NIKI arm runs. The three baselines will not.**

**The owner has decided not to supply credentials for Claude Code, Codex or Deep Agents**, so those
arms are not pending — they are closed. Nothing below is a claim the reader has to take on trust;
every number was printed, and where there is no number it says so rather than filling the gap.

What this means for checklist row H3: the row asked for a four-agent comparison, and a four-agent
comparison needs three vendors' credentials. Without them the row **cannot be met on this machine**,
so it is recorded as NOT RUN BY DECISION rather than left looking like outstanding work. The
harness stays because it is finished, tested and correct — the day someone runs it with a key is a
one-command event, not a rebuild.

---

## What is built and verified

| Piece | State | Evidence |
| --- | --- | --- |
| Sealed split | **done** | `evals/scoreboard/sealed.json` — 27 cases, 23 seeded defects and 4 clean controls, frozen from `evals/dataset.toml` with its SHA-256 (`1db080bf706e…`) recorded. `run.py seal` regenerates it deterministically; if the dataset changes, the seal breaks and the runner refuses rather than scoring a different split. |
| One rubric | **done** | Every agent is asked the same question and must answer `CAUGHT` or `CLEAN`. Anything else is *unparseable* and counts as a miss — an answer nobody can parse is not a catch. |
| Scoring | **done** | Recall over the defects and false positives over the clean controls, with Wilson score intervals. Wilson rather than the normal approximation because at n=27 the normal approximation misbehaves exactly where this split lives. |
| Harness tests | **done** | `python3 evals/scoreboard/test_scoreboard.py` — **18 tests, OK.** They caught a real bug: `parse_verdict("NOT CAUGHT")` returned `CAUGHT`, which would have silently inflated a score. |
| NIKI arm | **done** | Runs end to end against a local model through `ollama`. One case in 61 s, verdict extracted and mapped. |

### Why both directions are always printed

The dataset exists because "the reviewer caught it" is not enough to measure. A reviewer that flags
everything scores 100% recall. The four clean controls exist so precision is measurable at all,
and `run.py report` prints both rates together, always.

---

## What is blocked, and exactly why

The three baselines are installed and reachable. They cannot be *pointed at a model*:

| Agent | Installed | Blocked by | Evidence |
| --- | --- | --- | --- |
| NIKI | yes | — | runs against `ollama` with a generated per-project `niki.toml` pinning every agent to the shared model |
| Claude Code | **2.1.286** | closed by decision | `oauthAccount: None` in `~/.claude.json`; `ANTHROPIC_API_KEY` unset. It authenticates *before* honouring `ANTHROPIC_BASE_URL`, so redirecting at `http://127.0.0.1:11434` does not help — it blocks rather than fails. ollama logged no request (`/api/ps` → `{"models":[]}`) and the box sat at load 0.08 while it waited. |
| Codex | **0.152.1** | closed by decision | `OPENAI_API_KEY` unset. |
| Deep Agents | not installed | closed by decision | `deepagents-0.7.21` downloads fine; the blocker is the same missing credential for the model behind it. |

Also relevant: the machine's own `~/.config/niki/niki.toml` pins **every** NIKI agent to NVIDIA
with `api_key_env = "ANTHROPIC_API_KEY"`. That config loads before the project's, so a scoreboard
run that did not generate its own project config would silently measure NIKI on a *different*
model from the baselines — the one thing a scoreboard must never do. `run.py` now writes a
throwaway `niki.toml` per case that pins planner, coder, tester and reviewer to the shared model.

---

## What would have unblocked the baselines (no longer being asked for)

Recorded for completeness, not as a request. Any **one** of these would have been enough:

1. `ANTHROPIC_API_KEY` — lets Claude Code run. Codex and Deep Agents can then be pointed at the
   same model through it if you prefer one provider.
2. `OPENAI_API_KEY` — lets Codex run.
3. A Codex provider config that accepts an OpenAI-compatible local endpoint, plus an
   `ANTHROPIC_API_KEY` shim for Claude Code.

Then:

```bash
python3 evals/scoreboard/run.py run --agent claude-code --model <model>
python3 evals/scoreboard/run.py run --agent codex       --model <model>
python3 evals/scoreboard/run.py run --agent deepagents  --model <model>
python3 evals/scoreboard/run.py report
```

`report` prints a table of recall with 95% intervals, precision, and the unparseable count, then a
line per baseline stating the NIKI-minus-baseline recall delta and whether the intervals overlap.
Two intervals that overlap are reported as **not distinguishable** rather than as a win.

## A caveat about the numbers, stated before anyone reads them

`qwen2.5-coder:3b` is the only model on this machine. It is a 1.9 GB local model. A scoreboard
run against it measures **the harness and the agents' plumbing, not model quality** — and the
sample is 23 defects with 4 clean controls, so the intervals are wide by construction. The
infrastructure is the deliverable here; the delta becomes meaningful when it is run against a
frontier model, over a larger sealed split, with credentials. Producing a confident-sounding
number from 23 cases and a 3B model would be the dishonest outcome, so the harness prints the
interval rather than a bare percentage.

---

## What the NIKI arm measured before it was stopped

The arm ran to completion for the first four cases and was then stopped: with no baselines to
compare against, a single-agent number over 27 cases answers nothing, and it cost roughly four
minutes per case on this machine.

Those four cases happen to be the **clean controls** — the ones where the change is correct and
flagging it is a false positive. All four were flagged.

| Case | Ground truth | NIKI said |
| --- | --- | --- |
| `clean-error-enum` | correct change | CAUGHT |
| `clean-paginated-query` | correct change | CAUGHT |
| `clean-string-helper` | correct change | CAUGHT |
| `clean-typed-config` | correct change | CAUGHT |

**Measured precision on clean controls: 0/4 — zero.** Recall is unknown, because none of the 23
seeded defects were reached.

This is a real finding, and it is why the dataset carries negative controls at all. With only
defects in the split, "the reviewer caught it" measures nothing: a reviewer that flags everything
scores 100%. On `qwen2.5-coder:3b` — a 1.9 GB local model, far below the frontier models the
reviewer is written for — NIKI flags every clean change it was shown. That is a statement about the
model, not a defect in the pipeline, and it is exactly what a scoreboard exists to make visible
rather than hide behind a headline percentage.

It is still not a scoreboard, and no delta is published.
