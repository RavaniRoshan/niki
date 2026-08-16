# Security Policy

NIKI executes LLM-generated code and commands on your machine inside a
sandbox. Security is a first-class concern, not an afterthought.

## Supported versions

We support the latest released version of NIKI. Security fixes are released as
patch versions.

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, email **security@niki.dev** with:

- A description of the vulnerability and its impact.
- Steps to reproduce (proof-of-concept if possible).
- Affected version(s).

We aim to acknowledge reports within **72 hours** and to provide a remediation
timeline within **7 days**. You will be credited in the release notes unless you
request anonymity.

## Scope

In scope:

- Sandbox escape that allows code to affect the host outside the workspace.
- Command-injection or privilege-escalation in any backend (`docker`,
  `worktree`).
- Secret leakage (API keys) into logs, artifacts, or prompts.
- Prompt-injection paths that lead to arbitrary host command execution.
- Supply-chain issues in our release/build pipeline.

Out of scope:

- Issues requiring physical access or a malicious co-user on the same machine.
- Denial-of-service against third-party LLM providers.

## Hardening posture

NIKI applies defense-in-depth: container isolation, a command deny-list that is
enforced for **every** agent role, network egress restrictions, capability
dropping, and secret redaction.

## Threat model & trust commitments

NIKI is a **single-user, localhost, BYOK** tool. The dominant realistic risks are
prompt-injected destructive commands and exfiltration of local secrets by a
hijacked agent — not multi-tenant kernel attacks. Against that model we commit to:

- **No telemetry, ever.** NIKI has no analytics dependency and makes no outbound
  calls beyond your configured LLM provider (or local Ollama) and the optional,
  SSRF-guarded `[knowledge]` fetch. Verified in `Cargo.toml` (no analytics crates).
- **Repository contents are untrusted input.** Rule files (`AGENTS.md`,
  `.cursorrules`, etc.) and READMEs may contain prompt injection; the container +
  deny-list is the enforcement boundary, never the model's judgment.
- **Keys stay on the host.** Provider keys are read from env/config on the host and
  are redacted from logs, reports, and artifacts. NIKI never seeds host
  `~/.aws`, `~/.ssh`, or `.env` files into the agent context.
- **Egress is blocked by default.** The container backend sets `network_mode: "none"`
  unless you opt in: `network_disabled = false` (or `network_allowlist = ["*"]`) opens
  egress — needed when a task must fetch dependencies (e.g. `cargo fetch`, `npm install`).
  This matches Codex/Claude's network-off sandbox default and is a core trust guarantee.
- **Spend visibility, honesty first.** `general.spend_cap_usd` is a
  hard mid-run ceiling: a pre-run estimate is printed and NIKI aborts the run if
  the cumulative estimated cost over the cap before any further stage executes
  (pipeline.rs). For an additional hard ceiling at the provider, set one on your
  key.
- **Supply chain.** Releases ship from a `cargo-dist` pipeline with dependency
  audits enforced as a CI gate (cargo-audit `cargo deny check` on every build,
  see ci.yml); releases produce SHA-256 checksums for every artifact.
  Digest-pinned sandbox image pulls and Sigstore signing of the GHCR image are
  the immediate post-launch items (the bundled Docker image currently follows a
  tag pull, noted as a TODO in `docker/Dockerfile`).

## Trust boundaries by backend

| Backend | Isolation | Trust model |
|---------|-----------|-------------|
| `docker` / `podman` | Container sandbox with dropped capabilities, read-only mounts where possible, resource limits | LLM code is **untrusted**; host is protected by container boundaries |
| `worktree` | Git worktree on host filesystem | LLM code runs **directly on the host**; no sandbox boundary — use only in single-user, trusted environments |

### Worktree backend security considerations

The `worktree` backend creates a git worktree and executes LLM-generated commands
directly on your host machine. This is a deliberate design choice for speed and
simplicity, but it means:

- **No filesystem isolation.** The LLM agent can read/write any path the niki
  process can access. Never run `niki --backend worktree` in directories
  containing secrets, credentials, or data you would not want an LLM to see.
- **No network isolation.** Commands like `curl` or `npm install` execute with
  full host network access.
- **No capability dropping.** Unlike the Docker backend, there is no seccomp,
  AppArmor, or capability restriction.
- **Workspace config files are executable code.** Per the "Pillar: Week of
  Sandbox Escapes" finding (Jul 2026), files like `.cursorrules`, `AGENTS.md`,
  or custom config files in the workspace root can instruct LLM agents to execute
  arbitrary code. When using the worktree backend, treat every file in the
  workspace as potentially adversarial.

**Recommendation:** Use `docker` or `podman` for untrusted code or shared CI
environments. Use `worktree` only for personal, single-user development where
you trust the task description and the project contents.

## Responsible disclosure

We ask researchers to give us a reasonable window to fix issues before any
public disclosure.

### Machine-readable security policy

Security advisories are tracked via GitHub Security Advisories:

- **Report a vulnerability:** https://github.com/RavaniRoshan/niki/security/advisories/new

If you prefer GitHub's built-in advisory flow, you can also use
<https://github.com/RavaniRoshan/niki/security/advisories/new>.
