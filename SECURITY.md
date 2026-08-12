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
  `worktree`, `cloud`).
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

## Trust boundaries by backend

| Backend | Isolation | Trust model |
|---------|-----------|-------------|
| `docker` / `podman` | Container sandbox with dropped capabilities, read-only mounts where possible, resource limits | LLM code is **untrusted**; host is protected by container boundaries |
| `worktree` | Git worktree on host filesystem | LLM code runs **directly on the host**; no sandbox boundary — use only in single-user, trusted environments |
| `cloud` | Remote container (Upstash Box or similar) | LLM code is **untrusted**; host is not involved |

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
