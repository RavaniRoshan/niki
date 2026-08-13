# NIKI Benchmarks & Honest Evaluation

NIKI's thesis is **"proof, not promises"**: every run writes a `report.md` plus per-agent
`artifacts/*.json` capturing exactly what each agent decided and why. That audit trail is the
real benchmark — it is reproducible and inspectable by you, not a number we print.

## What we measure (and ship)
- **Per-agent cost & latency** — real token usage from provider APIs, persisted per run
  (`niki status` / `niki report`).
- **Revision rounds** — how many times the Reviewer bounced work back to the Coder before approval.
- **Verdict** — Approved / Changes requested / Forced-complete, with the reviewer's scored
  reasoning (correctness / quality / coverage).
- **Hermetic proof** — NIKI asserts that, for the default container backend, your working tree
  was never mutated mid-run. This is enforced, not claimed (security audit S9, `strict` mode).

## What we do NOT claim
We do **not** publish head-to-head SWE-bench / Terminal-Bench scores for NIKI. Reasons, stated
plainly:
1. NIKI is **BYOK and model-agnostic** — its output quality is a function of the models *you*
   assign per role (e.g. a strong Planner/Reviewer, a cheap Tester). Any single number would
   reflect your model choice, not NIKI.
2. Benchmark scores are noisy and frequently contested in 2026 (several vendor figures are
   reported on each maker's own tuned harness; independent scaffolds cluster far lower). We
   won't add to that noise with a number we can't make reproducible for *your* configuration.
3. The honest differentiator is **independence + hermetic + auditable**, not raw benchmark points.

## How to evaluate NIKI yourself (reproducible)
```bash
niki init                                   # pull image, write config, validate key
niki run "Add a GET /health endpoint returning {status:'ok'}" --project ./your-repo
niki report <id>                            # full report + artifacts
niki dashboard <id>                          # static HTML diff + annotations
```
Compare the `report.md` and `artifacts/*.json` across models by editing `[agents]` in
`niki.toml` — e.g. swap the Reviewer to a stronger model and watch the verdict reasoning change.
That A/B is the benchmark that matters for *your* workflow.

## Cost routing (a real, measured win)
Because you can assign a cheap model to the Tester and a strong model to the Planner/Reviewer,
NIKI's per-run cost is dominated by the roles you choose. `niki recommend` prints per-role
cost/quality tradeoffs from your own run history. This per-task model routing is the practical
"better per dollar" lever competitors mostly lack.
