# Reproduce: nikicode on PATH, mock provider, empty-ish dir at /tmp/g3demo.
# `vhs docs/demo.tape` from repo root writes demo.gif (then optimize:
# 720px, fps 10, palettegen/paletteuse; see PROGRESS G6 notes).
Output "demo.gif"
Set FontSize 14
Set Width 900
Set Height 600
Set TypingSpeed 15ms
Hide
Type "cd /tmp/g3demo && nikicode" Enter
Sleep 2.5s
Show
Sleep 1s
Type "run the tests" Enter
Sleep 3s
Type "/quit" Enter
Sleep 1s
Type "nikicode do --plan 'commit my changes'" Enter
Sleep 2s
Type "nikicode --version" Enter
Sleep 2.5s
