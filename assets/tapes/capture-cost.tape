Output assets/screenshots/cost.png

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
Sleep 15s

# Navigate to Cost page
Type "c"
Sleep 3s

Screenshot assets/screenshots/cost.png
