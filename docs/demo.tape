# Agent-loop tour: TUI cold open, 2 scripted tool turns, end card.
# Requires nikicode on PATH + NIKICODE_DEMO_TOUR=1 behaviour (mock, offline).
# `vhs docs/demo.tape` from repo root writes demo.gif (then 720px/fps10/palette).
Output "demo.gif"
Set FontSize 14
Set Width 900
Set Height 600
Set TypingSpeed 15ms
Hide
Type "export NIKICODE_DEMO_TOUR=1; cd /tmp/g3demo && nikicode" Enter
Sleep 2.5s
Show
Sleep 1s
Type "read the main file" Enter
Sleep 2.5s
Type "search for worker" Enter
Sleep 2.5s
Sleep 2.5s
