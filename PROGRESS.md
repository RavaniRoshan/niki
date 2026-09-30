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

