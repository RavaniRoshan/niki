#!/usr/bin/env bash
# Smoke: `/doctor` must not become an LLM prompt.
#
# Every unmatched slash command used to be pushed into the transcript as a
# user message and sent to the provider as text. `/doctor` and `/review` are
# both listed in the slash menu and in `/help`, so a user's first slash
# command was likely one of them — and the result was a model reply to the
# string "/doctor", with nothing on screen saying a command had been typed.
run() {
  tui_begin
  tui_send "/doctor"
  tui_send Enter
  if ! tui_wait_for "shell" 20; then
    echo "  FAIL: /doctor produced no explanation" >&2
    tui_capture >&2
    return 1
  fi
  local cap
  cap="$(tui_capture)"
  # The command must be answered by NIKI, not relayed to the model. If it were
  # relayed, the only visible text would be the user's own "/doctor" echoed
  # into a user bubble with no answer following it.
  if grep -q "Unknown command" <<<"$cap"; then
    echo "  FAIL: /doctor is not wired at all — it fell through" >&2
    tui_capture >&2
    return 1
  fi
  echo "  PASS: /doctor is answered by NIKI, not sent to the model"
}
