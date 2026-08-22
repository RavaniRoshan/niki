#!/usr/bin/env bash
# Fuzzy filtering of the slash command menu (nucleo subsequence matching).
#
# `/co` should match `/compact` and `/cost` (and others) even though it is not a
# contiguous substring of the command name.
set -euo pipefail
. "$(dirname "$0")/lib.sh"

run() {
  tui_begin
  # Open the slash menu and type a fuzzy prefix.
  tui_send "/" "co"
  tui_wait_for "/compact" 10
  tui_assert "/compact"
  tui_assert "/cost"
  # A non-matching prefix filters the menu to nothing.
  tui_send Backspace Backspace "zzzz"
  tui_send Escape
}