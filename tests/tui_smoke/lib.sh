#!/usr/bin/env bash
# Shared utilities for the tmux-based black-box TUI smoke suite.
#
# Drives the REAL `niki` binary through a dedicated, isolated tmux server
# (isolated socket so it never touches the developer's own tmux sessions),
# sends keystrokes, captures the rendered pane, and asserts on the output.
#
# Design follows the "TUI = PTY problem" pattern (see research/headless-tui-testing.md):
#   - poll for a state marker, never fixed sleeps
#   - explicit terminal size so layout assertions are deterministic
#   - one isolated session per case; cleaned up on exit (even on failure)
#   - offline / headless-first: no model call, network, or credentials
#
# Sourced by run.sh; case files define a `run` function.

set -euo pipefail

# ---- Configurable via env ------------------------------------------------
TUI_COLS="${TUI_COLS:-120}"
TUI_ROWS="${TUI_ROWS:-30}"
NIKI_BIN="${NIKI_BIN:-target/release/niki}"
REQUIRE_TMUX="${REQUIRE_TMUX:-0}"
TUI_LOG_DIR="${TUI_LOG_DIR:-tui-smoke-logs}"

# ---- Internal state ------------------------------------------------------
SMOKE_SOCK=""
SMOKE_SESS=""

tui_die() { echo "ERROR: $*" >&2; exit 1; }

# Refuse to run (or skip) if tmux is missing.
tui_require_tmux() {
  if ! command -v tmux >/dev/null 2>&1; then
    if [ "${REQUIRE_TMUX:-0}" = "1" ]; then
      tui_die "tmux is required (REQUIRE_TMUX=1) but is not installed"
    fi
    echo "SKIP: tmux not installed; set REQUIRE_TMUX=1 to fail" >&2
    exit 77
  fi
}

# Start the binary in a detached tmux session with a known size.
tui_new_session() {
  local proj="$1"
  SMOKE_SOCK="niki-tui-smoke-$$-$RANDOM"
  SMOKE_SESS="niki-smoke"
  tmux -L "$SMOKE_SOCK" new-session -d -s "$SMOKE_SESS" -x "$TUI_COLS" -y "$TUI_ROWS" \
    "env TERM=tmux-256color LANG=C.UTF-8 TZ=UTC '$NIKI_BIN' chat -p '$proj'"
  # Give the PTY a beat to attach, then dismiss the onboarding modal.
  sleep 1
  tmux -L "$SMOKE_SOCK" send-keys -t "$SMOKE_SESS" Escape
}

tui_send() { tmux -L "$SMOKE_SOCK" send-keys -t "$SMOKE_SESS" "$@"; }

tui_capture() { tmux -L "$SMOKE_SOCK" capture-pane -t "$SMOKE_SESS" -p 2>/dev/null || true; }

# Poll the rendered screen until `pattern` (ERE) appears, or time out.
tui_wait_for() {
  local pat="$1"
  local timeout="${2:-30}"
  local deadline=$((SECONDS + timeout))
  local cap
  while true; do
    cap="$(tui_capture)"
    if printf '%s' "$cap" | grep -qE "$pat"; then return 0; fi
    if [ "$SECONDS" -ge "$deadline" ]; then
      echo "  TIMEOUT waiting for: $pat" >&2
      printf '%s\n' "$cap" >&2
      return 1
    fi
    sleep 0.3
  done
}

# Assert the rendered screen currently contains `pattern` (ERE).
tui_assert() {
  local pat="$1" cap
  cap="$(tui_capture)"
  if printf '%s' "$cap" | grep -qE "$pat"; then
    echo "  PASS: /$pat/"
  else
    echo "  FAIL: /$pat/ not found" >&2
    printf '%s\n' "$cap" >&2
    return 1
  fi
}

tui_kill() {
  [ -n "${SMOKE_SOCK:-}" ] && tmux -L "$SMOKE_SOCK" kill-server 2>/dev/null || true
}

# Persist the current pane capture for post-mortem on CI.
tui_save_failure() {
  local case="$1"
  mkdir -p "$TUI_LOG_DIR"
  tui_capture > "$TUI_LOG_DIR/${case}.failure.txt" 2>/dev/null || true
}

# Per-case setup: isolated project dir, fresh session, dismiss onboarding,
# wait for the chat input to be ready. Installs an EXIT trap so the session
# and temp dir are always reclaimed.
tui_begin() {
  SMOKE_PROJ="$(mktemp -d "${TMPDIR:-/tmp}/niki-tui.XXXXXX")"
  trap 'tui_kill; rm -rf "$SMOKE_PROJ" 2>/dev/null || true' EXIT
  tui_require_tmux
  tui_new_session "$SMOKE_PROJ"
  tui_wait_for "Describe a change" 30
}
