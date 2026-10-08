# Third-Party Dependencies and Attributions

NikiCode adheres to strict provenance and licensing rules.

## 1. Compliance Statement

- **Zero Proprietary Code**: No leaked, extracted, decompiled, or source-map-reconstructed proprietary code (including copies of Claude Code or Google Antigravity CLI) was ever accessed, searched for, read, copied, or reasoned from.
- **Clean-Room Engineering**: Closed-source tools were evaluated strictly through published behavioral documentation and black-box interaction.
- **Reference Policy**: Open-source references (Codex, Kimi Code, Goose, pi, Charm Crush) were studied at the architecture and behavioral level. All Go code in NikiCode was authored directly.

---

## 2. Direct Go Module Dependencies (from go.mod, 2026-10-08)

| Module | Version | License | Usage / Purpose |
| :--- | :--- | :--- | :--- |
| `github.com/charmbracelet/bubbletea` | `v1.3.10` | MIT | Terminal UI runtime and event loop. |
| `github.com/charmbracelet/bubbles` | `v1.0.0` | MIT | TUI widgets (textarea, viewport). |
| `github.com/charmbracelet/lipgloss` | `v1.1.0` | MIT | Terminal styling and cell formatting. |
| `github.com/charmbracelet/x/term` | `v0.2.2` | MIT | Terminal state, raw mode, and size detection. |
| `github.com/creack/pty` | `v1.1.24` | MIT | Pseudo-terminal allocation for PTY tests and `bench`. |
| `github.com/google/uuid` | `v1.6.0` | BSD-3-Clause | Session and agent IDs. |
| `github.com/pelletier/go-toml/v2` | `v2.4.3` | MIT | TOML configuration parsing. |
| `github.com/spf13/cobra` | `v1.10.2` | Apache-2.0 | Subcommand dispatch (behind the B0 argv fast path). |
| `golang.org/x/sys` | `v0.48.0` | BSD-3-Clause | Low-level system call wrappers. |
| `modernc.org/sqlite` | `v1.60.1` | BSD-style wrapper license (see module LICENSE; SQLite itself is public domain) | Session store (pure Go, no cgo). |
| `mvdan.cc/sh/v3` | `v3.14.1` | BSD-3-Clause | Fail-closed shell AST allowlist. |

---

## 3. External System Dependencies

| Program | License | Usage |
| :--- | :--- | :--- |
| `bwrap` (Bubblewrap) | LGPL-2.0+ | Linux unprivileged sandboxing re-exec helper (`nikicode sandbox-run`). Invoked as an external binary. |

---

## 4. Attributions & Architecture References

- **Codex (`openai/codex`, Apache-2.0)**: Architectural reference for the boot critical path separation, JSONL rollout session format, and execution sandbox boundaries, studied under the foundation pack's reference policy. All Go code reimplemented from behavior. No Codex code is vendored.
- **Kimi Code (`MoonshotAI/kimi-code`, MIT)**: Architectural inspiration for event-driven mascot states and turn lifecycle.
- **Charm Crush (`charmbracelet/crush`, FSL-1.1-MIT)**: TUI design inspiration for inline split-screen layout and committing settled cells to terminal scrollback using `tea.Println`. Ideas only; nothing adapted.
- **Claude Code, Google Antigravity CLI (proprietary)**: behavior reference only (public docs, public performance write-ups, black-box runs). No source, logo, name, tagline, screenshot, or asset was ever read, copied, or imitated. NikiCode's name, wordmark, copy, and demo are original.

## 5. Clean-Room Statement (final pack)

During G0–G6 (2026-10-08): no leaked, extracted, decompiled, or
reconstructed proprietary code was read, searched for, copied, or
reasoned from. The only competitor artifacts on this machine are the
publicly installed reference CLIs (`codex`, run as black boxes for
benchmark timings) — never unpacked, never disassembled. The one
incidental contact (Codex's self-update notice during benchmarking) was
declined, not executed.
