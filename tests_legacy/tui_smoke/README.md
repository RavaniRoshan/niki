# TUI Smoke Suite (tmux black-box)

Drives the real `niki` binary through a dedicated, isolated tmux server and
asserts on the rendered terminal — a real-PTY complement to the tuiwright-based
`tests/headless_tui.py`. Offline / headless-first: no model call, network, or
credentials. Rationale and tool comparison: `research/headless-tui-testing.md`.

## Run locally

```bash
./tests/tui_smoke/run.sh --build          # build + run all cases
./tests/tui_smoke/run.sh                  # run against target/release/niki
./tests/tui_smoke/run.sh 03 05            # run only cases 03 and 05
NIKI_BIN=/path/to/niki ./tests/tui_smoke/run.sh
```

Cases (in `cases/*.sh`) are black-box: they only drive published keybindings
and assert on rendered text, so they stay stable as internals change.

| Case | Area |
|------|------|
| `01_startup`        | banner/tab, input hint, `MANUAL` mode badge |
| `02_command_palette`| `Ctrl+P` opens the command palette |
| `03_slash_status`   | `/status` → session-status view |
| `04_slash_permissions` | `/permissions` → permission-modes view |
| `05_slash_version`  | `/version` → version banner |
| `06_input_editing`  | typed echo + kill-ring word kill/yank round-trip |
| `07_quit`           | `Ctrl+C` ×2 exits and tears down the session |

## CI

`.github/workflows/tui-smoke.yml` installs tmux, builds, runs the suite with
`REQUIRE_TMUX=1`, and uploads pane captures on failure.

## Notes

- Isolated tmux socket per run (`niki-tui-smoke-<pid>-<rand>`) — never touches
  your own tmux sessions.
- Explicit pane size (`TUI_COLS`/`TUI_ROWS`, default 120×30) so layout assertions
  are deterministic.
- Polls for state markers (never fixed sleeps); on failure the pane is saved to
  `tui-smoke-logs/`.
