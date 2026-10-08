# Reproduce: bin/nikicode on PATH as `nikicode`, then `vhs docs/demo.tape` from repo root.
# Uses the mock provider (offline, no spend) in /tmp/g3demo.
Output "demo.gif"
Set FontSize 14
Set Width 900
Set Height 600
Hide
Type "cd /tmp/g3demo" Enter
Show
Sleep 1s
Type "nikicode --version" Enter
Sleep 1.5s
Type "nikicode do --plan 'run the tests'" Enter
Sleep 2s
Type "nikicode" Enter
Sleep 2s
Type "hello" Enter
Sleep 3s
Type "/quit" Enter
Sleep 1.5s
Type "nikicode git log" Enter
Sleep 2s
