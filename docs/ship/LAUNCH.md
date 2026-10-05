# NIKI — Launch Kit & Owner Execution Guide (P6, DoD-6)

This document contains the exact, ordered checklist of OWNER-ONLY actions required to finalize and publish a release, along with rollback references, launch communication drafts, and Harbor leaderboard submission steps.

---

## 1. Owner-Only Action Checklist (Strict Dependency Order)

All automated gates, builds, tests, and rehearsals are complete. The remaining steps cross external boundaries and require your keys, accounts, funds, or explicit human decision.

| Step | Scope | Action | Command / Procedure | Verification / Rollback |
|---|---|---|---|---|
| **1** | **W1 (Real Model Smoke)** | Verify first-answer against live LLM | `OPENAI_API_KEY=sk-... ./scripts/smoke_real_model.sh` | Prints `SMOKE PASSED (real provider)`. Exit 0. |
| **2** | **P5 (Eval Budget & Run)** | Approve budget & execute live evaluation | `cargo run --release -- bench run --budget-usd 25.0 --split SEALED` | Generates `bench/results/final_results.json` and updates `bench/SEALED_LOG.md`. |
| **3** | **P5 (Generate README)** | Generate honest benchmark report for README | `cargo run --release -- bench report --results bench/results/final_results.json --baseline bench/results/baseline_results.json` | Paste exact generated markdown into README.md without hand-edits. |
| **4** | **P4 (Tag & Push)** | Tag release commit and push to GitHub | `git tag v0.10.0 && git push origin master --tags` | Check `git describe --tags`. Rollback: `git tag -d v0.10.0 && git push --delete origin v0.10.0`. |
| **5** | **R12 (Undraft Release)** | Review Draft Release & publish | Open GitHub Releases -> Edit Draft `v0.10.0` -> Uncheck "Draft" -> Save | Assets available at `https://github.com/RavaniRoshan/niki/releases/download/v0.10.0/`. |
| **6** | **R8 (Homebrew Tap)** | Create `homebrew-niki` repo & push formula | `brew tap-new RavaniRoshan/niki && cp homebrew/niki.rb ... && git push` | Test: `brew install RavaniRoshan/niki/niki`. |
| **7** | **R7 (npm Publish)** | Publish thin launcher to npm | `cd packages/niki && npm publish --provenance --access public` | Test: `npx niki-cli --version`. Rollback: `npm deprecate niki-cli@0.10.0 "yanked"`. |
| **8** | **Harbor Submission** | Submit validated 5-trial package to leaderboard | `harbor leaderboard submit --results-dir bench/results/leaderboard_package/` | Inspect validation output before confirming submission prompt. |

---

## 2. Rollback & Emergency Contacts

Refer to [`docs/ship/ROLLBACK.md`](./ROLLBACK.md) for detailed emergency protocols.

Quick rollback reference:
- **Demote release to draft**: `gh release edit v0.10.0 --draft`
- **Delete tag**: `git push --delete origin v0.10.0 && git tag -d v0.10.0`
- **Deprecate npm**: `npm deprecate niki-cli@0.10.0 "Critical defect; use previous"`
- **Revert Homebrew formula**: `git -C /path/to/homebrew-niki revert HEAD && git push`

---

## 3. Launch Post Text Draft

*(Rule of proof: Performance numbers must be inserted directly from `niki bench report` output. Do not type numbers by hand.)*

```markdown
Title: Introducing NIKI: A Hermetic Multi-Agent Coding Harness

Today we're releasing NIKI v0.10.0, an open-source coding agent harness built in Rust with a TypeScript terminal shell.

Key design points:
- Hermetic sandboxing: isolated worktree and container execution backends.
- Real human approval safety: fail-closed by default, explicit review on every mutation.
- ATIF trajectory export: compliant with the Agent Trajectory Interchange Format.
- No telemetry, no unexpected outbound network calls, zero task-specific benchmark contamination.

Performance on Terminal-Bench:
[INSERT GENERATED TEXT FROM: niki bench report --results ... --baseline ...]

Install:
  curl -fsSL https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.sh | bash
  # Or on Windows:
  irm https://raw.githubusercontent.com/RavaniRoshan/niki/master/scripts/install.ps1 | iex

Source & Documentation:
  https://github.com/RavaniRoshan/niki
```

---

## 4. Harbor Leaderboard Submission Protocol

1. Ensure 5 trials were completed for every task on the benchmark using multiplier 1.0.
2. Run ATIF trajectory validator on all exported trajectories:
   ```bash
   find bench/results/trajectories/ -name "*.json" -exec niki bench validate {} \;
   ```
3. Verify zero contamination using the automated gate:
   ```bash
   cargo test --test contamination -j 2
   ```
4. Validate the package client-side:
   ```bash
   harbor leaderboard submit --validate-only --package bench/results/leaderboard_package/
   ```
5. Confirm submission manually when prompted.
