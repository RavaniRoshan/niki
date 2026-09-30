# RELEASE REPORT — NIKI hardening programme, batch 1

Branch `niki/hardening` · base `c7e297d` · 14 commits · 70 files changed, +6035 / −707.

Produced by executing `plans/namor-obsidian-jay-garrick.md`, with the four owner
decisions in §0a. Every gate below was produced by `./scripts/verify.sh`; the raw
output is in `EVIDENCE.md` and `.evidence/`.

---

## 1 · The gate table

| Gate | Result | Evidence |
|---|---|---|
| **G1** clean clone, install, README quick start | **PASS** | all scripts parse; every published download URL resolves; the README's commands are accepted by the real binary |
| **G2** build, lint | **PASS** | `cargo fmt --check`; `cargo clippy --all-targets -D warnings`; release build |
| **G3** tests + can-fail map | **PASS** | lib 951; `run_lifecycle` 13; 23 can-fail entries all resolve to a real test |
| **G4** the core flow, for real | **PASS** | `run_lifecycle` 13; `chat_runs_the_pipeline` 1; `scripts/demo.sh`; the tmux suite **12/12** |
| **G5** security | **PASS** | `cargo deny check` ok; `cargo audit` ok (11 pre-existing allowed warnings); no credentials in the tree or in git history; `security_exec` |
| **G6** failure paths | **PASS** | 12 cases, each asserting an exit code **and** a message a person can act on |
| **G7** docs match reality | **PASS** | `claims` and `docs_consistency`; every README command parses against the real binary |
| **G8** CI green | **UNVERIFIED-EXTERNAL** | the branch is committed locally and **not pushed**, per the owner's decision. G8 reports the last *pushed* run, whose one red job — `Manifest parity` — is the version skew this branch fixes. |
| **G9** no dead code, no fake features | **PASS** | no `todo!`/`unimplemented!` in production code; no `assert!(true)`; every subcommand appears in the README |

**Eight of nine pass locally.** G8 is unverified by construction, and its reason
is recorded rather than worked around. A green G8 needs one `git push`.

## 2 · What changed

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
- **The canary map.** 23 features, each naming the test that proves it can fail.
  G3 fails if one is added without an entry.
- **The manifests are repinned to 0.8.0.** `Cargo.toml` said 0.9.0, the newest
  published release was 0.8.0, and brew/scoop/winget all pinned 0.9.0 and 404'd —
  so `brew install niki` was broken while 21 of 22 CI jobs were green. Every URL
  **and every checksum** was repinned; `Cargo.toml` is back at 0.9.0 because that
  is what the source tree is. A release cut is what closes the gap.

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

1. **G8 is unverified.** Nothing here has run on CI. The most likely CI-only
   failure is the visual-regression job: `tests/visual/run.sh` documents that
   reference frames can only be blessed on the GitHub runner, and this branch
   changes chat rendering. If it goes red there and nowhere else, that is why.
2. **The TUI is not finished.** §1 of `ROADMAP.md` is the largest remaining
   cluster: `↑ ↓ j k` are dead on 10 of 14 pages, `Tab` is claimed by three
   controls, `q` quits from every sub-page, `?` does not reach the Help page.
   None of it blocks the core promise and all of it is in a user's way.
3. **The TUI still cannot check out a branch.** `git checkout` in the product is
   one printed hint on the *non*-TUI path. The last step of the journey still
   needs a second terminal.
4. **Permission prompting is broken in two ways.** Unreachable in chat (the
   event source is never wired), and where it does work it auto-denies after 5
   seconds while saying *"Command denied by user"*.
5. **There is a known path escape.** `WorktreeSandbox::apply_patch` joins a
   model-authored path onto the worktree root with no containment check, so an
   absolute or `..` path writes outside the project. `ROADMAP.md` §4.1 explains
   why it was deferred — it needs a judgement call about which backends may
   accept a path at all, and a schema change. **This is the most serious thing
   left, and it should be the first item of batch 2.**
6. **A signal-killed test suite is still reported as passing**, so an OOM-killed
   run can still cut a branch. Two lines; `ROADMAP.md` §5.
7. **No push has happened.** Per the owner's decision this branch is local.

## 6 · The roadmap

`ROADMAP.md` — navigation and the dead controls; chat ↔ pipeline depth; loop
unification; five security items; six reliability items; six coverage items; and
three things explicitly killed, with the reasoning.

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
5. **Optional:** an `OPENROUTER_API_KEY` to run G4's real-provider leg instead of
   the scripted server. Without it that leg is `UNVERIFIED-EXTERNAL`, which is
   how it is recorded.

Nothing else is blocked on you. Batch 2 starts at `ROADMAP.md` §4.1.
