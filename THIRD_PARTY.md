# Third-Party Dependencies and Attributions

NIKI adheres to strict provenance and licensing rules.

## 1. Compliance Statement

- **Zero Proprietary Code**: No leaked, extracted, decompiled, or source-map-reconstructed proprietary code (including copies of Claude Code or Google Antigravity CLI) was ever accessed, searched for, read, copied, or reasoned from.
- **Clean-Room Engineering**: Closed-source tools were evaluated strictly through published behavioral documentation and black-box interaction.
- **Reference Policy**: Open-source references (Codex, Kimi Code, Goose, pi, Charm Crush) were studied at the architecture and behavioral level. All Go code in NIKI was authored directly.

---

## 2. Direct Go Module Dependencies

| Module | Version | License | Usage / Purpose |
| :--- | :--- | :--- | :--- |
| `github.com/charmbracelet/bubbletea/v2` | `v2.0.0-alpha.2` | MIT | Terminal UI runtime and event loop. |
| `github.com/charmbracelet/lipgloss` | `v1.0.0` | MIT | Terminal styling and cell formatting. |
| `github.com/charmbracelet/x/term` | `v0.2.1` | MIT | Terminal state, raw mode, and size detection. |
| `github.com/BurntSushi/toml` | `v1.4.0` | MIT | Fast TOML configuration parser. |
| `github.com/creack/pty` | `v1.1.24` | MIT | Pseudo-terminal allocation for PTY smoke testing. |
| `golang.org/x/sys` | `v0.30.0` | BSD-3-Clause | Low-level Linux system call wrappers (termios, signals). |

---

## 3. External System Dependencies

| Program | License | Usage |
| :--- | :--- | :--- |
| `bwrap` (Bubblewrap) | LGPL-2.0+ | Linux unprivileged sandboxing re-exec helper (`niki sandbox-run`). Invoked as an external binary. |

---

## 4. Attributions & Architecture References

- **Codex (`openai/codex`)**: Architectural reference for the boot critical path separation, JSONL rollout session format, and execution sandbox boundaries. Reimplemented from behavior in Go.
- **Kimi Code (`MoonshotAI/kimi-code`)**: Architectural inspiration for event-driven mascot states and turn lifecycle.
- **Charm Crush (`charmbracelet/crush`)**: TUI design inspiration for inline split-screen layout and committing settled cells to terminal scrollback using `tea.Println`.
