#!/usr/bin/env bash
# Smoke: /version prints the binary/version banner.
run() {
  tui_begin
  tui_send "/version"
  tui_send Enter
  tui_wait_for "niki" 10
}
