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
dropping, and secret redaction. The `worktree` backend runs commands on the
host by design and is intended for trusted, single-user use — treat it as such.

## Responsible disclosure

We ask researchers to give us a reasonable window to fix issues before any
public disclosure.
