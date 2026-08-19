#!/usr/bin/env python3
"""
NIKI Terminal Demo Script for VHS Recording — Claude Code–style chat flow.
Records the actual niki chat UI: welcome → typing → streaming → tool calls →
permission prompt → /cost → completion.  ~38 s total.
"""

import sys
import time

# ── ANSI palette (token.md primitives) ──────────────────────────────────────
RESET      = "\033[0m"
BOLD       = "\033[1m"
DIM        = "\033[2m"
ITALIC     = "\033[3m"

CLAY       = "\033[38;2;204;120;92m"   # #cc785c
LIGHT_CLAY = "\033[38;2;212;139;112m"  # #d48b70
SAND       = "\033[38;2;212;163;115m"  # #d4a373
CREAM_100  = "\033[38;2;250;248;245m"  # #faf8f5
CREAM_200  = "\033[38;2;243;239;234m"  # #f3efea
CREAM_300  = "\033[38;2;230;223;213m"  # #e6dfd5
ASH        = "\033[38;2;138;132;128m"  # #8a8480
BORDER     = "\033[38;2;56;51;48m"     # #383330
BG_SURFACE = "\033[48;2;32;29;29m"    # #201d1d
BG_PILL    = "\033[48;2;40;36;35m"    # #282423
BG_HIGH    = "\033[48;2;40;36;35m"    # #282423 (input capsule)

THINKING   = "\033[38;2;78;190;130m"  # #4ebe82
SUCCESS    = "\033[38;2;52;211;153m"  # #34d399
ERROR_CORAL= "\033[38;2;231;111;81m"  # #e76f51
AMBER      = "\033[38;2;224;159;62m"  # #e09f3e
CYAN       = "\033[38;2;106;155;204m" # #6a9bcc
PURPLE     = "\033[38;2;150;130;200m" # #9682c8

SPINNER = ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"]

# ── helpers ─────────────────────────────────────────────────────────────────
def clear():
    sys.stdout.write("\033[2J\033[H")
    sys.stdout.flush()

def sleep(t):
    time.sleep(t)

def type_text(text, speed=0.028):
    for ch in text:
        sys.stdout.write(ch)
        sys.stdout.flush()
        time.sleep(speed)

def print_logo():
    logo = [
        f"                             {CREAM_100}███╗   ██╗██╗██╗  ██╗██╗{RESET}",
        f"                             {CREAM_100}████╗  ██║██║██║ ██╔╝██║{RESET}",
        f"                             {CREAM_100}██╔██╗ ██║██║█████╔╝ ██║{RESET}",
        f"                             {CREAM_100}██║╚██╗██║██║██╔═██╗ ██║{RESET}",
        f"                             {CREAM_100}██║ ╚████║██║██║  ██╗██║{RESET}",
        f"                             {CREAM_100}╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝{RESET}",
    ]
    for line in logo:
        print(line)
    print()

def input_capsule(text="", typing=False, mode="Build"):
    w = 86
    mode_colors = {"Build": CLAY, "Cmd": SAND, "Shell": CYAN}
    mode_col = mode_colors.get(mode, CLAY)
    if typing:
        sys.stdout.write(f"  {BORDER}╭{'─'*w}╮{RESET}\n")
        sys.stdout.write(f"  {BORDER}│{RESET} {mode_col}│{RESET}  {CREAM_100}{BOLD}{mode}{RESET}  {CREAM_200}")
        sys.stdout.flush()
        type_text(text, 0.032)
        pad = w - (2 + 1 + 2 + len(mode) + 2 + len(text)) - 22
        sys.stdout.write(f"{' '*pad}{BG_PILL}{ASH} sandbox {RESET}  {BG_PILL}{ASH} podman {RESET}  {BORDER}│{RESET}\n")
        sys.stdout.write(f"  {BORDER}╰{'─'*w}╯{RESET}\n")
        sys.stdout.flush()
    else:
        pad = w - (2 + 1 + 2 + len(mode) + 2 + len(text)) - 22
        print(f"  {BORDER}╭{'─'*w}╮{RESET}")
        print(f"  {BORDER}│{RESET} {mode_col}│{RESET}  {CREAM_100}{BOLD}{mode}{RESET}  {CREAM_200}{text}{' '*pad}{BG_PILL}{ASH} sandbox {RESET}  {BG_PILL}{ASH} podman {RESET}  {BORDER}│{RESET}")
        print(f"  {BORDER}╰{'─'*w}╯{RESET}")

def spinner(prefix, verb, dur=1.0):
    steps = int(dur / 0.06)
    for i in range(steps):
        g = SPINNER[i % len(SPINNER)]
        sys.stdout.write(f"\r  {THINKING}∴ {g}{RESET} {prefix} {ASH}· {verb}...{RESET}   ")
        sys.stdout.flush()
        time.sleep(0.06)
    sys.stdout.write(f"\r\033[K")
    sys.stdout.flush()

def status_bar():
    left = (
        f"{CREAM_100}{BOLD}tab{RESET}{ASH} toggle view   {RESET}"
        f"{CREAM_100}{BOLD}ctrl-p{RESET}{ASH} commands   {RESET}"
        f"{CREAM_100}{BOLD}ctrl-o{RESET}{ASH} thinking   {RESET}"
        f"{CREAM_100}{BOLD}esc{RESET}{ASH} quit (run continues){RESET}"
    )
    right = f"{ASH}main (feature/health) · $0.0034{RESET}"
    pad = 86 - len(left) - len(right) + 2*len(ASH) + len(RESET)
    sys.stdout.write("  " + left + " " * max(pad, 1) + right + "\n")
    sys.stdout.flush()

def context_gauge(pct=12):
    filled = int(pct / 10)
    empty = 10 - filled
    bar = f"{THINKING}{'▓'*filled}{RESET}{ASH}{'░'*empty}{RESET}"
    sys.stdout.write(f"  Context {bar} {pct}% · $0.0034 (~$2.97)\n")
    sys.stdout.flush()

# ── main demo flow ───────────────────────────────────────────────────────────
def main():
    # ── 0. Welcome screen ────────────────────────────────────────────────────
    clear()
    print()
    print_logo()
    print(f"  {CREAM_300}Open-source multi-agent coding pipeline{RESET}")
    print(f"  {ASH}Directory: /home/shiva/projects/my-app{RESET}")
    print(f"  {ASH}Model:     claude-sonnet-4-20250514{RESET}")
    print(f"  {ASH}Version:   0.4.0{RESET}")
    print()
    sleep(2.5)

    # ── 1. User types a request ──────────────────────────────────────────────
    input_capsule("Add GET /health -> { status, uptime } with hermetic test", typing=True)
    sleep(1.2)

    # ── 2. Pipeline invocation ───────────────────────────────────────────────
    print(f"  {ASH}$ niki run {CLAY}\"Add a GET /health endpoint\"{RESET} {ASH}--project ./my-app{RESET}")
    sleep(0.8)

    # ── 3. Agent streaming — Planner ────────────────────────────────────────
    spinner(f"{SAND}◈ Planner{RESET} {CREAM_300}TaskSpec ready{RESET}", "Planning task graph", 1.0)
    print(f"  {ASH}  files: src/routes/health.ts · tests/health.test.ts{RESET}")
    sleep(0.5)

    # ── 4. Coder streaming ───────────────────────────────────────────────────
    spinner(f"{CLAY}⟠ Coder{RESET} {CREAM_300}unified diff applied{RESET}", "Synthesizing solution", 1.2)
    sleep(0.4)

    # ── 5. Tester streaming ──────────────────────────────────────────────────
    spinner(f"{CREAM_300}◉ Tester{RESET} {CREAM_200}3 passed · 0 failed{RESET}", "Executing test suite", 1.0)
    sleep(0.3)

    # ── 6. Reviewer streaming ────────────────────────────────────────────────
    spinner(f"{AMBER}◆ Reviewer{RESET} {CREAM_200}approved · 0 revisions{RESET}", "Auditing invariant proofs", 0.9)
    sleep(0.5)

    # ── 7. Pipeline finished ─────────────────────────────────────────────────
    print(f"  {SUCCESS}✓{RESET} {CREAM_100}{BOLD}branch niki/a7f3c2 · report.md · changes.patch{RESET}")
    print(f"  {ASH}  working tree: untouched · hermetic sandbox clean{RESET}")
    print()
    sleep(1.0)

    # ── 8. User types slash command /cost ────────────────────────────────────
    sys.stdout.write(f"  {CLAY}❯{RESET} ")
    sys.stdout.flush()
    sleep(0.3)
    type_text("/cost", 0.04)
    print()
    sleep(0.3)

    print(f"  {SAND}{BOLD}Session Economics:{RESET}")
    print(f"    {ASH}• Total Spend:       {SUCCESS}$0.0034 USD{RESET}")
    print(f"    {ASH}• Input Tokens:      {CREAM_200}2,180{RESET}")
    print(f"    {ASH}• Output Tokens:     {CREAM_200}640{RESET}")
    print(f"    {ASH}• Cache Read Tokens: {CREAM_200}16,800{RESET} {SAND}(92.4% hit rate){RESET}")
    print(f"    {ASH}• Active Model:      {CLAY}claude-sonnet-4-20250514{RESET}")
    print()
    sleep(1.5)

    # ── 9. Divider ───────────────────────────────────────────────────────────
    print(f"  {BORDER}{'─'*86}{RESET}")
    sleep(0.3)

    # ── 10. Context-window gauge (Claude Code style) ─────────────────────────
    context_gauge(12)
    print()
    sleep(1.5)

    # ── 11. Status bar ───────────────────────────────────────────────────────
    status_bar()
    print()
    sleep(2.0)

    # ── 12. New user message — streaming assistant response ──────────────────
    sys.stdout.write(f"  {CLAY}❯{RESET} ")
    sys.stdout.flush()
    sleep(0.3)
    type_text("Explain the health endpoint implementation", 0.032)
    print()
    sleep(0.4)

    # Assistant thinking block
    print(f"  {THINKING}◈ assistant{RESET} {ASH}(thinking...){RESET}")
    sleep(0.6)

    # Streaming response with markdown
    response = (
        f"  {CREAM_200}The {BOLD}{CREAM_100}/health{RESET}{CREAM_200} endpoint is a lightweight "
        f"liveness probe:{RESET}\n"
        f"\n"
        f"  {CREAM_200}{BOLD}src/routes/health.ts:{RESET}\n"
        f"  {CYAN}```typescript{RESET}\n"
        f"  {CREAM_200}import {{ Router }} from {PURPLE}'express'{CREAM_200};{RESET}\n"
        f"  {CREAM_200}import {{ checkDB }} from {PURPLE}'../db'{CREAM_200};{RESET}\n"
        f"\n"
        f"  {CREAM_200}const router = {PURPLE}Router(){CREAM_200};{RESET}\n"
        f"\n"
        f"  {CREAM_200}router.{ASH}get{RESET}{CREAM_200}({PURPLE}'/health'{CREAM_200}, async (req, res) => {{"
        f"{RESET}\n"
        f"  {CREAM_200}  const uptime = process.{ASH}uptime{RESET}{CREAM_200}();{RESET}\n"
        f"  {CREAM_200}  const db = await {ASH}checkDB{RESET}{CREAM_200}();{RESET}\n"
        f"  {CREAM_200}  res.{ASH}json{RESET}{CREAM_200}({{ status: {SUCCESS}'ok'{CREAM_200}, uptime }});{RESET}\n"
        f"  {CREAM_200}}});{RESET}\n"
        f"  {CYAN}```{RESET}\n"
        f"\n"
        f"  {CREAM_200}Key design points:{RESET}\n"
        f"  {CREAM_200}  • No auth required — public liveness endpoint{RESET}\n"
        f"  {CREAM_200}  • DB check ensures downstream dependencies are healthy{RESET}\n"
        f"  {CREAM_200}  • Hermetic test verifies both JSON shape and 200 status{RESET}"
    )
    sys.stdout.write(f"  {THINKING}◈ assistant{RESET}\n")
    sys.stdout.flush()
    sleep(0.3)
    type_text(response, 0.012)
    print()
    print()
    sleep(1.5)

    # ── 13. Tool call (Bash) ──────────────────────────────────────────────────
    print(f"  {CYAN}⏵ bash{RESET} {CREAM_200}{BOLD}Run tests for health endpoint{RESET}")
    sleep(0.5)
    print(f"  {ASH}    $ npx vitest run tests/health.test.ts{RESET}")
    sleep(0.8)
    print(f"  {SUCCESS}⎿ bash{RESET} {CREAM_200}3 passed · 0 failed · 124ms{RESET}")
    print()
    sleep(1.0)

    # ── 14. Final input capsule ───────────────────────────────────────────────
    input_capsule("", typing=False)
    sleep(1.5)

    # ── 15. Final status bar ─────────────────────────────────────────────────
    status_bar()
    print()
    sleep(2.0)

if __name__ == "__main__":
    main()
