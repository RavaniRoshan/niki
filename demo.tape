# NIKI cinematic demo — source of truth is scripts/render_demo_cinematic.py
# (deterministic PIL frame renderer: zero terminal capture => zero flicker).
#
#   python -B scripts/render_demo_cinematic.py --preview-only   # stills check
#   python -B scripts/render_demo_cinematic.py                  # MP4 + GIF
#
# Canvas: 1280x840 @ 10fps, 800 frames = 80 s.
# Outputs: assets/demo.mp4 (1280x840, full quality) +
#          assets/demo.gif (800x525 palette, ~8 MB, README weight).
# Story: flawed /health (500 when DB down) -> user msg -> permission modal ->
#        Planner/Read/Coder/Bash-fail/Tester/Reviewer stages + tool cards ->
#        branch niki/a7f3c2 -> /cost. macOS window on wallpaper; every
#        terminal pixel replicates src/display (chat roles, stage headers,
#        tool cards, permission modal, input capsule, status bar).
#
# The VHS tape below is kept only as a live-capture fallback convention
# (mirrors the cinematic canvas); it is NOT how assets/demo.gif is produced.
Output assets/demo-vhs.gif

Set Shell "bash"
Set FontSize 16
Set FontFamily "JetBrains Mono"
Set Width 1280
Set Height 840
Set LetterSpacing 1
Set LineHeight 1.25
Set Padding 24
Set WindowBar "Colorful"
Set WindowBarSize 38
Set Margin 36
Set MarginFill "#2a3a6e"
Set BorderRadius 14
