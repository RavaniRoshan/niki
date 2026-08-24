#!/usr/bin/env bash
# Smoke: /permissions renders the permission-modes view.
run() {
  tui_begin
  tui_send "/permissions"
  tui_send Enter
  tui_wait_for "Permission modes" 10
}
