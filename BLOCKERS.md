# BLOCKERS

Things this programme cannot do for itself.

## B6 · The real-model key (2026-10-01)

An OpenRouter key was supplied for live pipeline testing. Two facts about it,
both measured:

- **`stepfun/step-3.7-flash` and `step-3.5-flash` are unreachable on it.**
  `{"error":{"message":"Insufficient credits. This account never purchased
  credits."}}` The key is valid — `/api/v1/key` returns it as a
  non-management key with `limit: null` — but those models are not free-tier
  and the account has `total_credits: 0`. The model named for this work needs
  a purchase.
- **`:free` models are upstream-rate-limited, individually.** On this key,
  measured: `qwen/qwen3.8-27b:free` and
  `inclusionai/ling-3.0-flash-sante:free` return 429 *"temporarily
  rate-limited upstream … shared_pool"* intermittently, while
  `poolside/laguna-s-2.1:free` and `stealth/space-bunny-alpha` answer
  consistently. `poolside/laguna-xs-2.1:free` did not answer. So the free
  pool is usable — one has to find which entries are live rather than assume.

**What each model showed**, because "works" is not the same question twice:

| Model | Result |
|---|---|
| `stealth/space-bunny-alpha` | Full pipeline, **Approved 10/10** end to end (B7-18), after a real revision round. **§9.3 found here** — a Coder that did the work and narrated it instead of submitting; fixed and live-verified against the same model. |
| `poolside/laguna-s-2.1:free` | Answers, but its **Planner emits no conformant artifact** — `Failed to parse artifact JSON: expected value at line 1 column 1`. The run failed in stage one, which is where **§9.4** came from: a failed run reported `No such file or directory (os error 2)` from its own recovery path. |

Two models, two different failures, and the second one is only reachable
because a model too weak to finish a task still fails *loudly and early* —
which is the other half of what a live model is for.

**What this changes.** Live-model behaviour is now *measurable*, and the first
thing measured was a defect no mock reproduced — `ROADMAP.md` §9.3, a Coder
that finishes the work and never submits it. Fixing it needs live runs to
prove, so it is a standing dependency on this key, and on a key that can reach
the model named for the work if that is the one wanted.

**The key is never written to disk.** It is passed through the environment, and
G5's secret scan covers the tree *and* git history on every run before a
commit.

## Live-run recipe

```bash
export OPENROUTER_API_KEY=…            # never on disk
# project: a real git repo, with [docker] backend = "worktree"
[providers.openrouter]
base_url = "https://openrouter.ai/api/v1"
default_model = "stealth/space-bunny-alpha"
# per-agent provider/model, reasoning_effort = "high"
./target/release/niki run '<task>' --backend worktree --quiet --project <dir>
```

## B7 · The classifier's model is the owner's decision (2026-10-01)

§8's third layer is built except for the part that talks to a model. `risk::`
has the `ActionClassifier` trait, the gate, the escalation limits, the
reasoning-blind `ClassifierView`, the input probe and now the hook layer — but
**nothing implements the trait against a real provider**, so the classifier
layer is exercised only by stubs in its own tests.

What is needed, and why it is not a slice I can just take:

1. **Which model answers the safety question.** §8 suggests "a cheap model
   (`claude-sonnet-4` or `gpt-4o-mini`)". NIKI is BYOK, so the classifier cannot
   pick one: it has to come from config, and the default has to be something
   every user has credentials for.
2. **What it costs.** A classifier call per *unlisted* tool call, on top of the
   model's own calls. On this key the cheapest answering models are the `:free`
   ones, which are the same ones B6 records as upstream-rate-limited — so the
   classifier would inherit exactly the flakiness that made B6 necessary.
3. **What happens when it is unavailable.** The gate already fails closed (any
   classifier failure is a deny), which is the right default for safety and the
   wrong one for availability: a user whose classifier provider is rate-limited
   gets a run that denies every unlisted tool and fails after twenty. Failing
   closed should be a *configurable* posture, and the default is a product
   decision, not a safety one.

**Meanwhile**, nothing is wired into the loop — so this layer is currently
inert in the product, exactly like `runtime/compaction.rs` and `ContextStore`
were. That is stated here rather than left to be discovered, and the layer is
not claimed as shipped.

**What I need from the owner:** (a) a model id and provider for the classifier,
or a config key to read it from with no default; (b) whether fail-closed is the
default; (c) whether the classifier is on by default at all, or opt-in.
