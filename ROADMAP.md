# ROADMAP — what was consciously deferred, and why

Everything here was found, evidenced, and left. Nothing here is a surprise; the
audit is in `plans/namor-obsidian-jay-garrick.md` §2. Ordered by what a user is
most likely to hit next, not by how interesting the fix is.

The rule this programme used: a slice is DO NOW if the core promise cannot hold
without it, or if it makes another slice provable. Everything else is here.

---

## 1 · Navigation and the dead controls (P2)

**The largest quality-of-life cluster, and the one left undone.** Not deferred
for risk — deferred because nothing was blocked on it and the router order is
where regressions bite.

| # | Defect | Evidence |
|---|---|---|
| 1.1 | `↑ ↓ j k` move `state.page_selection`, which **no renderer reads**. Every list page keeps its own private cursor, so their `j/k` arms are unreachable. **Dead on 10 of 14 pages** while six page footers advertise `[j/k]` and the status bar says `↑↓ select`. | `nav.rs:149-158`, `tui.rs:1141-1152`; grep shows only declaration, writes and tests |
| 1.2 | `h` and `l` mean prev/next page on every sub-page, shadowing History and TestLog. `[`/`]` on Diff navigate pages instead of hunks. | `nav.rs:80-85` before `global_page_jump` |
| 1.3 | `Tab` is claimed by three controls; the status bar wins, so Config's "next field" and Agents' "next tab" are both dead. | `tui.rs:1036/1418` vs `pages/config.rs:327`, `pages/agents.rs:307` |
| 1.4 | `q` quits the app from every sub-page, before the router, so all 11 pages' own `q → Run` handlers are unreachable. | `nav.rs:88`; `tui.rs:1159, 1467` |
| 1.5 | Fleet's footer advertises `P R K V`; **3 of 7 controls do nothing**. Session advertises `P` and `R`; both dead. | `pages/fleet.rs:150` vs `tui.rs:1843-1858` |
| 1.6 | `?` never reaches the Help page — it is consumed as `ToggleHelp` first. The only route is `Ctrl+P → help`, and *that* lands on a second, stale help page that says `[t] theme` when the key is `ctrl+t`. | `tui.rs:257-259`, `command_palette.rs:98-102`, `pages/help.rs:54` |
| 1.7 | Digit jumps cover 9 of 14 pages. | `nav.rs:91-94` |
| 1.8 | Session is 4/7 placeholder tabs with a permanently empty Conversation — the live conversation is in `state.chat_log`, which the page never reads. | `pages/session.rs:12-20, 59, 71, 145-149` |
| 1.9 | `docs/tui/key-matrix.md:28-33` tells a maintainer *not to fix* the theme-key divergence that `tui.rs:1522-1527` already fixed. | as cited |

**Exit criterion:** every key in `BINDING_TABLE` and every footer hint maps to
an implemented handler, asserted by a test that enumerates the table rather
than a hand-copied list.

## 2 · Chat ↔ pipeline depth (P3)

Blocked behind nothing now that T3 landed, but it is large and it should be
built on a stable router (§1).

- Tool cards in the transcript, with arguments and results. The renderer
  (`components/tool_card.rs`, `tool_detail.rs`, the Enter hit-test) is fully
  built and unreachable, because the chat sends `tools: None`.
- **Branch checkout from the TUI.** `grep 'checkout' src/display/` → one hit,
  and it is a printed hint on the *non*-TUI path. The last step of the core
  journey still needs a second terminal.
- History `Enter` now loads the task directory (T5) but does not switch to the
  Diff page for it.
- The permission modal is structurally unreachable in chat: `cli/chat.rs`
  creates a channel whose sender is never given to `create_sandbox`, so
  `DisplayEvent::PermissionRequest` can never be sent.
- `ask_user` / `approval` return "cannot ask" in any TUI run, because
  `TUI_OWNS_STDIN` makes `is_interactive_stdin()` false. No modal exists.
- The permission badge is still cosmetic: `state.permission_mode` is read only
  to pick the badge label, and never reaches `ToolContext`, `PermissionChecker`
  or the sandbox. `--permission-mode` does not gate the sandbox path either.
- Permission prompts still auto-deny after **5 seconds with no countdown**, and
  the message says *"Command denied by user"* — naming a user who did nothing.
  (`worktree.rs:438-444`, `docker.rs:611-617`)

## 3 · Loop unification (P4)

`run_tui` and `run_chat` are two hand-ordered event ladders that already
disagree on six behaviours. One renders a failure; the other discards the
error (fixed in T9 — but the duplication remains). `run_tui` loads
`Path::new(".")` for its theme while `run_chat` loads `project_path`, so
`niki run --project X --tui` renders against a different config than the
pipeline uses. Best done after §1 stabilises the router.

## 4 · Security (P5)

| # | Defect | Evidence | Why deferred |
|---|---|---|---|
| 4.1 | ~~Arbitrary host file write from model output.~~ **DONE in batch 2 — see below.** | `sandbox/worktree.rs:243-252` | Closed by routing the create branch through `resolve_tool_path`, the guard every read/write/edit/patch tool already uses. It rejects `..`, canonicalises through a symlinked parent, and refuses anything that does not resolve inside the root. Two canaries: `a_create_edit_cannot_write_outside_the_worktree` and `the_path_guard_still_allows_an_ordinary_nested_create` — the second because a guard that compared uncanonicalised prefixes would refuse every real create on macOS, where `/var` is a symlink to `/private/var`. |
| 4.2 | `niki doctor` reports a hard **Pass** for secret redaction in "logs, reports, artifacts". `redact_secrets` is called only from four provider error-body sites; `report.md`, `changes.patch` and `artifacts/*.json` are written unredacted. | `cli/doctor.rs:416-422`; `output/report.rs:622-700` | Must first narrow the `[A-Za-z0-9+/]{40,}` pattern, which currently destroys any 40-char alphanumeric run — a long diff line, a git SHA. Applying it as-is would make reports worse. |
| 4.3 | `git` tool has no `check_command_policy` and no timeout; `agent_access: &[]` makes it available to every role, and `Ask` auto-approves headless. `git {"subcommand":"push"}` from the Reviewer passes. | `runtime/tools.rs:2637-2697` vs the `bash` tool's policy call at `:1531-1534` | Belongs with 4.1 and the worktree isolation work. |
| 4.4 | Docker backend **silently** auto-approves every `Ask` (worktree `eprintln!`s, Docker only `tracing::warn!`s, and `main.rs:114` sets an ERROR-only filter). The comment three lines above the code says *"Loud by design: silent auto-approval is how agents end up running `curl | sh` in CI."* | `docker.rs:594-609` | Small and isolated; should have been DO NOW, and the honest reason it was not is that T9 was already large. |
| 4.5 | Three unsound advisories pass by policy, including `git2 0.20.4` — a direct dependency — carrying `RUSTSEC-2026-0184` (undefined behaviour). `deny.toml` sets `unsound = "none"`; nothing is acknowledged or pinned. `[bans]` and `[sources]` gate nothing. | `deny.toml:11-20, 41-52`; `Cargo.toml:46` | Needs a real dependency bump, not a config flip. `cargo deny check` currently passes and G5 reports that honestly. |

## 5 · Reliability (P5)

- **The 120 s client timeout is a *total* deadline whose error matches no retry
  classifier.** It surfaces as `Kind::Body` → *"request or response body error"*,
  which is in none of the transient lists, so a stage that runs past 120 s dies
  with a message that does not mention time and is not retried. At the default
  8192-token stage budget that needs >68 tok/s — a 3B model on CPU, i.e. exactly
  the README's zero-setup path. (`provider.rs:87` vs `agents/mod.rs:141-147, 666-672`)
- `anthropic::stream` bypasses `send_request`, so it gets no HTTP retry; the
  agent-level matcher catches 429/503 but not 500/502. The comment at
  `anthropic.rs:96-98` claims "Retries on 429/5xx" and is true of `complete()`
  only.
- Google never emits `StreamChunk::Finish`, so a truncated Google response is
  misdiagnosed as "model output was not a usable artifact" — the exact failure
  that enum exists to prevent on the pipeline path.
- A mid-pipeline failure destroys the Coder's work: all deliverable assembly
  happens after `execute_pipeline` returns, and `Drop for WorktreeSandbox`
  removes the worktree. A Tester OOM means no branch, no patch, no report. The
  per-stage checkpoint already holds the Coder's `CodeDiff`; nothing reads it
  back. (`run.rs:1010-1075`, `worktree.rs:520-536`, `pipeline.rs:3410-3418`)
- `niki resume` restores state and exits: *"Ready for continuation."* No code
  path re-enters the pipeline from a checkpoint. Checkpoints are also written
  with a bare `fs::write` where an atomic writer already exists.
- The Coder's tool loop has no transport retry and swallows its errors with
  `.await.ok()?` — a network failure silently degrades to the one-shot fallback
  with no message. (`pipeline.rs:1606-1607`)
- ~~`git diff`'s exit status is unchecked in all three diff producers.~~
  **DONE in batch 2.** All three now check it, and `working_tree_diff_scoped`
  returns `Result` rather than `String` — the signature was the defect, because
  a caller could not tell the two cases apart. The sandbox producers warn
  rather than abort, so a stale index lock does not destroy a run whose work is
  already on disk. Canary: `a_failed_diff_is_reported_rather_than_looking_like_
  no_changes`, which reproduces the real cause — a stale `.git/index.lock`,
  what two concurrent runs produce.
- ~~A signal-killed child is reported as **exit 0**.~~ **DONE in batch 2.**
  Now `128 + signal`, the convention a shell uses, so an OOM kill reads as 137
  and a segfault as 139. A missing status entirely stays -1, because "the
  process never ran" is a different fact from "the kernel killed it". Canary:
  `a_signalled_child_is_not_reported_as_exit_zero`.

## 6 · Coverage and hygiene (P5)

- 19 of 28 CLI commands have no test at either level. `src/llm/google.rs` has
  **zero** tests. `sandbox/worktree.rs` (853 lines, the recommended backend) and
  `sandbox/docker.rs` (634 lines, the default) have no unit tests.
- `tests/multi_provider.rs` — 8 of 26 assert `create_provider(X).provider_name()
  == X` where `provider_name` is a struct field holding `X`; two are *named*
  endpoint tests and never read the endpoint.
- `tests/docker_resource_caps.rs` — 8 tests on one string parser, in a file
  named after container resource caps. `cap_drop: ALL`, `pids_limit`,
  `network_mode: "none"` and `readonly_rootfs` are asserted by nothing.
- `tests/tui_navigation.rs` drives `PageRouter::handle_key`, a fallback the
  real chat loop reaches only for a handful of keys, and
  `run_page_ignores_navigation_hotkeys` asserts behaviour the shipped binary
  contradicts. Two `page_router_render_current_*` tests draw every page and
  assert nothing.
- 697 lines of dead code: `src/errors.rs` (70), `src/control_plane/` (294),
  `src/persistence/` (333) — all `pub`, so `dead_code` is silent.
- `.niki-worktrees/` is not git-ignored and passes `is_publishable_path`, so
  after a SIGKILL the user's next `git add -A` commits a whole sandbox copy.
  A fixed temp patch path (`git.rs:158`) also collides across concurrent runs.
- `niki acp` and `niki goal` run the pipeline and then destroy the Coder's
  work: they call `execute_pipeline`, never `deliver`. Fixed for the chat in
  T3a/T3; **not fixed for these two**. `acp/server.rs:149` also stores the diff
  *text* in a field named `branch`.
- MCP is a documented Advanced feature that is a stub: `McpManager::call_tool`
  has zero callers outside `src/mcp/`, stdio children leak, and
  `web_fetch` is registered to the model with a permanently empty allowlist.
  `web_search` returns `ToolStatus::Success` with *"not yet wired"*. **Flagged,
  not decided** — either wire them or remove the README rows.
- Two `cargo clippy` items the gate does not yet check: `tui_perf.rs` asserts
  wall-clock budgets with 2× headroom, so it can only fail on a machine twice
  as slow as the calibration box; and `tests/headless_tui.py` has two
  unconditional `pytest.skip`s for the paths that need a live model.

## 7 · Explicitly killed

- **Unifying the two event loops before stabilising the router.** A refactor
  nobody would notice missing, and it would have doubled the blast radius of
  every navigation fix in §1.
- **Broadening the CLI surface.** The competitive read is unambiguous: a git
  branch is the *category* output shape in 2026, not a differentiator — Devin
  ships draft PRs, Aider commits to your branch, OpenHands emits patches.
  NIKI's defensible wedge is the **typed artifact contract plus the named
  branch plus the sandbox, together**, and the genuinely-unclaimed thing is the
  per-run on-disk decision trail. Widening the surface dilutes that.
- **A `/run` that infers intent from every message.** A chat that silently
  starts a four-agent run — spending money and writing to git — on a message
  the user meant as a question is a chat that does things nobody asked for.
