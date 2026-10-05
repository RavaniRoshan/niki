# NIKI — ship + perform: PROGRESS

Append-only. Every entry records what was done, the exact command, and its real result. Re-read
`DESIGN.md`, `CHECKLIST.md` and this file at the start of every phase and after any compaction.

---

## 2026-10-05 — Phase 0: read-only audit (complete)

### Done

Ran every foundation gate and every named integration binary for the wiring rows. Full results are
in `CHECKLIST.md`; the design decisions are in `DESIGN.md`. Nothing in the repo was edited during
this phase.

Gates: `cargo fmt --check` (exit 0) · `cargo clippy --all-targets --workspace -j 2` (exit 0, 0
warnings) · `scripts/test-fast.sh` (1112 passed, 0 skipped) · `npx vitest run` (403 passed,
3 skipped, 22 files) · `cargo deny check` (all ok) · `cargo audit` (exit 0, 11 allowed warnings) ·
`cargo test --test journeys consumer_journeys_all_pass` (25 passed, 0 failed, 0 skipped) ·
`cargo build --release -j 2` (exit 0, 7m15s) · `./target/release/niki --version` (`niki 0.10.0`) ·
`scripts/demo.sh` (exit 0, branch `niki/06519d74`, verdict Approved).

Integration binaries: `sandbox_teardown` 4/4 · retry paths 23/23 · `security_exec` 16/16 ·
`supply_chain_policy_has_teeth` 4/4 · `mcp_call_path` 13/13 · `full_pipeline_branch` 2/2 ·
`run_lifecycle` 14/14 · `protocol_contract` 10/10 · `serve_protocol` 14/14 · `foundation_docs` 3/3
· `test_groups` 8/8 · risk/verifier/artifact/revision 19/19.

### Defects found by running the product, not reading it

1. **The release binary does not run on Debian bookworm-slim.**
   `niki: /lib/x86_64-linux-gnu/libc.so.6: version 'GLIBC_2.39' not found`. The build box is
   Ubuntu 24.04 (glibc 2.39); bookworm is 2.36. A static musl build removes the dependency.
2. **`tests/claims.rs` is red on `master`** — the tracked mission file documents `niki bench`.
3. **The fixture provider is unreachable** — `fixture-runtime` is off by default, no CI job
   enables it, so `niki serve --fixture` does not exist and three `fixture-loop` tests skip.
4. **No CI workflow runs the shell suite.**
5. **A tag push publishes a public, non-draft release.**
6. **The compiled shell writes escape garbage and exits 0 with no TTY.**
7. **`niki doctor` exits non-zero on a clean containerless box** while its fix text says otherwise.

### Decisions taken

- The owner approved the Phase 0 design note; P1 WIRING may proceed.
- `docs/ship/CHECKLIST.md` keeps the claims gate red until `niki bench` exists. Narrowing the scan
  was considered and rejected — it would weaken a gate to make a number look better.
- Bun-compile spike result recorded: works on ubuntu and debian, fails on alpine. Alpine will be
  documented as unsupported rather than silently broken.

---

## 2026-10-05 — P1 WIRING

### P1.0 — ship memory created

Created `docs/ship/DESIGN.md`, `docs/ship/CHECKLIST.md` and this file from the Phase 0 audit. No
code changed.

### P1.1 — `scripts/smoke_real_model.sh` (W1)

New script with two modes. `--provider fixture` starts `tests/integration/mock_llm.py` and needs
no key and no network; the default mode talks to a real provider via `NIKI_BASE_URL`, `NIKI_MODEL`
and the provider's key env var. Four checks: the binary starts, the provider answers, the answer
does not contain the key, and the same config drives a headless JSON run whose stdout is exactly
one JSON envelope.

Verification:

```
$ ./scripts/smoke_real_model.sh --provider fixture
  ok    niki 0.10.0
  ok    no escape sequences in the answer
  ok    chat exited 0
  ok    no key in the output
  ok    stdout is exactly one JSON envelope
SMOKE PASSED (fixture provider — plumbing proven, model reachability NOT proven)
```

The script was then run against two deliberate failures, because a check that has only ever been
green is not known to be a check:

```
$ NIKI_BASE_URL=http://127.0.0.1:9/v1 NIKI_MODEL=nope OPENAI_API_KEY=sk-test ./scripts/smoke_real_model.sh
  FAIL  chat exited 1
SMOKE FAILED — 1 check(s) failed                        exit 1

$ NIKI_BASE_URL=http://127.0.0.1:8080/v1 NIKI_MODEL=x ./scripts/smoke_real_model.sh
smoke: no API key in the environment for provider 'openai'.   exit 2
```

The real-provider path still needs an owner key and is OWNER-VERIFY.

### P1.2 — the fixture runtime is now reachable, and its absence fails the build

`fixture-runtime` is a cargo feature that was off by default and enabled by no workflow, so
`niki serve --fixture` did not exist and `shell/test/fixture-loop.test.tsx` skipped three tests.
The suite reported 403 passed / 3 skipped and looked healthy.

Three changes:

1. `shell/test/fixture-loop.test.tsx` — the test named `has the fixture-enabled binary` asserted
   only that the file existed. It now asserts the **feature**. Observed failing on the old binary
   before the fix, with the build command in the failure message.
2. `scripts/test-shell.sh` — builds `target/debug/niki` with `--features fixture-runtime` and
   refuses to run the suite if `--fixture` is absent, then runs vitest.
3. `.github/workflows/ci.yml` — a new `shell` job, because **no workflow had ever run the shell
   suite**. Its actions are pinned by full commit SHA; the rest of the file still uses tags and
   that debt is unchanged (R2).

Verification:

```
$ ./scripts/test-shell.sh
fixture runtime: present
 Test Files  22 passed (22)
      Tests  406 passed (406)
```

406 passed, **0 skipped** — the three silent skips are now three real tests, including
`replays the whole reference loop on the real screen`, which drives the real binary through a real
pseudo-terminal. That is the CI fixture-provider leg the W1 criterion asks for.
### P1.3 — W6 headless surface: `--atif-out`, `--max-time`, `--max-cost`, stdin

Four gaps in the scripting entry point, all closed behind one change.

1. **`--max-time` / `--max-cost`** are visible aliases on the existing `--max-wallclock-secs` and
   `--max-usd`. Aliases rather than new flags: one behaviour, one budget, two spellings — not two
   budgets that can disagree.
2. **`niki run -`** reads the task from stdin. Explicit, not inferred: a shell that pipes into
   `niki run` would otherwise have its pipe silently become the task, which is the kind of surprise
   that costs money.
3. **`--atif-out PATH`** writes an ATIF trajectory on both the success path and the failure path.
   A harness that only gets trajectories when the run worked cannot tell a crash from a skip.
4. **`src/artifacts/atif.rs`** — the writer. ATIF is an external, read-only interop format for
   Terminal-Bench's leaderboard; `niki-protocol` remains the only protocol NIKI speaks, and nothing
   in the engine ever reads an ATIF document back. It is built from the `events.jsonl` every run
   already writes plus the `StageMetric` records the pipeline keeps, so nothing in it is invented.
   Two properties are deliberate and tested: step ids are assigned by the writer and are strictly
   sequential from 1 (the validator rejects a gap), and a field the run did not report is **absent
   rather than zero-filled**, because a zero in a trajectory reads as a measurement.

Verification:

```
$ cargo test --lib atif                → 8 passed, 0 failed
$ cargo nextest run --test headless_flags --test run_lifecycle
  Summary  21 tests run: 21 passed, 0 skipped
$ cargo clippy --all-targets            → exit 0, 0 warnings
```

`tests/headless_flags.rs` spawns the **real binary** (nothing in-process, because the argument
parser and the exit contract do not exist inside an in-process call):

| Test | Asserts |
| --- | --- |
| `the_help_advertises_the_harness_spellings` | `--max-time`, `--max-cost`, `--atif-out` are in `niki run --help` |
| `the_harness_spellings_parse_into_the_same_budget` | both spellings reach the same `RunArgs` fields |
| `a_task_can_be_piped_in_on_stdin` | `run -` reads the task and the pipeline starts |
| `an_empty_stdin_task_is_refused_before_anything_is_spent` | whitespace on stdin is refused like any other empty task |
| `atif_out_writes_a_trajectory_that_validates` | valid JSON, declared schema version, ≥1 step, ids sequential from 1, sources legal, totals present, task recorded, no `.partial` left |
| `a_failed_run_still_writes_its_trajectory` | a run that fails non-zero still writes one |
| `an_unwritable_atif_path_warns_and_leaves_the_run_alone` | a failed export warns on stderr and does **not** fail a successful run |

One of these was itself wrong on the first run and was fixed rather than made to pass:
`an_unwritable_atif_path_warns_and_leaves_the_run_alone` originally blocked the target with a
*file* and expected a write failure. `write_to` writes a sibling and renames, and renaming a file
onto a file succeeds — so the test was asserting that a working export fails. It now blocks the
target with a directory.

### P1.4 — the Linux release artifact is not portable (blocker, partly mitigated)

`rustup target add x86_64-unknown-linux-musl` succeeds, but the musl build then fails here:

```
error occurred in cc-rs: failed to find tool "x86_64-linux-musl-gcc": No such file or directory
```

The C dependency is the vendored libgit2, and compiling it for musl needs `musl-tools`, which
needs sudo. `AGENTS.md` records that sudo is not available non-interactively on this box, so the
static musl build is an **owner action or a CI-only build**. It belongs in the release job, where
`sudo apt-get install -y musl-tools` is available; the target is already added to
`dist-workspace.toml` in P4 along with that step.

So that the regression is at least *visible* until then, `scripts/check-portability.sh` measures
the artifact instead of assuming it:

```
$ ./scripts/check-portability.sh --floor 2.36 target/release/niki
portability: target/release/niki
  statically linked: no (libz.so.1 libgcc_s.so.1 libm.so.6 libc.so.6 ld-linux-x86-64.so.2)
  glibc versions required: 2.2.5 2.3 … 2.34 2.39
  highest required: 2.39
  declared floor 2.36: FAILS — this build needs 2.39.
```

Three modes, each exercised against this repository's own artifact: `--report`,
`--floor X.Y` (passes at 2.39, **fails at 2.36 and at 2.2**), and `--musl` (fails for a gnu
binary, which is the truth today). The `--floor` gate is wired into a new `linux-portability` CI
job together with a selftest that requires the gate to reject an unmet floor.

The script had two real bugs when first written, both in the dangerous direction — a `--floor 2.36`
check **passed** a binary needing 2.39, and the printed version list truncated `GLIBC_2.2.5` to
`2.2`. Both were fixed and the corrected behaviour re-measured above, rather than the first green
result being accepted.

### P1.5 — the interface refused nothing when it had no terminal

`src/cli.tsx` called `enterTerminal()` unconditionally. With stdout redirected it wrote
`\e[?1049h\e[?25l\e[?1000h\e[?1006h\e[?2004h`, printed nothing, and **exited 0**.

`isInteractive()` now gates entry, and the refusal names the ways to work without a terminal —
the same bar `niki chat` already holds under `j_chat_without_a_terminal_says_so`. `TERM` is
deliberately not part of the test: `dumb` on a real TTY is a supported mode the render path
handles and the PTY suite proves, and folding it in here would take away a mode that works.

Before and after, on the **compiled** binary:

```
before:  ./dist/niki-shell < /dev/null   exit 0
         stdout: ^[[?1049h^[[?25l^[[?1000h^[[?1006h^[[?2004h
         stderr: (empty)

after:   ./dist/niki-shell < /dev/null   exit 2
         stdout: (empty — no ESC byte anywhere)
         stderr: niki-shell needs a terminal, and this process has none.
                 …niki run "…" --output-format json / --atif-out / niki chat -m …
```

`shell/test/no-tty.test.ts` is new (7 tests) and asserts no C0/C1 control byte on either stream, a
non-zero exit, the alternatives being named, and no panic. It was observed **failing** with the
fix reverted — `git stash push shell/src/cli.tsx` → `× writes no control bytes at all` — which is
how it is known to be a check rather than a description.

Full shell suite: **23 files, 413 passed, 0 skipped.**

### P1.6 — W13 uninstall

`scripts/uninstall.sh`, driving the same destination ladder `install.sh` uses in the same order,
so it removes what that installer placed rather than guessing. The rule that shapes it: **the
default removes files and keeps data.** Task history, run reports, memory and API keys are not
the installer's to delete, and `--purge` is how a user asks for that, out loud.

`tests/uninstall.rs` (7 tests) runs the real script against a real fake `$HOME`, because the
failure this guards against — a variable expanding to something enormous and a recursive delete
eating it — is only findable by running it. Covered: default keeps data; `--purge` removes it;
`--dry-run` changes nothing; **running it twice succeeds twice**; an explicit `NIKI_INSTALL_DIR`
is the directory cleaned and the default one left alone; an empty install dir falls through the
ladder exactly as the installer did; and an install dir of `/` is **refused** with exit 2 and
nothing removed.

Two of those assertions were wrong on their first run and were corrected against the script's
actual contract rather than made to pass:

- `an_empty_install_dir_does_not_delete_anything_unexpected` asserted the binary *survived* an
  empty install dir. An empty variable is an unset one, so the ladder correctly resolves
  `~/.local/bin` and removes what was installed. The test was asserting that the uninstaller
  could not find its own install.
- Its replacement asserted `niki-shell` survives, on the grounds that the uninstaller should not
  remove what it never claimed to own. It does claim it: the script removes both binaries, and
  the two-binary bundle is the point of R5.

Worth recording: both were written to a contract I had not re-read before asserting against it.
The correction was to go back to the script, not to loosen the expectation.

### P1.7 — W14 self-update is opt-in, and that is now asserted

`dist-workspace.toml` already sets `install-updater = true`, so cargo-dist ships an `niki-update`
binary next to the engine: a user runs it when they choose to, and cargo-dist verifies checksums.
The gap was that nothing *asserted* any of it, so any of it could change silently.

`tests/update_is_opt_in.rs` (7 tests) states the three properties as negatives, because that is
the only way to say "it never happens":

- **the engine never invokes an updater** — a walk over every `.rs` under `src/` for
  `niki-update` / `self_update` / `selfupdate`. A self-updating binary replaces the thing running
  it, the one operation where being wrong is unrecoverable.
- **no workflow runs an updater automatically** — a workflow that fetches and runs one on every
  push turns every commit into a supply-chain event. `cargo dist` *building* an updater is fine;
  *running* one is not, so the test distinguishes them.
- **the installer installs and nothing else**, and **uninstall removes the updater** too, so a
  removed install leaves nothing that can be run later.
- **the scanner can fail** — a decoy `Command::new("niki-update")` must be matched, so the first
  test is known to be a check rather than a description.

The decoy test was wrong on its first run (it expected two matching lines; only the invocation
matches, which is correct — the function signature does not mention an updater). Corrected
against what the scan should do, not loosened.

### P1.8 — W3 session export (markdown and ATIF)

`niki session export [<ID>] [--format markdown|atif] [--out PATH]`. The id is **positional**,
matching `niki session show`, which was the mistake the tests caught first.

Three properties are deliberate:

- **A message containing a code fence widens the wrapper fence.** Message content routinely
  contains its own ``` and a transcript that mangles them is not a transcript.
- **Usage and cost print only when the session recorded them.** Zeros would read as "this cost
  nothing", which is a claim the session never made.
- **An assistant turn maps to ATIF's `agent` source.** The stored role is `assistant`, which is
  not one of ATIF's three declared sources; inventing a fourth would be rejected by a validator.

Exporting when there is no session **fails and writes no file**, because an empty document is
indistinguishable from a session with no messages. A session that exists but has no messages still
exports and says so.

`tests/session_export.rs` — 7/7 against the real binary.

### P1.9 — W2 `niki config explain`: every value says where it came from

`niki config check` already reports syntax errors and unknown sections. What it could not do is
say what the loader actually used, which is the failure with no symptom: a setting that is not
doing what the file says simply is not doing what the file says.

`niki config explain` prints each tracked setting with its effective value and the layer that
produced it, using the loader's own precedence — taken from `NikiConfig::apply_env_lookup`, not
guessed: environment, then project file, then user file, then built-in default.

**Two real errors this introduced and the tests caught, both worth recording:**

1. **I invented environment variables.** The first version of the table named
   `NIKI_PLANNER_MODEL`, `NIKI_CODER_MODEL` and siblings. Nothing in the engine reads them, so
   the report would have claimed a file was overridden by an environment that does not exist — a
   lie a user would act on. The table now names only what `apply_env_lookup` actually consults:
   `ANTHROPIC_API_KEY` / `_BASE_URL` / `_MODEL` and the `OPENAI`/`GOOGLE` equivalents. The test
   that caught it had been *passing* against the wrong variable.
2. **The table would have printed API keys.** A diagnostic that prints a credential writes it to
   the terminal, the scrollback buffer and whatever CI captured. Secrets now report `(set)` and
   never their value, while still reporting their source.

`tests/config_explain.rs` — 9/9. Covers: a project-file value naming that file, the environment
beating a file and naming the variable, an unset value labelled a built-in default, the project
file beating the user file, **setting one section not being attributed for another** (the version
of this bug that makes `explain` actively misleading), numbers not rendering as strings, and the
secret case.

### P1.10 — W8: the Phase 0 audit was wrong, and one real gap it prompted

**Correction.** The Phase 0 audit recorded W8 as PARTIAL with "no path-traversal guard — no code,
no test". That was wrong. `resolve_tool_path` (`src/runtime/tools.rs:506`) exists, is applied to
`read`/`write`/`edit`/`patch`/`grep`, and `glob` refuses absolute and `..` patterns. It refuses
`..` outright rather than normalising, compares against a canonicalised root so a symlinked root
cannot make the prefix test lie, and canonicalises the deepest existing ancestor so a `write` to a
new file beneath a symlinked parent is still caught. 46 tests pass, including
`traversal_is_refused_rather_than_normalised` and
`a_symlink_out_of_the_tree_does_not_launder_a_write`.

The audit's grep was scoped to `src/safety/` and missed `src/runtime/`. The row is now **WORKS**.

**The gap it prompted.** Reading W8's actual wording — "secrets redacted in logs **and
trajectories**" — turned up something `--atif-out` introduced in P1.3: the journal reaches the
trajectory verbatim, and a tool argument routinely carries whatever the model was looking at. A
model shown a key will put it in a command line, and the trajectory is a file that leaves the
machine.

`steps_from_journal` now runs the same `redact_secrets` every log line gets, over the whole event
rather than a list of named keys — the keys a secret can appear in are exactly the ones nobody
enumerated. `a_secret_in_the_journal_does_not_reach_the_trajectory` covers both a bearer token and
an `OPENAI_API_KEY=` assignment, and asserts the tool-call record still survives, because a
redacted trajectory with no tool calls is one nobody can read. `cargo test --lib atif` → 9 passed.

### P1.11 — W5: an auto-approval left no record

`PermissionRequirement::Ask` under a headless mode returned `Ok(())` and wrote nothing. The
decision is correct — `dontask` means "Ask becomes Allow" — but it was silent, so a run that
allowed every command left nothing to audit afterwards. The sandbox backends already print
`niki: auto-approved '…'` for their own prompt; the **policy layer** had no equivalent.

`src/runtime/policy.rs` now logs to both `tracing` and stderr. `tests/approval_logging.rs` (7/7)
holds the line that matters: the change must not make anything *more* permissive. Asserted —

- `manual` still refuses an Ask headlessly (fails closed), and the message names both the tool
  and the posture.
- an explicit `Deny` override holds under every mode.
- the Planner still cannot run `bash` under **any** mode; no permission mode makes it read-write.
- an `Allow` declaration needs no posture and gains no log line.
- the default posture is still `manual` — nothing here may move the default.

**Not done, and it needs an owner decision.** `--permission-mode bypass` still takes no explicit
confirmation. Requiring a second flag is a safety-critical default change that alters the contract
of every existing invocation, and it is on the "ask me" list. The mission's requirement —
"a no-review mode needs explicit confirmation and is never the default" — is half met: the default
is `manual`, but the confirmation is not there.

### P1.12 — W12 `niki doctor`: the right severity, and a terminal check that did not exist

Two changes, both driven by the clean-container measurement in Phase 0.

**1. Severity of `(docker backend, no container runtime)`.** It reported `Fail`, so `niki doctor`
exited non-zero on machines where NIKI runs perfectly well — the same class of bug as the bare
"no container runtime" line this check replaced: true, useless, alarming. On a machine with git it
is now a **warning** that names the flag that makes the next command work
(`niki run "…" --backend worktree`). With no git either it stays a failure, because then there is
genuinely no backend the machine can run. The two differ on a fact, not on tone.

Measured in clean containers, both halves:

```
# git present, no container runtime
  ✓ git — git version 2.43.0
  ⚠ sandbox backend matches this machine — backend = docker but no container runtime was found.
    Your next run still works if you pass `--backend worktree`, which needs no container: …
Summary: 27 checks, 6 passed, 21 warnings, 0 failed        exit 0

# no git, no container runtime
  ✗ git — git not found
  ✗ sandbox backend matches this machine — … and git is not installed either, so neither backend
    can run. …
Summary: 27 checks, 5 passed, 20 warnings, 2 failed        exit 1
```

**2. A terminal-capability check, which did not exist at all.** `niki-shell` with no TTY used to
write escape sequences into whatever captured it and exit 0 (P1.5). `doctor` said nothing about
terminals, so a user in a container or a CI job had no way to learn it from the command the README
tells them to run first. It now distinguishes three states — a real terminal, `TERM=dumb` (a
supported reduced mode, not a problem), and no TTY (warns, and names the headless commands that
do work).

`cargo test --lib cli::doctor` → 12 passed, and the 25 consumer journeys still pass.

### P1.13 — W16 the arm64 Linux target now builds in CI, and cannot drift again

`aarch64-unknown-linux-gnu` has been declared in `dist-workspace.toml` all along and therefore
*ships* — but no workflow ever compiled it. A release target nothing builds breaks at release
time, in the one job where a failure is most expensive. It is now a `build-matrix` row on
`ubuntu-24.04-arm`, a native runner rather than a cross-compile.

`tests/target_matrix.rs` (5/5) closes the row in both directions: **every target the release
ships is built in CI**, and **every target CI builds is one the release ships**. The second
direction matters as much — a CI build for a target nobody ships is runner-minutes spent proving
nothing. Plus the supported-platform list (Linux x64/arm64, macOS x64/arm64, Windows x64) and the
existence of a Windows job that uses PowerShell.

**The gate was observed failing before it was satisfied:**

```
$ git stash push .github/workflows/ci.yml && cargo nextest run --test target_matrix
  FAIL every_target_the_release_ships_is_built_in_ci        2/5 run, 1 failed
$ git stash pop && cargo nextest run --test target_matrix
  Summary 5 tests run: 5 passed, 0 skipped
```

Two parser bugs were fixed along the way, both of which made the gate wrong rather than weak:

- it read only `- target:` matrix rows, so the `windows-latest` job — which builds
  `x86_64-pc-windows-msvc` by virtue of the runner, with the triple never written in the file —
  looked unbuilt. The runner→target mapping is now stated explicitly as four lines rather than
  guessed from runner names.
- it counted `${{ matrix.target }}` as a target triple, inventing a platform that does not exist.

### P1.14 — W17 SKILL.md skills load

The loader read `SKILL.md` but took every field from a sibling `metadata.json`, and discovered
only `<output_dir>/skills`. A skill written in the format Claude Code and Kimi Code use — `---`
fenced YAML frontmatter, no JSON sibling — was therefore invisible. This is the row that says
"so existing skills work", and they did not.

`src/skills/mod.rs` now parses flat `key: value` frontmatter and maps `name`, `description`,
`summary`, `when_to_use`, `version` and `status: retired`. `skill_search_paths` adds
`.claude/skills`, `.agents/skills` and `~/.agents/skills`; `list_all_skills` merges them with
NIKI's own promoted skills **winning** a name collision, because the promoted copy is the one
NIKI maintains and the one the lock file hashes.

Design decisions worth recording:

- **No new dependency.** Flat scalars only, which is what these fields are; a skill whose header
  uses nested YAML still loads, with the fields NIKI uses populated. The recommended-crates list
  is ask-before-adding, and this was not worth a YAML parser.
- **The directory name is the identity**, not the header's `name:`. A skill listed under a name
  it does not answer to is worse than one listed under the name it does.
- `summary` and `when_to_use` are **fallbacks, not additions** — the long `description` wins, and
  the fields are never concatenated into something no field described.
- A header with **no body** is not a skill; a file with **no header** is all body.

**Proven against real skills, not only fixtures.** `tests/skill_md_compat.rs` copies 25 entries
out of this machine's own `~/.agents/skills` and loads them through the same path: all 25 present,
all 25 with a description, all 25 with a non-empty body. Unit tests in `src/skills/mod.rs` (16 in
the module) cover a genuine third-party header — extra keys, single and double quotes, a
non-numeric version — and the `name:` vs `namespace:` prefix trap.

One parser bug was caught this way: the guard that was supposed to reject a longer key sharing a
prefix was inverted, so it rejected *every* key and the description came back empty. The
`strip_prefix(key).and_then(strip_prefix(':'))` chain already rejects `namespace:` for `name`;
the extra guard was not only wrong, it was unnecessary.

### P1.15 — W10 the budgets are now enforced

`shell/test/perf.test.tsx` measured first-frame latency, idle cost and per-token cost and
**printed** them. Printing a number is not holding a line: nothing failed when the first frame
went from 5 ms to 400 ms, because nothing compared it to anything. Memory was not measured at all.

`shell/test/perf-budgets.test.ts` (5 tests) is the comparison, plus the missing measurement.
Every number is printed on every run, so a regression shows what it cost, not only that it
happened:

```
first frame (budget 150ms) — 49x16: 2.56ms  50x16: 4.73ms  79x24: 4.48ms  80x24: 4.30ms
                              119x30: 4.75ms  120x38: 4.31ms  180x50: 4.34ms
idle render — one frame 6.069ms, over 50 frames 5.185ms/frame
per-token cost ratio, 10x transcript — 1.25 (budget 1.5)
memory with a large transcript — growth 0.6 MiB (budget 400), resident 174.8 MiB (budget 1024)
```

The memory ceiling is set an order of magnitude above the measured growth so it catches a real
leak rather than a GC pause, and a test asserts the budgets are the ones the file names — a
budget that can be silently raised is not a budget.

**The gate was observed failing:** with `FIRST_FRAME_BUDGET_MS` lowered to 0.001 ms the suite went
red with `first frame at 49x16 took 2.77ms, over the 0.001ms budget`, then green again on restore.

Full shell suite: **24 files, 418 passed, 0 skipped.**

### P1.16 — hand-typed numbers removed from the README, and a gate so they cannot come back

The README claimed *"946 unit tests · ~540 integration tests across 53 binaries"* while the
measured values were 1129 / ~1100 / 131 — stale for several releases, because nothing compared
them. It also claimed *"first verified branch in under five minutes"* and *"~$0.01 on Claude
Sonnet 4, <$0.005 on Haiku"* — a wall-clock figure and two cost figures, all typed by hand and
all outside the no-invention rule.

All of it is gone. The README now says what is true and points at what measures it: every run
reports its own real tokens and cost in the JSON envelope, and `scripts/demo.sh` runs the whole
first-run story if you want to time it yourself.

`scripts/gen-readme-counts.sh` is the gate, and its job is the **negative** one. Two obvious ways
to publish a generated count both lie:

- counting declarations gives **360** shell tests against **418** that run, because some are
  generated inside loops and `.each` blocks;
- counting a run records one machine on one day and is stale the moment a test is added.

So the script measures and prints, and `--check` **fails if a suite-size count reappears in the
README**. Its first regex flagged two false positives (`8/8 tests passed` in a sample transcript,
`entry points, tests, risk signals` in a description) and was narrowed to the shapes that are
actually a claim about suite size. A gate that flags prose trains people to disable gates.

Observed failing before the README was fixed:

```
$ scripts/gen-readme-counts.sh --check
README states a test count. Nothing regenerates it, so it will be stale:
51:<sub>946 unit tests · ~540 integration tests across 53 binaries · 11 canaries that inject real
exit 1
$ scripts/gen-readme-counts.sh --check
README states no hand-typed test count.      exit 0
```

## The claims gate is still red, and why

`tests/claims.rs::every_niki_command_in_every_documented_surface_exists` fails because
`niki bench` does not exist and is named in documents under `docs/`:

```
3 docs/ship/niki-ship-and-perform-mega-prompt.md   (the tracked mission file — pre-existing)
3 docs/ship/DESIGN.md
2 docs/ship/CHECKLIST.md
2 docs/ship/PROGRESS.md
```

The gate was **already red on arrival** for the first of those. The other seven are mine, and
they are accurate: they describe `niki bench` as a P2 deliverable, not as something to run. The
gate is not weakened and the scan is not narrowed — the fix is to build `niki bench`, at which
point all four sources go quiet. Recorded here rather than hidden, because a red gate that is
explained in one place is a task and a red gate that is quietly exempted is a lie.

### P1.17 — W2 the config is validated against the schema, and the schema was badly wrong

The last W2 gap. `NikiConfig::validate_against_schema` checks a raw `niki.toml` against
`config_schema_json()` — the same document `niki config schema` hands an editor — so a key added
to one is automatically known to the other. `load()` **warns**, `load_file_only()` **refuses**,
which is the split that function exists for. It catches unknown keys inside a section, scalar
types, and enumerated values.

Wiring it up turned up **far more than the gap it was built to close.**

**The schema was missing most of the product.** Measured before the fix, over the 23 sections
`NikiConfig` actually has:

| What | Count |
| --- | --- |
| Sections the schema never declared | 4 — `budget`, `tools`, `hooks`, `commands` |
| Sections declared with no field list at all | 9 |
| Sections with a field list that was incomplete | 7 |
| Individual fields missing | `general.language`, `docker.network_disabled`, `docker.network_allowlist`, `session.max_messages`→`max_sessions`, `mcp.servers`, `permissions.rules`, `risk.denylist_patterns`, and more |

`niki.example.toml` documents `docker.network_disabled`, which the engine reads and the schema
did not declare — so an editor consuming that schema would have flagged a correct line as invalid.

The schema literal is now **generated from the config structs** rather than hand-maintained, which
is the only way it stays complete. Fields whose Rust type is not a primitive declare **no type at
all** rather than a guessed one: a wrong `type` makes the validator reject correct files, which is
worse than not checking the field. Verified complete — 23 sections, 0 gaps.

**Six pre-existing test fixtures were wrong**, and had been wrong silently:

```
-  ("[session]", "max_messages = 10"),          → max_sessions
-  ("[hooks]", "enabled = true"),                → timeout_seconds
-  ("[knowledge]", "enabled = true"),            → doc_globs
-  ("[risk]", "enabled = true"),                 → mode
-  ("[goal]", "enabled = true"),                 → max_iterations
-  ("[commands]", "enabled = true"),             → extra_dirs
-  ("[permissions]", "enabled = true"),          → prompt_timeout_seconds
```

Each asserted "this section must be usable with one setting on its own", and each named a field
that never existed. The assertions passed because nothing checked. `tests/claims.rs`-style gates
only catch what they were written to look for.

**One finding is yours, not mine:** `~/.config/niki/niki.toml` on this machine sets
`providers.nvidia.api_key_env`, which `ProviderConfig` has never had. The validator says so on
every `niki config check`. That is the product telling you something true about your own config;
I did not edit it.

`tests/config_schema_validation.rs` — 10/10, including a test that fails if any declared section
is left without its field list (the escape hatch this design deliberately closed), one that a
`[ui]` typo is caught, and one that `niki.example.toml` matches the schema.
