# NIKI Product Positioning

> **Date:** 2026-08-17
> **Purpose:** Determine what Niki actually is before redesigning the marketing.

---

## Core Exercise

> **Niki is the open-source multi-agent coding pipeline that plans, codes, tests, and reviews — then hands you a verified git branch.**

This describes an actual Niki capability. No empty phrases. No "next-generation AI."

---

## Mental Category Matrix

| Product | Mental Category | One-Liner |
|---|---|---|
| **Claude Code** | AI coding agent | Single agent, conversation-based, you steer it |
| **Cursor** | AI-native IDE | Editor with AI autocomplete and chat |
| **Codex** | Agentic CLI | Single agent, sandboxed, one-shot output |
| **OpenManus** | General-purpose OSS agent | Single agent, tool-using, general tasks |
| **OpenMausBot** | Bot team with computers | Multiple bots, computer-use focus |
| **Grok** | AI assistant with coding | Chat-based, general purpose |
| **Aider** | Pair-programming agent | Single agent, git-native, conversation |
| **Niki** | **Multi-agent coding pipeline** | Four independent agents → verified branch |

---

## Why "Multi-Agent Coding Pipeline" Works

### 1. It's true
Niki literally runs four agents (Planner → Coder → Tester → Reviewer) in sequence. Each has its own prompt, model, and context. They exchange only typed artifacts.

### 2. It's differentiated
- Claude Code = one agent
- Codex = one agent
- Cursor = one agent
- OpenManus = one agent
- Niki = four agents, each independent, in a hermetic sandbox

No other open-source coding tool has this architecture.

### 3. It's concrete
"Plans, codes, tests, and reviews" describes exactly what happens. "Verified git branch" describes exactly what you get.

### 4. It's memorable
"Multi-agent coding pipeline" is a new category. It doesn't compete with "AI coding agent" — it's a different thing entirely.

---

## Positioning Statement (Final)

**Primary:**
> Niki is the open-source multi-agent coding pipeline that plans, codes, tests, and reviews — then hands you a verified git branch.

**Short (for headers/titles):**
> One sentence in, a verified pull request out.

**Technical:**
> Four independent LLM agents run in isolated sandboxes and produce a reviewable `niki/<id>` branch with full audit trail.

**Anti-positioning (what Niki is NOT):**
- Not a single agent (that's Claude Code, Codex, Aider)
- Not an IDE (that's Cursor)
- Not a chat interface (that's Claude, ChatGPT)
- Not a code generator (that's Copilot)
- Not magic on huge codebases (like every coding agent)

---

## Competitive Differentiation

### vs Claude Code
| Dimension | Claude Code | Niki |
|---|---|---|
| Agent count | 1 | 4 independent |
| Context sharing | Full conversation | Artifact-only (no shared context) |
| Output | Inline edits | Git branch + report |
| Sandbox | OS sandbox | Container sandbox |
| Revision loop | You steer | Reviewer bounces back to Coder |
| Provider | Anthropic only | BYOK, multi-provider mixing |
| Audit | Conversation history | report.md + artifacts/*.json |

### vs Codex
| Dimension | Codex | Niki |
|---|---|---|
| Agent count | 1 | 4 independent |
| Output | One-shot diff | Git branch + report |
| Sandbox | Ephemeral container | Persistent container + worktree option |
| Model | OpenAI only | BYOK, multi-provider |
| Revisions | None | Reviewer-driven loop |
| Audit | Limited | Full audit trail |

### vs Cursor
| Dimension | Cursor | Niki |
|---|---|---|
| Interface | IDE | CLI + TUI |
| Agent count | 1 (autocomplete) | 4 (pipeline) |
| Output | Inline edits | Git branch |
| Scope | File-level edits | Multi-file tasks |
| Review | Manual | Automated (Reviewer agent) |

### vs OpenManus
| Dimension | OpenManus | Niki |
|---|---|---|
| Scope | General-purpose | Coding-specific |
| Agent count | 1 | 4 |
| Sandbox | Browser/computer | Container |
| Output | Varies | Git branch |
| Focus | Tasks | Code changes |

---

## One-Line Story Variations

Choose one for each context:

| Context | Story |
|---|---|
| README hero | "Turn a sentence into a verified pull request." |
| Landing page | "One sentence in, a verified pull request out." |
| Twitter/social | "Four AI agents debate your code change so you don't have to." |
| HN comment | "Independent Planner→Coder→Tester→Reviewer in isolated containers, output is a real git branch." |
| Package manager | "Multi-agent coding pipeline: hermetic, auditable, BYOK." |
| Pitch deck | "Niki is the multi-agent coding pipeline. Four independent agents run in hermetic sandboxes and hand you a verified pull request branch with a full audit trail." |

---

## Positioning Validation

Checklist:
- [x] Describes an actual Niki capability (not empty hype)
- [x] Differentiated from every competitor (no other 4-agent coding pipeline)
- [x] Concrete (you know what you get)
- [x] Memorable ("multi-agent coding pipeline" is a new category)
- [x] True (verified against code: src/agents/, src/orchestrator/pipeline.rs, src/sandbox/)
- [x] Backed by proof (claims-audit.md maps each claim to code)
