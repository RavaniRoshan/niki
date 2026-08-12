# Product Hunt Launch Assets

## Tagline (≤60 chars)
Four AI agents that review each other — output: a git branch

## Thumbnail (240×240)
Use the NIKI logo (assets/logo.svg) on a dark background.

## Gallery images (1270×760) — 3 images

### Image 1: Hero run
- Screenshot of the TUI showing a live pipeline run
- Show all four stages (Planner, Coder, Tester, Reviewer) with status indicators
- Include cost tracking and timing

### Image 2: TUI live pipeline viewer
- Full-screen TUI showing agent logs
- Show the diff preview panel on the right
- Highlight the command palette

### Image 3: Reviewable output
- Screenshot of the git branch with the diff
- Show the `.niki/report.md` artifact
- Show the evaluation results

## YouTube video (required for PH)
- 60-90 second "0 → running" demo
- Silent with annotations (terminal video best practice)
- Embed the generated demo.mp4

## Product Hunt description (≤260 chars)
Niki runs four role-isolated AI agents in hermetic Docker containers. Each agent reviews the artifacts of the previous one — no shared state, no backchannels. Output is always a reviewable git branch. Open-source, Apache-2.0.

## Tags
- [x] Developer Tools
- [ ] Open Source
- [ ] AI
- [ ] Developer First
- [ ] Privacy

## First comment (maker comment — write this on launch day)

**Features:**
- Multi-agent pipeline: Planner → Coder → Tester → Reviewer
- Hermetic Docker/Podman sandboxes with command deny-list enforcement
- Adversarial Red agent probes diffs before review
- Live TUI pipeline viewer with cost tracking
- Output: always a git branch you can review

**Who it's for:**
- Developers who want AI to write code but still want to review it
- Teams that need hermetic, auditable agent workflows
- Anyone tired of AI agents that "hallucinate" and hide it

**Story:**
I built this after getting burned by AI coding agents that produce output I couldn't trust. The "four agents review each other" pattern is inspired by how human code review works — you don't let the same person write and approve code.

**Ask for feedback:**
- Does the isolated-agent approach feel compelling?
- What agent roles would you add?
- How should sessions/undo work?

---

## Launch timing
- Schedule: 12:01 AM PST on launch day
- Submit via: https://www.producthunt.com/products/niki
- Need PH account with karma
