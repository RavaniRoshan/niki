#!/usr/bin/env bash
# Smoke: Ctrl+P opens the command palette.
run() {
  tui_begin
  tui_send C-p
  tui_wait_for "Commands" 10
  tui_assert "Commands"
}
