# NIKI — Product Hunt Launch Checklist

Status legend: [x] done · [ ] open. Do not schedule until every Critical is [x].

## Critical (launch blockers)

- [ ] **Social proof seeding.** 50+ GitHub stars, 5+ real-user quotes with
  `niki report` screenshots. Channels: r/rust, r/ChatGPTCoding, Hacker News
  "Show HN" dry run, 2–3 Discord/Slack dev communities. The Show HN run
  doubles as objection-handling rehearsal for PH comments.
- [ ] **Zero-friction path verified by 3 outsiders.** Recruit three devs who
  have never seen NIKI; watch (don't help) them do install → `niki init` →
  `niki run --backend worktree` with Ollama. Fix every stall. Target: first
  branch in <10 min including `ollama pull`.
- [ ] **Launch-day staffing.** Maker + 1 helper in comments all day (first
  8 hours decide ranking). Pre-write answers for: "how is this different from
  Devin/Claude Code?", "what does it cost?", "does it work on Windows?",
  "what if the reviewer is wrong?"

## Major (week-before)

- [x] README roadmap current (v0.7.0), hero alt text fixed, cost-per-task line added.
- [x] PH assets: `assets/social/ph-thumbnail.png` (240×240), `ph-gallery-run.gif` (2.1MB), gallery order in `first-comment.md`.
- [ ] **Thumbnail sanity render.** Upload `ph-thumbnail.png` to a PH draft and
  check it at 240px and 48px (logo legibility at favicon size).
- [ ] **Pricing section.** PH page must say in one line: "Free & open source
  (Apache-2.0). You pay only your LLM provider — ~$0.01/task on Sonnet, $0 on
  Ollama." Link the README cost note.
- [ ] **Hunter vs self-post decision.** Self-post keeps the maker badge and
  first comment; a hunter with devtool audience adds reach. Decide 1 week out.
- [ ] **Timing.** Launch Tue–Thu, 00:01 PST. No competing big-launch days
  (check PH upcoming + major conference keynotes).

## Minor (polish)

- [ ] Record a 60s vertical cut of `demo.mp4` for X/LinkedIn announcements.
- [x] `niki smoke --backend worktree` so the advertised zero-friction path is
  one command after `niki init`.
- [ ] Pin the Show HN / soft-launch feedback issues with `launch-feedback`
  label before PH day, so visitors see a living backlog, not an empty tracker
  (0 issues today reads as "nobody uses this").

## Day-of runbook

1. 00:01 PST — post goes live; maker first comment within 15 min.
2. First 4 hours — reply to every comment; ship any trivial fix live and say so.
3. Hour 6 — post the cost-transparency + "what NIKI is NOT" angles on X/HN (second wave).
4. End of day — thank-you comment with the day's shipped fixes; convert top
   feedback into GitHub issues while watchers are hot.
