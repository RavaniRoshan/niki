# NIKI — Progress Ledger

Append an entry after every step. Re-read DESIGN.md, DECISIONS.md, CHECKLIST.md
at each phase start and after any context compaction.

## 2026-10-07 — Phase 0 (DESIGN)
- Read existing tree; confirmed a prior Go rebuild exists (module
  github.com/RavaniRoshan/niki, bubbletea v1 TUI, engine/tools/mcp/skills/
  session packages). That code will be superseded/migrated toward the spec
  layout (internal/core, internal/llm, internal/contextwin) in P1.
- Wrote docs/DESIGN.md: package graph, SQ/EQ runtime, boot pipeline +
  readiness contract, agent loop + tool model, MCP/skills/config,
  permissions/sandbox, TUI, performance contract, testing strategy.
- Wrote docs/DECISIONS.md with recommended defaults for the five
  `<decisions>` items + recorded choices (module path, engine migration,
  fixture build tag, golangci-lint).
- Wrote docs/CHECKLIST.md with every row UNVERIFIED.
- No application code changed. git status to be committed.

## Marking system (for this task)
- This file is the chronological ledger; docs/CHECKLIST.md holds per-row
  conformance state; docs/DESIGN.md is the architecture; docs/DECISIONS.md is
  the decision log. Do not track state anywhere else.
