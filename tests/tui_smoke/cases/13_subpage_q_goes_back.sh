#!/usr/bin/env bash
# `q` on a sub-page goes back. It does not quit.
#
# Every sub-page answers `q` with "back to Run" — `pages/diff.rs:189`,
# `pages/history.rs:281`, and nine more. None of them could run: the nav layer
# sat above the page router, read `q` as `NavIntent::Quit`, and broke the event
# loop. So the key that goes *back* everywhere else in the app destroyed the
# interface on every page that had a handler for it, and the confirm-quit modal
# written below it was unreachable dead code.
#
# This drives a real pty because a source check cannot tell whether a key
# reaches a handler — that is the whole defect: the code that handles `q` was
# present and correct, and simply never ran.
run() {
  tui_begin

  # Chat -> Run, then digit 4 jumps to Diff (PageId::all() index 3).
  tui_send Tab
  sleep 0.4
  tui_send 4
  sleep 0.4
  tui_assert "Diff"

  tui_send "q"
  sleep 0.6

  # Still here, and back on Run. A quit would have taken the session with it, so
  # the liveness check comes first: it is the stronger claim.
  if ! tmux -L "$SMOKE_SOCK" has-session -t "$SMOKE_SESS" 2>/dev/null; then
    echo "  FAIL: 'q' on the Diff page quit the app — the session is gone" >&2
    tui_capture >&2
    return 1
  fi
  echo "  PASS: 'q' on a sub-page did not quit the app"

  local cap
  cap="$(tui_capture)"
  if grep -q "Exit NIKI" <<<"$cap"; then
    echo "  FAIL: 'q' on the Diff page opened the quit modal instead of going back" >&2
    echo "$cap" >&2
    return 1
  fi
  echo "  PASS: 'q' on a sub-page did not ask to quit"

  tui_assert "Run"
  echo "  PASS: 'q' on a sub-page returned to Run"
}
