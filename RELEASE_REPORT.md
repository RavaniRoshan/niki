# RELEASE REPORT — NIKI hardening programme, batches 1–4

Branch `niki/hardening` · base `c7e297d` · 57 commits.

Produced by executing `plans/namor-obsidian-jay-garrick.md`, with the four owner
decisions in §0a. Every gate below was produced by `./scripts/verify.sh`; the raw
output is in `EVIDENCE.md` and `.evidence/`.

Batches 2, 3, 4 and 5 are §2b, §2c, §2d and §2e. Batch 1 is §2.

---

## 1 · The gate table

Run of 2026-09-30, after batch 2's last commit. Raw output in `EVIDENCE.md`.

| Gate | Result | Evidence |
|---|---|---|
| **G1** clean clone, install, README quick start | **PASS** | all scripts parse; every published download URL resolves; `niki 0.9.0 — README quick start commands accepted` |
| **G2** build, lint | **PASS** | `cargo fmt --check`; `cargo clippy --all-targets -D warnings` warning-free; debug and release both build |
| **G3** tests + can-fail map | **PASS** | lib **1002**; `run_lifecycle` **14**; **335 can-fail entries** all resolve to a real test |
| **G4** the core flow, for real | **PASS** | `run_lifecycle` 14; `chat_runs_the_pipeline`; `scripts/demo.sh`; the tmux suite **16/16**; and a **live** pipeline on `stealth/space-bunny-alpha` — Approved 10/10, §2h |
| **G5** security | **PASS** | `cargo deny check` ok; `cargo audit` ok (11 pre-existing allowed warnings); **no credentials in the tree or in history** |
| **G6** failure paths | **PASS** | every failure path exits non-zero *and* says something a person can act on |
| **G7** docs match reality | **PASS** | the audit's counts are re-derived from the tree; every README command parses against the real binary |
| **G8** CI green | **FAIL — by construction, and now diagnosable** | the branch is committed locally and **not pushed**, per the owner's decision. The last *pushed* run's one red job is `Manifest parity`. That job is now `scripts/manifest-parity.sh`, extracted from the workflow so it runs here too, and on **this** tree it reports `manifest parity: ok` — all six release URLs resolve. |
| **G9** no dead code, no fake features | **PASS** | no `todo!`/`unimplemented!` in production code; no `assert!(true)`; every subcommand appears in the README |

**Eight of nine pass locally. G8 is red because the branch has not been pushed**,
and that is the honest state rather than a workaround: it is the one gate this
programme cannot satisfy without the owner.

### The gate caught the batches' own drift, twice

`G7` failed on the full run with `docs/launch-audit.md disagrees with the
repository it describes: Rust source files: audit says 195, the tree has
196` — batch 2 added `src/session/branch.rs` and the audit's measurement was
not re-derived. Fixed, and the gate went green again. A count in a document is
a measurement, and this one re-derives itself.

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

Twelve commits, `4e57fdb`…`6c8a3be`. The canary map grew 65 → 122.
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
| the permission badge | Shift+Tab cycles the posture | Six variants, **one reader**: the status bar. Cycling it changed a label and no tool call. |

### The pattern, which is the argument for testing behaviour

Fixing a dead key kept revealing the next one. `q` → the unreachable modal.
`j`/`k` → an invisible Config cursor → a modulus that **could not have been
right**, because one of the fields is pushed inside a loop. Wiring `/branch`
to a real `git checkout` surfaced `git checkout -f --` discarding uncommitted
work and exiting 0. Reading the help text found the stale `ctrl+t` in a second
place. Enumerating `BINDING_TABLE` — §1's own stated exit criterion — found
`g` and `s`.

**Eight of the twelve were found by writing the next slice's test, not by
reading the code the slice was about.** An audit of this cluster would have
found roughly three of them.

### The one place retraction was the wrong call

The permission badge cycled a posture that governed nothing. Batch 1's default
is "retracting the claim beats wiring something approximate", and by that rule
the badge should have been deleted. It should not have been: the posture that
*does* govern a stage, `config.permissions.mode`, is read **per stage** by
`ToolContext`, so the value only had to travel with the submit. The control now
works, the label is true, both notices say the change lands from the next stage
(the one in flight keeps its posture), and the default is still `manual` —
asserted, because a change that makes a badge real must not make the product
less safe by accident.

### And a claim in `ROADMAP.md` that was wrong

> ~~The permission modal is structurally unreachable in chat: `cli/chat.rs`
> creates a channel whose sender is never given to `create_sandbox`.~~

A plain chat turn calls `stream_reply` — no tools, no sandbox, no
`create_sandbox` at all. The modal is unreachable in chat because **chat does
not run tools**, which is the owner's §0a decision (`/run <task>` starts the
pipeline, plain messages stay conversation turns), not a wiring bug. The
sandbox *does* emit `DisplayEvent::PermissionRequest`. There was nothing to
fix, and "fixing" it would have meant making chat run tools — a product
decision dressed as a repair.

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

## 2d · Batch 4 — data loss, silent degradation, and dead weight

Twelve commits, `c9da119`…`d6ef83b`. Canary map 122 → 175.

Batch 4 is the first one not driven by the ranked list: §1 was already closed, so
the work came from the roadmap's deferred items and from the defects the new
tests kept finding *next to* the ones being fixed.

### Three of these were data loss

**Two front doors ran the pipeline and threw the work away.** `niki run` and the
TUI chat have delivered since T3a; `niki acp` and `niki goal` did not. So the
product's central promise held on two of its three front doors — and ACP
additionally wrote the **diff text** into `record.branch`, a field every reader
treats as a branch name, so `niki report` printed a unified diff where a branch
belonged.

**A SIGKILL left a complete copy of your repository in `.niki-worktrees/`,** and
the next `git add -A` committed all of it. Git's own per-clone `exclude` file now
gets the entry — never your tracked `.gitignore`, which would be a change to
your repository made by a tool you ran once.

**A truncated `checkpoint.json` does not parse**, so the file you need *after* a
crash was the one most likely to be unreadable after one. `write_restricted` was
a bare `fs::write` at fifteen sites, and the atomic writer beside it — used for
JSON session state, so the choice was a coin flip — had a **fixed temp name** and
raced under concurrency. One writer now, atomic, unique temp per writer, cleanup
on a failed rename.

### Two tools that reported work they did not do

`web_search` returned `Success` with an **empty result set** and the truth in
`diagnostics` — a field no model reads. So a model that called it concluded the
*web* had nothing on the subject. It now returns `Failed` with no
`WebSearchResults` payload at all, and says what to use instead.

`web_fetch` could never fetch anything: constructed with `vec![]`, and its own
`is_allowed` says *"Empty allowlist = block all"*. `[network] domain_allowlist`
existed, was documented for exactly this, and was read by nothing. It is
threaded now through seven call sites, and **the default does not move** — empty
still blocks, and that is asserted.

### The through-line: seven tests that passed the sabotage

| What the test asserted | Why it passed anyway |
|---|---|
| Five page-render tests | Drew every page and asserted nothing but "no panic" |
| Two "endpoint resolves" tests | Set a `base_url` and asserted the provider's *name* |
| Nine `create_provider(X)` tests | Compared a struct field to the string it was built from |
| Seven worktree tests | Each called the helper itself, so deleting the call site changed nothing |
| `contains("network_allowlist…")` | The string appears at seven sites; emptying one still matched |
| `contains("!sub_page_owns(…)") == 2` | A later slice legitimately added more guards |
| Seven atomic-write tests | A completed write looks the same with or without a temp file |

The corrective pattern was always the same: make the assertion **count**, name
the specific region, or state in the test body that the property is not
observable from outside — and say so.

### Also

- **697 lines of unreferenced code deleted** (`errors.rs`, `control_plane/`,
  `persistence/`), all `pub` and so invisible to `dead_code`. A test now
  requires every `pub mod` to be referenced from outside its own subtree, and
  states its own blind spot as a passing test.
- **A test that took 232 seconds** — it re-read every source file per module.
  One pass now: 232s → 0.67s.

## 2e · Batch 5 — verifying the record, and finding it wrong

Twelve commits, `a3303de`…`a409bbf`. Canary map 175 → 217. PTY 14 → 15.

Batch 5 is the first drawn from `ROADMAP.md` §9 — the list this programme
wrote about itself. Most of it was **checking the previous batches' claims
rather than making new ones**, and three claims did not survive.

### Three wrong numbers, found by measuring instead of reading

- **"Three unsound advisories."** `cargo audit` reports **four**, across two
  crates. The count lived in a document whose whole job is to stay true.
- **"19 of 28 CLI commands have no test."** Re-measured: **one** — only
  `dashboard`. The other eighteen were closed by batches 1–4 and the number was
  never updated.
- **`run_page_ignores_navigation_hotkeys`** asserted that page letters do
  nothing on the Run page. It passed, and the shipped binary opens Diff: the
  test drove `PageRouter::handle_key`, a *page-local* handler, while global
  jumps live in `global_page_jump` and run after the router declines. Now
  measured by a pty case.

### What it fixed

- **Two runs could apply each other's patch** — a fixed `.niki-tmp.patch` that
  `git apply` reads back *by path*, so concurrent runs mixed two tasks' work
  and deleted the file under each other.
- **"Is this failure worth retrying?" existed three times with three answers**,
  and the agent loop *above* the transport had the weakest list, so a 502 got
  **less** resilience for having a retry layer above it.
- **Redaction destroyed git SHAs** — 40 hex, redacted, in a report whose
  commit references are the evidence.
- **MCP leaked a process per configured server** and told the model to call
  tools `McpManager::call_tool` has no production path for.
- **The container's four hardening settings were asserted by nothing** — a
  test file *named* after them, spending six of eight tests on a string
  parser.
- **`niki run --project X --tui` rendered with the shell cwd's `niki.toml`**,
  because `run_tui` took `project_path` and never used it.

### The theme, carried from batch 4: a test that cannot fail

Fifteen times across five batches. Batch 5's own share, with the reason each
one stayed green:

| Test | Why it passed anyway |
|---|---|
| `the_processor_applies_the_badge` | Asserted `is_retryable_code(` — a function *reference* to `is_some_and`, no parenthesis |
| `no_prompt_instructs_a_model_to_call_an_mcp_tool` | Searched for the old instruction; both files **quote it in a comment** explaining why it is gone |
| `every_unsound_advisory_is_named_with_a_reason` | Compared against every `ID:` line, including six `unmaintained` advisories |
| `the_page_letter_case_exists…` | The canary named a *filename* — not a `grep` target — so it resolved against a doc comment |
| `perf_is_machine_independent` | Compared a "cold" and a "warm" pass; `render_once` builds a fresh `TestBackend`, so they were the same measurement, 2% apart |
| `the_most_recent_run_is_the_one_dashboarded` | The sabotage left `read_dir` order to decide, and it happened to land on the newer task |
| `the_dashboard_escapes_what_it_embeds` | I sabotaged the wrong **file**, so nothing changed and the probe "passed" |

The corrective is always three moves: **count** instead of `contains`, **name
the region** instead of searching for a string, and — the slowest to learn —
**assert the sabotage actually applied** before believing its result.

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

**Batch 8, and it is the largest limitation on this page:**

**No gate runs the test suite.** G3 verifies that every entry in
`scripts/canary-map.txt` names a test that *exists*. It does not run the tests,
because the suite does not fit this machine (`AGENTS.md`: never run it here).
So **the canary map cannot tell you the suite is green — only CI can.**

This was not theoretical. Running `tests/reverse/` and `tests/agent_tool_loop/`
by hand in batch 8 found **three tests red on the branch** while G1–G7 and G9
all reported PASS:

| Test | What it was |
|---|---|
| `cost::a_fallback_served_call_is_priced_by_the_fallback` | a **real product defect**: a 500 wrapped in NIKI's own error prefix was classified permanent at both call sites, so the failover chain did not fail over. Fixed in batch 8. |
| `money::the_coder_loop_bills_before_every_bail_out` | a **real product defect** behind a stale hard-coded count. Fixed in batch 8. |
| `a_truncation_that_never_resolves_is_reported_as_truncation` | a **test defect**: it counted requests *containing* the notice rather than notices *issued*, and history accumulates. Corrected in batch 8. |

Two of the three were real defects in the resilience and accounting paths — the
parts a user hits when a provider is slow, rate-limited or drops a connection.
Neither was visible to any gate, and one had been red long enough that its cause
was assumed to be something else.

**What this means for you, concretely:** before trusting a green gate run here,
run the suite in CI. G3 tells you the canaries *exist*; it does not tell you they
*pass*. Closing this properly means a fast lane in `scripts/verify.sh` that runs
the cheap integration binaries (`reverse`, `agent_tool_loop`, `run_lifecycle`)
serially — that is a gate change, and gates are not something to add at the end
of a batch without the owner agreeing to the runtime cost.

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
- ~~The TUI could not check out a branch.~~ `/branch` runs git, with the
  argument validated first (`B2-13`).
- ~~Two of the three front doors delivered.~~ All three do (`B4-01`).
- ~~A crash in a tool could commit a copy of your repository.~~ `B4-03`.
- ~~`web_search` and `web_fetch` reported work they did not do.~~ `B4-07`,
  `B4-08`.
- ~~A truncated checkpoint did not parse.~~ `B4-10`.
- ~~697 lines of unreferenced code.~~ Deleted, with a gate (`B4-11`).
- ~~The permission badge is cosmetic.~~ It governs every stage that has not
  started (`B3-12`), with the default still `manual`.

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

6. **Three decisions about the safety classifier, none of which block a
   release.** `risk/` now has the hook layer, the gate, the escalation limits
   and the reasoning-blind view — but **nothing implements `ActionClassifier`
   against a real provider**, so that layer is inert in the product and is not
   claimed as shipped. Turning it on needs: a model id and provider (NIKI is
   BYOK, so there can be no sensible default), whether failing closed is the
   default when that provider is unavailable, and whether it is opt-in at all.
   Full statement, with the cost argument, in `BLOCKERS.md` **B7**.

### What is **not** on this list, and why

**The NVIDIA key used for batch 2's live runs** is not a go-live step and is not
anywhere in the repository. It was held in the environment only. G5's secret
scan is what proves it stayed there, and it runs before every commit; planting
a contiguous key-shaped literal in `src/` was verified to fail the gate.

## 8 · Where batch 3 leaves the plan

`ROADMAP.md` §1 is closed and `ROADMAP.md` §9 now lists what batch 4 left open
— eight items, each with the reason it is still open. The order they should be
taken:

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

## 2f · Batch 6 — giving the model a way to ask, and the record a way to rot

Eleven commits, `84d3941`…`ee3b5d3`. Canary map 221 → 287. PTY 15 → 15.
Lib tests 943 → 995.

Batch 6 opened on the largest capability gap in the product and spent its
middle on the process problem that gap exposed.

### In a TUI run, the model could not talk to you

`ask_user` and `approval` read stdin, and the interface owns stdin — it holds
it in raw mode and runs its own `event::read()`. A `read_line` in a tool races
that for the next keypress and, in raw mode, returns after a single keystroke
with no newline, so a stray `y` typed at the *interface* could be taken as
consent to a command it never showed the user. Failing closed was correct.

What was wrong was the conclusion: *nobody is there to ask*. There is. The
interface had been collecting answers for the sandbox's `PermissionRequest`
all along; the two were never connected. Both tools are offered to every
agent, and in the product's primary surface both could only ever fail.

- `ToolContext` carries an optional `HumanInput` — that channel plus
  `[permissions] prompt_timeout_seconds` — and the pipeline builds it from
  `display.tui_tx()`.
- `approval` puts the command to the interface and waits, rendering the
  **same modal** the sandbox's own prompts use. A command approved through the
  tool and one approved through the sandbox take one path; no new UI.
- `ask_user` got `components/ask_user.rs` — a moving cursor, `1`–`9` to pick a
  choice, Enter to send, and **Esc as a cancel, not a refusal**.

The three outcomes stay three. An explicit refusal says the *user* refused; an
unanswered question says it is a timeout and not a refusal, and ends at the
configured deadline; a run with no interface says **nobody was asked** and
never says "denied by user".

### Four record corrections in ten slices

Nothing was wrong with the code in any of these — §2's tool cards, §6's
`.niki-worktrees/` ignore, §6's `acp`/`goal` data loss, §6's tautological
provider tests, §5's Google `Finish`, §5's swallowed Coder errors, §9.3's
retry matcher, §9.4's temp path. All eight described fixes that had shipped
one or two batches earlier, because closing a defect in the code did not
close the bullet that named it.

The fifth §6 claim was real, and it was the **documentation**: the README
feature table and `niki.example.toml` both told a user their MCP tools are
injected into agent prompts. They are not — `tools_summary` says `NOT YET
CALLABLE` and routes the line to a display notice, never to a model.

### The fix for the fix: pins

`tests/record_claims_are_pinned.rs` registers every open claim that reduces
to something checkable, with the test that checks it, and that test must
itself be in the canary map. So a claim cannot be left unpinned, a pin cannot
point at a test that no longer exists, and a **new** numbered item naming a
source with no row fails the gate. `ROADMAP.md` §0 says all of this.

`the_docker_backend_still_has_no_unit_tests` was written in B6-07 and deleted
in B6-09, in the commit that made it false. The failure *is* the signal.

### Five defects the tests found

| Defect | Consequence |
|---|---|
| The permission modal floored its height at 8 **after** clamping to the area | a box taller than the screen; the renderer indexed outside the buffer — a panic, on resize, with a destructive command waiting on the answer |
| A dismissed question reported `Success` | the model was told a user had engaged when nobody did, and the run counted an interaction that never happened |
| `cleanup_worktrees_for_task` matched any `<id>-` prefix | Ctrl+C for `task-1` deleted `task-1-backup`; a sibling is always `<id>-<digits>` |
| The container name is `{:?}`-formatted and unsanitised | a role name that is not a plain identifier fails the run at create, with an error naming a string the user never typed |
| The question modal gated on a payload rather than the visible flag | an invisible question answering itself and swallowing the key |

### Nine false greens, which is the number that matters

Every one was a test believed before it was broken: a card test asserting
`ToolCard::new(` where a sabotage had replaced its *arguments*; a ladder test
asserting `Consumed` where something behind the question consumed the key; an
`eprintln!` search that an unrelated one four lines away satisfied; a table
parser that read `| 9.9` as having no leading number; a greedy-walk example
that passed with the exact table switched **off**; two docker tests that
could not fail by construction. One check I wrote was simply *wrong* — it
flagged a struck bullet carrying a live sub-claim — and was deleted rather
than made to fit.

### What G8's red actually is, measured rather than assumed

The last *pushed* CI run (36665173960, branch `fix/trust-and-demo`, 2026-09-30)
has one red job: **Manifest parity**, step 3, "Check manifest download URLs
resolve". It was a real failure — three package manifests pointed at
`releases/download/v0.9.0`, the newest published release was v0.8.0, and
`Cargo.toml` was already at 0.9.0, so `brew install niki`, the headline
install command in the README, resolved to a 404.

**On this tree all six URLs resolve.** So the red is the version skew this
branch fixes, seen from a run of a branch that is gone.

That was a claim, and the way to know is to run the thing. The job lived
inline in `.github/workflows/ci.yml`, which meant the only place the rule
could be exercised was a push — so a dead install URL could not be caught
before it shipped, and was not. It is now `scripts/manifest-parity.sh`, with
the workflow and `verify.sh` G8 both calling it, and G8 reports the two
answers separately:

```
G8   FAIL   rc=1 — see .evidence/g8-ci.log      ← the last pushed run
G8   PASS   manifest parity: ok                 ← this tree
```

A red G8 on a local branch now says *what is wrong* rather than only *that
something is*. It goes fully green on a push, which remains the owner's
decision.

### The run this section describes

`scripts/verify.sh --update-evidence` at the end of batch 7, pasted into
`EVIDENCE.md`: G1–G7 and G9 **PASS**, G8 **FAIL**, tmux **16/16**, lib
**996**, **317** can-fail entries. G8's failure is the unpushed branch, and
the one CI job it covers is green here via `scripts/manifest-parity.sh`.

## 2g · Batch 7 — the call path, the salvaged diff, and a harness that runs

Nine commits, `0e52610`…`aab7685`. Canary map 303 → 317. PTY 15 → 16.
Lib tests 995 → 996.

The ranked list was drained when batch 7 opened, so it worked the two items
that were records rather than repairs — and found that both were hiding a
defect underneath.

### The MCP call path was pointing at corpses

`ROADMAP.md` called it "a feature, not a repair", because the manager had to be
held for the whole run. Measuring found the manager was a local of the
**discovery block**: the connections dropped with it, `kill_on_drop(true)`
killed the stdio children, and every configured server was dead before the
Planner's first token. Not unreachable — dead.

Which is worse than never starting: the user got a summary naming tools that
were already gone, and paid the spawn cost either way. Four slices closed it,
and the flag that existed to notice the closing **fired**.

### §9.1 was two problems wearing one name

`niki resume` says re-run the task, which spends the Planner and the Coder
again and produces a *different* diff. Salvaged work was being thrown away for
want of a way to look at it — and the blocker was not resuming the pipeline. It
was that a failed run wrote the change as **JSON** while every surface that
points at a diff points at `changes.patch`, which a failed run never wrote.

A salvaged run now renders a real unified diff, in a scratch copy, with its
own "UNREVIEWED" header. The working tree is never touched. The path rewriting
took three attempts and only `git apply --check` caught the third: `git diff
--no-index` concatenates its prefix with the path, so stripping the
leading-separator form first leaves `asrc/lib.rs`.

### What the new harness found on its first run

Every pty case drove `niki chat`, which by §0a sends no tools — so
`ask_user` and `approval` had no end-to-end coverage at all, and the mock's
tool loop was two hardcoded calls so no test could drive anything else.

The first execution of the new case found a crash: the mock's
`anthropic_json_response` referenced `scripted` without computing it, raised
`NameError`, and killed the connection. B7-06's own tests had only driven the
OpenAI path and passed. A feature scripted on one provider and run on the
other is tested on both, or it is a trap.

### §9.2a, narrowed rather than guessed

After an answer, the run does not reach a verdict. Two follow-up tests closed
the obvious explanations: the **loop** asks once, feeds the answer back and
produces its artifact; the **interface** closes the modal, takes the request,
clears the field and hands the tool the text typed. So the remaining question
is how `niki chat`'s event loop and a live run interleave — the one dimension
no unit test covers.

### Process failures, recorded because they cost time

- **A commit shipped with a clippy failure**, because `| tail` made the
  pipeline's exit status `tail`'s. Clippy was right about a real defect in the
  same commit. *A pipe at the end of a gate hides the gate.*
- **Five signature threads, five regex failures**, all caught by the compiler.
  The roadmap's estimate was four signatures; it is five.
- **Two harness lessons.** `tui_send` (key names) and `tui_type` (literal
  characters) are different operations, and conflating them cost a run in each
  direction. And the mock's stderr goes to a file, because a harness that
  cannot explain its own failure teaches the reader to guess.

## 2h · Batch 7, the live half — what a real model found

Seven commits, `49c5c31`…`e7bba55`. Canary map 323 → 335. PTY 16 → 16.

The first half of batch 7 closed the ranked items that were records rather
than repairs. This half is what a **real model** found — six defects, three of
them in the Coder's tool loop, none reachable from a scripted server.

### §9.3 — a model that finished the work never submitted it

Six steps: `read`, `edit`, `bash`, the edit applied and compiling. Then: *"The
change is in place and compiles cleanly"* — and no `submit_artifact`.
`recover_submission` found no JSON, returned `None`, and the caller
**discarded the loop and re-ran the whole Coder one-shot**. The work was in the
worktree the entire time.

The loop now asks once before giving up: stop exploring, only
`submit_artifact` becomes part of the result, call it now. **Live-verified** —
the same model and task then submitted in 36 s.

### Three more, from the same runs

| | Defect | The wrong first reading |
|---|---|---|
| §9.3b | The artifact re-applies edits the tool loop already wrote, so every `search` is gone | "a no-op edit was accepted" — wrong; the validator already rejects those. One grep corrected it. |
| §9.3c | `first_json_object` took the **first** `{` in the text, so an artifact behind a brace in prose was never parsed | — and the test found a pre-existing weakness: `edits.is_some()` accepted a *description* of the shape |
| §9.5 | One stale path in a round-accumulated file list made `git add -N` fail for the **whole invocation**, so every new file dropped out of the diff | "the run warned about a missing file" — the warning was not the defect. **The diff was short.** |

### Two models, and why both were needed

`stealth/space-bunny-alpha` runs the whole pipeline. `poolside/laguna-s-2.1:free`
cannot finish a task — its Planner emits no conformant artifact — and *that* is
what exposed the recovery path reporting `No such file or directory (os error
2)` on a run that had already failed for a perfectly good reason.

**A model too weak to get through stage one still fails loudly and early**,
which is how the failure paths get exercised for real.

### The final run

Release build, `--backend worktree`, and **no "the patch did not apply"
anywhere** — including across the revision round, which is where §9.3b used to
bite:

Planner 47s → Coder 90s → Tester 5/8 → Reviewer **revision needed** (2
critical, both correct) → Coder 48s → Tester **7/7** → Reviewer **Approved,
10/10 · 10/10 · 9/10**.

### Three times in two slices, a proof that did not run

A can-fail entry whose test name did not match my filter (`0 passed` reads
like a pass). A test that drove the shared helper and not the backend's own
inline check. A test whose premise was broken by its own fixture. Each found
by reading the **count**, not by trusting the word "ok" — and the last is why
`ROADMAP.md` §0 exists.

---

## 2i · Batch 7, §8 — the interface, the defence, and the context

Four tasks from the Claude-architecture brief. What each one turned out to be
is not what §8 predicted, and the differences are the useful part.

### T1 · the living working status — shipped and rendered

`components/working_status.rs` is pure: it emits no event, so
`--output-format json` is untouched by construction rather than by a flag
someone remembered to set. `tests/visual_baselines_are_unaffected.rs` proves
the baselines did not move — no tape types `/run`, and every tape launches
`chat`.

### T4 · context compression — the loop's transcript was never compressed

Two compressors already existed and **neither touches the conversation the model
is in**. `memory/compression.rs` writes a knowledge block to disk and its one
call site is `let _ = compress_context(…)` — the result discarded.
`runtime/compaction.rs` has **zero callers in `src/`**: `ContextCompactor` is
exercised only by its own tests. So a Coder running sixty tool steps grew
without bound and nothing noticed.

`runtime/transcript.rs` is the first compressor over the loop's own
`Vec<LoopMessage>`, wired where a tool result has been pushed and the next
request has not been built. Three strategies in §8's order; `Summarise` is
deliberately **unbuilt** — it needs a model call and loses detail — and a test
pins that no run ever reports a strategy that did not run.

Only tool results are compressed. Eliding the model's own prose leaves it
reasoning about a conversation it no longer has; an elided user instruction is a
task that changed with nobody deciding it should.

The trigger is content size, not §8's 95%-of-a-token-budget: the loop has no
context-window figure, and inventing one would put a number in the code that
looks authoritative and is not.

**9/9 can-fail proven**, including deleting the `compress()` call from the loop
— a compressor nothing calls is a component that does nothing, which had
already happened twice in this repository.

### T3 · the concurrency half — measured, and not built

§8 promises *"reads parallel, writes exclusive … this will make NIKI feel 2-3x
faster"*. Measured through the real `ToolRegistry::execute` on three 400-line
files:

| | 3 reads |
|---|---|
| sequential | **1.62 ms** |
| joined | **0.88 ms** |

**0.74 ms per turn**, against a model request measured in seconds. And the
schedulable set is smaller than it reads: `bash` has no path, `web_fetch` takes
a `url`, `grep`/`glob` touch no single path — all exclusive by construction.
What is left is reads of different files, at about half a millisecond each.

Wiring the scheduler into `run_tool_loop` would restructure ~170 lines of the
hottest code in the repository to buy less than a frame. **Not built.** The
latency in §8 is in the *streaming*, not the parallelism — mid-stream dispatch
needs the loop to call `provider.stream()` where it currently calls
`provider.complete()` (`src/runtime/tools.rs:3727`), which is a separate change
with separate risk.

`path_lock.rs` and `scheduler.rs` stay: they are the safety content, both are
can-fail proven, and **neither is wired into the loop**, which is now stated
rather than left to be discovered.

### T2 · the hook layer — the model cannot overrule a rule

The property is not "the hooks deny things". It is **"the classifier is never
consulted once the hooks have decided"** — because a model that can overrule a
rule somebody wrote in a file is a hole in the first layer, not a second one.

The rules come from `permissions::PermissionConfig`, so `niki.toml` needs no new
format. `Ask` is deliberately not a decision, and a hard denial does not charge
the model's denial tally — a user's own rules denying twenty commands must not
trip the escalation limits and fail the run.

7/7 can-fail proven. **Still not built:** an `ActionClassifier` backed by a real
provider. The trait, the gate, the escalation limits and the reasoning-blind
view are all built and tested; what is missing is the thing that talks to a
model, which is a provider and model choice — the owner's decision.

### Three sabotages that did not bite, and two tests that were wrong

The hook layer's determinism test **could not** fail when the sort was removed,
because `HashMap` iteration order is stable within a process: it compared one
built object with itself. It now asserts the outcome the sort produces.

A test that called the `pending()` helper directly **could not** fail when its
caller stopped calling it — the exact sabotage that leaves the hooks deciding
nothing while every other test stays green. It now drives `adjudicate` end to
end.

And in T4, the "trust the elision marker" sabotage initially guarded only *one*
of the two elision strategies, so the other still dropped the middle and the
test stayed green. Guarding both made it red.

**Five proofs that did not run, across §8 — more than in any earlier batch**,
and every one found the same way: by reading the count and then asking whether
the *sabotage* was wrong before assuming the *test* was.
