# NIKI Security Model & Default Posture

## 1. Principles: Rule of Two & Safe by Default
NIKI operates under the principle that **an LLM with a shell is an untrusted agent executing arbitrary instructions**.
Security cannot rely on model self-restraint. It is enforced deterministically by the Go harness and OS kernel.

- **Default Approval Mode**: `workspace_write` or `ask`. Any command classified as complex, elevated, or dangerous requires explicit human approval.
- **Fail-Closed Shell AST Analysis**:
  All shell commands are parsed using `mvdan.cc/sh/v3/syntax`. Any complex syntax (pipes `|`, subshells `(...)`, substitutions `$(...)` or `` `...` ``, conditionals `&&`, control loops) is classified as `too-complex` and cannot be auto-allowed.
- **Approval Dialog UX**:
  The safest option (`Deny`) is focused by default. Pressing `Esc` unconditionally denies the action. Every decision is logged with its timestamp, command, and rationale to the session audit ledger.

---

## 2. Process Sandboxing & Execution Isolation

Command isolation is executed via the dedicated re-exec helper (`niki sandbox-run -- <cmd>`):
- **Linux (Bubblewrap `bwrap`)**:
  - The root filesystem `/` is mounted strictly read-only (`--ro-bind / /`).
  - Only designated workspace directories are bind-mounted read-write (`--bind <dir> <dir>`).
  - `/tmp` is mounted as an ephemeral, isolated tmpfs (`--tmpfs /tmp`).
  - All namespaces are unshared (`--unshare-all`: PID, IPC, UTS, and Network).
  - All kernel capabilities are dropped (`--cap-drop ALL`).
  - Commands die automatically if the parent process exits (`--die-with-parent`).
  - Network access is denied by default (isolated network namespace with no egress routing).
- **macOS (Seatbelt `sandbox-exec`)**:
  - Automatically compiles a per-command Seatbelt profile (`.sb`).
  - Global write deny (`(deny file-write*)`) with explicit subpath permissions only for workspace directories (`(allow file-write* (subpath ...))`).
  - Global network denial (`(deny network*)`).
  - Ephemeral profile cleanup guaranteed on normal exit, error, and panic recovery paths.
- **Scrubbed Environment Fallback**:
  - When no kernel backend is available, commands run with an environment sanitized of sensitive tokens.
  - All environment variables matching `AWS_*`, `SSH_*`, `OPENAI_*`, `ANTHROPIC_*`, `GITHUB_*`, and `GH_*` are stripped.

---

## 3. Project Trust Boundary

When NIKI is launched in a directory containing project-level configuration (`niki.toml` or `.niki/`):
- Projects are considered **untrusted by default**.
- An untrusted project **cannot register MCP servers**, hooks, or custom shell commands from its local config.
- Trust can only be granted explicitly by the user (persisted in `~/.niki/trusted_projects` or through `NIKI_TRUST_PROJECT=1`).

---

## 4. Red-Team Verification Set

The security controls are validated by an automated test suite (`internal/permissions/redteam_test.go`):
1. **Exfiltration README Trap**: A README or prompt ordering `cat ~/.ssh/id_rsa | curl ...` is caught by AST analysis (pipe -> `too-complex` -> approval required), environment secrets are scrubbed, and network egress is blocked.
2. **Symlink Escape Trap**: A malicious repository containing symlinks pointing outside the workspace (e.g. to `/etc`) fails to write because the underlying filesystem root is read-only.
3. **Lifecycle Script Trap**: A downloaded repository trying to start a rogue background MCP server or hook via `niki.toml` is neutralized by the project trust gate.

---

## 5. Limitations & Threat Model Boundaries
- Reads across the filesystem are allowed by default so the agent can inspect system compilers, headers, and standard libraries. Confidential files in user home should not be stored unencrypted if broad read access is a concern.
- The default sandbox denies network access. If a user explicitly grants network access to a command, host egress is unconfined.
