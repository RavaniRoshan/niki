#!/usr/bin/env bash
# Smoke: a first run in a fresh directory must answer, quickly.
#
# This is the most likely first session in the product — bare `niki`, a fresh
# directory, no `niki.toml` — and it had **zero** coverage anywhere: not here,
# not in the PTY suite, not in the visual tapes. Every other assertion in this
# directory runs against a state where something is already configured.
#
# What is asserted is the property, not a particular message. What a first run
# can legitimately do depends on the machine: with nothing reachable it must
# say so and name a command; with a local Ollama up it must reach that, and may
# report that the model is not pulled — which is exactly what this box does,
# and which is a correct and useful answer. What must never happen is silence:
# the case exists because "press Enter, nothing appears" was indistinguishable
# from a hung program.
#
# The *no provider at all* path is covered deterministically elsewhere, where it
# does not depend on what happens to be running: `tests/onboarding_truth.rs`.
run() {
  tui_begin
  tui_send "hello"
  tui_send Enter

  # Something must come back, and quickly. 20s is generous for a real provider
  # and not for a hang.
  local deadline=$((SECONDS + 20))
  local cap
  while [ $SECONDS -lt $deadline ]; do
    cap="$(tui_capture)"
    # The composer echoes the user's own text, so look for anything after it:
    # a provider notice, a real reply, or an error the chat produced itself.
    if grep -qE "No LLM provider|LLM error|does not recognise|assistant:|error:" <<<"$cap"; then
      if grep -q "Unknown command" <<<"$cap"; then
        echo "  FAIL: the message was rejected as an unknown command" >&2
        tui_capture >&2
        return 1
      fi
      echo "  PASS: a first run answers instead of hanging"
      return 0
    fi
    sleep 0.3
  done

  echo "  FAIL: 20s after Enter there is still no answer — the first session is a hang" >&2
  tui_capture >&2
  return 1
}
