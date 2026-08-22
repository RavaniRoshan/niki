#!/usr/bin/env bash
# Smoke: /status renders the session-status view.
run() {
  tui_begin
  tui_send "/status"
  tui_send Enter
  tui_wait_for "Session Status" 10
  tui_assert "Model"
}
