# NIKI demo recording (v0.4.0)
# Rendered offline: local `niki` binary + mock LLM server + `--backend worktree`.
# No Docker/Podman needed for the pipeline; vhs runs the terminal locally via VHS_NO_SANDBOX=1.
#
#   VHS_NO_SANDBOX=1 vhs docs/marketing/vhs-script.tape
#
# Requires: `mock_llm.py` running on $MOCK_LLM_PORT (8080) and a seeded
# /tmp/demo-niki (index.js = `console.log("hello");`, with niki.toml -> mock).

Output "assets/demo.gif"
Output "assets/demo.mp4"
Set Shell "bash"
Set Width 1200
Set Height 600
Set FontSize 18
Set Theme "Twilight"

# --- prepare env (hidden) ---
Hide
Type "export PATH=/home/shiva/projects/niki/target/release:$PATH"
Enter
Sleep 1
Type "export MOCK_LLM_PORT=8080"
Enter
Sleep 1
Show

# --- version + help ---
Type "niki --version"
Enter
Sleep 2
Type "niki --help"
Enter
Sleep 3

# --- run the agent pipeline ---
Type "cd /tmp/demo-niki"
Enter
Sleep 1
Type "niki run 'Add a health check endpoint that returns {status: ok}' --backend worktree --project /tmp/demo-niki"
Enter
Sleep 10
Hide

# --- show the result ---
Type "git -C /tmp/demo-niki branch"
Enter
Sleep 2
Type "git -C /tmp/demo-niki diff --stat"
Enter
Sleep 2
Type "cat /tmp/demo-niki/index.js"
Enter
Sleep 3
Type "head -14 /tmp/demo-niki/.niki/tasks/*/report.md"
Enter
Sleep 3
Show
Type "echo 'Review the branch: git checkout niki/<id>'"
Enter
Sleep 3
