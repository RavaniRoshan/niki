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
| 1.2 | ~~`h` and `l` mean prev/next page on every sub-page, shadowing History and TestLog.~~ **DONE in batch 3** (`65619a1`). `h` and `l` are page letters now; `[`/`]` and the arrows keep page navigation. `no_page_letter_is_claimed_by_the_navigator` walks all twelve letters and asserts `from_key` still maps each, so the fixture cannot go stale. | `nav.rs:80-85` | The "`[`/`]` on Diff navigate pages instead of hunks" half is **not** done and is not the same fix: it needs a key of its own for hunk navigation, and `[`/`]` are now the *only* prev/next-page keys on those pages. Revisit if Diff ever gains a hunk cursor. |
| 1.3 | ~~`Tab` is claimed by three controls; the status bar wins, so Config's "next field" and Agents' "next tab" are both dead.~~ **DONE in batch 3** (`0e40439`). `sub_page_owns` yields `Tab` on Config, Agents and Session — the three whose footers advertise it — and nowhere else. | as cited | |
| 1.10 | ~~`ConfigPage::render` never reads `selected_field`, so the now-live `[Tab] next field` moves a cursor nobody can see; the page also cycles a hand-typed `% 15` over 13 fields, giving two dead stops.~~ **DONE in batch 3** (`43f60db`). The form builder returns the rows *and* the field indices; the render marks the selected row, and the key handler cycles by that list's length. | `pages/config.rs` (`build_form`, `ConfigPage::field_count`) | The count could never have been typed: one of the fields is pushed **inside a loop** over the agents. Deriving it is what makes it trustworthy. |
| 1.4 | ~~`q` quits the app from every sub-page, before the router, so all 11 pages' own `q → Run` handlers are unreachable.~~ **DONE in batch 2** (`71ba200`). The page's handler runs first; a page that declines `q` falls back to the confirm-quit modal, which had been unreachable dead code. Fleet and Session answer `q` for themselves and `continue` before the router, so they got their own arm. Proven by a real pty — `cases/13_subpage_q_goes_back.sh`, which asserts the tmux session still exists, because asserting only that "Run" rendered would pass against an app about to exit. | as cited |
| 1.5 | ~~Fleet's footer advertises `P R K V`; Session advertises `P` and `R`.~~ **DONE in batch 3** (`bbf1889`). Six advertised controls, none handled. `P` wired to the existing `state.paused` toggle; `R` retracted (pause and resume are one action); `K Kill` and `V Diff` retracted, no implementation exists. | `pages/fleet.rs`, `pages/session.rs`, `tui.rs` (`handle_fleet_nav`, `handle_session_nav`) | The roadmap said "3 of 7"; measured it is 4 of 7. **A footer's *label* matching its key's *action* is not statically checkable** — `K Kill` resolving to `select_prev` is a lie a grep cannot see — so the guard is the explicit retraction list in `tests/tui_footers_advertise_what_works.rs`, not a general property. |
| 1.6 | ~~`?` never reaches the Help page — it is consumed as `ToggleHelp` first. The only route is `Ctrl+P → help`, and *that* lands on a second, stale help page that says `[t] theme` when the key is `ctrl+t`.~~ **DONE in batch 3** (`a50a1b1`) — but by making the two surfaces honest rather than by merging them. | as cited | The Help page's GLOBAL rows are now generated from `BINDING_TABLE`, so the page and the which-key overlay cannot disagree again, and a user's overrides show up in the page. The palette no longer claims `?` reaches Help, because `ToggleHelp` consumes it first. **Merging the two surfaces is the remaining option and it is not done**: `?` stays the quick global reference because that is what the status bar advertises it as, and consolidating would retire `show_help`, which the mouse handlers and their tests depend on. That is a real piece of work, not a five-line edit. |
| 1.7 | ~~Digit jumps cover 9 of 14 pages.~~ **DONE in batch 3** (`d59a653`). `0` is the tenth page, taking it to 10 of 14, and the Help page gained a `PAGE NUMBERS` section generated from `PageId::all()`. | `nav.rs:91-104`; `pages/help.rs` (`page_number_rows`) | `Chat` deliberately stays off a digit — `Tab` is its route, and a digit that meant different things on different pages is the defect this cluster is about. The old numbering was the *internal order of a Rust enum*, which is why the documentation half mattered more than the binding half. `PageId::shortcut()` is a new inverse of `from_key` that scans its candidates, so it cannot name a key that does not work. |
| 1.8 | ~~Session is 4/7 placeholder tabs with a permanently empty Conversation.~~ **Conversation DONE in batch 3** (`f08185b`): the tab now renders the live `AppState::chat_log` — the tail, roles labelled — because `SessionState::messages` has **no writer anywhere in the tree**, so the tab read "No messages yet" on every mission, forever. | `pages/session.rs` (`render_conversation`, `render_session`) | **The other 3 placeholder tabs are still placeholders**, and the roadmap's "4/7" is now "3/7". They need the pipeline's per-stage data wired in, which is a feature rather than a fix. `SessionState::messages` is kept as the mission-scoped store; the *view* shows the chat view, which is the only thing that stays true as a conversation grows. |
| 1.9 | ~~`docs/tui/key-matrix.md` tells a maintainer *not to fix* divergences that are all resolved.~~ **DONE in batch 3** (`1fa2f7f`). All four were fixed; the section is now empty and **checked** by six tests, so a live entry without evidence fails the build. | as cited |

**Exit criterion:** every key in `BINDING_TABLE` and every footer hint maps to
an implemented handler, asserted by a test that enumerates the table rather
than a hand-copied list.

## 2 · Chat ↔ pipeline depth (P3)

Blocked behind nothing now that T3 landed, but it is large and it should be
built on a stable router (§1).

- Tool cards in the transcript, with arguments and results. The renderer
  (`components/tool_card.rs`, `tool_detail.rs`, the Enter hit-test) is fully
  built and unreachable, because the chat sends `tools: None`.
- ~~**Branch checkout from the TUI.**~~ **DONE in batch 2** (`aa1a244`).
  `session::branch`, shared by the TUI and the CLI, with the argument
  validated first — `git checkout -f --` discards uncommitted work and exits
  0, and the trailing `--` that makes branch-vs-path safe does not help
  because the flag is parsed before it.
- ~~History `Enter` loads the task directory but does not switch to the Diff
  page.~~ **DONE in batch 3** (`0599708`): `[Enter] open` goes to the diff, and
  `view` moves with `current_page` so the rendered page and the footer agree.
- ~~The permission modal is structurally unreachable in chat: `cli/chat.rs`
  creates a channel whose sender is never given to `create_sandbox`.~~
  **This was wrong, and the correction matters.** A plain chat turn calls
  `stream_reply` — no tools, no sandbox, no `create_sandbox` at all. So the
  modal is unreachable in chat because **chat does not run tools**, which is
  the owner's §0a decision ("`/run <task>` starts the pipeline, plain messages
  stay conversation turns"), not a wiring bug. The sandbox *does* emit
  `DisplayEvent::PermissionRequest` (`sandbox/worktree.rs:453`,
  `sandbox/docker.rs:606`) and `state.rs:1702` handles it. There is nothing to
  fix unless chat is to run tools, which is a product decision, not a repair.
- `ask_user` / `approval` return "cannot ask" in any TUI run, because
  `TUI_OWNS_STDIN` makes `is_interactive_stdin()` false. No modal exists.
- ~~The permission badge is still cosmetic.~~ **DONE in batch 3** (`6c8a3be`).
  The posture travels with `ChatSubmit` and lands on every stage that has not
  started, because `ToolContext` reads `config.permissions.mode` per stage.
  Both notices say so, since the stage in flight keeps its posture. The
  default is still `manual`, asserted.
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
| ~~4.2b~~ **DONE in batch 5** (`fbe6443`) — see §9.5. The catch-all now requires an uppercase letter and a digit, so identifiers survive while encoded secrets are still caught. | ~~`llm/provider.rs:542`~~ `redact_encoded_runs` | |
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
- ~~`anthropic::stream` bypasses `send_request`, so it gets no HTTP retry.~~
  **DONE in batch 4** (`49d84a1`). One 429 or 503 killed a stream on its first
  attempt while the identical non-streaming call retried four times. Retrying a
  *stream* is safe only because `send_request` returns on response headers,
  before anything is yielded — a retry around the body read would duplicate
  output, which is why the fix is at that line and the reason is in the comment.
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
- ~~`niki resume` restores state and exits: "Ready for continuation."~~ **DONE
  in batch 4** (`5b4c318`). It said so and then nothing happened — the runtime
  holding the session was dropped on the next line. Actually resuming is a
  *feature* (starting the pipeline partway and deciding which stages a
  checkpoint's `produced_artifacts` satisfy), so the command now says plainly
  that nothing was re-run and names the commands that do something.
  ~~Checkpoints written with a bare `fs::write`.~~ **DONE** (`150762f`): one
  atomic writer, temp name unique per writer, cleanup on a failed rename.
  *Resuming mid-flight is still open* and is listed in §9.
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
- ~~697 lines of dead code: `src/errors.rs`, `src/control_plane/`,
  `src/persistence/` — all `pub`, so `dead_code` is silent.~~ **DONE in batch 4**
  (`d6ef83b`). All three had **zero** external references. Two were not
  accidents: `persistence` was a working mission store superseded by
  `mission::MissionStore`, and `control_plane` was a documented Convex mirror
  whose own header said "intentionally not wired". Git keeps both.
  `tests/no_unreferenced_public_modules.rs` now requires every `pub mod` to be
  referenced from outside its own subtree, and names its own blind spot as a
  passing test.
- `.niki-worktrees/` is not git-ignored and passes `is_publishable_path`, so
  after a SIGKILL the user's next `git add -A` commits a whole sandbox copy.
  A fixed temp patch path (`git.rs:158`) also collides across concurrent runs.
- `niki acp` and `niki goal` run the pipeline and then destroy the Coder's
  work: they call `execute_pipeline`, never `deliver`. Fixed for the chat in
  T3a/T3; **not fixed for these two**. `acp/server.rs:149` also stores the diff
  *text* in a field named `branch`.
- MCP is a documented Advanced feature that is a stub: `McpManager::call_tool`
  has zero callers outside `src/mcp/`, and stdio children leak. **Flagged, not
  decided** — either wire them or remove the README rows.
  ~~`web_search` returns `ToolStatus::Success` with "not yet wired".~~ **DONE in
  batch 4** (`54b64a2`): it now returns `Failed` with no
  `WebSearchResults` payload at all, and says what to do instead — an empty
  success told the model the *web* had nothing on the subject.
  ~~`web_fetch` has a permanently empty allowlist.~~ **DONE** (`1a698d9`): it
  now honours `[network] domain_allowlist`, threaded through seven call sites.
  The default is unchanged — empty still means block-all, and that is asserted.
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

## 8 · Queued after the hardening programme (owner directive, 2026-09-30)

**These start only when §1–§7 above have no unstarted item left.** The owner's
ordering is explicit, and the reason is the one this whole programme has been
demonstrating: every defect in §1–§7 was found by *using* the product, and a
new feature built on a surface that still has dead controls and lying help
pages would inherit all of it. Ordering is recorded here so it survives.

### T1 · Claude-style "living" working status — `src/display/`

A pulsing glyph from `['·','✢','✳','✶','✻','✽']` on a bounce, a rotating
gerund, a live elapsed counter and a token count, resolving to `⎿ Thought for
Xs · N tokens`. Tool calls as bold `⏺ name args`, results as a dim `⎿`, thinking
as dim+italic `∴`.

**Constraint that decides the design:** NIKI's existing `◈ ⟠ ◉ ◆` are the
*agent* glyphs and must not be reused or moved. The new glyphs are for working
state only, and `--output-format json` must stay pure — so the widget is a
render-time concern with no effect on the event stream. `tests/visual` and the
VHS job reference the current frames, so a glyph change is a baseline change
and must be blessed on the runner, not locally.

### T2 · Two-layer permission defence — `src/risk/`

Static deny (regex) → hooks → LLM transcript classifier, in that order, so
anything allow/deny-listed in settings is a hard rule and the classifier only
sees the residual. Plus an input probe over tool results.

**The reasoning-blind property is the security property:** the classifier sees
user messages and tool calls only, never assistant prose or tool output, so it
cannot be talked into an approval by text the agent itself wrote. A
classifier that reads the transcript *including* tool output inherits every
prompt injection those outputs contain, which is the attack it exists to stop.

### T3 · Streaming tool executor — `src/orchestrator/`, `src/runtime/`

Begin a tool when its `tool_use` block finishes streaming rather than after
the whole turn. `isConcurrencySafe()`: reads parallel, writes exclusive.

**The concurrency rule is the whole safety content of this task.** Two
parallel writes to one file is a lost update, and "reads are safe" is only
true if a read cannot observe a half-applied write. The executor needs a
per-path lock, not a read/write classification alone.

### T4 · Context compression at a budget — `src/memory/`

Above 95% of the context window, fire in priority order: snip duplicate
system messages, microcompact recent tool results, collapse long file reads,
summarise. Seven strategies in the original; four here, because the last three
need a summarisation model call and their ordering is an empirical question
this codebase has no data for.

**The failure mode to avoid is silent truncation.** Every strategy must be
counted and reported in the transcript, so a run that lost context says which
strategy took it — the same rule B2-01 through B2-12 have been applying to
every other silent degradation in this codebase.

## 9 · Still open after batch 4

| # | Item | Why it is not closed |
|---|---|---|
| 9.1 | **Resuming a pipeline from a checkpoint.** `niki resume` now says honestly that nothing is re-run. Actually resuming means starting `execute_pipeline` partway and deciding which stages a checkpoint's `produced_artifacts` already satisfy — a design question with product consequences, and the roadmap's to answer. | `cli/resume.rs`; `runtime/checkpoint.rs` |
| 9.2 | **MCP.** `McpManager::call_tool` has zero callers outside `src/mcp/`, and stdio children leak. Flagged in §6 and still undecided: wire it or remove the README rows. | `src/mcp/` |
| 9.3 | **The `anthropic::stream` *agent-level* matcher.** The transport now retries 429/5xx. The agent-level matcher above it still catches 429 and 503 but not 500 or 502, so those are not retried a second time. Narrower than the bug it sat next to, and worth settling when the retry sets are unified. | `agents/mod.rs:141-147` |
| 9.4 | **`git.rs:158` fixed temp patch path.** Collides across concurrent runs, the same shape as the temp-file bug fixed in `write_restricted_atomic` (`150762f`). Different file, same class, not yet done. | `src/output/git.rs:158` |
| ~~9.5~~ **§4.2b, the base64 catch-all — CLOSED in batch 5** (`fbe6443`). `[A-Za-z0-9+/]{40,}` blanked any unbroken 40+ character alphanumeric run. Measured: it destroyed git SHAs (40 hex), sha256 digests, long identifiers and minified chunks — and a redacted commit reference in `report.md` is an unreferenceable piece of evidence, the same failure as the empty artifacts in B2-01. What separates an encoded secret from an identifier is **shape, not length**, so the run must now contain an uppercase letter *and* a digit. The 13-shape corpus in `tests/secret_redaction.rs` is unchanged and still green, plus a new case for the catch-all's real purpose: a base64 token inside a JSON provider error body, with the rest of the body left readable. | `redact_encoded_runs`, `looks_like_encoded_secret` | The trade is explicit: an all-lowercase base64 secret would now survive. That is speculative, while a redacted commit hash in every report is not. If a real lowercase key ever appears, the shape test gains a case rather than the pattern growing back. |
| 9.6 | **§4.5, three unsound advisories** including `git2 0.20.4` with `RUSTSEC-2026-0184`. Needs a real dependency bump, not a config flip. | `deny.toml` |
| 9.7 | **`tests/tui_perf.rs` and `tests/headless_tui.py`** carry the two clippy items the gate does not check: wall-clock budgets with 2× headroom that can only fail on a machine twice as slow, and two unconditional `pytest.skip`s. | as cited |
| 9.8 | **`G8` has never run on this branch.** Not a defect; the owner's decision to work locally. It goes green on a push and nothing before. | — |
