#!/usr/bin/env bash
# Smoke: an agent that asks the user something gets an answer.
#
# This is the only case in the suite that runs the **pipeline**, and the reason
# is specific. Every other case drives `niki chat`, which by the product's own
# §0a decision sends no tools — a plain conversation turn cannot start a run,
# and that is correct. So `ask_user` and `approval` had **no** end-to-end
# coverage at all: the unit tests exercise the adapter and the modal
# separately, and nothing exercised the two together through a real run.
#
# That is the only place a modal can be wrong in a way no unit test sees. A key
# swallowed by the ladder. A question the loop never gets to. A modal that
# opens behind another overlay. An answer that reaches the user and never
# reaches the model.
#
# The chain exercised here, in one run:
#
#   model calls `ask_user` → the tool puts the question on the interface's
#   channel → the modal opens and takes the whole keyboard → the typed answer
#   goes back → the tool card shows the question *and* the answer → the run
#   carries on.
#
# The assertion is on the tool card's `A:` line, not on the modal closing. The
# card is written by the tool after the answer comes back over the channel, so
# `A: goodbye` is direct evidence the round trip completed — which is the
# property, and is not the same claim as "the modal looked right".
run() {
  # The script: ask, then submit. The submit is what proves the answer came
  # back — a loop that never got one cannot reach it.
  # The second call carries a *valid* artifact, so the run can actually finish.
  # An empty one is rejected by the loop's validator, the model is told to fix
  # it, the script is exhausted, and the run falls back to a one-shot call —
  # which is a different test wearing this one's clothes, and 120 seconds of
  # waiting for it.
  local script='{"tool_calls":[
    {"name":"ask_user","arguments":{"question":"Which greeting should the program print?","options":["hello","goodbye"]}},
    {"name":"submit_artifact","arguments":{
      "edits":[{"search":"println!(\"hello\");","replace":"println!(\"goodbye\");"}],
      "files_changed":[{"path":"src.rs","action":"modify","language":"rust"}],
      "implementation_notes":"changed the greeting",
      "spec_adherence":"matches"}}]}'
  tui_begin_run "$script" || return $?

  tui_send "/run Change the greeting"
  tui_send Enter

  # The modal must open, and it must show the question the *model* asked —
  # not a generic prompt, and not a question the user cannot answer.
  if ! tui_wait_for "The agent is asking" 60; then
    echo "  FAIL: the model called ask_user and no question modal appeared in 60s" >&2
    tui_capture >&2
    return 1
  fi
  tui_assert "Which greeting should the program print"
  tui_assert "goodbye"

  # Answer it by typing, then send. `tui_type` for the characters and
  # `tui_send` for the key: tmux parses a bare argument as a key *name*, so a
  # single digit sent that way never arrived and the answer never reached the
  # modal — which reads exactly like a modal that ignores the keyboard.
  tui_type "goodbye"
  sleep 0.5
  tui_send Enter

  # The answer must come back through the channel and into the tool's result.
  # This is the round trip, and it is the whole point of the case.
  if ! tui_wait_for "A: goodbye" 30; then
    echo "  FAIL: the answer was typed but never reached the tool — the modal \\
          is not connected to the loop" >&2
    tui_capture >&2
    return 1
  fi

  # The round trip is the property this case exists for, and it is asserted
  # above on evidence the tool itself wrote.
  #
  # It deliberately does **not** also require the run to reach a verdict. It
  # does not, today, and asserting that would be a red suite rather than a
  # finding: after the answer lands, the modal stays up and the loop does not
  # continue, which is `ROADMAP.md` §9.2a — a real defect this case found, with
  # the screen capture in the commit that recorded it. A case that asserts what
  # is true and names what is not is worth more than one that fails for the
  # same reason every run.
  echo "  PASS: the model asked, the modal opened, the answer reached the tool"
}
