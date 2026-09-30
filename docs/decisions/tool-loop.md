# Decisions on the accumulated surface

> **Date:** 2026-09-30 · **Crate:** 0.9.0
> **Purpose:** NIKI grew 31 module directories in twelve weeks with one
> maintainer. Some of that surface is write-only, off by default, or disagrees
> with itself. This file records what was decided about it, and why, so the next
> person does not have to re-derive it from scratch.

Each entry is a decision, not a status report. Where something was left alone,
that is the decision and the reasoning is given.

---

## D1 · The tool loop stays experimental, and the documentation now says so

**The question.** `src/runtime/` is 7,200 lines implementing twenty-two tools and
a bounded agent tool loop. `[tools] experimental_tool_loop` defaults to `false`.
The only production call site is `run_experimental_research`
(`src/orchestrator/pipeline.rs:1275`) — a pre-Planner research pass, bounded to
`[tools] max_steps`, default 4.

In a default run, none of the twenty-two tools execute. The four agents receive
a deterministic context pack from `build_context_pack` and each return a single
schema-valid JSON artifact.

**The decision.** Keep it off by default. Document it plainly.

**Why.** The alternatives were to delete it or to promote it.

Deleting 7,200 lines of tested code would be a large, unreviewable change made
for the sake of tidiness, and the seam is genuinely used by the research pass.
Promoting it would change the product's defining property: a stage becomes a
tool loop that can wander, and the artifact contract that makes the pipeline
auditable stops being the whole story. A multi-agent pipeline whose selling
point is *"you can read exactly what every stage did and why"* is better served
by one bounded structured call per stage.

**What this cost.** `CONTRIBUTING.md` tells contributors how to add a tool as
though a run uses it, and the README's module map reads the same way. That
mismatch is now stated in `docs/launch-audit.md` and in
`niki-starter/HONESTY.md` §2 rather than left for a reader to discover.

**What would change it.** A stage that genuinely needs to explore — rather than
be handed a context pack — is the case for turning it on for *that role*, not
for the whole pipeline. If that experiment is wanted, it should be per-role and
it should be measured, not flipped globally.

---

## D2 · The goal loop carries what it learns (fixed, not deferred)

**The question.** `src/goal/runner.rs` accumulated `context_summary` and
`negative_knowledge` faithfully — appended to, persisted, readable in
`niki goal show` — and then handed the pipeline a `Task` built from
`task_desc` and `project_path` alone. Nothing the loop had learned reached any
agent.

**The decision.** Fix it. `description_with_prior_knowledge` now carries the
accumulated history and the recorded negative knowledge into the task
description, which is the one field the pipeline already reads into the
Planner's prompt.

**Why not defer.** "Autonomous multi-iteration goal runner" is a claim in the
README and the `niki goal --help` text. A loop that repeats iteration 1's
mistake in iteration 2 is not that, and no documentation change makes it that.
This was a bug, not a design question.

**Deliberately not done.** The loop still does not *retry* a blocked task — it
advances past one. Retrying changes the semantics of a persisted task list (a
blocked task becomes a loop that may never terminate) and deserves its own
decision. `docs/launch-audit.md` and `HONESTY.md` §3 say so rather than leaving
it to be found.

---

## D3 · Promoted skills are visible to the agents that load them (fixed)

**The question.** Promotion wrote to `config.general.output_dir/skills`. The
runtime `skill_list` / `skill_load` tools read a hardcoded `.niki/skills`,
because `ToolContext` carries no `&NikiConfig`. Under a custom `output_dir` the
two never met: a skill was promoted with a success message, appeared in
`niki skills list`, and was invisible to the agents it was distilled for.

**The decision.** One function resolves the directory, from the project's own
configuration, for every caller. `ToolContext` is unchanged — 22 construction
sites did not need to grow a field to fix a path disagreement.

**Why.** Two functions that disagree about a path *is* the bug. Merging them
removes the class rather than the instance.

---

## D4 · The learning layer keeps its write-only parts, and says so

**The question.** In `src/store/`, `load_index` has no production callers and
`query_store` is test-only. The persisted BM25/vector index is built by manual
`niki index` and nothing in the agent path rebuilds or freshness-checks it, so
retrieval quality silently degrades to live-scan keyword overlap. In
`src/memory/`, `record_memory_use` has no callers at all.

**The decision.** Leave the code; state the limitation. They are public APIs
with a `deny.toml`-justified dependency footprint, and deleting them is a
breaking change for anyone extending the crate.

**What was done instead.** `docs/launch-audit.md` lists them under "thinner
than the documentation implies", and `HONESTY.md` repeats the point for the
reader who is about to be disappointed by it.

**The one thing that *is* wired**, and is worth knowing is real: hierarchical
memory is injected per-role at `src/orchestrator/pipeline.rs:1875`, and the
context pack ranks learned patterns at `src/knowledge/context_pack.rs:92`. The
learning layer is not decorative. The *index* under it is.

---

## D5 · An expired key no longer defeats a fallback chain (fixed)

**The question.** `src/llm/failover.rs` treated any non-5xx, non-transport
error as fatal and returned immediately. A 401 from a stale primary key — the
single most common reason anyone configures a fallback — killed the run before
the fallback was ever tried.

**The decision.** `classify_error` now separates *"this provider is unusable
for this run"* from *"a different provider would do any better"*. Auth
failures and upstream/transport failures move on. A 404 or a 400 still aborts,
because an unknown model or a malformed request fails identically everywhere
and retrying it just pays twice for the same error.

**Worth recording:** the first fix matched only on `http 401`, and the test
caught that OpenAI's actual message — "Incorrect API key provided" — contains no
status at all. Status codes and wording are both checked now, and both providers'
real messages are in the test.

---

## D6 · `docs/launch-audit.md` is rewritten and cannot go stale again

**The question.** The previous edition sat at the top of the repository for six
weeks and five releases describing version 0.4.0, and `README.md` cited it as
the methodology behind the project's honesty. It rotted because nothing read
it: the claims gate covered three filenames.

**The decision.** Rewrite it against the current tree, and make its
version-shaped facts re-derived by `tests/docs_consistency.rs` on every build.

**Why this is a decision and not a chore.** A hand-maintained status document
will always drift. The only question is whether the drift is noticed. Now it
is, by the build — and during the rewrite it immediately caught two wrong
numbers I had typed by hand, which is the argument in miniature.

---

## The standing rule these decisions follow

When a choice was available between *building*, *documenting*, and *deleting*,
it was made explicitly and written down. "Off by default and never mentioned" is
the only option that is not on the list, because it is the one that produces a
product which is not what it says it is while every document claims otherwise.

Where something is genuinely a limitation rather than a decision, it is in
`docs/launch-audit.md` under "What is thinner than the documentation implies",
and in the handover starter's `HONESTY.md`. Both are checked by
`tests/claims.rs`; neither can be deleted quietly.
