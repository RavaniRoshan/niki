#!/usr/bin/env bash
# `d` on the Run page navigates to Diff — in the shipped binary.
#
# `tests/tui_navigation.rs::run_page_ignores_navigation_hotkeys` asserts that
# `p`, `a`, `d`, `v`, `c`, `f`, `h`, `?`, `,` and `l` leave the page on Run.
# It passes — and it is not what the product does.
#
# It drives `PageRouter::handle_key`, which is a *page's own* key handler.
# Global page jumps live in `global_page_jump`, and the event loop applies it
# after the router declines:
#
#     } else if router.handle_key(key, &mut state) { … }
#     else if let Some(page) = global_page_jump(key) { state.current_page = page }
#
# So on the Run page, `d` *does* open Diff. The test's name and its comment —
# "All these keys should be ignored on Run page now" — describe the product,
# and the product does the opposite. A green test asserting a product
# behaviour that does not exist is the failure this whole pass has been
# removing, and only a real terminal can settle which of the two is right.
run() {
  tui_begin

  # The chat view is the front door and its composer has focus, so a typed
  # letter is *text* — pressing `d` there puts "Build d" in the composer, which
  # is correct and which the first version of this case got wrong by expecting
  # navigation from the chat view. `Tab` leaves the chat view for the page
  # (`tab pages` in the status bar), and only then are page letters live.
  tui_send Tab
  sleep 0.5
  tui_send "d"
  sleep 0.6

  local cap
  cap="$(tui_capture)"
  if grep -q "files" <<<"$cap" && grep -qi "CHANGES" <<<"$cap"; then
    echo "  PASS: 'd' opened the Diff page, as the shipped binary does"
  elif grep -qi "no diff" <<<"$cap"; then
    # Also the Diff page: it says so when there is nothing to show.
    echo "  PASS: 'd' opened the Diff page (empty diff)"
  else
    echo "  FAIL: 'd' did not navigate to the Diff page" >&2
    echo "$cap" >&2
    return 1
  fi

  # And back, so the case proves navigation rather than a one-way jump.
  #
  # `q` on a sub-page goes to **Run**, not to the chat view — the page's own
  # `q` handler, restored in batch 2. The first version of this case asserted
  # it returned to the composer and failed, against correct behaviour for the
  # second time in one case.
  tui_send "q"
  sleep 0.5
  cap="$(tui_capture)"
  if grep -q "cancel run" <<<"$cap" || grep -q "esc cancel" <<<"$cap"; then
    echo "  PASS: 'q' went back to the Run page"
  else
    echo "  FAIL: 'q' did not leave the Diff page" >&2
    echo "$cap" >&2
    return 1
  fi
}
