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
- **`:free` models are upstream-rate-limited.** `qwen/qwen3.8-27b:free` and
  `inclusionai/ling-3.0-flash-sante:free` return 429
  *"temporarily rate-limited upstream … shared_pool"*, intermittently, on a
  shared pool. `stealth/space-bunny-alpha` answered 3/3 attempts and is what
  the live run used.

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
