# Niki Launch Playbook: Borrowed Demand, Earned Trust

> **Owner:** Niki launch team  
> **Launch tier:** Tier 2 — targeted open-source launch  
> **Launch type:** New developer-tool release  
> **Primary outcome:** developers can install a production binary, run it locally, and review a generated Git branch without surrendering control of their machine or code.  
> **Decision rule:** do not trade trust for launch-week signups.

This is an execution checklist, not a promise that every channel is already published. A checkbox is complete only when its stated evidence exists.

## 1. Operating thesis

- [ ] Lead with the job Niki does better: **a bounded, reviewable change**, not an always-on coding companion.
- [ ] Treat trust as the product. The launch claim is credible only when the behavior is reproducible:
  - [ ] no telemetry is collected by default;
  - [ ] updates are never forced or silent;
  - [ ] sandbox permissions are visible and reviewable;
  - [ ] every agent handoff leaves an artifact;
  - [ ] the output is a Git diff a human can inspect.
- [ ] Borrow demand from existing AI-coding communities by answering a specific complaint, not by posting a generic “new agent” announcement.
- [ ] Make the setup path as honest as the architecture:
  - [ ] a local text-only path for developers who want no API key and no container runtime;
  - [ ] a restricted Docker/Podman path for the multi-agent sandboxed workflow;
  - [ ] no claim that a local model or a container is a perfect security boundary.
- [ ] Measure public, user-volunteered signals rather than inventing hidden analytics.

## 2. Demand stealing

### 2.1 Frustrated audiences and unmet requests

The following is a demand map for **launch messaging**, not a claim that every complaint represents every user of the named project. Validate the linked complaints and current issue state before quoting them publicly.

- [ ] **Cursor**
  - [ ] Audience: developers who like an editor-integrated agent but dislike opaque, credit-consuming loops and changes that are hard to attribute.
  - [ ] Repeated request/complaint: make long-running agent work bounded, inspectable, and stoppable; show exactly what changed and why.
  - [ ] Niki response: typed Planner/Coder/Tester/Reviewer artifacts, visible attempts, and a reviewable branch instead of an invisible editor-side cascade.
- [ ] **Windsurf**
  - [ ] Audience: developers who want IDE context but worry about mistake-proneness and loss of confidence after a bad step.
  - [ ] Repeated request/complaint: make each step inspectable and reversible, with a clear review boundary before work continues.
  - [ ] Niki response: explicit requirements and review criteria in typed handoffs, a separate review stage, and a diff that can be rejected without undoing the editor session.
- [ ] **Devin**
  - [ ] Audience: teams attracted to autonomous task execution but frustrated by opaque trajectories, repeated work after context loss, and difficulty knowing what actually ran.
  - [ ] Repeated request/complaint: durable, inspectable execution evidence; no hidden re-execution; a clear boundary between model output and submitted changes.
  - [ ] Niki response: independent specialist stages, named attempt artifacts, and Git-state invariants recorded outside the conversational payload.
- [ ] **Aider**
  - [ ] Audience: terminal-first developers who value direct repository control but want stronger independent review and bounded multi-agent coordination.
  - [ ] Repeated request/complaint: optional supervisor/reviewer roles, less conversational drift, and a cleaner record of failed and successful attempts.
  - [ ] Niki response: an independent Reviewer stage with typed handoffs and persistent evidence beside the patch; solo mode remains available for simple solo work.
- [ ] **Claude Code**
  - [ ] Audience: developers who use a powerful coding agent but need to resume, audit, and reason about concurrent or long-running work without duplicate side effects.
  - [ ] Repeated request/complaint: reliable execution identity across resume/background work, fewer duplicate actions, and an auditable record of what was submitted.
  - [ ] Niki response: stage-scoped artifacts, explicit attempts, and an audit command that exposes task, safety, test, trace, report, and patch evidence before review.
- [ ] **SWE-agent**
  - [ ] Audience: benchmark and agent-infrastructure developers who need trustworthy submission boundaries, reproducible commands, and honest failure records.
  - [ ] Repeated request/complaint: model-controlled submission markers, spoofable shell state, unreadable or missing failed instances, and weak command restrictions.
  - [ ] Niki response: keep submission/review evidence outside the model payload, preserve failed attempts, restrict the sandbox where possible, and report Git-state invariants without calling them cryptographic guarantees.

### 2.2 Three positioning angles

Use exactly one primary angle per channel and rotate the other two in follow-up material.

- [ ] **Angle 1 — “A reviewable branch, not autonomous autopilot.”**
  - [ ] Promise: Niki stops at a human-readable Git diff after bounded specialist work.
  - [ ] Proof: Planner, Coder, Tester, and Reviewer artifacts; `niki audit`; no auto-merge or auto-push goal.
  - [ ] Borrowed audience: Cursor, Windsurf, Devin, and Claude Code users who want agency without losing the ability to say no.
- [ ] **Angle 2 — “A local-first, capability-bounded agent harness—not a cloud identity layer.”**
  - [ ] Promise: bring your own model/provider, keep source and execution local, and choose the sandbox boundary.
  - [ ] Proof: no telemetry by default, no account requirement, no cloud account, no vendor tracking, and no vendor lock-in.
- [ ] **Angle 3 — “Independent specialist agents with typed handoffs—not one endlessly drifting conversation.”**
  - [ ] Promise: separate planning, implementation, testing, and review with explicit evidence at each boundary.
  - [ ] Proof: typed artifacts under the task output directory; repeated attempts are named; the reviewer does not merely continue the coder’s narrative.
  - [ ] Borrowed audience: Aider, SWE-agent, and multi-agent users who need reproducibility and failure visibility.

### 2.3 Message hierarchy

- [ ] **One sentence:** Niki is an open-source Rust CLI that turns a plain-English request into a bounded, reviewable Git branch using sequential Planner, Coder, Tester, and Reviewer stages that share one selected backend.
- [ ] **One paragraph:** It is for developers who want AI assistance without an always-on cloud identity, hidden telemetry, or an opaque autonomous loop. You choose the model, inspect typed artifacts, and accept or reject the final diff.
- [ ] **Proof stack:** Rust binary → typed artifacts → sequential specialist stages → restricted container controls → `niki audit` → reviewable branch.
- [ ] **Avoid:** “fully autonomous,” “perfectly secure,” “hermetic,” or “mathematically guaranteed sandbox.”
- [ ] **Avoid:** “fully autonomous,” “fully secure,” “hermetic sandbox,” or “mathematically guaranteed safety.” Use “restricted,” “bounded,” “auditable,” and “reviewable” instead.

## 3. The credibility wave

### 3.1 Public Wave 1 roadmap

Create and maintain this section as the public trust roadmap. Wave 1 is deliberately small: each item should be independently testable and should make the next trust claim easier to verify.

#### Wave 1 — Trust and compounding reliability

- [ ] **Publish the trust contract.** Add a short, stable document that states what Niki collects (nothing by default), what it does not collect, how updates are initiated, what the sandbox can and cannot guarantee, and where users can inspect evidence.
- [ ] **Make the zero-telemetry claim testable.** Document the absence of analytics/crash-reporting dependencies and provide a reproducible network-observation or build-inspection check. Do not rely on a marketing sentence alone. If the project ever adds optional OTLP export, it must be explicitly opt-in through `--otel-endpoint` or `OTEL_EXPORTER_OTLP_ENDPOINT`, not enabled by default.
- [ ] **Make update behavior explicit.** Ensure every update path is opt-in, versioned, and visible; add a launch gate that proves there is no silent updater. If the current release configuration cannot prove that, disable the updater before launch.
- [ ] **Expose the artifact contract.** Document the exact artifact names, schemas, and locations for Planner, Coder, Tester, Reviewer, safety, trace, report, and patch outputs. Keep repeated attempts distinguishable.
- [ ] **Make `niki audit` the first-class review entry point.** Verify that it reads task, safety, test, trace, report, and patch artifacts and fails clearly when required evidence is missing.
- [ ] **Clarify `safety_proof.json`.** State that it records Git-state invariants and execution evidence; it is not a cryptographic proof or a substitute for sandbox review.
- [ ] **Add a local text-only smoke path.** Verify a no-API-key, no-container workflow using a locally installed Ollama text model and a solo/`Auto` task. Label it the Ollama + worktree local path. Do not claim it runs the full Planner → Coder → Tester → Reviewer pipeline, and do not claim it is a security sandbox. Verify that the command completes without Docker/Podman, without a container runtime, and without an external API key. Use `llama3.1` only as an example model; verify compatibility with the tested Ollama version and provider implementation before publishing.
- [ ] **Add a restricted sandbox smoke path.** Verify the documented Podman/Docker path on a clean machine, including image build, repository bind mount, UID/GID behavior, capability dropping, resource limits, and disabled outbound networking where supported.
- [ ] **Harden failure visibility.** Preserve failed attempts, distinguish a failed test from a failed review, and make the user’s next action obvious without hiding the original evidence.
- [ ] **Publish a compatibility matrix.** Record supported Rust/OS/container/model combinations and mark unsupported combinations explicitly rather than silently degrading.
- [ ] **Add a trust-focused release checklist.** Every release must show artifact integrity, update behavior, sandbox warnings, and a reproducible install path.
- [ ] **Invite adversarial review.** Add a public issue template for sandbox escapes, misleading claims, privacy regressions, and audit failures; respond with evidence and a fix or a documented limitation.

Wave 1 is complete only when each checkbox has a link to an issue, implementation, test, or release note. Do not mark the roadmap complete because the wording exists.

### 3.2 Ten strict non-goals

These are product boundaries, not aspirational language. They directly address trust failures reported around developer tools such as Warp, Tabby, Cursor, and Devin.

1. **No built-in telemetry, analytics, or crash reporting.** Niki must not phone home, collect usage fingerprints, or make privacy-dependent operation conditional on an account.
2. **No cloud accounts or identity lock-in.** A developer must be able to run the CLI against a local repository and a chosen provider without creating or maintaining a vendor identity.
3. **No forced updates or silent autoupdaters.** Updates are opt-in, visible, versioned, and reversible; a release must never surprise a developer with a changed binary.
4. **No vendor/model tracking.** Niki must not rank, steer, or record a developer’s provider choices for a third party, and it must not make one vendor’s model a hidden prerequisite.
5. **No always-on background daemons.** The CLI should do work when invoked and stop when the task stops; it is not a resident agent, terminal replacement, or background surveillance process.
6. **No proprietary hosted-only operation.** The core workflow must remain runnable from source or a local binary with user-controlled infrastructure; hosted convenience may exist only as an optional addition.
7. **No unreviewed auto-merge or auto-push.** Niki may create a branch and patch, but it must not silently push, merge, or present an unreviewed change as accepted work.
8. **No silent filesystem/network access.** Permissions, mounts, environment exposure, and network policy must be explicit and inspectable; the sandbox must not imply capabilities it does not enforce.
9. **No cross-session secret persistence.** API keys and credentials belong to the user’s chosen secret store or environment; Niki must not copy them into artifacts, traces, caches, or reusable agent history.
10. **No editor/terminal lock-in or opaque GUI behavior.** Niki remains a CLI with reviewable output; it must not require a particular editor, terminal, or visual shell to understand or reject its work.

## 4. Power-user distribution

### 4.1 Treat the release like a production binary

- [ ] Build and sign/tag release artifacts through the project’s supported release process; do not hand-roll binaries outside `cargo dist`.
- [ ] Publish checksums and a changelog with the exact commit and Rust toolchain used.
- [ ] Keep the CLI’s `--version` and help output deterministic and test them in CI.
- [ ] Provide a minimal `niki doctor` check that reports model/provider reachability, sandbox availability, and artifact-directory health without collecting usage data.
- [ ] Keep package manifests in dedicated packaging repositories or the platform’s expected location; do not make users build from source unless they explicitly choose `cargo install`.
- [ ] Test each install path on a clean machine or disposable VM before announcing it.
- [ ] Publish a “last verified” date beside each command so stale package instructions are obvious.

### 4.2 Package-manager commands

The commands below are the intended publication targets. A command is launch-ready only after its package is publicly resolvable and the install has been tested.

- [ ] **Homebrew tap**
  ```sh
  brew install RavaniRoshan/niki/niki
  ```
  ```
- [ ] **Scoop bucket**
  ```powershell
  scoop bucket add niki https://github.com/RavaniRoshan/scoop-niki.git
  scoop install niki
  ```
- [ ] **Nix**
  - [ ] Publish and verify a Nix package or overlay before listing an install command.
  - [ ] Do not invent a Nix command while publication is unverified.
  - [ ] Once verified, add the exact `nix profile install`, flake, or overlay command here with its revision and verification date.
- [ ] **WinGet manifest**
  ```powershell
  winget install --id RavaniRoshan.niki -e
  ```
- [ ] **Cargo fallback**
  ```sh
  cargo install --locked niki
  ```
  - [ ] Label Cargo as the source-build fallback, not the primary production path.
  - [ ] Verify the MSRV and release features documented by the project before publishing this command.

### 4.3 Zero-friction local path: Ollama, no API key, no container runtime

This is the **Ollama + worktree local path** for a text-only or solo workflow. It intentionally does not claim the full Planner → Coder → Tester → Reviewer pipeline, and it is not a security sandbox.

- [ ] Install Ollama from its official local installer and start the local service.
- [ ] Pull a locally runnable text model, for example:
  ```sh
  ollama pull llama3.1
  ```
- [ ] Configure the local provider:
  ```toml
  [providers.ollama]
  base_url = "http://localhost:11434"
  default_model = "llama3.1"
  ```
- [ ] Confirm that no API key is required for this provider, that no container runtime is used, and that no telemetry is sent by Niki.
- [ ] Run a small text-only task using the project’s supported solo/`Auto` mode:
  ```sh
  niki run "Summarize the repository structure" --backend worktree --project /path/to/repo
  ```
  - [ ] Replace the example with the exact supported CLI invocation after checking the current command help.
  - [ ] Verify that the command completes without Docker/Podman, without a container runtime, and without an external API key.
  - [ ] Verify that the output is clearly labeled as text-only/solo and does not imply the full four-stage pipeline ran.
  - [ ] Verify that the path does not run the full four-stage pipeline and does not imply security sandboxing.
- [ ] Record the tested model, Ollama version, OS, and command in the launch verification log.

### 4.4 Restricted sandbox path: Podman/Docker

This is the intended path for the full multi-agent workflow. Describe it as a **restricted sandbox**, not an absolutely hermetic or capability-secure one.

- [ ] Install Podman (or Docker where Podman is unavailable) and enable the rootless socket where supported:
  ```sh
  systemctl --user enable --now podman.socket
  ```
- [ ] Build the sandbox image from the repository:
  ```sh
  podman build -t niki-sandbox:24.04 -f docker/Dockerfile .
  ```
- [ ] Run the doctor check:
  ```sh
  niki doctor
  ```
- [ ] Run a bounded task through the supported Docker/Podman backend:
  ```sh
  niki run "Add a health endpoint" --backend docker --project /path/to/repo
  ```
  - [ ] Confirm the repository is bound to `/workspace`.
  - [ ] Confirm Unix containers run as the host UID/GID.
  - [ ] Confirm the documented controls include dropped capabilities, PID/memory/CPU limits, optional read-only root filesystem, and outbound networking disabled by default.
  - [ ] Check the current defaults and warnings for `readonly_rootfs`, mutable image tags, and headless permission fallback. If `fail_closed_headless` is not enabled, say so plainly and do not imply fail-closed behavior.
  - [ ] Treat the worktree backend as isolation for Git state, not as a security sandbox: it creates `.niki-worktrees/<uuid>`, runs as the host user, and replays the verified final diff.
- [ ] Validate the complete Podman path on a clean machine before publishing it as a launch instruction.
- [ ] Add a fallback note for Docker-only hosts and a clear failure message when neither backend is available.

## 5. Targeted infiltration

### 5.1 Conduct rule

- [ ] Disclose Niki affiliation in every post or reply.
- [ ] Answer the complaint first with a useful technical observation, workaround, or question.
- [ ] Link to Niki only when it directly answers the thread.
- [ ] Never astroturf, use sockpuppets, copy-paste identical replies, or imply that a complaint was independently discovered if it was not.
- [ ] Do not turn a bug report into a sales pitch; if maintainers ask not to promote, stop and respect the thread.

### 5.2 Five GitHub targets

Post only after checking that each issue/discussion is still open, active, and permits promotional or relevant. The wording below is a template, not a claim that the thread is current.

1. [ ] **Claude Code — execution identity and duplicate work**
   - [ ] Target: [`anthropics/claude-code#91353`](https://github.com/anthropics/claude-code/issues/91353)
   - [ ] Complaint to address: duplicate or divergent execution around a background agent and `SendMessage` resume, contradictory side effects, and uncertainty about whether the original execution remained live.
   - [ ] Reply value: “This is exactly the class of failure Niki tries to make visible: planning, coding, testing, and review are separate stages with typed artifacts, and each attempt is retained. `niki audit` exposes task, safety, test, trace, report, and patch evidence before a human reviews the branch, so duplicate work and side effects are easier to attribute rather than hidden inside one resume stream.”
   - [ ] CTA: ask whether an artifact-backed execution identity would help their workflow; do not claim Niki fixes Claude Code’s implementation.

2. **Cursor — bounded work and credit exhaustion**
   - [ ] Target: [`cursor/cookbook#51`](https://github.com/cursor/cookbook/issues/51)
   - [ ] Complaint to address: a local agent looped roughly 30 times and exhausted API credits.
   - [ ] Reply value: “Niki takes a different boundary: specialist stages are bounded and each attempt is written to a visible artifact instead of disappearing into an endless conversational loop. A reviewer can see where work stopped, what changed, and why before spending another request.”
   - [ ] CTA: invite discussion about explicit attempt limits and cost visibility; disclose that you are building Niki.

3. **Windsurf — rules, context, and hallucinated edits**
   - [ ] Target: [`Exafunction/windsurf.vim#494`](https://github.com/Exafunction/windsurf.vim/issues/494)
   - [ ] Complaint to address: rules/context failures, hallucinations, and negative productivity.
   - [ ] Reply value: “Niki makes the requirements and review criteria explicit in typed Planner and Reviewer artifacts, then produces a reviewable branch rather than silently continuing inside an editor session. That does not make a model correct, but it makes a wrong step inspectable and reversible.”
   - [ ] CTA: ask which rule/context failure should be represented as a hard gate in an agent harness.

4. **Aider — independent review**
   - [ ] Target: [`Aider-AI/aider#669`](https://github.com/Aider-AI/aider/issues/669)
   - [ ] Complaint/request to address: optional supervisor/reviewer and independent review.
   - [ ] Reply value: “Niki’s default pipeline already separates an independent Reviewer stage from the Coder, with typed handoffs and review evidence stored beside the patch. Failed and repeated attempts remain visible instead of being collapsed into one conversation.”
   - [ ] CTA: offer a concise comparison of the artifact format and ask what review signal terminal-first users would trust.

5. **SWE-agent — submission integrity and failure visibility**
   - [ ] Target: [`SWE-agent/SWE-agent#1535`](https://github.com/SWE-agent/SWE-agent/issues/1535)
   - [ ] Complaint to address: model-controlled submission markers and patch content, readable gold-test patches, spoofable `PS1`, missing failed instances, and weak command blocklisting.
   - [ ] Reply value: “Niki keeps submission/review evidence outside the model’s conversational payload, preserves failed-attempt artifacts, and reports Git-state invariants through `safety_proof.json`. This is not cryptographic sandboxing, but it gives reviewers a separate evidence trail and makes missing or failed instances visible.”
   - [ ] CTA: propose comparing artifact boundaries and fail-closed behavior; do not imply that Niki has solved every SWE-bench integrity problem.

### 5.3 Reddit targets

Rules, self-promotion limits, and current activity change. Verify each subreddit’s current rules and activity before posting; do not claim a community is active until checked.

- [ ] **r/rust**
  - [ ] Post only if the rules allow a project showcase or technical discussion.
  - [ ] Angle: a Rust CLI that makes multi-agent execution auditable through typed artifacts and reviewable Git output.
  - [ ] Lead with architecture and limitations; ask Rust developers to challenge the sandbox and release design.
- [ ] **r/LocalLLaMA**
  - [ ] Post only if local-model tooling is within current rules.
  - [ ] Angle: a no-API-key local Ollama text path, clearly separated from the containerized four-agent workflow.
  - [ ] Include tested model/version details and avoid implying that a local model removes all execution risk.
- [ ] **r/programming**
  - [ ] Post only if the community accepts launch posts and the submission is framed as an engineering case study.
  - [ ] Angle: why a coding agent should stop at a reviewable branch, with typed handoffs and explicit trust boundaries.
  - [ ] Do not use generic AI hype or claim that Niki replaces human review.

### 5.4 Hacker News submission

- [ ] Submit one technical story titled along the lines of: **“Niki: a Rust CLI for bounded, reviewable multi-agent coding with restricted Docker/Podman sandboxes.”**
- [ ] Lead the submission text with:
  - [ ] the Rust binary and CLI design;
  - [ ] sequential Planner/Coder/Tester/Reviewer stages;
  - [ ] typed artifact boundaries and `niki audit`;
  - [ ] restricted container controls and their explicit limitations;
  - [ ] the local Ollama text-only path and the full sandbox path;
  - [ ] no telemetry by default and opt-in updates.
- [ ] Do not lead with “autonomous coding,” “hermetic security,” or a star-count goal.
- [ ] Be present in the comments to answer architecture, safety, packaging, and limitation questions with evidence.

## 6. Launch gates

Do not publicly announce the launch until every blocking gate has evidence.

### Must-pass gates

- [ ] **Updater gate:** prove the updater is opt-in and never silent, or disable it before launch. `dist-workspace.toml` currently has `install-updater = true`; this conflicts with the strict no-forced/no-silent-updates non-goal until resolved and verified.
- [ ] **Telemetry gate:** verify from dependencies, source, and a network-observation test that Niki sends no telemetry, analytics, or crash reports by default.
- [ ] **Artifact gate:** run a task and verify typed Planner, Coder, Tester, Reviewer, safety, trace, report, and patch artifacts are present and readable.
- [ ] **Audit gate:** run `niki audit` against a successful task and a deliberately failed/incomplete task; verify clear evidence and failure behavior.
- [ ] **Sandbox gate:** validate the documented Podman/Docker path on a clean machine, including repository mount, UID/GID, capability/resource controls, network policy, and warnings for mutable images/read-only defaults/headless fallback.
- [ ] **Local path gate:** run the Ollama text-only/solo smoke test with no API key and no container runtime; verify the output does not imply a full four-agent run.
- [ ] **Package gate:** verify Homebrew, Scoop, and WinGet catalog publication and install commands. Publish and verify a Nix package before listing a Nix install command.
- [ ] **Claims gate:** review all launch copy for “autonomous,” “hermetic,” “secure,” “private,” and “guaranteed” overclaims; replace them with bounded, auditable, restricted, and reviewable language.
- [ ] **Community gate:** verify current rules/activity for each GitHub thread, subreddit, and Hacker News submission; prepare disclosure language and thread-specific replies.

### Recommended preflight checklist

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets` with no warnings
- [ ] `cargo test --verbose`
- [ ] `cargo build --release`
- [ ] `./target/release/niki --version --help`
- [ ] `cargo deny check`
- [ ] `cargo audit`
- [ ] package install smoke tests on clean hosts
- [ ] README/launch copy link check
- [ ] release notes and trust-contract link check

## 7. Measurements

These are **Estimated launch targets anchored to pre-launch baselines**, not industry benchmarks. Record the baseline before launch and replace estimates if the team has better evidence.

### D0 — launch day

- [ ] **Installation:** at least one successful install through every published package channel, plus one source-build fallback. Target: **1 verified install per channel** (Estimated; baseline is zero until measured).
- [ ] **Local path:** one successful Ollama text-only/solo smoke test with no API key/container runtime. Target: **1 passing run** (Estimated).
- [ ] **Sandbox path:** one successful restricted Docker/Podman smoke test on a clean machine. Target: **1 passing run** (Estimated).
- [ ] **Trust evidence:** one public artifact/audit walkthrough and one published limitation list. Target: **1 complete walkthrough** (Estimated).
- [ ] **Community response:** reply usefully to every on-topic launch thread without deleting critical questions. Target: **100% response rate to direct technical questions** (Estimated).
- [ ] **Repository signal:** record stars, forks, issues, and package downloads at a fixed timestamp. These are public counters, not telemetry.

### W1 — first week

- [ ] **Quality:** triage every launch-related issue into bug, limitation, documentation, or unsupported environment. Target: **100% triaged** (Estimated).
- [ ] **Reproducibility:** reproduce or request a minimal reproduction for every crash or sandbox failure. Target: **100% crash reports have a next action** (Estimated).
- [ ] **Trust:** publish fixes or explicit non-fixes for the top three recurring complaints. Target: **3 evidence-backed responses** (Estimated).
- [ ] **Distribution:** verify that package install instructions still resolve and record the date of the last check. Target: **all published channels checked once** (Estimated).
- [ ] **Adoption:** count voluntary GitHub issues, discussions, and package downloads; do not infer usage from stars alone. Target: establish a baseline rather than a fixed growth number (User-provided/Estimated).

### M1 — first month

- [ ] **Reliability:** compare failed runs by backend, model/provider, OS, and task type using only user-volunteered reports and reproducible tests. Target: publish the top three failure modes and their status (Estimated).
- [ ] **Trust:** close or explicitly disposition every launch-week trust/security issue. Target: **100% dispositioned** (Estimated).
- [ ] **Roadmap:** complete or schedule the next Wave 1 items that received repeated user requests. Target: **at least 3 compounding improvements** (Estimated).
- [ ] **Community:** identify which channel produced qualified technical feedback rather than raw traffic. Target: name the top three feedback sources (Estimated).
- [ ] **Decision:** continue, narrow, or pause each distribution channel based on signal quality and maintainer capacity; document the decision publicly.

## 8. Rollback and stop criteria

- [ ] **Security/privacy regression:** stop promotion immediately if Niki sends unexpected network traffic, persists secrets, or exposes repository data outside the declared paths. Disable the affected path, publish the evidence, and resume only after a reproducible fix.
- [ ] **Updater regression:** stop launch if any update installs or restarts without explicit user action. Disable the updater and rebuild before resuming.
- [ ] **Sandbox overclaim or escape:** stop using “hermetic,” “secure,” or equivalent language; restrict the claim to tested controls, open an issue, and publish a limitation note. If an escape is reproducible, pause the affected backend.
- [ ] **Package failure:** remove or mark broken any package command that cannot be installed on a clean host. Do not leave a stale command in the launch post.
- [ ] **Community harm:** stop posting in a thread if maintainers or moderators object, if disclosure is missing, or if the reply is perceived as astroturfing. Apologize once, correct the record, and do not repost through another account.
- [ ] **Quality regression:** pause new positioning experiments if crash rate, failed-task reports, or audit failures rise above the pre-launch baseline. Fix the underlying workflow before buying more distribution.
- [ ] **Trust contradiction:** if a product boundary in the ten non-goals is violated, treat it as a launch blocker until the behavior is removed or the boundary is publicly revised with user input.

## 9. Launch-day runbook

- [ ] **T-7 days:** freeze claims, finish trust contract, verify updater/telemetry gates, and prepare thread-specific replies.
- [ ] **T-3 days:** test release artifacts, package installs, Ollama text-only path, and restricted Podman/Docker path on clean machines.
- [ ] **T-1 day:** record public baselines for stars, forks, issues, package availability, and documentation links; confirm moderators/rules.
- [ ] **T0:** publish the release, roadmap, trust contract, and one primary positioning angle; post only to channels whose rules permit it.
- [ ] **T0 + 2 hours:** answer technical questions, collect reproducible failures, and correct any ambiguous security/update wording.
- [ ] **T0 + 24 hours:** publish a short evidence update: what installed, what failed, what was fixed, and what remains limited.
- [ ] **T+7 days:** complete the W1 review and decide whether to broaden distribution or narrow to the highest-signal communities.
- [ ] **T+30 days:** complete the M1 review, update Wave 1, and publish the next trust-focused milestone.

## 10. Definition of done

- [ ] A developer can install Niki through a published production package path.
- [ ] A developer can run a local text-only/solo task with Ollama and no API key or container runtime.
- [ ] A developer can run the restricted Docker/Podman workflow and understand its actual controls and limitations.
- [ ] A reviewer can inspect typed artifacts and the final diff without trusting an opaque agent narrative.
- [ ] The public roadmap contains small, testable trust improvements rather than vague capability promises.
- [ ] The ten non-goals are enforced by behavior, tests, or explicit launch gates.
- [ ] Launch distribution is transparent, thread-specific, and free of astroturfing.
- [ ] The launch can be paused without losing evidence, user trust, or a reproducible path back to a known-good release.
