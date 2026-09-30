#!/usr/bin/env bash
# Smoke: a terminal too small to draw must say so, not go blank.
#
# Below 10 rows the surface used to `return` silently while the overlay
# ladder kept consuming every keystroke. At 80x9 and 80x8 — both verified
# before the fix — the user got a black void that ate their typing, with `Esc`
# as the only exit and nothing saying so. The whole test suite pins 80x24 or
# 120x30, so this band was untested.
run() {
  local saved_rows="$TUI_ROWS"
  TUI_ROWS=8
  tui_begin
  local cap
  cap="$(tui_capture)"
  if ! grep -qi "too small" <<<"$cap"; then
    echo "  FAIL: an 80x8 terminal produced a blank screen instead of an explanation" >&2
    tui_capture >&2
    tui_kill
    TUI_ROWS="$saved_rows"
    return 1
  fi
  tui_kill
  TUI_ROWS="$saved_rows"
  echo "  PASS: a too-small terminal says so"
}
