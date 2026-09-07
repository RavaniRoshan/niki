Output "@FRAMES@/07-cost.gif"
Set Shell "bash"
Set FontSize 14
Set FontFamily "DejaVu Sans Mono"
Set Width 1200
Set Height 800
Hide
Type "NIKI_REDUCED_MOTION=1 NIKI_CI=1 @BIN@ chat -p @FIXTURE@" Enter
Sleep 2s
Show
Sleep 1s
Type "/cost" Enter
Sleep 1s
Screenshot "@FRAMES@/07-cost.png"
Sleep 1s
