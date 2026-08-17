# NIKI Demo GIF Refresh — Visual Task Prompt

> **This is a visual task.** Requires a screen recording tool (asciinema, LICEcap, or OBS)
> and a machine with Niki installed and running.
> Run this prompt on the main Niki repo (`/home/shiva/projects/niki`).

---

## Context

The current `assets/demo.gif` was created on 2026-08-16 and is 1200×600 (2.2MB). It shows the older TUI output. The TUI has since been significantly updated (Phases 1–5: new pages, fleet dashboard, session view, typing indicator, etc.). The demo needs to reflect the current v0.4.0 state.

**Current demo:** `assets/demo.gif` (2.2MB, 1200×600)
**Current video:** `assets/demo.mp4` (967KB, same content)

---

## Goal

Create a fresh demo GIF that shows Niki v0.4.0 running end-to-end:
1. A realistic coding task being described
2. The four agents running (Planner → Coder → Tester → Reviewer)
3. The final output (branch, report, artifacts)

The demo should be **under 30 seconds** and **1200×600** (or similar landscape aspect ratio) for the README.

---

## Option A: TUI Demo (Recommended)

Shows the rich ratatui TUI with the agentic transcript view.

### Setup

```bash
cd /home/shiva/projects/niki
cargo build --release

# Make sure you have:
# 1. A project to run against (use a small demo project or Niki itself)
# 2. An API key configured
# 3. Podman or Docker running
# 4. A screen recording tool (asciinema recommended)

# Create a small demo project
mkdir -p /tmp/niki-demo && cd /tmp/niki-demo
git init
cat > index.js << 'EOF'
const express = require('express');
const app = express();

app.get('/', (req, res) => {
  res.json({ message: 'Hello World' });
});

app.listen(3000, () => {
  console.log('Server running on port 3000');
});
EOF
git add . && git commit -m "initial"
```

### Record

```bash
# Using asciinema (recommended — lightweight, SVG-friendly)
asciinema rec /tmp/niki-demo.cast

# In another terminal:
cd /tmp/niki-demo
niki run "Add a GET /health endpoint that returns { status: 'ok', uptime: process.uptime() }" \
  --project . --tui

# Wait for the pipeline to complete (Planner → Coder → Tester → Reviewer)
# Type 'q' to exit when done

# Stop recording (Ctrl+D or exit the cast)
```

### Convert to GIF

```bash
# Using agg (asciinema → GIF converter)
agg /tmp/niki-demo.cast assets/demo.gif --cols 120 --rows 30 --font-size 14

# Or using svg-term + gifski
svg-term --in /tmp/niki-demo.cast --out /tmp/niki-demo.svg
# Then convert SVG to GIF with your preferred tool

# Or using LICEcap (GUI tool)
# Record directly to .gif at 1200×600
```

### Screenshot the Output

After the pipeline completes, capture the final output:

```bash
# Show the report
niki report <task-id>

# Show the branch
git log --oneline -3

# Show the diff
git diff main..niki/<id>
```

Capture these as separate screenshots if needed for the README "See it run" section.

---

## Option B: CLI Demo (Simpler)

Shows the non-TUI log output (simpler to record, less visual).

### Record

```bash
# Using script (built-in Unix tool)
script -q /tmp/niki-demo.log

cd /tmp/niki-demo
niki run "Add a GET /health endpoint that returns { status: 'ok', uptime: process.uptime() }" \
  --project .

# Wait for completion
exit

# Convert to GIF using asciinema or similar
asciinema rec /tmp/niki-demo.cast --command "niki run 'Add a GET /health endpoint' --project /tmp/niki-demo"
```

---

## Option C: Animated Terminal Screenshot (Static)

If recording isn't feasible, create an animated terminal screenshot using a tool like:
- [Termtosvg](https://github.com/nicholasgasior/termtosvg) — records terminal to SVG
- [Carbon](https://carbon.now.sh/) — beautiful code screenshots
- [Figlet](https://github.com/nicholasgasior/figlet) — ASCII art for terminal output

Use the existing sample output from the README:

```text
 ◈ ⟠ ◉ ◆   NIKI
   "Add a GET /health endpoint…"

 [Planner]   Done — Spec: 1 file to modify
 [Coder]     Done — Changed 1 file · index.js [modified]
 [Tester]    Done — 8/8 tests passed
 [Reviewer]  Done — Approved · correctness 10/10 · quality 8/10 · coverage 10/10
 [NIKI]      Task complete — Branch: niki/6d281d6d · Verdict: Approved · Revisions: 0
```

---

## Assets to Update

| File | Action |
|---|---|
| `assets/demo.gif` | Replace with new recording (keep 1200×600 or similar) |
| `assets/demo.mp4` | Replace with video version (optional) |
| `README.md` | Verify `<img src="assets/demo.gif">` still works |

---

## Recording Guidelines

1. **Terminal size:** 120–140 columns × 30–40 rows (landscape, wide)
2. **Font:** Monospace, 14–16pt (readable in GIF)
3. **Theme:** Use a dark terminal theme (matches Niki's dark TUI)
4. **Speed:** Normal speed (don't speed up — users want to see the real output)
5. **Duration:** Under 30 seconds (shorter is better)
6. **Resolution:** 1200×600 or 1920×1080 (scale down for GIF)
7. **Task:** Use a realistic but simple task (add a health endpoint, add a comment, create a file)

---

## GIF Optimization

After recording, optimize the GIF:

```bash
# Using gifsicle (recommended)
gifsicle -O3 --lossy=80 assets/demo.gif -o assets/demo-opt.gif
mv assets/demo-opt.gif assets/demo.gif

# Check file size (should be under 2MB)
ls -la assets/demo.gif

# If too large, reduce colors or frame rate
gifsicle -O3 --colors 128 --lossy=80 assets/demo.gif -o assets/demo-opt.gif
```

---

## Verification Checklist

- [ ] GIF is under 30 seconds
- [ ] GIF is under 2MB
- [ ] All four agents visible (Planner, Coder, Tester, Reviewer)
- [ ] Final output visible (branch name, verdict)
- [ ] Terminal text is readable (not blurry)
- [ ] No cursor blinking or artifacts
- [ ] README.md image tag still works: `<img src="assets/demo.gif">`
- [ ] Works in both dark and light GitHub themes

---

## Commit Message

```
demo: refresh demo GIF for v0.4.0

- Updated to show current TUI output with v0.4.0 features
- All four agents visible: Planner → Coder → Tester → Reviewer
- Final output shows branch, verdict, and artifacts
```
