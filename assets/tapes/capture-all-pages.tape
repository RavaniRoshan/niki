Output assets/screenshots/all-pages.gif

Set Shell "bash"
Set FontSize 14
Set FontFamily "JetBrains Mono"
Set Width 1200
Set Height 600
Set Padding 20
Set WindowBar "Colorful"
Set WindowBarSize 36
Set Margin 20
Set MarginFill "#141211"
Set BorderRadius 10

Set Theme { "background": "#1a1716", "foreground": "#faf8f5", "cursor": "#cc785c", "selection": "#383330", "black": "#141211", "red": "#e76f51", "green": "#4ebe82", "yellow": "#e09f3e", "blue": "#6a9bcc", "magenta": "#cc785c", "cyan": "#d4a373", "white": "#f3efea", "brightBlack": "#383330", "brightRed": "#f08060", "brightGreen": "#34d399", "brightYellow": "#f4a261", "brightBlue": "#8ecae6", "brightMagenta": "#d48b70", "brightCyan": "#e6dfd5", "brightWhite": "#faf8f5" }

Hide
Type "./target/release/niki run 'Add health endpoint' --backend worktree --tui"
Enter
Sleep 3s
Show

# Wait for pipeline to finish
Sleep 20s

# Run page
Screenshot assets/screenshots/run.png

# Pipeline
Type "p"
Sleep 2s
Screenshot assets/screenshots/pipeline.png

# Agents
Type "a"
Sleep 2s
Screenshot assets/screenshots/agents.png

# Diff
Type "d"
Sleep 2s
Screenshot assets/screenshots/diff-wide.png

# Verdict
Type "v"
Sleep 2s
Screenshot assets/screenshots/verdict.png

# Cost
Type "c"
Sleep 2s
Screenshot assets/screenshots/cost-page.png

# Artifacts
Type "f"
Sleep 2s
Screenshot assets/screenshots/artifacts.png

# History
Type "h"
Sleep 2s
Screenshot assets/screenshots/history.png

# Config
Type ","
Sleep 2s
Screenshot assets/screenshots/config.png

# Help
Type "?"
Sleep 2s
Screenshot assets/screenshots/help.png

# TestLog
Type "l"
Sleep 2s
Screenshot assets/screenshots/testlog.png

# Fleet
Type "g"
Sleep 2s
Screenshot assets/screenshots/fleet.png

# Session (may need mission selection)
Type "s"
Sleep 2s
Screenshot assets/screenshots/session.png
