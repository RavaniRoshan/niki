# NIKI Foundation — EVENT MAP

Extracted from `DESIGN.md` §4 so the design note stays inside its 70-line cap. Every source
below was verified by reading the file at the cited line; nothing here is inferred.

`src/event/` is **dead** — 24 of 26 variants have no producer and nothing calls `subscribe()`.
`StoreEvent` (`src/display/state.rs:2188`) is dead too (tests only). The only live model is
`DisplayEvent` (`src/display/tui.rs:48-238`), so `RuntimeEvent` is a new, narrower, UI-only enum:

| RuntimeEvent | Verified source |
| --- | --- |
| `RunStarted` / `RunEnded` | `src/cli/run.rs:709` / `DisplayEvent::Final` `tui.rs:116` |
| `TurnStarted/TurnDelta/TurnEnded` | `ChatMessage/ChatDelta/ChatFinished` `tui.rs:126/144/167` |
| `StageStarted/Token/Done/Failed` | `agent_start/stream_token/agent_done/agent_failed` `agent_stream.rs:273/319/350/460` |
| `ToolCall/Result/Failed` | emitted **inside** the tool loop: `src/runtime/tools.rs:4057,4146` |
| `ApprovalRequested/Resolved` | `tools.rs:715`, `sandbox/worktree.rs:548`, `sandbox/docker.rs:683` |
| `Notice/BranchReady/DiffReady/VerdictReady` | `notice` `agent_stream.rs:688`; `BranchName` `tui.rs:123`; `DiffContent` `tui.rs:89` |
| `ContextUsage` / `CostUpdated` | `StageTotals` `tui.rs:193` → `src/cost.rs:173` |

A row exists only for a real event. `StageToken` is the only reasoning source; raw private
chain-of-thought is never rendered. Every message carries a `trace_id`.

