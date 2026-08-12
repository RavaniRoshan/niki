# VHS Demo Script
# 0 → running Niki in 60-90 seconds
# Render with: vhs --renderer gif --output assets/demo.gif docs/marketing/vhs-script.tape
# Also: vhs --renderer mp4 --output assets/demo.mp4 docs/marketing/vhs-script.tape

Output "docs/marketing/vhs-script.tape"

# Setup: set a clean terminal state
Set Width 120
Set Height 40
Set Font "JetBrainsMono Nerd Font"
Set Theme "twilight"

# Hide typing for setup commands
Hide

# Type the install command
Type "cargo install niki"
Enter
# Wait for install (this is simulated - real install takes longer, but we trim)
Wait "Finished"
Wait 1s

# Show the installed version
Type "niki --version"
Enter
Wait "niki 0.3.0"

# Show help
Type "niki --help"
Enter
Wait "Commands:"

Show
Type "export ANTHROPIC_API_KEY=sk-ant-YOUR_KEY_HERE"
Enter
Wait

# Create a test project to run on
Type "mkdir /tmp/demo-niki && cd /tmp/demo-niki"
Enter

Type "git init"
Enter
Wait "Initialized empty"

Type "echo 'fn main() {}' > main.rs"
Enter

Type "git add . && git commit -m init"
Enter
Wait "main (root-commit)"

Hide
Type "niki run 'Add a health endpoint to main.rs'"
Enter

# Show the TUI pipeline running
Wait
Wait "Planner"
Wait "Coder"
Wait "Tester"
Wait "Reviewer"

Show
Wait 3s
Hide

# Show the resulting git branch
Type "git branch"
Enter
Wait

# Show the diff
Type "git diff feature/"
Enter
Wait
Wait "Health endpoint"

# Show the report
Type "cat .niki/report.md"
Enter
Wait
Wait "# Niki Report"

# End with a cursor prompt
Show
Type "echo 'Review the branch with: git checkout feature/ -- '"
Enter

# Screenshot for the landing page
Screenshot "--capture-file=assets/landing-demo.png"

# End
EOF
