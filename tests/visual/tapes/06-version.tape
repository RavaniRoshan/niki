Output "@FRAMES@/06-version.gif"
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
Type "/version" Enter
Sleep 1s
Screenshot "@FRAMES@/06-version.png"
Sleep 1s
