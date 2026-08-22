#!/usr/bin/env bash
# Smoke: typed text echoes, kill-ring word kill + yank round-trips.
run() {
  tui_begin
  tui_send "hello world"
  tui_wait_for "hello world" 10
  tui_send C-w   # kill word -> "hello "
  tui_send C-y   # yank word -> "hello world"
  tui_wait_for "hello world" 10
  tui_assert "hello world"
}
