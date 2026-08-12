# Social Media Kit

All posts are hand-written (per Show HN rules, LLM-generated posts are penalized).

## X / Twitter thread (primary)

Tweet 1 (hook):
AI coding agents write code nobody can trust. They're chatbots with keyboard access. I built something different: 4 agents that review each other's work in isolated containers. Each sees only the artifacts, not the process.

Tweet 2 (demo):
60-second demo of installing Niki and running a task:
```
cargo install niki
export ANTHROPIC_API_KEY=sk-...
niki run "Add health endpoint"
```
→ Watch 4 agents plan, code, test, review in parallel
→ Output is a git branch with a diff + report

Tweet 3 (differentiator 1 - isolation):
Key design choice: agents exchange typed artifacts, not shared memory. The Reviewer only sees the final diff — not the Coder's "thinking." This prevents agents from covering for each other's mistakes.

Tweet 4 (differentiator 2 - hermetic):
Every agent runs in a Docker/Podman container with:
- Dropped capabilities
- Read-only mounts where possible
- Command deny-list: git push, rm -rf, curl|sh blocked
- Resource limits enforced

Tweet 5 (differentiator 3 - adversarial):
A Red agent probes the Coder's diff *before* the Reviewer. The Reviewer must reconcile each finding. This is what "independent review" actually means — not a rubber stamp.

Tweet 6 (honest limitations):
What Niki ISN'T:
- Not a chat agent (TUI is viewer-only; interact via `niki run "task"`)
- Not hermetic everywhere (--backend worktree runs on host)
- Not magic (still needs your API key, still makes mistakes)

Tweet 7 (CTA):
GitHub: https://github.com/RavaniRoshan/niki
Try: cargo install niki

HN Show thread coming tomorrow. Would love feedback on the isolated-agent approach.

## LinkedIn post

I've been working on Niki, an open-source tool that runs four AI agents in hermetic containers to review each other's code. Each agent sees only the artifacts of the previous one — no shared memory, no backchannels.

The motivation: AI coding agents today are chatbots with keyboard access. The output is "trust me," and you end up reviewing every line anyway. Niki takes the code-review process seriously: the Reviewer can only see the diff, not the Coder's reasoning. A Red agent probes the diff before review. And it all runs in isolated containers.

The honest version: the TUI viewer works, the pipeline produces reviewable git branches, but the chat UI is still wired-only. Sessions, MCP, and undo/redo are on the roadmap. What's shipped today already catches bugs that single-agent workflows miss.

If you work on agentic coding infrastructure, I'd love to swap notes: what's your approach to independent review?

#ai #coding #devops #security #opensource

## dev.to cross-post (tutorial angle)

Title: "Run your first multi-agent code review with Niki (4 agents in Docker)"

Content: A step-by-step tutorial:
1. Install Niki (cargo install / brew / curl)
2. Configure API keys (env vars or keyring)
3. Run a task: `niki run "Add a health endpoint"`
4. Watch the pipeline in the TUI
5. Review the git branch with the diff

Include: code snippets, TUI screenshot, expected output structure.

## Reddit posts (only where already active)

### r/rust
Niki v0.3.0: Four Rust agents reviewing each other's code in Docker containers. Built with tokio + bollard. Each agent is a hermetic container with a command deny-list. The Reviewer only sees the diff artifact — not the Coder's internal state.

GitHub: https://github.com/RavaniRoshan/niki

Looking for feedback on the isolation model and the Rust async runtime usage.

### r/programming (if appropriate)
Same framing as r/rust but more general audience.

## Instagram (social kit)
- Story 1: "What if AI coding agents reviewed EACH OTHER?" — screenshot of Niki architecture diagram
- Story 2: "0 → running in 60 seconds" — cropped demo video
- Story 3: "Not a chatbot with keyboard access" — the 3 differentiators
- Post: carousel of 3 screenshots (TUI, diff review, report)

## Mastodon
Same as X thread but trimmed to 3-4 posts with #AI #Rust #DevTools hashtags.
