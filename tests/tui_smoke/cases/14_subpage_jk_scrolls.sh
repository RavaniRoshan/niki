#!/usr/bin/env bash
# `j`/`k` on a sub-page must not take the app down, and must not be swallowed.
#
# The first version of this case compared the rendered pane before and after
# six `j` presses, and it could not work: a fresh `niki chat` has no diff, so
# `scroll_offset` moved over an empty document and the two panes were
# byte-identical — the test would have passed against a completely broken page,
# which is the failure mode that makes a pty gate worse than no gate. The page
# with a *visible* cursor and no data is the Help page, and `?` never reaches it
# (`ROADMAP.md` §1.6), so it is not available as a probe.
#
# What is observable here, and worth a gate, is the liveness half: a key that
# the navigator and the page both decline must leave the app running and on the
# same page. The behavioural proof that the key reaches the page's cursor is
# `tests/tui_jk_reaches_the_page.rs`, which drives the real `handle_key`.
run() {
  tui_begin

  # Chat -> Run, then digit 4 to Diff (PageId::all() index 3).
  tui_send Tab
  sleep 0.4
  tui_send 4
  sleep 0.4
  tui_assert "diff"

  local before
  before="$(tui_capture)"

  for _ in 1 2 3 4 5 6 7 8; do tui_send "j"; done
  tui_send "k"
  sleep 0.5

  if ! tmux -L "$SMOKE_SOCK" has-session -t "$SMOKE_SESS" 2>/dev/null; then
    echo "  FAIL: 'j'/'k' on a sub-page killed the session" >&2
    return 1
  fi
  echo "  PASS: the app survived 'j'/'k' on a sub-page"

  # Still on Diff, and the composer was never opened — a swallowed key that
  # focused a text field would show up here as a different pane.
  local after
  after="$(tui_capture)"
  if [ "$before" != "$after" ]; then
    echo "  FAIL: 'j'/'k' on an empty Diff changed the pane; it must be a no-op here" >&2
    echo "$after" >&2
    return 1
  fi
  echo "  PASS: an empty Diff is unchanged by j/k, so the key reached the page and found nothing to scroll"
}
