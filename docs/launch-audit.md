# NIKI Repository Audit — Launch Readiness

> **Date:** 2026-09-30
> **Crate version:** 0.9.0 (unreleased) · **MSRV:** 1.88 · **Edition:** 2024
>
> 0.9.0 has not been cut. The installer manifests in `homebrew/`, `scoop/`
> and `winget/` therefore point at **0.8.0**, the newest release that exists,
> with 0.8.0's checksums — while `Cargo.toml` says 0.9.0, which is what the
> source tree is. `docs_consistency.rs` asserts the two agree; it is the
> *release*, not the docs, that closes the gap. The count in the table below
> is the tree as it stands.
> **Purpose:** State, without hedging, what this repository actually is.

---

## How to read this document

The previous edition of this file was dated 2026-08-17 and described version
0.4.0. It sat at the top of the repository for six weeks and five releases
while the crate moved to 0.9.0, and it was cited from `README.md` as the
methodology behind the project's honesty. It claimed "Hooks: Not
implemented", "No `niki session` CLI", "No `niki init`", "Scoop/Winget stale
v0.3.1" and "~30,600 lines" — every one of which had been false for months,
including a module map that omitted eleven of the thirty-one module
directories.

That is not a drafting failure. Nothing read it. The claims gate
(`tests/claims.rs`) covered three filenames, and this was not one of them.

So this edition is written to be checkable, and `tests/docs_consistency.rs`
fails the build when the version-shaped facts below stop matching the tree. If
you change the crate version, this file has to change with it — that is the
point of the test, and the previous edition had no such test.

Every number in the "Current shape" section is derived from the tree on the
date above. Where a number is a judgement rather than a count, it says so.

---

## Current shape

| Fact | Value | Where it comes from |
|---|---|---|
| Crate version | 0.9.0 | `Cargo.toml:3` |
| Edition / MSRV | 2024 / 1.88 | `Cargo.toml:4-5` |
| Rust source files | 202 | `find src -name '*.rs'` |
| Rust lines in `src/` | ~85,700 | `find src -name '*.rs' -exec cat {} + \| wc -l` |
| Module directories | 29 | `ls -d src/*/` |
| CLI subcommands | 22 | `enum Commands` in `src/main.rs` |
| Baseline tools registered | 22 | `build_baseline_registry()`, `src/runtime/tools.rs:2704-2737` |
| Distinct LLM client implementations | 4 | `anthropic.rs`, `openai.rs`, `google.rs`, `ollama.rs` |
| Release targets | 5 | `dist-workspace.toml` |
| Documentation pages | 39 | `find docs/content -name '*.mdx'` |
| Test binaries | 52 | `ls tests/*.rs` |
| Canary defects | 11 | `mutants/canaries.toml` |

**One of these is worth reading twice, because the number and the implication
differ:**

- **Providers: 4 clients, 12 slugs.** The README advertises twelve providers,
  which is true as a list of accepted slugs. `src/llm/provider.rs:253-256`
  routes `openrouter`, `nvidia`, `together`, `groq`, `deepseek`, `zen`, `kimi`
  and `kilo` through a single OpenAI-compatible client that differs only by
  base URL — and says so in a comment. That is a reasonable design for
  OpenAI-compatible gateways; it is not twelve independently-implemented
  providers, and the documentation should not imply that it is.

*(An earlier draft of this table said 20 baseline tools. It is 22, and the
README was right. The number in this file is now re-derived by
`tests/docs_consistency.rs` on every build, precisely so that a count nobody
re-measured cannot become a published fact.)*

---

## What works, end to end

Not "implemented" — exercised by a gate that has been shown to fail.

- **Four role-isolated agents, Planner → Coder → Tester → Reviewer**, each
  producing a schema-validated JSON artifact, with the Reviewer able to bounce
  work back to the Coder.
- **A real git branch.** `tests/product/runners/run_scenarios.sh` asserts that a
  run creates exactly one `niki/<id>` branch carrying at least one commit that
  changes at least one file, that the starting commit is still an ancestor, and
  that the starting branch was not moved. Those assertions are themselves
  verified by `run_scenarios_selftest.sh`, which builds a repository for each
  way the product could be broken and requires every one to be rejected.
- **A verdict with provenance.** `RunOutcome` is derived at the single
  `PipelineResult` build point, and `verdict` is a projection of it — a run no
  independent stage reviewed cannot report an approval.
- **A trust boundary that is enforced where it is used, not only where it is
  tested.** Network egress, permission modes and the `Ask`-deny-fail-closed
  rule live in `ToolRegistry::permission_denial`, the enforcement path, because
  the rule they replaced lived in `permissions::resolve_tool`, which no product
  code called.
- **Hermeticity by default**: egress blocked unless allow-listed, `CapDrop
  ALL`, optional read-only rootfs.
- **A working no-container, no-key path.** `niki init --interactive` detects
  what the machine can run and writes a config that runs; `niki doctor` reports
  the backend *against* the machine; `niki smoke` inherits the configured
  backend. Pinned by journeys J20–J25, including one that forces the worktree
  backend so the containerless path is exercised even on a machine that has
  containers.
- **Honest numbers where they are measured**: per-run token counts and cost,
  spend-cap enforcement, and unpriced models warned about rather than silently
  billed at $0.00.

## What is thinner than the documentation implies

Fixed in this release, and each fix has a test that fails if it regresses:

- **The goal loop now carries what it learns.** It did not: it accumulated
  `context_summary` and `negative_knowledge` faithfully and then handed the
  pipeline a `Task` built from the description alone, so iteration 2 could
  repeat iteration 1's mistake. It still does not *retry* a blocked task — that
  changes the semantics of a persisted task list and is a separate decision.
- **Promoted skills are visible to the agents that load them.** Under a custom
  `[general] output_dir`, promotion wrote to the configured directory and the
  runtime `skill_list`/`skill_load` tools read a hardcoded `.niki/skills`, so a
  skill was promoted with a success message and then never loaded.
- **An expired key no longer defeats a fallback chain.** A 401 from a stale
  primary aborted the run before the fallback was tried — the most common
  reason anyone configures one.

Still true, and the reason each is stated rather than fixed:

- **The tool loop is off by default.** `[tools] experimental_tool_loop`
  defaults to `false`, and the only production call site is
  `run_experimental_research` — a bounded pre-Planner research pass. In a
  default run the four agents receive a deterministic context pack and emit
  schema-valid JSON; they do not use the twenty-two baseline tools. This is
  arguably the right design for a pipeline whose selling point is
  reproducibility, but `CONTRIBUTING.md`'s "add a tool" instructions and the
  README's module map read as though a run is tool-driven. It is not. The
  reasoning, and what would change it, is in
  [`decisions/tool-loop.md`](decisions/tool-loop.md).
- **Parts of the learning layer are written and never read.** `load_index` and
  `query_store` in `src/store/` have no production callers; the persisted
  index is built only by manual `niki index` and nothing in the agent path
  rebuilds or freshness-checks it. `record_memory_use` has no callers at all.
  Hierarchical memory and learned-pattern ranking *are* wired into the agent
  path; the index beneath them is not.

## Known rough edges, deliberately left

- **The visual-regression gate cannot be made green from a developer machine.**
  The render differs from the CI runner's by 6.6–9.4% of pixels. Recovery is
  `gh workflow run ci.yml -f regen=true` plus a human looking at the frames.
- **The marketing screenshots in `assets/screenshots/` are orphaned and
  misleading.** `diff.png` shows the first-run onboarding modal rather than a
  diff, and both have body text running past the window edge. Nothing links to
  them. The verified frames for the current UI are `tests/visual/reference/`.
- **Cloud execution is deferred.** The sandbox trait has a seam for it; there is
  no cloud backend.

---

## Exit criteria

| Question | Verdict |
|---|---|
| Can a newcomer reach a first branch with no container runtime and no key? | **Yes**, and it is asserted by J20–J25 rather than claimed |
| Does a run leave a real branch with a real commit? | **Yes**, and the assertion is verified against six broken repositories |
| Can a verdict be reported that no independent stage granted? | **No** — `RunOutcome` is derived, `verdict` is a projection |
| Are the user-facing claims machine-checked? | **Yes for commands, links, false guarantees and self-reported versions.** **No** for prose claims about behaviour that no test can falsify, and the README's cost figures are in that category |
| Is the gate known to be able to fail? | **Yes.** `run_scenarios_selftest.sh` and the claims walk's own coverage guard exist for exactly this |
| Can a small local model complete a run? | **Often not.** `qwen2.5-coder:3b` fails at the Coder stage on ordinary tasks because each stage must emit a schema-conformant artifact. `scripts/dogfood.sh` measures this against your own machine |

**The honest summary:** the product is sound where it is measured, and the
measured surface is now much larger than it was. The remaining risk is
concentrated in prose that no test can falsify, and the standing instruction
for that is the one this project already uses — write down what is actually
true, and mark what is not.
