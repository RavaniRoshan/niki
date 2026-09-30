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

### Next

T1 · Make the PTY TUI gate able to fail. `.github/workflows/ci.yml:553, 636-652` runs pytest through
`|| true` and decides on `grep -qE '[0-9]+ passed'`, so `3 passed, 17 failed` is **green**. Until
this is fixed, T2–T10 would be "done" with no gate able to prove it.

### Blockers

None.

### Assumptions in force

- The 3 dirty files were an interrupted prior session; only the labelled sabotage was reverted.
- `niki chat` is a **coding agent**, not a viewer (owner decision, §0a).
- For the 12 slash commands that lie, **retracting the claim is the default**.
