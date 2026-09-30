# RELEASE REPORT — NIKI hardening programme, batches 1–3

Branch `niki/hardening` · base `c7e297d` · 42 commits.

Produced by executing `plans/namor-obsidian-jay-garrick.md`, with the four owner
decisions in §0a. Every gate below was produced by `./scripts/verify.sh`; the raw
output is in `EVIDENCE.md` and `.evidence/`.

Batches 2 and 3 are §2b and §2c. Batch 1 is §2.

---

## 1 · The gate table

Run of 2026-09-30, after batch 2's last commit. Raw output in `EVIDENCE.md`.

| Gate | Result | Evidence |
|---|---|---|
| **G1** clean clone, install, README quick start | **PASS** | all scripts parse; every published download URL resolves; `niki 0.9.0 — README quick start commands accepted` |
| **G2** build, lint | **PASS** | `cargo fmt --check`; `cargo clippy --all-targets -D warnings` warning-free; debug and release both build |
| **G3** tests + can-fail map | **PASS** | lib **953**; `run_lifecycle` **14**; **65 can-fail entries** all resolve to a real test |
| **G4** the core flow, for real | **PASS** | `run_lifecycle` 14; `chat_runs_the_pipeline`; `scripts/demo.sh`; the tmux suite **13/13** |
| **G5** security | **PASS** | `cargo deny check` ok; `cargo audit` ok (11 pre-existing allowed warnings); **no credentials in the tree or in history** |
| **G6** failure paths | **PASS** | every failure path exits non-zero *and* says something a person can act on |
| **G7** docs match reality | **PASS** | the audit's counts are re-derived from the tree; every README command parses against the real binary |
| **G8** CI green | **FAIL — by construction** | the branch is committed locally and **not pushed**, per the owner's decision. The last *pushed* run's one red job is `Manifest parity`, the version skew this branch fixes. |
| **G9** no dead code, no fake features | **PASS** | no `todo!`/`unimplemented!` in production code; no `assert!(true)`; every subcommand appears in the README |

**Eight of nine pass locally. G8 is red because the branch has not been pushed**,
and that is the honest state rather than a workaround: it is the one gate this
programme cannot satisfy without the owner.

### The gate caught batch 2's own drift

Worth recording, because it is the second time. `G7` failed on the full run with
`docs/launch-audit.md disagrees with the repository it describes: Rust source
files: audit says 195, the tree has 196` — batch 2 added `src/session/branch.rs`
and the audit's measurement was not re-derived. Fixed, and the gate went green
again. A count in a document is a measurement, and this one re-derives itself.

## 2 · What changed (batch 1)

The contract was wrong and the correction was the whole point: **bare `niki` on a
terminal opens a full-screen chat, so the TUI is the front door, not a feature.**
The audit found that the front door was a complete-looking shell with no engine
under it — ~85% chrome, ~0% function. The four clauses it now satisfies:

| Clause | Before | After |
|---|---|---|
| **C-J1** the chat remembers | `CompletionRequest` had no history field; all four providers hand-wrote a one-element `messages` array. Turn 3 was sent with turns 1–2 erased while the transcript scrolled, persisted and resumed. | One shared `message_chain()` across all four providers; 3 wire tests. |
| **C-J2** the chat does the work | `grep 'orchestrator' src/cli/chat.rs` → **nothing**. The front door could talk and never act. | `/run <task>` runs the four agents through the same `deliver()` `niki run` uses, and ends in a branch carrying the Coder's change. |
| **C-J3** the UI never lies | ~40 instances. A failed run rendered **"A P P R O V E D" in pulsing green**. `[r]etry` quit the app. The Run page printed a fake branch, a fake project path and four phantom agents. | Every ending has a name; fabricated content is derived or absent; every lying command says it is not wired. |
| **C-J4** the first minute is survivable | Onboarding claimed telemetry the README denies, offered a nonexistent theme and an OAuth flow that does not exist. A blank screen below 10 rows ate every keystroke. | Onboarding makes no claim the product cannot honour; a too-small terminal says so; `niki auth login` is a real path. |

Four supporting changes worth naming:

- **Delivery left `cli/run.rs`.** It was 281 lines inside one function, and
  `create_branch_and_commit` had exactly one call site — so `niki acp` and
  `niki goal` ran the four agents, destroyed the sandbox, and handed back
  nothing. `orchestrator::deliver` is now the single place a diff becomes a
  reviewable branch.
- **`scripts/verify.sh`.** The repo had twelve verification scripts and no
  single source of truth; `verify-product.sh` is a full product verifier that
  **no workflow runs**.
- **The canary map.** 65 features after batch 2, each naming the test that
  proves it can fail. G3 fails if one is added without an entry.
- **The manifests are repinned to 0.8.0.** `Cargo.toml` said 0.9.0, the newest
  published release was 0.8.0, and brew/scoop/winget all pinned 0.9.0 and 404'd —
  so `brew install niki` was broken while 21 of 22 CI jobs were green. Every URL
  **and every checksum** was repinned; `Cargo.toml` is back at 0.9.0 because that
  is what the source tree is. A release cut is what closes the gap.

## 2b · Batch 2 — what changed

Fifteen commits, `c0e5548`…`71ba200`. The canary map grew 49 → 65; the pty
suite 12 → 13 cases.

**Five of the fifteen are defects no audit found**, because they were found by
running the product against a real model and reading what it printed. That is
the part of batch 2 worth the most, because it is the part that generalises: the
audit reads code, and these are defects in what the code *does*.

### What a live run found

`z-ai/glm-5.3-flash` through NVIDIA's catalogue, `RUST_LOG=info`:

```
15:49:40Z [Planner] Done (165s, in 1146 / out 1640) — Spec: 1 files to modify
15:51:51Z [Coder] Starting...
```

The Planner is fine. The Coder never finished, and five real defects came out of
that one run:

| Defect | What it did |
|---|---|
| A **total** deadline masquerading as a read timeout | 120s × 4 steps × 3 attempts could burn **12+ minutes** before a stage started |
| A killed run left `status: "Running"` **forever** | the run record lied until the record was deleted |
| **2m11s of silence** after the Planner | the longest stage announced nothing, and the announcement that did appear came from the *fallback*, describing a stage that had already lost ten minutes |
| `.ok()?` on the loop's result | a lost network and a declined tool loop were the same value, so the run reported the benign one |
| A 5s permission window reported `denied by user` | the user was blamed for a fuse nobody could see |

### A hypothesis, tested and killed

The Coder sends 22 tool specs; the Planner sends none. The obvious reading of
`os error 110` (ETIMEDOUT on *connect*) was that the payload was too large to
establish a connection. Measured against the live endpoint:

```
  0 tools:      97 bytes  OK in 45.3s
 12 tools:    8894 bytes  OK in 22.8s
 22 tools:   16224 bytes  OK in 28.0s
```

**Refuted** — the largest payload came back *faster* than the smallest. No
tool-spec trimming was done; it would have been a plausible change with nothing
behind it. The fault is a free tier dropping connections, and what NIKI owes the
user there is a bounded stage and a visible error, which is what it now does.

### The three that found something bigger than themselves

**`niki doctor` was reporting a security property it never measured.** A
hardcoded `Pass("always-on: provider keys redacted from logs, reports,
artifacts")`. Running the real redactor over 13 key shapes found two leaks — and
the serious one was structural: the field patterns required an `=`, so they
caught `api_key=…` and missed `{"api_key": "…"}`. **Provider error bodies are
JSON, and they are the one place `redact_secrets` is applied**, so a key echoed
back in an error response reached the log and `report.md` untouched. The check is
now the same corpus the tests assert; README, the security doc and
`docs/claims-audit.md` were all carrying a broader claim than the code supported.

**Eleven `q` handlers were dead.** Every sub-page answers `q` with "back to
Run". None could run: the nav layer sat above the router, read `q` as
`NavIntent::Quit`, and broke the event loop. The key that goes back everywhere
else in the app destroyed the interface on every page that had a handler for it.
The same defect made the confirm-quit modal unreachable dead code — `grep` found
it, no user could ever see it.

**Wiring `/branch` to a real `git checkout` surfaced a data-loss hazard** that
has nothing to do with the TUI:

```
$ git checkout -f --      # repo with an uncommitted edit
$ echo $?; cat a.txt
0
committed                 # the edit is gone
```

`Command::args` is not shell injection — it is git reading our argument as one
of its own options, and the trailing `--` that makes branch-vs-path safe does
**not** help, because the flag is parsed first. A user-typed name starting with
`-` is now refused, and the refusal says why.

## 2c · Batch 3 — the terminal UI, finished

Eleven commits, `4e57fdb`…`0599708`. The canary map grew 65 → 116.
**`ROADMAP.md` §1 — "navigation and the dead controls" — is closed: all ten
items.** It was called "the largest quality-of-life cluster, and the one left
undone", and for a product whose front door is a full-screen chat it was the
cluster that mattered most.

### What was dead

| Surface | The claim | The truth |
|---|---|---|
| 11 sub-pages | `q` goes back | The nav layer read `q` as *quit* and broke the event loop. The confirm-quit modal below it was unreachable dead code. |
| 10 pages | `[j/k]` in the footer | Written into `state.page_selection`, which no renderer reads, above the router. |
| 3 pages | `Tab` next field / next tab | The global chat toggle was checked first. Both meanings dead. |
| Config | `[Tab] next field` | Moved a cursor that was **never rendered**, cycled by a hand-typed `% 15` over 13 fields — so two presses did nothing. |
| 2 pages | `h` history, `l` test log | Claimed by the navigator as prev/next page. Both unreachable by their own key. |
| 2 footers | 6 controls | **None handled.** `P R K V` on Fleet, `P R` on Session. |
| any page | digits `1`–`9` | Covered 9 of 14 pages, and the numbering was the *internal order of a Rust enum*. |
| Session | first tab | `SessionState::messages` has **no writer anywhere**, so the tab read "No messages yet" on every mission, forever. |
| `niki chat` | `g`, `s` | `g` did nothing; `s` navigated to a **blank screen**. |
| History | `[Enter] open` | Loaded the run and navigated nowhere. |
| Help page | `[t] theme` | The binding is `ctrl+t`. The same stale letter was in the command palette. |
| `docs/tui/key-matrix.md` | 4 "do not fix" divergences | All four were already fixed. |

### The pattern, which is the argument for testing behaviour

Fixing a dead key kept revealing the next one. `q` → the unreachable modal.
`j`/`k` → an invisible Config cursor → a modulus that **could not have been
right**, because one of the fields is pushed inside a loop. Wiring `/branch`
to a real `git checkout` surfaced `git checkout -f --` discarding uncommitted
work and exiting 0. Reading the help text found the stale `ctrl+t` in a second
place. Enumerating `BINDING_TABLE` — §1's own stated exit criterion — found
`g` and `s`.

**Eight of the eleven were found by writing the next slice's test, not by
reading the code the slice was about.** An audit of this cluster would have
found roughly three of them.

### Four tests I wrote and then deleted or weakened on purpose

Recorded because a report that only accumulates wins is not a report:

- one that **copied** two handlers and called the copies — a test of the copy,
  which passes the moment the production arm is deleted;
- one that checked a footer's *label* against its key's *action*, which is a
  judgement call and would pass on the cases it happened to model;
- a PTY case comparing panes across six `j` presses on a Diff page with no
  diff — byte-identical, so green against a completely broken page;
- a cycle test asserting all *n* presses differ from the start, which asserts
  the cycle never wraps — the opposite of the truth.

And one test that was **too narrow**: five stayed green when the Session tab
was wired back to the field nothing writes, because they checked what the
event loop *passed* and not what the page *used*.

## 3 · Can every one of these prove it can fail?

Yes, and each was run. The proof is in the commit message for that slice and in
`PROGRESS.md`. Representative:

```
the_code_change_is_on_the_branch_not_only_in_a_sidecar_file ... FAILED
    Non-Hermetic: committed state changed during the run.

the_tui_gates_decide_on_the_suites_result                    ... FAILED
a_task_typed_in_the_chat_produces_a_branch_carrying_the_change ... FAILED
    a task typed in the chat must produce a branch, got: ""
appstate_apply_event_final_reports_what_actually_happened     ... FAILED
a_failed_run_is_never_approved                                ... FAILED
the_run_page_invents_nothing_before_a_run_exists              ... FAILED
an_unrecognised_command_never_reaches_the_model               ... FAILED
onboarding_makes_no_claim_the_product_cannot_honour           ... FAILED
loading_a_config_consults_the_keyring                         ... FAILED
a_draw_failure_ends_the_chat_rather_than_freezing_it         ... FAILED
```

One honest exception: the new tmux case `10_first_run_no_key` stayed green under
the sabotage that broke its two siblings, because the two defects reintroduced
were not on its path. It is a regression guard against a first run that hangs,
not a proof that T6 or T7 landed. That is recorded in its own comment.

## 4 · The gate found six defects it was written to find

`scripts/failure-paths.sh` (G6) failed on its first run:

- `niki report` exited **0** on an unknown id, on no tasks, and on a missing
  report — three `return Ok(())` branches. `niki report zzzzzzzz` printed "No task
  matching" and exited 0, which a script cannot tell from success.
- `niki status` exited 0 while displaying a failed run.
- `niki chat --message` exited 0 after a provider error, on the documented
  non-interactive path.

All fixed. The other two failures were **my own wrong expectations** — a test that
depends on what happens to be running on the developer's machine is a test that
will fail on someone else's.

The gate also needed six fixes to itself, all found by running it: a canary loop
whose `IFS='|'` left the spaces around each pipe in the field (so all 23 canaries
reported missing); a `cfg(test)` filter that counted three test-module trait stubs
as production code; a secret scan that flagged the fixtures whose entire purpose
is holding credential-shaped strings; `--only G3` doing nothing because `for arg
in "$@"` iterates a snapshot; a manifest URL built from a `$version` variable
curled as a literal; and a 302 to the release CDN counted as a dead download.

Three of those six are false positives, which is the failure mode that gets a gate
switched off. They are worth more than the six real defects.

## 5 · Honest known limitations

**Live, as of batch 2:**

1. **G8 has not run.** Nothing here has been on CI. The most likely CI-only
   failure is the visual-regression job: `tests/visual/run.sh` documents that
   reference frames can only be blessed on the GitHub runner, and this branch
   changes chat rendering. If it goes red there and nowhere else, that is why.
2. ~~**The TUI is not finished.**~~ **Closed in batch 3.** `ROADMAP.md` §1 is
   done in full: `q` goes back, `j`/`k` reach the page, `Tab` has one owner,
   Config shows its cursor with no dead stops, `h`/`l` reach History and
   TestLog, the footers advertise only what works, digits reach ten pages and
   the numbering is documented, the Session tab is live, `g`/`s` work in
   `niki chat`, `[Enter] open` opens the run, and the help and key matrix
   describe the codebase that exists. 116 can-fail entries behind it.
3. **The base64 catch-all in the redactor is still too broad.**
   `[A-Za-z0-9+/]{40,}` blanks any unbroken 40+ character alphanumeric run, so
   a long identifier with no separators — a git SHA is 40 — is blanked in a
   report. Confirmed **pre-existing**, not introduced by batch 2. It is also the
   reason batch 2 did *not* add redaction at the report write boundary: doing so
   on top of this pattern would replace evidence with `[REDACTED]`, which is its
   own failure. `ROADMAP.md` §4.2b.
4. **Three unsound advisories still pass by policy**, including `git2 0.20.4` —
   a *direct* dependency — carrying `RUSTSEC-2026-0184` (undefined behaviour).
   `deny.toml` sets `unsound = "none"` and nothing acknowledges or pins it.
   Needs a real dependency bump, not a config flip. `ROADMAP.md` §4.5.
5. **The permission prompt is better and still not good.** The 5s window that
   reported `denied by user` is fixed — a timeout is now distinguishable from a
   refusal, and the window is configurable. But `q`-from-a-sub-page, `Tab`, and
   the j/k cluster above are the same class of problem: the surface advertises
   controls the router cannot deliver.
6. **No push has happened.** Per the owner's decision this branch is local.

**Fixed in batches 2 and 3**, listed here because a report that only accrues
limitations stops being readable:

- ~~Permission prompting is unreachable in chat (the event source is never
  wired)~~ — **this was wrong, and it is worth saying so plainly.** The sandbox
  has emitted `DisplayEvent::PermissionRequest` since the live-LLM work
  (`sandbox/worktree.rs:453`, `sandbox/docker.rs:606`), and
  `state.rs:1702` has handled it. The claim was made in batch 1's report
  without tracing the path. Only the 5s window was real, and it is fixed.
- ~~The TUI still cannot check out a branch.~~ `/branch <name>` now runs git,
  with the argument validated first (`B2-13`).
- ~~There is a known path escape.~~ `resolve_tool_path` guards it
  (`B2-01`), and macOS `/var` still works.
- ~~A signal-killed test suite is reported as passing.~~ It is `128 + signal`
  now (`B2-02`).
- ~~A failed `git` call reads as an empty diff.~~ It returns `Result`
  (`B2-03`).
- ~~`q` quits from every sub-page.~~ 11 handlers work again (`B2-15`).
- ~~The TUI cannot check out a branch.~~ `/branch` runs git, with the argument
  validated first (`B2-13`).
- ~~The redaction Pass was a constant.~~ It is a 13-shape corpus, and two
  shapes it missed were leaking (`B2-14`).
- ~~Every dead control in §1.~~ All ten items (`B3-01`…`B3-11`).

One batch-2 claim in this report was **wrong** and is corrected in place
rather than dropped: "permission prompting is unreachable in chat (the event
source is never wired)". The sandbox has emitted `DisplayEvent::PermissionRequest`
since the live-LLM work (`sandbox/worktree.rs:453`, `sandbox/docker.rs:606`).
Only the 5s window was real.

## 6 · The roadmap

`ROADMAP.md` — §1 navigation and the dead controls, §2 chat ↔ pipeline depth,
§3 loop unification, §4 security, §5 reliability, §6 coverage and hygiene, and
§7 three things explicitly killed with the reasoning. Four items are marked
**DONE in batch 2**; §4.2b is the new one.

## 7 · Exactly what you must do to go live

1. `git push origin niki/hardening` and open a PR. **G8 cannot pass until this
   happens**, and the visual-regression job is the one to watch.
2. **Cut the v0.9.0 release.** Repin `homebrew/niki.rb`, `scoop/niki.json` and
   `winget/*.yaml` to 0.9.0 with 0.9.0's checksums, then `./scripts/verify.sh
   --only G1`. G1 is the gate for this and it is currently green *at 0.8.0* —
   the moment you repin, it will 404 again until the release exists. That is the
   gate working.
3. **Decide the brew tap.** `brew install niki` in the README has no tap behind
   it and no homebrew-core formula. Either publish `RavaniRoshan/tap` or delete
   the row from `README.md`, `docs/content/01-overview/03-quickstart.mdx` and
   `niki-starter/README.md`.
4. **Decide the base image.** `niki.example.toml` and `config/types.rs` say
   `niki-sandbox:24.04`; `docker/Dockerfile:14` is
   `FROM cgr.dev/chainguard/wolfi-base:latest`. The tag is a lie in three
   places. This branch left it alone because a `podman build` is needed to prove
   G4 on the default backend, and that needs your call on which base is intended.
5. **Optional:** a metered provider key to run G4's real-provider leg against a
   provider that does not drop connections. A free tier was used for batch 2's
   live runs and is *not* sufficient evidence: every request took 22–45s for
   eight tokens, and the Coder's tool loop ended on a connect timeout. The
   Planner completed reliably; the Coder never did. That is a property of the
   tier, and it is why the real-model leg is not a gate.

### What is **not** on this list, and why

**The NVIDIA key used for batch 2's live runs** is not a go-live step and is not
anywhere in the repository. It was held in the environment only. G5's secret
scan is what proves it stayed there, and it runs before every commit; planting
a contiguous key-shaped literal in `src/` was verified to fail the gate.

## 8 · Where batch 3 leaves the plan

`ROADMAP.md` §1 is closed. What remains, in the order it should be taken:

**§2 · Chat ↔ pipeline depth.** The largest item is a *feature*, not a fix:
the tool cards are fully built (`components/tool_card.rs`, `tool_detail.rs`,
the Enter hit-test) and unreachable, because the chat sends `tools: None`.
Wiring that is real work and is a product decision about how much of the agent
s loop belongs in a conversation.

The honest candidates in §2 are the permission-wiring items, because they are
claims about the product's *security posture* and not features:

- the permission modal is structurally unreachable in chat — `cli/chat.rs`
  creates a channel whose sender is never given to `create_sandbox`;
- `ask_user` / `approval` return "cannot ask" in any TUI run;
- the permission badge is still cosmetic: `state.permission_mode` is read to
  pick a label and never reaches `ToolContext` or the sandbox.

**§5 Reliability** and **§6 Coverage and hygiene** follow, then §4.5 (the
three unsound advisories, which need a real dependency bump rather than a
config flip) and §4.2b (the base64 catch-all, which needs entropy-based
detection).

The order is the same one that produced batches 1–3: the contract clauses
first, then whatever unblocks the most downstream work, then the honesty and
correctness defects a first-time user hits, then coverage and hygiene.
