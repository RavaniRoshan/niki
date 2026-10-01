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
# Launch the chat.
#
# `HOME` points at the throwaway project unless `TUI_KEEP_HOME` is set. A real
# first run has no `~/.config/niki/niki.toml`, and inheriting the developer's
# means a "first run" case silently exercises their configured provider
# instead — which is how `10_first_run_no_key` failed on a machine that has
# one, reaching a real endpoint and reporting a 404.
tui_new_session() {
  local proj="$1"
  SMOKE_SOCK="niki-tui-smoke-$$-$RANDOM"
  SMOKE_SESS="niki-smoke"
  local home_env=()
  [ "${TUI_KEEP_HOME:-0}" = "1" ] || home_env=( "HOME=$proj" "XDG_CONFIG_HOME=$proj/.config" )
  tmux -L "$SMOKE_SOCK" new-session -d -s "$SMOKE_SESS" -x "$TUI_COLS" -y "$TUI_ROWS" \
    "env TERM=tmux-256color LANG=C.UTF-8 TZ=UTC ${home_env[*]:-} '$NIKI_BIN' chat -p '$proj'"
  # Give the PTY a beat to attach, then dismiss the onboarding modal.
  sleep 1
  tmux -L "$SMOKE_SOCK" send-keys -t "$SMOKE_SESS" Escape
}

# Key *names* (`Enter`, `Escape`, `C-c`, `Down`). tmux parses a bare argument
# as a name, which is what every case wants.
tui_send() { tmux -L "$SMOKE_SOCK" send-keys -t "$SMOKE_SESS" "$@"; }

# Literal *characters*, for typing into a field.
#
# These are two different operations and conflating them costs a run of case
# 16 in each direction. A bare digit sent as a name did not arrive at all, so
# the answer never reached the modal and it looked like a product bug. Making
# `-l` the default for every send then broke `Enter` in nine other cases,
# because `-l` sends the seven characters `Enter`.
tui_type() { tmux -L "$SMOKE_SOCK" send-keys -l -t "$SMOKE_SESS" "$@"; }

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

# ── A session that actually runs the pipeline ──────────────────────────────
#
# Every other case here drives `niki chat`, which by the product's own §0a
# decision sends **no tools**: a plain conversation turn cannot start a run, and
# that is correct. So the surface where a tool can run — and therefore where a
# tool can *ask the user something* — was unreachable from this suite
# entirely.
#
# `tui_begin_run` starts the scripted mock LLM, gives the session a real git
# repository and a provider pointed at it, and leaves the user at the chat
# prompt with `/run` available. The caller types `/run <task>`.
#
# Everything it needs is local: the mock is a Python file in the repository, and
# the case is skipped rather than failed if python3 is missing, because a
# machine without Python is an environment fact and not a broken product.
tui_begin_run() {
  local script_json="$1"
  tui_require_tmux
  command -v python3 >/dev/null 2>&1 || { tui_skip "python3 is needed to run the scripted model"; return 1; }

  SMOKE_PROJ="$(mktemp -d "${TMPDIR:-/tmp}/niki-tui-run.XXXXXX")"
  # `TUI_KEEP_HOME=1` keeps the project — and the mock's stderr, which is the
  # only explanation a failing case will have. Honoured here as well as in
  # `tui_new_session`, because a run's directory is where the evidence is.
  if [ "${TUI_KEEP_HOME:-0}" = "1" ]; then
    trap 'tui_kill; tui_stop_mock' EXIT
    echo "  (project kept at $SMOKE_PROJ)" >&2
  else
    trap 'tui_kill; tui_stop_mock; rm -rf "$SMOKE_PROJ" 2>/dev/null || true' EXIT
  fi

  # A real repository. The pipeline creates a branch, and `git worktree add`
  # fails on anything that is not a repository — so a temp directory is not
  # enough for a run, however complete it looks.
  git -C "$SMOKE_PROJ" init -q
  git -C "$SMOKE_PROJ" config user.email "smoke@niki.dev"
  git -C "$SMOKE_PROJ" config user.name "TUI smoke"
  printf 'fn main() {\n    println!("hello");\n}\n' > "$SMOKE_PROJ/src.rs"
  git -C "$SMOKE_PROJ" add -A
  git -C "$SMOKE_PROJ" commit -qm "initial"

  # A port from the OS rather than a constant: cases can run in parallel, and a
  # fixed port is how a passing suite turns red on a busy machine.
  MOCK_PORT="$(python3 -c 'import socket;s=socket.socket();s.bind(("127.0.0.1",0));print(s.getsockname()[1]);s.close()')"
  printf '%s' "$script_json" > "$SMOKE_PROJ/script.json"
  # stderr goes to a file, not /dev/null. The first version discarded both
  # streams, so when the server died mid-run the case said "connection closed"
  # and nothing else — a harness that cannot explain its own failure teaches
  # the reader to guess.
  MOCK_ERR="$SMOKE_PROJ/mock.stderr"
  MOCK_LLM_SCRIPT="$SMOKE_PROJ/script.json" MOCK_LLM_PORT="$MOCK_PORT" \
    MOCK_LLM_TRACE="${MOCK_LLM_TRACE:-0}" \
    python3 "$HERE/../integration/mock_llm.py" >/dev/null 2>"$MOCK_ERR" &
  MOCK_PID=$!
  local deadline=$((SECONDS + 20))
  while [ $SECONDS -lt $deadline ]; do
    curl -sf "http://127.0.0.1:$MOCK_PORT/health" >/dev/null 2>&1 && break
    sleep 0.2
  done

  cp "$HERE/../integration/niki.test.toml" "$SMOKE_PROJ/niki.toml"
  # The test config points at 8080; this case took a free port instead, so the
  # two are rewritten together rather than one being assumed right.
  sed -i.bak "s|http://localhost:8080|http://127.0.0.1:$MOCK_PORT|g" "$SMOKE_PROJ/niki.toml"
  rm -f "$SMOKE_PROJ/niki.toml.bak"
  # The worktree backend, not docker: a container is not available on a CI
  # runner and the point here is the interface, not the isolation.
  #
  # The key is `[docker] backend`, not `[sandbox]` — the struct is
  # `DockerConfig` and the section is named for the *default* backend whatever
  # it is set to. The first version of this wrote `[sandbox]`, TOML ignored it,
  # and the run silently fell through to podman and died on `cpu.max`; a case
  # that fails for the wrong reason teaches nothing, so the fix is here rather
  # than in a comment.
  # Inserted into the **existing** `[docker]` table rather than appended as a
  # second one: TOML rejects a duplicate table, so appending made `niki.toml`
  # unparseable, the binary exited, and the case failed waiting for a prompt
  # that was never coming. A case that fails for the wrong reason teaches
  # nothing, and this one taught it twice before being fixed.
  sed -i.bak '/^\[docker\]/a backend = "worktree"' "$SMOKE_PROJ/niki.toml"
  rm -f "$SMOKE_PROJ/niki.toml.bak"

  SMOKE_SOCK="niki-tui-run-$$-$RANDOM"
  SMOKE_SESS="niki-smoke"
  local home_env=( "HOME=$SMOKE_PROJ" "XDG_CONFIG_HOME=$SMOKE_PROJ/.config" )
  tmux -L "$SMOKE_SOCK" new-session -d -s "$SMOKE_SESS" -x "$TUI_COLS" -y "$TUI_ROWS" \
    "env TERM=tmux-256color LANG=C.UTF-8 TZ=UTC ANTHROPIC_API_KEY=mock-key OPENAI_API_KEY=mock-key \
     ${home_env[*]} '$NIKI_BIN' chat -p '$SMOKE_PROJ'"
  sleep 1
  tmux -L "$SMOKE_SOCK" send-keys -t "$SMOKE_SESS" Escape
  tui_wait_for "Describe a change" 30
}

tui_stop_mock() {
  if [ -n "${MOCK_PID:-}" ]; then
    # A server that already exited is not worth reporting as an error here, but
    # its stderr is: it is the only explanation a failed case will have.
    if ! kill -0 "$MOCK_PID" 2>/dev/null && [ -s "${MOCK_ERR:-}" ]; then
      echo "  mock server exited on its own; it said:" >&2
      sed 's/^/    /' "$MOCK_ERR" >&2
    fi
    kill "$MOCK_PID" 2>/dev/null
  fi
  MOCK_PID=""
  return 0
}

# Report a skip rather than a failure, and say why. `tests/skips_and_budgets_
# stay_honest.rs` requires every skip to name its reason and where the
# behaviour is covered, because a skip with no redirect is a permanent hole.
tui_skip() { echo "  SKIP: $*"; return 0; }
