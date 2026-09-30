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

### Next

T3 · **A task typed in the TUI runs the pipeline.** The core promise (C-J2). `cli/chat.rs` contains
no reference to the orchestrator at all, so nothing typed in the default front door can produce a
branch. Every other TUI page is dead code until something populates `state.stages`.

### Blockers

None.

### Assumptions in force

- The 3 dirty files were an interrupted prior session; only the labelled sabotage was reverted.
- `niki chat` is a **coding agent**, not a viewer (owner decision, §0a).
- For the 12 slash commands that lie, **retracting the claim is the default**.
