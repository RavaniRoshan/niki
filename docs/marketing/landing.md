# NIKI — Landing Page Content

## Headline
Independent AI agents that can't influence each other. Output: a reviewable git branch.

## Subheadline
NIKI runs a team of role-isolated agents (Planner → Coder → Tester → Reviewer) in hermetic containers. Each agent works independently — no shared memory, no backchannel. The result is code you can actually review, not a black box.

## Hero Demo
[Embed demo.gif or demo.mp4 — 60s "0 → running" sequence]

```bash
brew install niki
export ANTHROPIC_API_KEY=your_key
niki run "Add a health endpoint to the API"
# → opens a reviewable git branch with diff + report
```

## Feature bullets

### Three independent agents, not one
Planner, Coder, Tester, Reviewer run as separate agents — each in its own sandbox. They exchange artifacts, not state. The Reviewer can actually catch what the Coder misses.

### Hermetic by default
Docker/Podman containers with dropped capabilities, read-only mounts, and a command deny-list enforced for every role. Your codebase is safe: `git push`, `rm -rf`, `curl|sh` are blocked by policy.

### Adversarial review built-in
A Red agent probes the Coder's diff before the Reviewer runs. The Reviewer must reconcile each finding. This is what "independent review" actually means — not a rubber stamp.

### Live pipeline viewer
Watch the multi-agent pipeline run in real-time with a terminal TUI: per-stage logs, cost tracking, live diff preview, and artifact inspection.

## What it's NOT

- **Not a chat agent.** NIKI is a pipeline that produces reviewable changes. The TUI lets you chat with agents during a run, or run headless via `niki run`.
- **Not hermetic everywhere.** The `--backend worktree` option runs on your host filesystem for speed. Use `docker` for untrusted code.
- **Not a monorepo copilot.** NIKI excels at discrete tasks (add feature, fix bug, refactor) — not infinite auto-commits.
- **Not magic.** It still needs your API key. It still makes mistakes. But the artifacts make them visible.

## Waitlist
Stay updated on new features (sessions, MCP, headless CI mode):

- [ ] Email (1-2 emails/month, unsubscribe anytime)

## Install
```bash
# Homebrew
brew install niki

# Scoop (Windows)
scoop install niki

# Winget (Windows)
winget install RavaniRoshan.niki

# Or download a binary
# https://github.com/RavaniRoshan/niki/releases/latest
```

## Links
- [GitHub](https://github.com/RavaniRoshan/niki) · [Install Guide](docs/install.md) · [Docs](docs/) · [Security Policy](SECURITY.md)
