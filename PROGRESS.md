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
### T12 · RELEASE_REPORT.md + ROADMAP.md ✅

`RELEASE_REPORT.md` carries the gate table, the four contract clauses before and
after, a can-fail sample, the six defects the gate found, **seven honest known
limitations**, and exactly what the owner must do to go live.

`ROADMAP.md` carries everything consciously deferred, each with its evidence, in
the order a user is most likely to hit it. Three things are marked **killed**
with the reasoning, so the next person does not re-derive them.

The most serious deferred item is named first in §7 of the report: the
`apply_patch` path escape, where a model-authored absolute or `..` path writes
outside the project. It was deferred because it needs a judgement call about
which backends may accept a path at all, and a schema change — not because it is
small. **It is the first item of batch 2.**

## Iteration 2 — 2026-09-30 · batch 2, twelve slices

Twelve commits, `c0e5548`…`71ba200`. All can-fail proven. Canary map grew 49
→ 65; PTY suite 12 → 13 cases.

The batch is not the one the plan predicted. Five of the twelve are defects no
audit found, because they were found by **running the product against a real
model** and reading what it printed. That is the part of the batch worth
noticing.

### What a live run found that no audit found

`z-ai/glm-5.3-flash` through NVIDIA's catalogue, with `RUST_LOG=info`:

```
15:49:40Z [Planner] Done (165s, in 1146 / out 1640) — Spec: 1 files to modify
15:51:51Z [Coder] Starting...
```

The Planner is fine — a schema-conformant TaskSpec in 165–235s. The Coder
never finished, and five real defects came out of that one run:

| Slice | What the run showed |
|---|---|
| B2-06 / B2-09 | The 120s bound was a **total** deadline, so 4 steps × 3 attempts could burn **12+ minutes** before a stage even started |
| B2-08 | A killed run left `status: "Running"` in the run record **forever** |
| B2-11 | **2m11s of silence** after the Planner, and the `[Coder] Starting` that did appear came from the *fallback* — describing a stage that had already lost ten minutes |
| B2-12 | The loop's error was erased by `.ok()?`, so a lost network and a declined tool loop were the same value |
| B2-10 | The permission prompt's 5s window reported `denied by user` — blaming the user for a fuse nobody saw |

**A hypothesis, tested and killed.** The Coder sends 22 tool specs; the Planner
sends none. The obvious reading of `os error 110` (ETIMEDOUT on *connect*) was
that the payload was too big to establish a connection. Measured against the
live endpoint:

```
  0 tools:      97 bytes  OK in 45.3s
 12 tools:    8894 bytes  OK in 22.8s
 22 tools:   16224 bytes  OK in 28.0s
```

**Refuted** — and the largest payload came back *faster* than the smallest. No
tool-spec trimming was done: it would have been a plausible change with nothing
behind it. The fault is a free tier dropping connections, and what NIKI owes
the user there is a bounded stage and a visible error. Recorded in
`EVIDENCE.md` under *Live-provider observations*.

### The twelve

| # | Commit | Slice |
|---|---|---|
| B2-01 | `c0e5548` | A model-authored path cannot write outside the worktree |
| B2-02 | `60a0230` | A signalled child is not a success |
| B2-03 | `831bf84` | A diff that could not be produced is not a diff that is empty |
| B2-04 | `bd62649` | The `git` tool honours the deny-list and is bounded |
| B2-05 | `92130e7` | The default backend no longer auto-approves in silence |
| B2-06 | `88694e1` | A deadline expiry is recognisable as one |
| B2-07 | `96637ec` | A late failure preserves the Coder's work as evidence |
| B2-08 | `970cfbb` | A killed run stops claiming to be running |
| B2-09 | `4fdb964` | A stalled stage fails in bounded time |
| B2-10 | `c94aaff` | A timeout is not a refusal, and 5s is not a decision |
| B2-11 | `c6851c0` | The Coder announces itself, and says which step it is on |
| B2-12 | `9a7ced4` | A failed tool loop is visible, not a silent fallback |
| B2-13 | `aa1a244` | The branch is reachable from the TUI |
| B2-14 | `1182110` | The doctor's redaction Pass is a real check |
| B2-15 | `71ba200` | `q` on a sub-page goes back, instead of quitting |

### Three of these found something the check itself could not see

**B2-14.** `niki doctor` printed `Pass("always-on: provider keys redacted from
logs, reports, artifacts")` — a constant. Running the real redactor over 13 key
shapes found two leaks, one of them serious: the field patterns required an `=`,
so they caught `api_key=…` and missed `{"api_key": "…"}`. Provider error bodies
are JSON, and they are the one place `redact_secrets` is applied — so a key
echoed back in an error response reached the log and `report.md` untouched. The
check is now the same corpus the tests assert, and README, the security doc and
`docs/claims-audit.md` were all carrying a broader claim than the code
supported.

**B2-15.** Eleven pages answer `q` with "back to Run". None could run: the nav
layer sat above the router, read `q` as `NavIntent::Quit`, and broke the event
loop. The same defect made the confirm-quit modal unreachable dead code. Found
by reading the router while looking for something else.

**B2-13.** Wiring `/branch` to a real `git checkout` surfaced a data-loss
hazard that had nothing to do with the TUI, and it is now pinned by a test:

```
$ git checkout -f --      # repo with an uncommitted edit
$ echo $?; cat a.txt
0
committed                 # the edit is gone
```

`Command::args` is not shell injection — it is git reading our argument as one
of its own options, and the trailing `--` that makes branch-vs-path safe does
not help, because the flag is parsed first.

## Iteration 3 — 2026-09-30 · batch 3, twelve slices

Commits `4e57fdb`…`6c8a3be`. Canary map 65 → 122. **`ROADMAP.md` §1 — navigation
and the dead controls — is closed: all ten items.**

§1 was called "the largest quality-of-life cluster, and the one left undone".
It is the one that mattered for the user's brief that the terminal UI be
"fully ready end to end", and it took eleven slices rather than the ten items
because the work kept finding things next to the thing being fixed.

| # | Commit | Slice |
|---|---|---|
| B3-01 | `4e57fdb` | `j`/`k` reach the page whose footer advertises them |
| B3-02 | `a50a1b1` | The help a user reaches describes the keys they press |
| B3-03 | `0e40439` | `Tab` has one owner on each page, not three at once |
| B3-04 | `43f60db` | The Config field cursor is visible, and its cycle has no dead stops |
| B3-05 | `65619a1` | A page's own shortcut reaches that page |
| B3-06 | `bbf1889` | A footer's claims are the ones the handlers back |
| B3-07 | `d59a653` | The page numbers are bound, and the numbering is documented |
| B3-08 | `f08185b` | The Session Conversation tab is not permanently empty |
| B3-09 | `1fa2f7f` | The key matrix describes the codebase that exists |
| B3-10 | `d22def0` | `g` and `s` work in `niki chat`, and `s` is not a blank page |
| B3-11 | `0599708` | `[Enter] open` opens the run it selected |
| B3-12 | `6c8a3be` | The permission badge governs a run instead of describing one |

### The pattern: fixing a dead key reveals the next dead thing

This is the shape of the whole batch, and it is the argument for testing
*behaviour* rather than auditing code:

- `q` was dead on 11 pages. Fixing it made the **confirm-quit modal** — written
  below the nav layer, unreachable — live again.
- `j`/`k` were dead on 10. Fixing them made the **Config field cursor**
  visible-but-wrong: `selected_field` was never rendered, and its cycle was a
  hand-typed `% 15` over 13 fields, so `Tab` had two dead stops.
- The Config count came from a **push inside a loop**, which is why it could
  never have been typed right.
- Wiring `/branch` to a real `git checkout` surfaced **`git checkout -f --`**
  discarding uncommitted work and exiting 0.
- Writing a test that reads the help text found the same stale `ctrl+t` in the
  **command palette** as a second copy.
- Enumerating `BINDING_TABLE` — §1's own stated exit criterion — found `g` dead
  in `niki chat` and `s` navigating to a **blank screen**.
- Fixing `s` meant refusing Fleet and Session in `global_page_jump`, which is
  the call that leaves a page with no state behind it.

Eight of the eleven were found by writing the *next* slice's test, not by
reading the code the slice was about.

### Tests I wrote, and then deleted or weakened on purpose

Four, and the reasons are in the commits because they are the point:

- A test that **copied** two handlers and called the copies — a test of the
  copy, which passes the moment the production arm is deleted.
- A test that checked a footer's *label* matched its key's *action*. That is a
  judgement call, not a static check, and a test that guessed at it would pass
  on the cases it happened to model. Replaced by an explicit retraction list.
- A PTY case comparing the pane before and after six `j` presses, on a Diff
  page with no diff — byte-identical, so it would have passed against a
  completely broken page.
- A cycle test asserting all *n* presses differ from the start, which asserts
  the cycle never wraps — the opposite of the truth.

And one real gap caught by a test that was **too** narrow: five tests stayed
green when the Session tab was wired back to the field nothing writes, because
they checked what the event loop *passed* and not what the page *used*. Two
wiring points, and they fail separately.

## Iteration 4 — 2026-09-30 · batch 4, twelve slices

Commits `c9da119`…`d6ef83b`. Canary map 122 → 175.

Batch 4 is the first one that was **not** driven by the ranked list. §1 was
already closed, so the work came from three places the earlier passes had
surfaced: the roadmap's own deferred items, the defects the new tests kept
finding next to the ones being fixed, and one standing instruction from the
owner — the four Claude-architecture tasks, recorded in `ROADMAP.md` §8 as
**queued, not started**.

| # | Commit | Slice |
|---|---|---|
| B4-01 | `c9da119` | Every front door that runs the pipeline delivers its work |
| B4-02 | `07c2c33` | A truncated response says so, and `google.rs` has tests at all |
| B4-03 | `122f9a9` | A leftover sandbox is not in the user's next commit |
| B4-04 | `5b4c318` | `niki resume` does not claim it continued something |
| B4-05 | `4c539b3` | The provider tests assert an endpoint, not a name |
| B4-06 | `15f0849` | The page-render tests assert what is drawn |
| B4-07 | `54b64a2` | A tool that cannot do the thing does not report that it did |
| B4-08 | `1a698d9` | `web_fetch` honours the configured allowlist |
| B4-09 | `49d84a1` | A stream retries a transient failure like `complete()` does |
| B4-10 | `150762f` | State files survive the crash that writes them |
| B4-11 | `d6ef83b` | 697 lines of unreferenced code, deleted and stopped accumulating |

### Three of these were data loss, not cosmetics

**`niki acp` and `niki goal` ran the pipeline and threw the work away.** The
TUI chat was fixed in T3a; these two were not, so the product's central promise
held on two of its three front doors. ACP also wrote the *diff text* into
`record.branch`, a field every reader treats as a branch name.

**A SIGKILL left a complete copy of your repository in `.niki-worktrees/`**, and
the next `git add -A` committed all of it. Now git's own per-clone `exclude`
file gets the entry — never your tracked `.gitignore`.

**A truncated `checkpoint.json` does not parse**, so the file you need *after* a
crash was the one most likely to be unreadable after one. `write_restricted` was
a bare `fs::write` at fifteen sites, and the atomic writer that sat beside it —
used for JSON session state, so the choice was a coin flip — had a **fixed temp
name** and raced under concurrency.

### The through-line: tests that passed the sabotage

Seven times in this batch, a test stayed green while the thing it claimed to
cover was broken. Every one is recorded in the commit that fixed it, because the
pattern is the lesson:

| What the test asserted | Why it passed anyway |
|---|---|
| Five page-render tests | Drew every page and asserted nothing but "no panic" |
| The provider endpoint tests | Compared a struct field to the string it was built from |
| `create_provider("anthropic").provider_name() == "anthropic"` | True of *any* implementation |
| Seven worktree tests | Each called the helper itself, so deleting the call site changed nothing |
| `contains("network_allowlist…")` | The string appears at seven sites; emptying one still matched |
| `contains("!sub_page_owns(…)") == 2` | A later slice legitimately added more guards |
| Seven atomic-write tests | A completed write looks the same with or without a temp file |

The corrective pattern was always the same: make the assertion **count**, name
the specific region, or state in the test body that the property is not
observable from outside and say so.

### The four queued tasks

`ROADMAP.md` §8 records the owner's Claude-architecture work — the living
working status, the two-layer classifier, the streaming tool executor, and
context compression — with the constraint that decides each one's design rather
than only the intent. They start when §1–§7 have no unstarted item left.

## Iteration 5 — 2026-09-30 · batch 5, twelve slices

Commits `a3303de`…`a409bbf`. Canary map 175 → 217. PTY suite 14 → 15 cases.

Batch 5 is the first one drawn from `ROADMAP.md` §9 — the list this programme
wrote about itself — plus the two §6 items it had measured and not fixed. It
is also the first batch where **most of the work was verifying the previous
batches' claims rather than making new ones**, and three of those claims were
wrong.

| # | Commit | Slice |
|---|---|---|
| B5-01 | `a3303de` | Two runs cannot apply each other's patch |
| B5-02 | `1d700e4` | One retry rule, three call sites, no disagreements |
| B5-03 | `fbe6443` | Redaction no longer destroys the evidence it sits next to |
| B5-04 | `1c758c5` | The gate's two unchecked items, now checked |
| B5-05 | `e57767a` | MCP stops leaking a process and stops lying to a model |
| B5-06 | `2e4497e` | Unsound advisories are denied, and the four exceptions say why |
| B5-07 | `67fec56` | The container is hardened, and the tests now say so |
| B5-08 | `01e4f29` | A page letter reaches its page in the shipped binary |
| B5-09 | `bbc4d62` | `niki dashboard` — the one command of 28 with no test |
| B5-10 | `a409bbf` | Both surfaces of one run read the same project's config |
| B5-11 | this commit | Batch 5's record |
| B5-12 | — | Full nine-gate run |

### Three claims in the record that were wrong

The roadmap and the previous batches' reports were treated as hypotheses, not
facts, and three did not survive:

- **"Three unsound advisories."** `cargo audit` reports **four**, across two
  crates. The count was in a document whose whole job is to stay true.
- **"19 of 28 CLI commands have no test."** Re-measured: **one** — only
  `dashboard`. The other eighteen were closed by batches 1–4 and the number was
  never updated.
- **`run_page_ignores_navigation_hotkeys`** asserted that page letters do
  nothing on the Run page. It passed, and the shipped binary opens Diff. The
  test drove a page-local handler and its name described the *product*.

### The theme of the batch: a test that cannot fail

Fifteen times across five batches, a test stayed green while the thing it
claimed to cover was broken. Batch 5's own share:

| Test | Why it passed |
|---|---|
| `the_processor_applies_the_badge` | Asserted `is_retryable_code(` — it is a function *reference* to `is_some_and`, with no parenthesis |
| `no_prompt_instructs_a_model_to_call_an_mcp_tool` | Searched both files for the old instruction; both **quote it in a comment** explaining why it is gone |
| `every_unsound_advisory_is_named_with_a_reason` | Compared against every `ID:` line `cargo audit` prints, including six `unmaintained` ones |
| `the_page_letter_case_exists…` | The canary named a *filename*, which is not a `grep` target, so it resolved against a doc comment |
| `perf_is_machine_independent` | Compared a "cold" and a "warm" pass; `render_once` builds a fresh `TestBackend`, so they were the same measurement — 2% apart, so it failed 1 run in 3 |
| `the_most_recent_run_is_the_one_dashboarded` | The sabotage left `read_dir` order to decide, and it happened to land on the newer task |
| `the_dashboard_escapes_what_it_embeds` | I sabotaged the wrong **file**, so nothing changed and the probe "passed" |

The corrective is always the same three moves: **count** instead of
`contains`, **name the region** instead of searching for a string, and — the
one that took longest to learn — **assert the sabotage actually applied**
before believing its result.

### Next

Batch 3 is complete: twelve slices, and `ROADMAP.md` §1 is closed in full.

Next is `ROADMAP.md` §2. Two items there are now **corrected rather than
pending**, and both corrections matter more than the fix would have:

- The permission modal is not "structurally unreachable in chat" because of a
  missing sender. A plain chat turn calls `stream_reply` — no tools, no
  sandbox — so the modal is unreachable *because chat does not run tools*,
  which is the owner's §0a decision. There is nothing to repair.
- The permission badge was cosmetic, and is now real (B3-12).

What is genuinely left in §2 is one item, and it is a feature: the tool cards
are fully built (`components/tool_card.rs`, `tool_detail.rs`, the Enter
hit-test) and unreachable because the chat sends `tools: None`. Wiring it is
real work and is a product decision about how much of the agent's loop belongs
in a conversation — not a defect.

### Blockers

None.

### Assumptions in force

- The 3 dirty files were an interrupted prior session; only the labelled sabotage was reverted.
- `niki chat` is a **coding agent**, not a viewer (owner decision, §0a).
- For the 12 slash commands that lie, **retracting the claim is the default**.
- A plain chat message stays a conversation turn; `/run <task>` starts the pipeline. Inferring
  "this is a task" from every message would be magic, and the product's ethos is honesty.
- The NVIDIA key lives in the environment only and is never written to a file. G5's secret
  scan is what proves it stayed there, so it runs before every commit.
- The free tier's connection stability, not NIKI, bounds what a live-model run on this box
  can evidence. G4's real-model leg is therefore not a gate.
- When a footer or help page claims something, **retracting the claim beats wiring something
  approximate** — a key that kills the wrong thing is worse than a key that is not there.


Batch 3 is drawn from `ROADMAP.md` §1 (the remaining dead controls: `j/k` dead
on 10 of 14 pages, `Tab` claimed by three, `?` never reaching Help, the Fleet
and Session footer keys), then §2 (chat ↔ pipeline depth), §5 (the remaining
reliability items), and §6 (coverage and hygiene).

### Known cosmetic defect, not fixed

`88694e1` and `e563da2` share a commit subject. The second is the ROADMAP
completion note for the first, and the subject was copy-pasted — so `git log`
shows the deadline fix twice. Correcting it means rewriting history, which is a
stop-and-ask item, and the branch has not been pushed. The *content* of both is
correct; only the second subject is wrong.

### Blockers

None.

### Assumptions in force

- The 3 dirty files were an interrupted prior session; only the labelled sabotage was reverted.
- `niki chat` is a **coding agent**, not a viewer (owner decision, §0a).
- For the 12 slash commands that lie, **retracting the claim is the default**.
- A plain chat message stays a conversation turn; `/run <task>` starts the pipeline. Inferring
  "this is a task" from every message would be magic, and the product's ethos is honesty.
- The NVIDIA key lives in the environment only and is never written to a file. G5's secret
  scan is what proves it stayed there, so it runs before every commit.
- The free tier's connection stability, not NIKI, bounds what a live-model run on this box
  can evidence. G4's real-model leg is therefore not a gate.


## Iteration 6 — 2026-10-01 · batch 6, twelve slices

Commits `84d3941`…`ee3b5d3`. Canary map 221 → 287. PTY suite 15 → 15.
Lib tests 943 → 995.

Batch 6 opened on the largest capability gap in the product and spent its
middle on the process problem that gap exposed. Four of its ten slices are
**record corrections** — the same failure batch 5 found three of, at a
higher rate, because nothing was checking.

| # | Commit | Slice |
|---|---|---|
| B6-01 | `84d3941` | The two human-input tools say what the model can act on |
| B6-02 | `733533a` | §2's tool-card renderer is live; the record said it was not |
| B6-03 | `a6dde45` | `approval` can reach the user in a TUI run |
| B6-04 | `cc8f8a6` | `ask_user` has a modal of its own, and can reach the user |
| B6-05 | `824bb9d` | Four of §6's five claims had rotted; the fifth was two doc lies |
| B6-06 | `01fbd24` | §4.5, §5 and §9.3/§9.4 were four claims the code had fixed |
| B6-07 | `11c4bd5` | The record gets teeth: open claims are pinned |
| B6-08 | `62b7c22` | The recommended backend gets in-file tests, and one finds a bug |
| B6-09 | `8fd69d0` | The default backend gets in-file tests, and its name gets sanitised |
| B6-10 | `ee3b5d3` | The question modal answers a flag the user cannot see |
| B6-11 | this commit | Batch 6's record |
| B6-12 | — | Full nine-gate run |

### The gap: in a TUI run, the model could not talk to you

`ask_user` and `approval` read stdin, and the interface owns stdin — it holds
it in raw mode and runs its own `event::read()`. A `read_line` in a tool would
race that for the next keypress and, in raw mode, return after a single
keystroke with no newline, so a stray `y` typed at the *interface* could be
taken as consent to a command it never showed the user. Failing closed was
correct, and stayed.

What was wrong was the conclusion drawn from it: *nobody is there to ask*.
There is. The interface had been collecting answers for the sandbox's
`PermissionRequest` all along; the two were simply never connected. So:

- `ToolContext` carries an optional `HumanInput` — that channel plus
  `[permissions] prompt_timeout_seconds` — and the pipeline builds it from
  `display.tui_tx()`.
- `approval` puts the command to the interface and waits. It renders the
  **same modal** the sandbox's own prompts use, so a command approved through
  the tool and one approved through the sandbox take one path. No new UI.
- `ask_user` got `components/ask_user.rs`: a cursor you can move, `1`–`9` to
  pick a choice, Enter to send, and **Esc as a cancel rather than a refusal**.

The three outcomes stay three throughout. An explicit refusal says the *user*
refused; an unanswered question says it is a timeout and not a refusal; a run
with no interface says **nobody was asked** and never says "denied by user",
which would attribute a decision to a human who was never consulted.

### Four record corrections, and why the rate went up

Nothing was wrong with the *code* in any of these. The record had moved on and
the record had not:

| Claim | Measured |
|---|---|
| §2's tool cards are "unreachable" | called from two production sites; the chat sends no tools by the §0a decision, the pipeline's render |
| §6's `.niki-worktrees/` is not ignored | `ensure_git_excluded` writes `.git/info/exclude`, and `is_publishable_path` refuses the path |
| §6's `acp`/`goal` destroy the Coder's work | both call `deliver`; `record.branch` holds a branch name |
| §6's `multi_provider.rs` is 8/26 tautological | 28 tests, every provider case asserts an endpoint; 27 prior tests stayed green when one was hardcoded |
| §5's Google never emits `Finish` | `google.rs:308-311`, with 7 wiremock tests |
| §5's Coder loop swallows errors with `.ok()?` | an `Err` arm writing to stderr since batch 2 |
| §9.3's agent retry matcher misses 500/502 | it reads the status and judges it with `send_request`'s predicate |
| §9.4's `git.rs:158` fixed temp path | that line is the `git diff` error arm |

The fifth §6 claim was **real**, and it was not the code: the README feature
table and `niki.example.toml` both told a user their MCP tools are injected
into agent prompts. They are not. Both now say so, and two tests hold them
there.

B6-07 is the answer to the pattern. Every open claim that reduces to something
checkable is registered in `tests/record_claims_are_pinned.rs` with the test
that checks it, and that test must itself be in the canary map — so a claim
cannot be left unpinned, a pin cannot point at a test that no longer exists,
and a **new** numbered item naming a source with no row fails the gate.

`ROADMAP.md` gains a §0 saying so: the record is re-measured, not appended
to; a bullet is not evidence; a code comment is not evidence.

### The pin, fired and retired

`the_docker_backend_still_has_no_unit_tests` was written in B6-07 and
deleted in B6-09, in the same commit that made it false. §6's bullet went with
it. That is the cycle the pin was built for, run to completion for the first
time: the failure *is* the signal to update the record.

### Five defects found by writing the tests

| Found by | Defect |
|---|---|
| `a_tiny_terminal_does_not_break_the_permission_modal` | the modal clamped its height to the area and then floored it at 8, so a short terminal produced a box **taller than the screen** and the renderer indexed outside the buffer — a panic, on resize, with a destructive command waiting on the answer |
| `dismissing_a_question_is_not_an_answer` | a dismissed question reported `Success`: `matches!(answer, Questioned(_))` is true for a cancel |
| `teardown_takes_this_task_and_its_siblings_and_nothing_else` | `cleanup_worktrees_for_task` matched any `<id>-` prefix, so Ctrl+C for `task-1` deleted `task-1-backup`; a sibling is always `<id>-<digits>` |
| `the_greedy_walk_scores_below_the_floor_where_the_table_does_not` | (the test found no defect — it found that **its own first example was a false green**, see below) |
| `every_role_produces_an_acceptable_container_name` | the container name is `{:?}`-formatted from a role and passed through no sanitiser; a role name that is not a plain identifier fails the run at create, with an error naming a string the user never typed |

### The theme, again: a test that cannot fail

Nine false greens this batch, and the count is the number of times a test was
written and *believed* before it was broken:

| Test | Why it stayed green |
|---|---|
| `a_tool_call_becomes_a_card` | asserted `ToolCard::new(`; a sabotage that kept the call and replaced its **arguments** passed |
| `an_open_question_takes_every_key` | asserted `Consumed`; with the question wired out of the ladder, something *behind* it consumed the key |
| `the_coder_loop_reports_its_failure` | searched the whole function for `eprintln!`; an unrelated one four lines away satisfied it |
| `a_new_numbered_item_…_is_accounted_for` | read the table opener `\| 9.9` as a line with no leading number, so a new unpinned row sailed past the check the file exists for |
| `the_greedy_walk_…_where_the_table_does_not` | its "mis-remembered line" example passed with the exact table switched **off** — it was not witnessing the regression at all |
| `two_runs_get_different_names` (docker) | compared `name + "x"` with `name + "y"`, which cannot differ |
| `the_sanitiser_matches_the_runtimes_rule` | asserted `is_acceptable(out) == true` on every row, so a sanitiser that mangled valid names passed |
| `every_pinned_claim_is_still_an_open_bullet` | flagged §9.2, a **struck** bullet carrying a live sub-claim — and was deleted rather than fixed, because it was wrong |
| `a_short_string_is_not_truncated` | my own miscount: `"exactly-10!"` is 11 characters |

The three corrective moves are now reflexes: **count** rather than `contains`,
**name the region** rather than window it, and **assert the sabotage applied**
before believing its result. Two sabotages in this batch did not apply at all
because `cargo fmt` had rewrapped the anchor, and the test passed anyway.

### Next

§9.1 (resuming a pipeline from a checkpoint) and §9.2 (MCP's agent→server call
path) are the only two open items left that are not a decision, and both are
recorded as decisions. §9.8 is the unpushed branch. Everything in §1, §2, §4,
§5 and §6 is closed. `ROADMAP.md` §8 — the four Claude-Code-architecture tasks
— is queued and untouched, per the owner's ordering.

### Blockers

Unchanged from batch 5; see `BLOCKERS.md`. **B1** (no push, so G8 stays red) is
the owner's standing decision, not a failure to fix.

### Assumptions in force

- A run has one interface per process, which is what lets a single
  `HumanInput` channel stand in for "whoever is driving this run". Two TUIs in
  one process would need a per-run channel.
- `tokio`'s `spawn_blocking` is available wherever the tool loop runs, so
  `ask_permission` awaits rather than calling `block_in_place` — the sandbox's
  own prompt still does, and would panic on a current-thread runtime.
- The 8-character container-name truncation is a known ~1-in-4·10⁹ collision
  window, chosen over a longer name for readability, and now asserted as a
  window rather than assumed away.

## Iteration 7 — 2026-10-01 · batch 7, nine slices

Commits `0e52610`…`aab7685`. Canary map 303 → 317. PTY suite 15 → 16 cases.
Lib tests 995 → 996.

Batch 6 ended with the ranked list drained and two items left that were
records rather than repairs. Batch 7 closed §9.2, split §9.1 into a part that
needed no decision and a part that does, and built the harness that made the
remaining gap visible at all.

| # | Commit | Slice |
|---|---|---|
| B7-01 | `0e52610` | The MCP servers were alive for four lines |
| B7-02 | `df976ed` | A discovered tool is a `Tool` a model can call |
| B7-03 | `c271956` | The summary stops denying what now works, and the flag fires |
| B7-04 | `1f05172` | The research loop registers MCP tools too |
| B7-05 | `ee7747b` | A salvaged run hands a human a diff, not JSON |
| B7-06 | `dae43c9` | The mock server can script any tool call, in order |
| B7-07 | `9592755` | The first pty case that runs a pipeline |
| B7-08 | `e9bcdf2` | §9.2a's loop half, measured |
| B7-09 | `aab7685` | §9.2a's interface half, so one explanation is left |

### §9.2: the call path was pointing at corpses

The roadmap said the agent→server call path "is still a feature, not a
repair", because the manager was scoped to a block. Measuring found something
worse than unreachable: **the manager was a local of the discovery block**, the
connections dropped with it, and `kill_on_drop(true)` killed the stdio
children. Every configured server was dead before the Planner's first token,
and `call_tool` was not an unreachable function — it was pointing at corpses.

A server that is started and immediately killed is *worse* than one never
started: the user got a summary naming tools that were already gone, and paid
the spawn cost either way.

Four slices closed it. The manager is an `Arc` held for the run; a discovered
tool becomes a `Tool` named `mcp__{server}__{tool}`, so a server's `bash`
cannot shadow NIKI's; the permission mirrors the governance; and
`isError: true` is a failure rather than a payload that reads like an answer.

**The flag fired.** `the_missing_call_path_is_still_recorded_as_missing`
existed so closing the gap would be a visible event; its assertion was
`callers == 0`, and there are now callers. Its replacement pins the other
side, because a feature that is wired can also be unwired quietly.

### §9.1 was two things, not one

`niki resume` tells a user to re-run the task — which spends the Planner and
the Coder again and produces a *different* diff, because an LLM is not
deterministic. So salvaged work was thrown away for want of a way to look at
it, and the thing blocking that was not "resume mid-pipeline". It was that a
failed run wrote the change as **JSON** while the Run page, the completion
screen and `niki report` all point at `changes.patch`, which a failed run
never wrote.

`render_salvaged_patch` renders it in a scratch copy. The working tree is
never touched, no branch is cut, and the patch carries its own warning. The
path rewrite was wrong three times and only a real `git apply --check` caught
it: `git diff --no-index` prints its prefix *concatenated* with the path, so
stripping the leading-separator form first leaves `asrc/lib.rs` — the prefix
welded to the file. Every intermediate version looked right, because the
hunks were right.

Resuming the pipeline partway stays a decision (§9.1), now with both halves
named.

### The harness, and what it immediately found

Every pty case drove `niki chat`, which by §0a sends no tools — so
`ask_user` and `approval` had **no** end-to-end coverage. The mock's tool
loop was two hardcoded calls, so no test could drive any other tool.

`16_agent_asks_the_user` runs the whole chain and found a crash on its first
execution: the mock's `anthropic_json_response` referenced `scripted` without
computing it, raised `NameError`, and killed the handler's connection. B7-06's
own tests only drove the OpenAI path. **A feature scripted on one provider and
run on the other is tested on both, or it is a trap.**

### §9.2a, narrowed twice

The case found that after an answer the run does not reach a verdict. Two
follow-up tests closed the obvious explanations:

| Test | Excludes |
|---|---|
| `a_questions_answer_reaches_the_loop_and_it_moves_on` | the tool loop — the question is asked once, the answer is in the conversation, the artifact is produced |
| `a_question_closes_when_it_is_answered` | the interface's state machine — the modal closes, the request is taken, the field is cleared, focus returns, the tool gets the text typed |

So what remains is how `niki chat`'s event loop and a live run interleave —
the one dimension no unit test covers, because every unit test drives the
state machine directly rather than through a running pipeline. Written down
in §9.2a so the next reader does not re-run either.

### Process failures in this batch, recorded because they cost time

- **A commit shipped with a clippy failure.** `cargo clippy … | tail -2` makes
  the pipeline's exit status `tail`'s, so the `&&` chain continued. Clippy was
  right about a real defect: the mock's startup-timeout path leaked a python
  process. *A pipe at the end of a gate hides the gate.*
- **Every one of the five signature threads for the MCP manager was attempted
  with a regex first**, and all five went wrong in ways the compiler caught —
  misplaced arguments, a regex that reached into a macro, a `mcp` that landed
  after the parameter it was meant to precede. Done by hand in the end. The
  roadmap's estimate ("four signatures") was low; it is five.
- **Two harness lessons, both paid for in runs.** `tui_send` and `tui_type`
  are different operations: tmux parses a bare argument as a key *name*, so a
  digit sent that way never arrived and the answer never reached the modal —
  which reads exactly like a modal that ignores the keyboard. Making `-l` the
  default then broke `Enter` in nine cases. And the mock's stderr now goes to
  a file, because a harness that cannot explain its own failure teaches the
  reader to guess.

### Next

§9.2a, from a narrowed position. §9.1 (resuming the pipeline) and §9.8
(unpushed) are decisions, not repairs. `ROADMAP.md` §8 — the four
Claude-architecture tasks — is queued and untouched, per the owner's ordering.

### Blockers

Unchanged; see `BLOCKERS.md`. **B1** (no push, so G8 stays red) remains the
owner's standing decision, and the local `scripts/manifest-parity.sh` proves
that the one job it covers is green on this tree.

### Assumptions in force

- One interface per process is what lets a single `HumanInput` channel stand
  in for "whoever is driving this run"; two TUIs in one process would need a
  per-run channel.
- The container name's 8-character id truncation is a known ~1-in-4·10⁹
  collision window, chosen over a longer name for readability, and now
  asserted as a window rather than assumed away.
- A salvaged run's recovery is inert by design. `render_salvaged_patch` makes
  that a tested property rather than a comment.

## Iteration 7b — 2026-10-01 · batch 7, the live half

Commits `49c5c31`…`e7bba55`. Canary map 323 → 335. PTY 16 → 16.

The first half of batch 7 closed the two ranked items that were records
rather than repairs. This half is what a **real model** found, which no mock
in this repository could: six defects, three of them in the Coder's tool loop,
none of them reachable from a scripted server.

| # | Commit | Slice |
|---|---|---|
| B7-14 | `49c5c31` | A model that finished the work is asked to submit it — **live-verified** |
| B7-14b | `d522b82` | A failed run that never checkpointed said "os error 2" |
| B7-15 | `d99ea03` | An artifact edit the tool loop already made is not a failure |
| B7-16 | `89968ea` | A bare artifact in prose is recovered, even behind another brace |
| B7-17 | `e7bba55` | A path the agent removed must not empty the whole diff |
| B7-18 | `84827b5` | Batch 7's records and a full nine-gate run |

### What a live model showed that no mock could

The Coder's tool loop ran six steps — `read`, `edit`, `bash`, the edit applied
and compiling — and then said *"The change is in place and compiles cleanly"*
without ever calling `submit_artifact`. `recover_submission` found no JSON in
the prose, returned `None`, and the caller **discarded the loop and re-ran the
whole Coder one-shot**. The work was in the worktree the entire time.

That is §9.3, and the loop now asks once — plainly, naming the tools already
used — before it gives up. **Live-verified**: the same model and task then
submitted in 36 s.

Three more followed, each from the same run or its successors:

- **§9.3b — a double-apply.** The Coder used the edit tools *and* submitted
  an artifact, so the artifact re-applied what the tools had already written
  and every block's `search` text was gone. My first reading — "a no-op edit
  was accepted" — was **wrong**, and one grep corrected it: the validator
  already rejects a no-op. Both sandboxes now treat an edit as already applied
  when its `search` is absent, its `replace` is present, and the `replace` is
  big enough for its presence to mean something. Not in the `edit` tool,
  where a no-op is what the user asked for.
- **§9.3c — a bare artifact in prose, dropped.** `first_json_object` took the
  **first** `{` in the text, so a model whose prose contained a brace before
  the artifact started the span in the wrong place. Writing the test also
  found a pre-existing weakness: the "looks like an artifact" check was
  `edits.is_some()`, so a *description* of the shape was taken at its word.
- **§9.5 — a stale path emptied the diff.** The agent's file list accumulates
  across revision rounds; one the Coder declared and a later revision removed
  made `git add -N` fail for the **whole invocation**, so every genuinely new
  file dropped out of the patch. The run warned "a brand-new file may be
  missing". It may indeed have been.

### Two models, two failures, and that is the point

`stealth/space-bunny-alpha` runs the whole pipeline. `poolside/laguna-s-2.1:free`
cannot finish a task at all — its Planner emits no conformant artifact — and
*that* is what exposed the recovery path reporting `No such file or directory
(os error 2)` on a run that had already failed for a perfectly good reason.

A live model is not only a way to find defects on the happy path. **A model
too weak to get through stage one still fails loudly and early**, which is how
the failure paths get exercised for real.

### The final run

A full pipeline, release build, `--backend worktree`, and **no "the patch did
not apply" anywhere** — including across a revision round, which is where
§9.3b used to bite:

| Stage | Result |
|---|---|
| Planner | 47s |
| Coder | 90s — submitted |
| Tester | 5/8 passed, 4 edge cases identified |
| Reviewer | **Revision needed** — 2 critical, both correct |
| Coder | 48s — the spurious file gone |
| Tester | **7/7 passed** |
| Reviewer | **Approved** — 10/10, 10/10, 9/10 |

### Three times in two slices, a proof that did not run

The recurring failure, and the reason §0 exists:

- A can-fail entry whose test name did not match my filter: `0 passed`, which
  reads like a pass unless you read the count.
- A test that drove the shared helper and not the worktree backend's own
  inline check, so removing the latter left everything green.
- A test whose *premise* was broken by its own fixture — it named `src/lib.rs`
  as a removed file, and `repo()` creates `src/lib.rs`.

Each was found by reading the count, not by trusting the word "ok".

### Blockers

Unchanged, plus **B6**: `stepfun/step-3.7-flash` is unreachable on the
supplied key (*"this account never purchased credits"*), and `:free` models are
unevenly rate-limited. `poolside/laguna-s-2.1:free` and
`stealth/space-bunny-alpha` answer consistently. The key is environment-only;
G5's secret scan over the tree *and* git history passed before every commit.

---

## Iteration 7c — §8 T4, context compression (B7-22)

### What was measured first

Two compressors already existed and **neither touches the conversation the
model is in**:

| Existing | Operates on | Callers in `src/` |
|---|---|---|
| `memory/compression.rs` | knowledge between stages, writes a block to disk | one — `let _ = compress_context(…)` at `pipeline.rs:2450`, result **discarded** |
| `runtime/compaction.rs` | a `ContextStore` of typed `Fragment`s | **zero** |

`grep -rn "ContextCompactor" src/` returns nothing outside that module's own
tests. So the loop's `Vec<LoopMessage>` grew unbounded and nothing noticed.

### What shipped

`src/runtime/transcript.rs` — the first compressor that operates on the loop's
own transcript, called at `src/runtime/tools.rs:4055` after a tool result is
pushed and before the next request. Named for what it compresses, not for
`compaction`, which is a different thing with no callers.

Three strategies, in §8's order: snip duplicate system messages (lossless),
microcompact long tool results (head+tail, marks the middle gone), collapse
long file reads (head/tail lines). `Summarise` is **deliberately unbuilt** — it
needs a model call and loses detail — and a test pins that no run ever reports
it.

Two decisions that are not obvious from the code:
- **Only `ToolResult` turns are compressed.** Eliding assistant prose leaves the
  model reasoning about a conversation it no longer has; eliding a user
  instruction is a task that changed with nobody deciding it should.
- **The trigger is content size, not §8's 95%-of-window.** The loop has no
  context-window figure. Inventing one would put a number in the code that
  looks authoritative and is not.

### Evidence

```
$ cargo test -j 2 --lib -- --test-threads=1
test result: ok. 1068 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

$ cargo clippy --all-targets -j 2 -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 35.57s

$ ./scripts/verify.sh --only G3    → PASS  400 can-fail entries all resolve
$ ./scripts/verify.sh --only G5    → PASS  no credentials in the tree or in history
$ ./scripts/verify.sh --only G7    → PASS  every README command parses
$ ./scripts/verify.sh --only G9    → PASS  no todo!/unimplemented! in production code
```

**8/8 sabotages bit** (9 with the call site): the report emptied while the
transcript shrank; the strategies reordered; duplicates kept; elision emptying
the body; user turns compressed; a line printed when nothing happened; a report
overcounting by one character; a file quoting the marker skipping compression;
and the `compress()` call deleted from the loop.

The eighth did not bite on the first run — the guard had been added to *one* of
the two elision strategies, so the other still dropped the middle and the test
stayed green. Guarding both made it red. That is the eighth time in this
programme a sabotage has needed a second look, and the reason §0 says to read
the count and to ask whether the *sabotage* was wrong before assuming the test
was.

Two of my own tests were wrong before they were ever run: one asserted a marker
appears exactly once when the content itself quoted it, and the other compared
the second request against the wrong direction. Both were rewritten to assert
something true, not weakened.

### §8 T3's concurrency half — measured, and closed on evidence

§8 promises *"reads parallel, writes exclusive … this will make NIKI feel 2-3x
faster"*. Measured before building, through the real `ToolRegistry::execute`:

| | 3 `read` calls, 400-line files |
|---|---|
| sequential | **1.62 ms** |
| joined concurrently | **0.88 ms** |

**0.74 ms per turn**, against a model request measured in seconds. The
schedulable set is smaller than it reads: `bash` has no path (exclusive by
construction), `web_fetch` takes a `url` not a `path` (also exclusive), and
`grep`/`glob` touch no single path — which leaves reads of different files at
about half a millisecond each.

So wiring the scheduler into `run_tool_loop` would restructure ~170 lines of
the hottest code in the repository to buy less than a frame. **Not done**, and
the measurement is in ROADMAP.md §8 T3 so the idea is not re-derived from §8's
estimate later.

The latency in §8 is in the *streaming*, not the parallelism: mid-stream
dispatch needs the loop to call `provider.stream()` where it currently calls
`provider.complete()` (`src/runtime/tools.rs:3727`). That is a separate change
with separate risk.

`runtime/path_lock.rs` and `runtime/scheduler.rs` stay — they are the safety
content, both are can-fail proven, and neither is wired into the loop. That is
stated rather than left to be discovered.

---

## Iteration 7d — §8 T2's hook layer (B7-26)

`src/risk/hooks.rs`, 7 tests, 7/7 can-fail proven.

§8's order is **static deny → hooks → classifier**, and the reason it insists on
that order is the whole reason this layer exists. So the property to hold is not
"the hooks deny things" — it is **"the classifier is never consulted once the
hooks have decided."** That is the first test, and it needed the most work to
make bite.

No new rule format: the rules come from `permissions::PermissionConfig`, which
`niki.toml` already deserialises into. `Ask` is deliberately not a decision.
A hard denial does not charge the model's denial tally — a user's own rules
denying twenty commands must not trip the escalation limits and fail the run.

### Two tests were wrong before they proved anything

- A determinism test that built the same `Hooks` twice and compared **cannot**
  fail when the sort is removed: `HashMap` order is stable within a process. It
  now asserts the outcome the sort produces — the lexicographically-first
  pattern wins — and fails against a reversed order every time.
- A test that called the `pending()` helper directly **cannot** fail when its
  caller stops calling it, which is the sabotage that would leave the hooks
  deciding nothing while every other test stayed green. It now drives
  `adjudicate` end to end.

Both were found by reading "1 passed" on a sabotage and asking what the
sabotage actually did, rather than accepting the green.

### Evidence

```
$ cargo test -j 2 --lib -- --test-threads=1 risk::hooks
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 1068 filtered out

$ cargo clippy --all-targets -j 2 -- -D warnings    → clean
$ ./scripts/verify.sh --only G3 → PASS  407 can-fail entries all resolve
$ ./scripts/verify.sh --only G5 → PASS  no credentials in the tree or in history
$ ./scripts/verify.sh --only G7 → PASS  every README command parses
```

### Not built

An `ActionClassifier` backed by a real provider. The trait, the gate, the
escalation limits and the reasoning-blind view are built and tested; what is
missing is the thing that talks to a model, which needs a provider and a model
choice — a decision for the owner, not a slice.

---

## Iteration 7e — B8-01, B8-02

### B8-01 · §8's T1 was half-rendered

`resolved_line` was built, had nine unit tests, and had **zero callers outside
them**. `render_activity_spinner` only ever drew `working_line`, and the strip
was rendered only `if state.has_running_stage()`, so the line the brief asks for
— *"On finish, resolve to ⎿ Thought for Xs · N tokens"* — was never on screen.
The component's tests all passed; a component's tests cannot see that nothing
draws it. Third time in this repository a component shipped complete, tested and
inert, after `working_status` and `input_probe`.

`AppState::resolved_run` is set when the **last** running stage finishes — not on
every stage, because "Thought for 12s" under a run that still has a Reviewer to
go is a claim about the wrong run. Tests assert on the rendered `TestBackend`
buffer. Can-fail proven twice: removing the render branch, and never setting the
state.

### B8-02 · §9.2a closed — the failing case never produced a screen

The row said the next step was to read the trace. **There was nothing to read.**
All twelve `.failure.txt` files were **0 bytes**. Two causes, both in the
harness:

1. `run.sh` runs each case in a subshell and `tui_begin` installs
   `trap 'tui_kill; …' EXIT` inside it, so tmux was dead before the parent
   captured.
2. Every case sources `lib.sh`, whose line 16 is `set -euo pipefail` — which
   undoes the harness's own `set +e`, so the subshell exited at the first
   failing assertion: the exact path that needed to reach the capture.

Fixed by capturing **inside the EXIT trap, before the teardown**.

**The obvious fix was wrong and was caught.** `set +e` inside the subshell —
which cause (2) invites — makes a case whose third assertion fails and whose
last command succeeds report **OK**. Verified with a probe case written to fail
midway then `true`: it reports **FAIL**, with a 2 200-byte capture. Both probe
cases deleted; the suite runs 16 cases again.

### Evidence

```
$ bash tests/tui_smoke/run.sh --bin ./target/release/niki
RESULT  pass=16  fail=0  skip=0

$ ./scripts/verify.sh --only G3 → PASS  409 can-fail entries all resolve
$ ./scripts/verify.sh --only G5 → PASS  no credentials in the tree or in history
$ ./scripts/verify.sh --only G7 → PASS  every README command parses
```

**Two probes in one slice**, and the second one is the point: the first version
of this fix would have made a failing suite green.

---

## Iteration 7f — B8-04, B8-06, and the gate gap

### B8-04 · the errored Coder loop is now billed

`LoopSpend` is a running tally the caller keeps whether the loop succeeds or
fails, mirrored onto **every completed step** rather than filled in on the way
out — everything after that point can exit through `?`. `run_tool_loop_spending`
is a new entry point; `run_tool_loop_with` keeps its signature and discards the
value, which is correct for a caller that does not bill.

**The test that was checking this could not see either bug.** Source-level, and
**green twice** against code that did not bill at all, and green again against
code that billed a hard-coded zero. The call is there; the call does nothing.
So the hard-coded `bails == 2` is gone (a snapshot — bumping it to 3 would have
hidden the defect for ever), replaced by "every `return None` has a billing call
before it", plus a behavioural test that drives a provider which answers once
and then fails and asserts the tally carries the **4 242** input tokens it was
charged for. Bites: mirroring only on the success path gives `left: 0, right:
4242`.

**Measured, not assumed:** replacing the pipeline's `&loop_spend.usage` with a
hard-coded default leaves both tests green. The mechanism is held behaviourally;
the one-line wiring is held only by the source check. Recorded as B8-05.

### B8-06 · the third red test

`a_truncation_that_never_resolves_is_reported_as_truncation` counted *requests
containing* the truncation notice rather than notices *issued* — and the notice
is appended to the conversation, so every later request carries it in its
history. It now asserts `out.feedback_turns == 1`, which is the count, and bites
correctly: `MAX_TRUNCATED_ANSWER_RETRIES = 2` gives `left: 2, right: 1`. The
old version could not have detected that.

### The finding underneath all three

**No gate runs the test suite.** G3 checks that each canary entry names a test
that exists; it does not run the tests, because the suite does not fit this
machine. So the canary map cannot tell you the suite is green — only CI can.

Running two suites by hand found **three red tests** while every gate reported
PASS: two real defects in the resilience and accounting paths (failover not
failing over; a failed Coder loop billed as free) and one test that could not
see a change. All three are fixed or corrected in batch 8, and the gate gap is
now in `RELEASE_REPORT.md` §5 rather than left as knowledge someone has to
rediscover.

---

## Iteration 7g — B8-07, the coverage gap closed

Last slice left a measured hole: with the pipeline's `&loop_spend.usage`
replaced by `TokenUsage::default()`, **both** the source-level check in
`tests/reverse/money.rs` and the behavioural test in `runtime::tools` stayed
green. Neither reaches the layer that turns a tally into a `StageMetric`.

`a_coder_loop_that_failed_still_leaves_a_bill_behind` drives the real
`run_coder_tool_loop` — private, so the test lives in `pipeline.rs` — with a
provider that answers once and then fails, and asserts on the metrics the
pipeline is left holding: exactly one, carrying the **3 131** input and **77**
output tokens the model was actually charged for.

| Sabotage | This test | source-level check |
|---|---|---|
| bill a hard-coded zero | **RED** (`left: 0, right: 3131`) | green |
| skip billing entirely | **RED** (`left: 0, right: 1`) | green |

**The test was wrong before the code was.** It passed
`"code_diff.schema.json"` where the real caller passes
`"schemas/code_diff.schema.json"`, so `load_asset` missed the embedded copy and
the function returned `None` on its third line without spending anything — and
the `metrics.len() == 1` assertion caught it rather than the test passing for
the wrong reason. That is the *same class of bug* the function's own comment
warns about: `load_asset` splits on the first `/`, a bare name misses the
embedded lookup, and the caller reads `None` as "the loop never ran".

```
$ ./scripts/verify.sh --only G3 → PASS  413 can-fail entries all resolve
$ ./scripts/verify.sh --only G5 → PASS  no credentials in the tree or in history
$ ./scripts/verify.sh --only G7 → PASS  every README command parses
$ cargo clippy --all-targets -j 2 -- -D warnings → clean
$ cargo test -j 2 --lib → 1079 passed; 0 failed
```

---

## Iteration 7h — B8-08, a KNOWN_FAILING entry that was no longer failing

`KNOWN_FAILING` listed `INV-VERDICT-NOT-FABRICATED` as *"SingleAgent assigns
`verdict = Verdict::Approved` without a Reviewer (`pipeline.rs:2395)`"* — a line
number that has moved, for code that no longer says what the entry claims. The
product now emits `RunOutcome::SelfVerified`, derives the final verdict from
the outcome, and gates `Reviewed` on `reviewer_ran && verdict_source.is_some()`.
A **real** run asserts it: `tests/run_lifecycle.rs` reads `task.json` back and
checks `outcome.outcome == "self_verified"`.

So the entry was reporting a defect no run produces, and the ratchet was
pinning the programme to a **synthetic** failure. Struck — and the strike held
in both directions: the shape the product writes must pass, the shape the
invariant forbids must still fail. Both proven by sabotage, plus deleting the
registration entirely.

**The replacement assertion was vacuous the first time it ran.**
`is_completed()` looks for `"Completed"` with a capital C; the draft used
`"completed"`, so every check returned `pass()` at the top — including the one
that appeared to prove the strike was safe.

That is worth the record. The usual failure this programme hunts is a red test
that should be green. This one was the reverse: **a green test that proved
nothing**, and the only reason it was caught is that the second half of the same
assertion was supposed to fail and did not. If the fabricated trace had been
written first and the honest one second, both would have passed and the strike
would have shipped with the check effectively deleted.

```
$ ./scripts/verify.sh --only G3 → PASS  414 can-fail entries all resolve
$ ./scripts/verify.sh --only G5 → PASS
$ ./scripts/verify.sh --only G7 → PASS
$ cargo clippy --all-targets -j 2 -- -D warnings → clean
```

---

## Iteration 7i — B8-09, a KNOWN_FAILING entry whose check was dead

`INV-STAGE-MANIFEST` said *"a SingleAgent run records no stage metrics at all"*.
The cause was real (fixed in B8-04), but the entry was false against the
product — and proving that turned up something worse.

**A real run meters both stages.** `tests/run_lifecycle.rs` now drives the
pinned `topology = "singleagent"` path against the real mock and reads
`task.json` back: metered roles **`["planner", "coder"]`**. The ratchet could
never have established this — it only sees synthetic traces, and a synthetic
trace cannot say whether the pipeline meters a topology it has never run.

**The invariant could never fire.** It asked
`topology.contains("Single") && executed.is_empty()`. `TopologyMode` is
`#[serde(rename_all = "lowercase")]`, so a real run records `"singleagent"` and
the capital-S comparison never matched. The check existed to catch a SingleAgent
run that metered nothing and was structurally unable to catch one by a SingleAgent
run — it only ever fired on the hand-built trace that justified keeping it. That
is the whole entry: a check held open by its own reproduction.

Now case-insensitive, held in both directions, and can-fail proven: restoring
the capital-S comparison goes red, and a real run with `agent_metrics` emptied
goes red.

**A note on how the first version of this assertion was worthless.** It asserted
that `agent_metrics` was non-empty on the fast path, and it passed — because the
Planner's metric alone makes it non-empty, so it said nothing about the Coder.
Disabling the Coder's billing entirely did **not** turn it red. The version
shipped asks for the `coder` role by name, and that one bites.

---

## Iteration 7j — B8-10, and `KNOWN_FAILING` is empty

The last entry said *"artifact schemas declare no minItems/minLength, so a no-op
validates cleanly"*. **False against the shipped schemas**: `code_diff.schema.json`
declares `"minItems": 1` on both `edits` and `files_changed`, and
`validate_artifact` rejects an empty diff with *"[] has less than 1 item; [] has
less than 1 item"*. The protection is in the schema, where it belongs. The
failure *message* repeated the false claim, which would have sent a reader
looking for a schema bug that is not there.

The check also had the opposite problem. `is_semantically_empty` read `summary`
— a field `review_verdict.schema.json` has not had for some time — so `has_text`
was permanently false and **every approved review with no issues was flagged
hollow**. A clean approval is the correct outcome. An audit-style check that
cries wolf on a perfect review trains a reader to ignore it.

Now reads `overall_assessment`, keeping `summary` as a fallback. Proven both
ways: restoring the `summary`-only read makes a clean approval fail; disabling
the hollow branch makes a genuinely hollow verdict pass.

**`KNOWN_FAILING` is now empty.** All four entries struck in batch 8, each
against a measurement. The ratchet's scope was corrected on the way: the first
draft generalised "every entry must fire on this trace", which failed on
`INV-TERMINAL-SAFE` — reproduced by a *different* trace. A check written to be
exhaustive rather than true drifts into demanding things nobody wanted.

---

## Iteration 7k — B8-11, records that outlived the thing they described

Chasing a stale comment turned up four more, in three files, all the same shape:
**a check kept a record alive after the record stopped being true.**

**A pin pointing at a test that was deleted.** `tests/record_claims_are_pinned.rs`
held the claim *"§9.2 · MCP has no agent→server call path"* pinned to
`the_missing_call_path_is_still_recorded_as_missing`. That test was **retired the
day the call path landed** (B7-01) — the pin was doing its job as a flag to
close, and the flag was never cleared. `every_pinned_claim_names_a_test_that_exists`
had been **red**, and the canary map carried `mcp-gap-still-recorded` pointing at
the same deleted test, which is how **G3 failed** when I first ran it after the
edit. The gate caught it; the record did not.

Replaced with the true claim — *"§9.2 · MCP has an agent→server call path that
works"* — pinned to `a_discovered_mcp_tool_can_actually_be_called` in
`tests/mcp_call_path.rs`, which drives a real JSON-RPC server over stdio.

**Seven numbered items nothing pinned.** §1.1, §1.2, §1.7 and §4.1–§4.4 all
name a source (`file.rs:`) and so make checkable claims, and the pinning test
demanded rows for them. §4.1–§4.4 carry their strike **inside** the cell
(`| 4.1 | ~~Arbitrary host file write…~~ **DONE in batch 2** |`) rather than at
the front, so the "struck items need no pin" rule did not recognise them. All
seven now have pins to real holders, and all seven resolve to canary entries.

**A comment that had quietly become wrong.** `pipeline.rs` said
"`McpManager::call_tool` has no production caller" and "`shutdown()` has no
production caller either". Both false: `mcp_tool.rs:192` and `pipeline.rs:4501`.
They are now explicitly **past tense**, kept rather than deleted — so the next
reader who greps for "no production caller" learns the sentence is a record, not
a claim.

**A roadmap line contradicting the row above it.** §9.3 ended with "Needs a live
model to prove" — under a row whose every bullet was struck and whose own table
already recorded a full live run (B7-14, B7-18). Struck.

**A failure message restating a false cause** was fixed in B8-10; this slice's
lesson is the same one from the other direction — the *checks* can keep a lie
alive just as convincingly as a comment can.

```
$ ./scripts/verify.sh --only G3 → PASS  416 can-fail entries all resolve
$ ./scripts/verify.sh --only G5 → PASS
$ ./scripts/verify.sh --only G7 → PASS
$ cargo clippy --all-targets -j 2 -- -D warnings → clean
$ cargo test --test record_claims_are_pinned → 3 passed
$ cargo test --test reverse → 95 passed
$ cargo test --test run_lifecycle → 14 passed
```
