# Show HN Post Draft

## Title
Show HN: Niki – independent AI agents that review each other's code

[Post this from an established account with karma history]

---

## Body

**GitHub:** https://github.com/RavaniRoshan/niki
**Try it:** (demo video/GIF below)

I built Niki because I was tired of AI coding agents that write code nobody can trust. Claude Code writes great diffs, Cursor generates whole files, but the output is always "trust me, I got this." When the code touches production, you still need to review every line.

Niki takes a different approach: **a team of role-isolated agents that review each other.**

### How it works

You give Niki a task: "Add a health endpoint to the API." Niki runs four agents, each in its own sandbox:

1. **Planner** — reads your codebase, breaks the task into steps, writes a plan artifact
2. **Coder** — reads the plan, writes the code in a git worktree, produces a diff artifact
3. **Tester** — reads the diff, writes tests that cover edge cases
4. **Reviewer** — reads the diff + tests, catches mistakes, checks security

The key insight: each agent only sees artifacts from the previous agent, not the internal state. The Reviewer can't see the Coder's "thinking" — only the final diff. If the Coder tried to be sneaky (say, hiding a backdoor in a comment that gets stripped by obfuscation), the Reviewer catches it because it's reviewing the artifact, not the process.

### What's different

- **No shared memory between agents** — prevents the "I'll fix it in the next message" problem where agents cover for each other's mistakes
- **Adversarial review** — a Red agent probes the Coder's diff *before* the Reviewer. The Reviewer must reconcile each finding
- **Hermetic by default** — every agent runs in a Docker/Podman container with dropped capabilities and a command deny-list (`git push`, `rm -rf`, `curl|sh` are blocked)
- **Output is always a git branch** — you review the diff in your normal workflow, not in a chat window

### Honest limitations

- **Chat UI is viewer-only today** — the TUI shows the live pipeline but doesn't accept chat input. You interact via `niki run "task"`.
- **Worktree backend runs on host** — use `--backend docker` (default) for untrusted code
- **Sessions/MCP/undo are on the roadmap** — the config sections exist but are not wired yet (we warn at load time)
- **SWE-bench scores pending** — we're working on the eval methodology. No headline claims yet.

### Try it

```bash
cargo install niki
export ANTHROPIC_API_KEY=your_key
niki run "Add health endpoint to src/api.rs"
```

(Include 60s demo GIF showing install → run → git branch with diff)

**What I'd love feedback on:**
- Is the isolated-agent approach compelling vs. single-agent-in-chat?
- What agent roles would you want to add/remove?
- How should sessions/undo work? (We're building this next.)

I'll be here all day to answer questions.

---

## Notes for launch

- **Account**: Must be from an established account (karma history required for Show HN)
- **Timing**: Post Wednesday or Thursday 7-10 AM PST for best engagement
- **Response**: Reply to every top-level comment within ~2 hours during launch window
- **No marketing language**: No "groundbreaking," "revolutionary," "best-in-class"
- **No LLM detection**: This post was hand-written (per March 2026 Show HN rules)
