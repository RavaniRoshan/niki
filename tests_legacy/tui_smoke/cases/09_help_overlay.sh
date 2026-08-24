#!/usr/bin/env bash
# `?` opens the which-key style keybinding overlay; Esc dismisses it.
#
# NOTE: `+` is a regex metacharacter, so every `Ctrl+X` pattern is escaped as
# `Ctrl\+X` (grep -E) to match the literal glyph.
set -euo pipefail
. "$(dirname "$0")/lib.sh"

run() {
  tui_begin
  tui_send "?"
  tui_wait_for "Keybindings" 10
  tui_wait_for "Ctrl\+P" 10
  tui_wait_for "Ctrl\+E" 10
  tui_wait_for "Ctrl\+T" 10
  tui_wait_for "Esc" 10
  tui_send Escape
}