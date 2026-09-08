# NIKI — Product Hunt Maker's First Comment (draft)

> Hey PH! I'm Roshan, maker of NIKI. 👋
>
> **The problem.** Every AI coding tool I used — Cursor, Copilot, Devin-style
> agents — is *one* agent in *one* long conversation. That produces the same
> three failures on repeat: it never challenges its own assumptions
> (confirmation bias), quality rots as context grows (context drift), and I
> end up babysitting the thing I bought to save time (the babysitting tax).
>
> **What NIKI does differently.** One sentence in, a verified pull request out.
> Four *independent* agents — Planner → Coder → Tester → Reviewer — each in its
> own sandbox, sharing no history, exchanging only typed artifacts. The
> Reviewer can bounce work back to the Coder. You get a `niki/<id>` git branch
> with a real commit, a diff, and a full audit trail (`report.md`,
> `changes.patch`, per-agent JSON). Nothing lands on `main` until you say so.
>
> **Three things I refused to compromise on:**
> 1. **Your working tree is never touched mid-run.** Hermetic Podman/Docker
>    sandboxes (or a git-worktree backend that needs no container at all).
> 2. **Honest costs.** Every run reports exact tokens and dollars, a spend cap
>    aborts past your ceiling, and unpriced models warn instead of pretending
>    to cost $0.00. A small task is ~$0.01 on Sonnet, $0.00 on local Ollama.
> 3. **No telemetry, BYOK only.** Your code never trains anything. 12
>    providers, mix-and-match per agent.
>
> **Try it in ~2 minutes** (no container, no API key — just Ollama):
> `niki init` → `niki run "Add a /health endpoint" --project ./my-app --backend worktree`
>
> **What NIKI is NOT:** not a replacement for your judgment (you review the
> branch), not magic on giant vague codebases (it shines on clear specs with
> testable outcomes). The `docs/launch-audit.md` in the repo lists what we
> verified *and* what we didn't — I'd rather you read that than marketing.
>
> Free and open source (Apache-2.0). Roast it, break it, file issues — I'll be
> in the comments all day. 🦀

## Tagline options (≤60 chars)

1. One sentence in, a verified pull request out. *(current, 47 chars)*
2. Four AI agents debate; you review the branch. *(46 chars)*
3. Stop babysitting your AI coding agent. *(38 chars)*

## Gallery order

1. `assets/demo.mp4` (80s, 1MB) — full run, primary video
2. `assets/social/ph-gallery-run.gif` (2.1MB) — autoplays in feed
3. `assets/screenshots/run-pipeline.gif` — pipeline close-up
4. `assets/screenshots/cost.png` — honest cost report
5. `assets/screenshots/diff.png` — the reviewable branch output
