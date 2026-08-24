#!/usr/bin/env bash
# Smoke: two Ctrl+C presses exit the TUI and tear down the session cleanly.
run() {
  tui_begin
  # `niki chat` (run_chat) exits via the Quit confirm modal, not Ctrl+C:
  # Tab leaves the Chat page, `q` opens "Exit NIKI?", Enter confirms.
  tui_send Tab
  sleep 0.3
  tui_send "q"
  tui_wait_for "Exit NIKI" 10
  tui_send Enter
  local i
  for i in $(seq 1 30); do
    if ! tmux -L "$SMOKE_SOCK" has-session -t "$SMOKE_SESS" 2>/dev/null; then break; fi
    sleep 0.3
  done
  if tmux -L "$SMOKE_SOCK" has-session -t "$SMOKE_SESS" 2>/dev/null; then
    echo "  FAIL: session still alive after quit" >&2
    tui_capture >&2
    return 1
  fi
  echo "  PASS: session exited cleanly"
}
