# Loop-tour cut: TUI cold open, 3 scripted tool turns (read/grep/status),
# end card on the ledger. Requires nikicode on PATH + NIKICODE_DEMO_TOUR=1
# (mock, offline). `vhs docs/demo.tape` from repo root writes demo.gif
# (then 960px/fps10/palette).
Output "demo.gif"
Set FontSize 15
Set Width 1100
Set Height 700
Set TypingSpeed 15ms
Hide
Type "export PATH=/home/shiva/.local/bin:$PATH NIKICODE_DEMO_TOUR=1; cd /tmp/g3demo && nikicode" Enter
Sleep 3s
Show
Sleep 2s
Type "read the main file" Enter
Sleep 4s
Sleep 2s
Type "search for worker" Enter
Sleep 4s
Sleep 2s
Type "show the status" Enter
Sleep 3s
Sleep 4s
