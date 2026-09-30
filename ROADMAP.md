# ROADMAP — what was consciously deferred, and why

Everything here was found, evidenced, and left. Nothing here is a surprise; the
audit is in `plans/namor-obsidian-jay-garrick.md` §2. Ordered by what a user is
most likely to hit next, not by how interesting the fix is.

The rule this programme used: a slice is DO NOW if the core promise cannot hold
without it, or if it makes another slice provable. Everything else is here.

---

## 1 · Navigation and the dead controls (P2)

**The largest quality-of-life cluster, and the one being worked now.** Not
deferred for risk — deferred because nothing was blocked on it and the router
order is where regressions bite. Three items closed in batch 3.

| # | Defect | Evidence |
|---|---|---|
| 1.1 | ~~`↑ ↓ j k` move `state.page_selection`, which **no renderer reads**. Every list page keeps its own private cursor, so their `j/k` arms are unreachable. **Dead on 10 of 14 pages** while six page footers advertise `[j/k]`.~~ **DONE in batch 3.** Both event loops now defer to the focused page on `q`, `j` and `k` through one named predicate, `sub_page_owns`. | `nav.rs:149-158`; `tui.rs` (two call sites) | The **up arrows/arrows-down** case is untouched: they still write the unread `page_selection`. They are the same defect with a different key, and folding them in is a follow-up. `page_selection` itself is now written by nothing a user can see, so deleting the field is the honest finish — it touches `AppState`, `nav.rs` and their tests. |
| 1.2 | `h` and `l` mean prev/next page on every sub-page, shadowing History and TestLog. `[`/`]` on Diff navigate pages instead of hunks. | `nav.rs:80-85` before `global_page_jump` |
| 1.3 | `Tab` is claimed by three controls; the status bar wins, so Config's "next field" and Agents' "next tab" are both dead. | `tui.rs:1036/1418` vs `pages/config.rs:327`, `pages/agents.rs:307` |
| 1.4 | ~~`q` quits the app from every sub-page, before the router, so all 11 pages' own `q → Run` handlers are unreachable.~~ **DONE in batch 2** (`71ba200`). The page's handler runs first; a page that declines `q` falls back to the confirm-quit modal, which had been unreachable dead code. Fleet and Session answer `q` for themselves and `continue` before the router, so they got their own arm. Proven by a real pty — `cases/13_subpage_q_goes_back.sh`, which asserts the tmux session still exists, because asserting only that "Run" rendered would pass against an app about to exit. | as cited |
| 1.5 | Fleet's footer advertises `P R K V`; **3 of 7 controls do nothing**. Session advertises `P` and `R`; both dead. | `pages/fleet.rs:150` vs `tui.rs:1843-1858` |
| 1.6 | ~~`?` never reaches the Help page — it is consumed as `ToggleHelp` first. The only route is `Ctrl+P → help`, and *that* lands on a second, stale help page that says `[t] theme` when the key is `ctrl+t`.~~ **DONE in batch 3** (`a50a1b1`) — but by making the two surfaces honest rather than by merging them. | as cited | The Help page's GLOBAL rows are now generated from `BINDING_TABLE`, so the page and the which-key overlay cannot disagree again, and a user's overrides show up in the page. The palette no longer claims `?` reaches Help, because `ToggleHelp` consumes it first. **Merging the two surfaces is the remaining option and it is not done**: `?` stays the quick global reference because that is what the status bar advertises it as, and consolidating would retire `show_help`, which the mouse handlers and their tests depend on. That is a real piece of work, not a five-line edit. |
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
- ~~Permission prompts auto-deny after **5 seconds**, saying
  *"Command denied by user"*.~~ **DONE in batch 2.** A timeout and a refusal
  are now distinct outcomes, the default window is two minutes rather than
  five seconds, and it is `[permissions] prompt_timeout_seconds`. The error
  names both the timeout and the two ways out. (`worktree.rs`,
  `docker.rs`, `permissions/mod.rs`)

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
| 4.2 | ~~`niki doctor` reports a hard **Pass** for secret redaction in "logs, reports, artifacts".~~ **DONE in batch 2 — the check is real, and two shapes were leaking.** The constant is replaced by a 13-shape corpus run through the actual redactor, with a `Fail` arm naming the shapes that survive. Measured, it found a Hugging Face token and — worse — *any key in a JSON body*, because the field patterns required an `=`. Provider error bodies are JSON, and they are the one place `redact_secrets` is applied, so a key echoed in an error response reached the log and `report.md` unredacted. Both closed; the scope claim in README, the security doc and `docs/claims-audit.md` is narrowed to what the code does. | `cli/doctor.rs` (`redaction_corpus`, `redaction_failures`); `llm/provider.rs:535-590`; `tests/secret_redaction.rs` | |
| 4.2b | The base64 catch-all `[A-Za-z0-9+/]{40,}` still redacts **any** unbroken 40+ character alphanumeric run, so a long identifier with no separators — a git SHA is 40, some diff lines are longer — is blanked in a report. Confirmed **pre-existing**: present before this batch's change, verified by stashing. | `llm/provider.rs:542` | This is the reason 4.2 could not simply have been "redact at the report write boundary" — doing so on top of this pattern would replace evidence with `[REDACTED]`, which is its own failure. The fix is entropy-based detection (charset, digit ratio, length distribution) rather than a length threshold, and it needs its own corpus of real identifiers that must survive. Larger than the rest of §4 combined. |
| 4.3 | ~~`git` tool has no `check_command_policy` and no timeout.~~ **DONE in batch 2.** It now runs the same check the `bash` tool has run since Phase 5.3, and is bounded at 30s — `git daemon` is a valid subcommand and had no bound. Canaries drive the real `GitTool` with permissions set to `bypass`, so the permission layer cannot be what stops it. | `runtime/tools.rs:2637-2697` | The timeout is a constant, not a config key, because `ToolContext` carries no config — a follow-up could add one. |
| 4.4 | ~~Docker backend **silently** auto-approves every `Ask`.~~ **DONE in batch 2.** The comment claimed "Loud by design" three lines above a `tracing::warn!` that is off unless `RUST_LOG` is set, on the **default** backend, where `tools.bash` defaults to `Ask` — so every command in every default run was auto-approved without a word. Now on stderr, identically to the worktree backend. | `docker.rs:594-609` | |
| 4.5 | Three unsound advisories pass by policy, including `git2 0.20.4` — a direct dependency — carrying `RUSTSEC-2026-0184` (undefined behaviour). `deny.toml` sets `unsound = "none"`; nothing is acknowledged or pinned. `[bans]` and `[sources]` gate nothing. | `deny.toml:11-20, 41-52`; `Cargo.toml:46` | Needs a real dependency bump, not a config flip. `cargo deny check` currently passes and G5 reports that honestly. |

## 5 · Reliability (P5)

- ~~The 120 s client timeout is a *total* deadline whose error matches no retry
  classifier.~~ **DONE in batch 2.** Now `connect_timeout(15s)` +
  `read_timeout(120s)`, so the bound is between bytes rather than across a whole
  generation and a slow model is no longer killed mid-answer. `is_timeout_error`
  classifies by type and, measured rather than assumed, walks the `source`
  chain — because reqwest reports a stalled *body* as a body-decode failure
  whose `is_timeout()` is false. Wired into all three retry paths.
  (`provider.rs:85` vs `agents/mod.rs:141-147`, `failover.rs:203-216`)
- `anthropic::stream` bypasses `send_request`, so it gets no HTTP retry; the
  agent-level matcher catches 429/503 but not 500/502. The comment at
  `anthropic.rs:96-98` claims "Retries on 429/5xx" and is true of `complete()`
  only.
- Google never emits `StreamChunk::Finish`, so a truncated Google response is
  misdiagnosed as "model output was not a usable artifact" — the exact failure
  that enum exists to prevent on the pipeline path.
- ~~A mid-pipeline failure destroys the Coder's work.~~ **DONE in batch 2.**
  The error path now salvages: the per-stage checkpoint's `produced_artifacts`
  are written to `artifacts/` and a `SALVAGED.md` records what survived.
  Deliberately **no branch and no working-tree mutation** — a failed run has
  nothing verified to deliver, and quietly applying an unreviewed diff is a
  larger semantic change than a hardening pass should make alone. The user
  gets the `CodeDiff` and decides. (`run.rs` error arm,
  `pipeline.rs:3410`, `runtime/mod.rs:321`)
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
