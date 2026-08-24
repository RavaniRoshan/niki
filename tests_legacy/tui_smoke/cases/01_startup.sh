#!/usr/bin/env bash
# Smoke: the initial TUI chrome renders (banner/tab, input hint, mode badge).
run() {
  tui_begin
  tui_assert "Build"
  tui_assert "Describe a change"
  tui_assert "MANUAL"
}
