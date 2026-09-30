# NIKI — hardening progress

Branch `niki/hardening` · programme: **the terminal UI is the product** · started 2026-09-30.

Plan: `plans/namor-obsidian-jay-garrick.md` in the session plan directory. Decisions locked in §0a.

---

## Iteration 1 — 2026-09-30

### T0 · Restore the branch hand-off ✅

**What was done.** `src/output/git.rs:266-272` carried an uncommitted sabotage from an interrupted
prior session — labelled `TEMPORARY SABOTAGE (G3 can-fail proof)` — that built the `git add`
argument list and then discarded it. Nothing was staged, so `index.write_tree()` returned the
parent's tree, the guard at `:286` returned `Ok(true)` = "a branch was actually created", and the
run reported `status: completed` with an `niki/<id>` branch carrying no commit.

**Can-fail proof (G3), run before the fix:**

```
$ CARGO_BUILD_JOBS=2 cargo test --test run_lifecycle -j 2 -- \
      --test-threads=1 the_code_change_is_on_the_branch

running 1 test
test the_code_change_is_on_the_branch_not_only_in_a_sidecar_file ... FAILED

thread '…' panicked at tests/run_lifecycle.rs:997:5:
run must succeed, stderr:
  Error: Strict safety mode: hermetic invariants broken.
         NON-HERMETIC: committed state changed during the run.

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 12 filtered out
```

Full log: `.evidence/T0-canfail-RED.log`.

**Notable:** the strict safety layer caught the sabotage *independently of the test*. It is not
only the test that can fail here — `safety::prove` flagged the branch-without-a-commit as a broken
hermetic invariant and aborted the run. Two independent gates, both red.

**The fix.** Restored `run_git(repo_path, &args)?;`.

**What was kept from the prior session**, both verified as sound:
- `tests/run_lifecycle.rs` +169 — `the_code_change_is_on_the_branch_not_only_in_a_sidecar_file`.
  Asserts against `git show <branch>:src/list.rs`, the committed blob, which cannot be satisfied by
  the working tree (the worktree backend mutates it by design) or by a sidecar artifact. This is
  the right gate for the product's central claim.
- `src/cli/doctor.rs` +177 — `check_container_can_start`. A real fix for a real failure: on stock
  WSL2, podman's `crun` cannot write `cpu.max` without cgroup delegation, so every container run
  fails while `niki doctor` reported three green checks and exited 0. It has its own unit test.

**Gate:** G3, G4 — red before, green after.

### T1 · Make the PTY TUI gate able to fail ✅

**What was done.** Both TUI jobs piped pytest into `tee` and appended `|| true`, on the reasoning
recorded in the step comment that the vacuity grep was the decider. It was not. `|| true` discarded
pytest's exit status, leaving one check: `grep -qE '[0-9]+ passed'`. That regex has **no floor** —
`3 passed, 17 failed` matches it, so the job went green with 17 failures.

This is the only layer in the repo that can catch a key bound in the table but never dispatched —
the workflow comment above the job cites exactly that bug. The one gate that catches the worst class
of TUI defect was discarding its own result.

Both jobs now capture pytest's exit status (`set -uo pipefail`, so `$?` is pytest's, not tee's) and
fail on it. The vacuity check is kept and runs **first**, because tuiwright is imported with
`importorskip` — a missing dependency skips the whole file and pytest exits 0, so a green exit code
there means nothing ran.

**Can-fail proof**, run against the four shapes that matter:

| pytest output / exit | before | after |
|---|---|---|
| `23 passed, 2 skipped` / 0 | PASS | PASS |
| `3 passed, 17 failed` / 1 | **PASS** | **FAIL** |
| `25 skipped` / 0 (tuiwright missing) | FAIL | FAIL |
| `20 failed` / 1 | FAIL | FAIL |

Added `the_tui_gates_decide_on_the_suites_result` to `tests/ci_contracts.rs`. The pre-existing
contract test pins the vacuity regex's *form*; pinning the form while the result is discarded is how
the weakness got ratified in the first place. **27/27** in the binary.

**Gate:** G3, G8.

### T2 · The chat remembers, streams, and can be cancelled ✅

Clause **C-J1**. The transport had nowhere to put a conversation.

**Root cause.** `CompletionRequest` had no history field, and all four providers hand-wrote a
single-element `messages` array. Turn 3 was sent with turns 1 and 2 erased — while the transcript
scrolled, persisted to `.niki/chat.json`, and resumed, showing a conversation the model had never
seen. A user testing recall got a confidently wrong answer and nothing on screen said why.

**What changed.**

- `CompletionRequest.history: Vec<ChatTurn>`, plus one shared `message_chain()` builder used by all
  four providers. Anthropic/OpenAI/Ollama send `"assistant"`; Google sends `"model"`. A chain never
  starts with an assistant turn (Anthropic 400s on that, and a resume truncated mid-turn can produce
  one), and blank turns are dropped.
- `agents/mod.rs` and the Coder tool loop set `history: Vec::new()` — **deliberately**. An agent
  stage is single-shot by design; a Planner that could see the Coder's later output would not be an
  independent Planner.
- Chat now streams. `stream_reply` uses `provider.stream()` — which already existed and which
  `niki run` already consumed — and emits `ChatDelta` fragments. A 20-second answer is no longer 20
  seconds of a static screen.
- A real in-flight indicator. `chat_pending` drives a `⟠ thinking… (esc to cancel)` line. The old
  spinner was gated on `has_running_stage()`, which is permanently false in chat because
  `state.stages` is empty, so it never drew.
- Esc now cancels. `run_chat` never set `state.cancel`, so `request_cancel` had nothing to set and
  the notice said "Stopping…" while the request ran to completion. One `Arc<AtomicBool>` is now
  created by the caller and shared by the TUI and the processor thread — two flags is exactly how it
  came to stop nothing.
- Errors are errors. Every failure was rendered as `(offline) LLM error: …` inside an *assistant*
  bubble, so a 401 told the user to check their network. `describe_error` names the class (auth,
  unknown model, rate limit, timeout, dropped connection) and `DisplayEvent::ChatError` renders it
  as `error`/`cancelled`, not as a turn the model took.
- Truncation is surfaced. The provider's stop reason was parsed and thrown away, so a reply cut at
  the token limit was presented as a finished answer. `chat_truncated` now says so on screen.
- An empty reply renders as a labelled turn. `"".lines()` yields nothing, so the assistant header
  was never emitted and the user saw a blank gap indistinguishable from "not replied yet".
- The system prompt lost its literal `__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__` marker (sent to the
  provider verbatim; nothing in the repo substitutes it) and the two tool rules it carried while
  `tools: None` told the model about "dedicated native tools" it was never given.

**Found by the new test, and fixed here:** `google.rs` ignored `config.base_url` and hardcoded
`https://generativelanguage.googleapis.com`, so a user behind a proxy could not reach Google at all —
and the provider was the one shipped implementation no test could exercise. The first run of the
Google test hit the **real** Google API and came back `API_KEY_INVALID`. It now honours `base_url`
like the other three.

**Can-fail proof** — `message_chain` made to drop history, as it did before:

```
test a_turn_carries_the_conversation_that_came_before_it ... FAILED
test anthropic_puts_the_whole_conversation_on_the_wire    ... FAILED
test openai_puts_the_whole_conversation_on_the_wire      ... FAILED
test google_names_the_assistant_turn_the_way_google_does  ... FAILED
test a_chain_never_starts_with_an_assistant_turn          ... FAILED
test blank_turns_are_dropped_rather_than_sent_as_empty_content ... FAILED
test result: FAILED. 6 passed; 6 failed
```

All three **wire** tests go red, which is the point: it proves the history reaches the HTTP body,
not just the request struct. Restored: **12/12**.

**Gate:** G3, G6. fmt clean, `clippy --all-targets -D warnings` clean.

#### T2b · The transcript renders markdown, and a cache bug that would have hidden it

`src/display/chat/` is 1,367 lines of tested pulldown-cmark rendering — fenced code with language
hints, tables, lists, inline styles. None of it rendered the conversation: `build_chat_lines` used
`text.lines()`, so a reply containing ` ```rust ` appeared literally, with its backticks, on the one
surface where a coding assistant's output is read. The engine was reachable only for *stage bodies*,
and stages are empty in chat. Assistant turns now render through it, in a streaming form while in
flight so a reply does not restyle itself when it lands. Errors stay verbatim — a diagnostic has to
survive into a copy and a bug report exactly as written.

**Bug found while doing it, and fixed.** `chat_content_hash` — the guard that decides whether to
rebuild the cached line map — hashed `chat_log.len()` but **not** `chat_stream`. So every streamed
delta was discarded before it could be drawn: the surface would have held a frozen frame for the
whole request and then jumped straight to the finished turn. The state machine would have been
correct and the screen would not have been, which is the exact shape of defect this slice exists to
remove. The hash now covers the stream, the pending flag, the truncation flag, and message content
rather than count alone.

**Can-fail proof** — hash reverted to its previous contents:

```
test display::pages::chat::tests::the_line_map_rebuilds_as_a_turn_streams_in ... FAILED
test display::pages::chat::tests::the_line_map_tracks_pending_and_truncation ... FAILED
test result: FAILED. 0 passed; 2 failed
```

**T2 total: 17 new tests** (15 in `tests/chat_conversation.rs`, 2 unit tests in
`src/display/pages/chat.rs`), all can-fail proven.

### T3 · A task typed in the TUI runs the pipeline ✅

Clause **C-J2**. The chat is a coding agent, not a viewer. This is the one High-risk slice in the
programme, and measuring it first is what changed the plan.

#### T3a · Delivery extracted out of `cli/run.rs` ✅

**What the measurement found.** `grep 'orchestrator\|execute_pipeline' src/cli/chat.rs` returns
nothing, so wiring chat to the pipeline looked like one function call. It is not. The step that
turns a diff into a reviewable branch — artifacts, the red-suite gate, replaying the sandbox diff
onto the host working tree, conflict-marker blocking, `create_branch_and_commit`, the hermetic
safety proof, the report — was **281 lines inside `cli/run.rs::run_inner`**, and
`create_branch_and_commit`, `apply_diff_to_working_tree` and `generate_report` each had exactly one
call site, all in that file.

Calling `execute_pipeline` from chat without moving that would have reproduced the exact bug the
audit found in `niki acp` and `niki goal`: the four agents run, `sandbox.destroy()` removes the
worktree on the way out, the Coder's diff is deleted from disk, no branch exists — and the surface
reports success. The deliverable lived one layer above the orchestrator, so the orchestrator could
not deliver.

**What changed.** `src/orchestrator/deliver.rs` — the 281 lines, plus `role_filename` and
`write_plan_md`, which only delivery used. `run.rs` 1912 → 1578 lines. The body was moved, not
rewritten: the CLI's locals are reconstructed at the top of `deliver()` so the extracted lines read
exactly as they did. A refactor of the most safety-critical code in the product should be a move.
Eleven `needless_borrow` lints came from the destructured reference bindings and were auto-fixed.

**Verification.** `run_lifecycle` 13/13, lib 949/949, fmt clean, `clippy --all-targets -D warnings`
clean. The core promise is unchanged by the move.

**A regression this caught, in my own previous commit.** Three lib tests in
`display::pages::chat` failed: `build_lines_header_and_messages` asserted the single string
`"assistant: world"`, and two selection tests passed hard-coded columns `13..18`. All three were
pinned to the *old pixel layout*, not to the property they claimed to test — so the markdown
rendering in T2b broke them for a reason that had nothing to do with correctness. I shipped that
without noticing because I ran a filtered lib suite and the TUI integration binaries but not the
whole thing. They now derive columns from the row they actually found and assert that a turn is
labelled and its body addressable.

#### T3b · The chat runs the pipeline ✅

Clause **C-J2**, the core promise.

**What changed.** `/run <task>` in the chat starts the four agents and ends in a branch. The chat
calls `execute_pipeline` with an `AgenticDisplay` attached to its own event sink, then calls the
same `deliver()` that `niki run` calls — so a task started here produces the same four artefacts:
a branch carrying the Coder's change, `report.md`, `changes.patch`, `artifacts/*.json`, and a
hermetic proof.

Two supporting changes:

- `AgenticDisplay::attach_sink` forwards events to an **existing** channel instead of spawning a
  render thread. `enable_tui` spawns a second, competing full-screen app — right for `niki run
  --tui`, wrong for the chat, which already owns the screen. It forces `muted` on, because
  unmuted every stage also writes a timestamped line straight to stdout and corrupts the
  alternate-screen buffer the chat is drawing into.
- `/run` is in the slash menu and in `/help`, first in both. Named `/run` and not `/run <task>`,
  because the menu is a suggestion list with no accept action — whatever is listed is what the user
  has to type.

**Design decision.** A plain message stays a conversation turn; only `/run` starts the pipeline. A
chat that silently begins a four-agent run — spending money and writing to git — on a message the
user meant as a question is a chat that does things nobody asked for, and this product's character
is being honest about what it did.

**Bug the test caught, in my own wiring.** I first set `uses_docker` from *"is a container runtime
reachable"*. This box has Podman installed and the fixture config says `backend = "worktree"`, so
delivery was told the diff was already on the host, never replayed it, and the commit came out
empty — the run failed with `NON-HERMETIC: committed state changed during the run`. A reachable
runtime says nothing about which backend a run uses. It now comes from `config.docker.backend`, and
the runtime is only connected when that backend is actually selected.

**Can-fail proof** — `deliver()` made to report success without doing anything, which is precisely
the `niki acp` / `niki goal` failure:

```
test a_task_typed_in_the_chat_produces_a_branch_carrying_the_change ... FAILED
a task typed in the chat must produce a branch, got: ""
```

`tests/chat_runs_the_pipeline.rs` drives the real pipeline through the real chat entry point against
the in-process `mock` provider, then reads the committed blob with `git show` — the same assertion
`run_lifecycle` uses for `niki run`, so the two entry points are held to one standard.

### T4 · The UI stops lying about outcomes ✅

`DisplayEvent::Final` carried no outcome; `state.rs` set `RunState::AwaitingApproval` on receipt, and
the Verdict tile rendered that as a pulsing green **A P P R O V E D** — for *every* ending.
`show_failure` emits the same event on the way out, so a run that died on an API error and a run a
Reviewer rejected both ended up painted approved. `Final` now carries `{ verdict, error }` and the
state is derived: approved / rejected / no-verdict / failed. `NoVerdict` is deliberately distinct
from `Approved` — `verdict_source` exists because "a Reviewer approved this" and "nothing reviewed
it" are different facts. `AwaitingApproval` is also gone as a name: nothing awaits approval.

`show_failure` had **zero call sites** for its life. Both it and `render_failure` took a
`&NikiError` and a `&PipelineState` neither used, which is a good way to keep a function from ever
being called; both now take the message, and both are wired. Added `show_cancelled` — a run the user
stopped is not a run that failed.

The `[r]etry` button is **removed**: it was the primary bold-amber action and its only effect was
`OverlayOutcome::Quit`, so the first thing a user pressed on a failed run, having been told
"retry", quit NIKI. `ModalAction::Retry` is still produced by the key and mouse handlers and now
resolves to a no-op rather than the quit path.

Added `DisplayEvent::Notice`. The pipeline's diagnostics were all `eprintln!`, which under `--tui`
writes into the alternate-screen buffer that `LeaveAlternateScreen` then discards — "Branch
blocked: test suite `cargo test` failed (exit 1)" was written to a screen about to be thrown away,
and a comment in the pipeline claimed a "TUI notice line" that did not exist.

Can-fail: `Final` reverted to ignoring its payload → **2 failed**, including
`a_failed_run_is_never_approved`.

### T5 · Delete the fabricated UI ✅

The Run page is one Tab from every user and, until `/run` existed, was permanently empty of real
data. Everything on it was invented: a `--project ./my-app` literal, a `niki/xxxxx` ref that does not
exist, **"working tree: untouched" in success green with nothing behind it**, four agents marked
`queued` for a run nobody started, and `["sandbox","docker"]` on every run.

The Cost page printed the literal `anthropic/claude-sonnet-4` for every agent in every
configuration, so a local `qwen2.5-coder` run was displayed as four Anthropic calls — `StageMetric`
already carried provider and model; the JSON just was not passing them across. And `state.cost` was
never assigned by anything, so the status bar's `$` and `/cost`'s "Total Spend" both read $0.0000
after real paid API calls.

History's `Enter` copied a branch string and jumped to the Run page — reading as "I opened my last
run" and not reading the task directory it pointed at. It loads it now. The header read `main`
whenever no run had set a branch. Fleet said "Start one from Chat (press Tab)"; Tab cannot start
anything, and `/run` can.

**A regression this caught in my own previous commit:** three chat tests were pinned to the *old
pixel layout* (a literal `"assistant: world"`, hard-coded columns `13..18`) and failed for reasons
unrelated to what they claimed to test. They now derive columns from the row they find.

### T6 · Slash commands work, or say they are not wired ✅

Twelve commands printed success for work they did not take. `/compact` was destructive *and*
misreported — it said "Compacted N previous turns into memory checkpoint" and then `split_off` the
turns. The worst needed no command: everything unmatched fell through to
`chat_log.push(("user", trimmed))` and was **sent to the LLM as text**, so `/doctor` and `/review` —
both in the slash menu and in `/help` — were silently dispatched to the model as the string
"/doctor". A user's first slash command was likely one of them.

Unwired commands now share one `NOT_WIRED` constant. An unknown command is an error naming itself.
`/doctor` and `/review` got real handlers. `/model` also lied in its read-only branch: it printed
`state.model`, which `/model` could set without changing anything.

**The new tests caught a bug this slice introduced:** `/run` is dispatched by the processor, one
layer below the key handler, so the new unknown-command arm rejected the command the previous slice
added. Fixed, and the menu now carries each command's syntax.

### T7 · Onboarding tells the truth, and the first minute survives ✅

Bare `niki` opens onboarding, so it is the first thing a new user reads. Three of its five pages
asserted things the product does not do: `[1][2][3]` with no digit handler at all (the screen was
byte-identical before and after pressing 1) and a "Colorblind" theme that does not exist; "Sign in
with your provider (API key or **OAuth**)" with no OAuth in the crate and no command named; and
"Niki collects anonymous usage data… Telemetry is OFF by default" — no collector in the binary, and
`README.md:299` says "No telemetry". A consent screen that contradicts the product's own
documentation is worse than none.

The `telemetry.enabled` settings row is removed: it wrote a config key nothing reads. `niki doctor`
now lists the OTLP endpoint, which its "the *only* hosts NIKI will ever contact" check omitted.

The small-terminal black hole is fixed. Below 10 rows the surface `return`ed silently while the
overlay ladder kept eating keystrokes — at 80x9 and 80x8 the user got a black void with no way out
but an unmentioned `Esc`.

### T8 · `niki auth login` is a real path, not a dead end ✅

`niki auth login` writes to the OS keyring and says "Run `niki doctor` to verify your setup."
`niki doctor` reads it. **Nothing on the request path did.** So the most likely first session was:

    $ niki auth login     "stored securely in your OS keyring"
    $ niki doctor         green
    $ niki                 > hello
                          "No LLM provider is configured yet. Run `niki auth login`…"

The chat naming as the fix the command that was just run. `NikiConfig::load` now consults the
keyring, so `run`, `chat` and `doctor` cannot disagree about whether a key exists.

**The four behavioural tests call `resolve_keyring_with` directly and would have stayed green with
the wiring removed** — which is exactly why a fifth, source-scanning test exists for the real path.
Both kinds are here, and the second is what caught the missing slice.

### T9 · Survive the terminal it is given ✅

Four defects, one shape: the surface looked alive while doing nothing. A draw error was swallowed
with `.ok()` and the chat kept consuming keystrokes against a frozen frame — `run_tui` already broke
on the same error. A panic in the chat thread exited **0**. The mouse addressed an 80-column line
map while the screen drew at the real width, so in a wide terminal clicking a message copied a
different one. And `active_focus` knew about three overlays while four are painted, so a click on
the status bar during onboarding cycled the permission mode toward BYPASS.

Can-fail, each defect reintroduced individually: **4 tests red**.

One assertion here was wrong on first run: the discarded-join test read this file's own comment
quoting the old line as the defect. Now line-anchored.

### T10 · Gate the surface that has never been gated ✅

`niki chat` was in **zero** e2e legs, and the tmux suite's nine cases covered no first run. Three
cases added, each aimed at a defect from the last three slices. `lib.sh` now points `HOME` at a
throwaway so a "first run" case is actually one — inheriting the developer's `~/.config/niki` is how
the first version of case 10 failed on a machine that has a provider configured.

`10_first_run_no_key` asserts the *property* (a first run answers rather than hangs), not a
particular message: what a first run can legitimately do depends on the machine. The two defects
reintroduced for the can-fail proof did not touch its path, so it stayed green — the honest report
is that it is a regression guard, not a proof that T6 or T7 landed. The other two cases went red.

Also closed: `page_id_titles` and `page_id_key_hints` each listed 11 of 14 `PageId` variants, and
`Fleet`/`Session` appeared **zero** times in a 1,664-line file — both deliberately absent from
`PageRouter`, so a key routed to either is a silent no-op.

**Smoke suite: 9 → 12, all passing.**

### T11 · `scripts/verify.sh` — one command, nine gates ✅

The repository had **twelve** verification scripts and no single source of truth.
`verify-product.sh` is a full product verifier with startup-time, demo-time and eval-recall gates —
and **no workflow runs it**.

**The G6 gate found six real defects on its first run**, which is the argument for writing it:
`niki report` exited 0 on an unknown id, on no tasks, and on a missing report; `niki status` exited 0
while printing a failed run; and `niki chat --message` exited 0 after a provider error. All fixed.
The other two were my own wrong expectations — a test that depends on the machine is a test that
will fail on someone else's.

Four bugs in the gate itself, all found by running it: the canary-map loop never matched, the
`cfg(test)` filter flagged three trait stubs inside a test module as production, the secret scan
flagged the fixtures whose entire purpose is holding credential-shaped strings, and the README
command extractor kept its backtick. All four are fixed, and three of them are the kind of false
positive that gets a gate switched off.
### Next

T4 · The UI stops lying about outcomes. `DisplayEvent::Final` sets `AwaitingApproval`
unconditionally and that renders **"A P P R O V E D" in pulsing green** — for failed and rejected
runs too. `[r]etry` on a failure modal quits the app. `show_failure` has zero call sites.

### Blockers

None.

### Assumptions in force

- The 3 dirty files were an interrupted prior session; only the labelled sabotage was reverted.
- `niki chat` is a **coding agent**, not a viewer (owner decision, §0a).
- For the 12 slash commands that lie, **retracting the claim is the default**.
- A plain chat message stays a conversation turn; `/run <task>` starts the pipeline. Inferring
  "this is a task" from every message would be magic, and the product's ethos is honesty.
