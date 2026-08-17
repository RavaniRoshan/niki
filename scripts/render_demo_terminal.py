#!/usr/bin/env python3
"""
NIKI Terminal Showcase Script for VHS Recording.
Matches Screenshot 2026-08-17 212129.png and token.md specifications 1:1.
"""

import sys
import time

# --- ANSI Color Codes (Matching token.md Primitives) ---
RESET = "\033[0m"
BOLD = "\033[1m"
DIM = "\033[2m"
ITALIC = "\033[3m"

CLAY = "\033[38;2;204;120;92m"       # #cc785c (Terracotta Clay Hero Accent)
LIGHT_CLAY = "\033[38;2;212;139;112m" # #d48b70
SAND = "\033[38;2;212;163;115m"      # #d4a373 (Warm Sand Accent)
CREAM_100 = "\033[38;2;250;248;245m" # #faf8f5 (Bright Hero & Logo)
CREAM_200 = "\033[38;2;243;239;234m" # #f3efea (Body Text)
CREAM_300 = "\033[38;2;230;223;213m" # #e6dfd5 (Dim Labels & Tester)
ASH = "\033[38;2;138;132;128m"       # #8a8480 (Muted Metadata & Branches)
BORDER = "\033[38;2;56;51;48m"        # #383330 (Capsule Frame & Dividers)
BG_SURFACE = "\033[48;2;32;29;29m"   # #201d1d (Elevated Card Background)
BG_PILL = "\033[48;2;40;36;35m"      # #282423 (Pill Badge Surface)

THINKING_GREEN = "\033[38;2;78;190;130m" # #4ebe82 (Spinner Only)
SUCCESS_GREEN = "\033[38;2;52;211;153m"  # #34d399 (Checkmark Only)
AMBER = "\033[38;2;224;159;62m"      # #e09f3e (Reviewer Amber)

SPINNER_FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]

def type_text(text: str, speed: float = 0.035):
    for ch in text:
        sys.stdout.write(ch)
        sys.stdout.flush()
        time.sleep(speed)

def print_banner():
    sys.stdout.write("\033[2J\033[H") # Clear screen
    print()
    logo_lines = [
        f"                             {CREAM_100}███╗   ██╗██╗██╗  ██╗██╗{RESET}",
        f"                             {CREAM_100}████╗  ██║██║██║ ██╔╝██║{RESET}",
        f"                             {CREAM_100}██╔██╗ ██║██║█████╔╝ ██║{RESET}",
        f"                             {CREAM_100}██║╚██╗██║██║██╔═██╗ ██║{RESET}",
        f"                             {CREAM_100}██║ ╚████║██║██║  ██╗██║{RESET}",
        f"                             {CREAM_100}╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝{RESET}",
    ]
    for line in logo_lines:
        print(line)
    print()

def print_input_capsule(query_text: str = "", typing: bool = False):
    border_len = 84
    # Top border
    top_line = f"  {BORDER}╭─{RESET}"
    # Mode prefix and cursor
    capsule_content = f" {CLAY}│{RESET}  {CREAM_100}{BOLD}Build{RESET}  {CREAM_200}{query_text}{RESET}"
    badges = f"{BG_PILL}{ASH} sandbox {RESET}  {BG_PILL}{ASH} podman {RESET}"
    
    # Render elevated capsule box
    print(f"  {BORDER}╭{'─' * border_len}╮{RESET}")
    if typing:
        sys.stdout.write(f"  {BORDER}│{RESET}  {CLAY}│{RESET}  {CREAM_100}{BOLD}Build{RESET}  {CREAM_200}")
        sys.stdout.flush()
        type_text(query_text, 0.038)
        # Pad remaining space
        vis_len = 2 + 1 + 2 + 5 + 2 + len(query_text)
        badge_vis_len = 9 + 2 + 8
        pad = border_len - vis_len - badge_vis_len
        sys.stdout.write(f"{' ' * pad}{badges}  {BORDER}│{RESET}\n")
        sys.stdout.flush()
    else:
        vis_len = 2 + 1 + 2 + 5 + 2 + len(query_text)
        badge_vis_len = 9 + 2 + 8
        pad = border_len - vis_len - badge_vis_len
        print(f"  {BORDER}│{RESET}  {CLAY}│{RESET}  {CREAM_100}{BOLD}Build{RESET}  {CREAM_200}{query_text}{' ' * pad}{badges}  {BORDER}│{RESET}")
    print(f"  {BORDER}╰{'─' * border_len}╯{RESET}")
    print()

def simulate_spinner(prefix: str, verb: str, duration: float = 1.3):
    frames = len(SPINNER_FRAMES)
    steps = int(duration / 0.065)
    for i in range(steps):
        glyph = SPINNER_FRAMES[i % frames]
        sys.stdout.write(f"\r  {THINKING_GREEN}∴ {glyph}{RESET} {prefix} {ASH}· {verb}...{RESET}   ")
        sys.stdout.flush()
        time.sleep(0.065)
    sys.stdout.write(f"\r  {prefix}                                                              \n")
    sys.stdout.flush()

def main():
    print_banner()
    time.sleep(0.5)

    # 1. Elevated Input Capsule Bar with Interactive Typing
    prompt = "Add GET /health -> { status, uptime } with hermetic test verification"
    print_input_capsule(prompt, typing=True)
    time.sleep(0.4)

    # 2. Command Invocation Line
    print(f"  {ASH}$ niki run \"Add a GET /health endpoint\" --project ./my-app{RESET}")
    time.sleep(0.5)

    # 3. Agent Execution Stream
    # ◈ Planner
    simulate_spinner(f"{SAND}◈ Planner{RESET} {CREAM_300}TaskSpec ready{RESET}", "Planning task graph", 1.1)
    print(f"  {ASH}  files: src/routes/health.ts · tests/health.test.ts{RESET}")
    time.sleep(0.4)

    # ⟠ Coder
    simulate_spinner(f"{CLAY}⟠ Coder{RESET} {CREAM_300}unified diff applied{RESET}", "Synthesizing solution", 1.2)
    time.sleep(0.3)

    # ◉ Tester
    simulate_spinner(f"{CREAM_300}◉ Tester{RESET} {CREAM_200}3 passed · 0 failed{RESET}", "Executing test suite", 1.0)
    time.sleep(0.3)

    # ◆ Reviewer
    simulate_spinner(f"{AMBER}◆ Reviewer{RESET} {CREAM_200}approved · 0 revisions{RESET}", "Auditing invariant proofs", 0.9)
    time.sleep(0.4)

    # ✓ Final Verdict Line
    print(f"  {SUCCESS_GREEN}✓{RESET} {CREAM_100}{BOLD}branch niki/a7f3c2 · report.md · changes.patch{RESET}")
    print(f"  {ASH}  working tree: untouched · hermetic sandbox clean{RESET}")
    print()
    time.sleep(0.6)

    # 4. Interactive Slash Command /cost
    sys.stdout.write(f"  {CLAY}❯{RESET} ")
    sys.stdout.flush()
    time.sleep(0.3)
    type_text("/cost", 0.04)
    print()
    time.sleep(0.2)
    print(f"  {SAND}{BOLD}Session Economics:{RESET}")
    print(f"    {ASH}• Total Spend:       {SUCCESS_GREEN}$0.0034 USD{RESET}")
    print(f"    {ASH}• Input Tokens:      {CREAM_200}2,180{RESET}")
    print(f"    {ASH}• Output Tokens:     {CREAM_200}640{RESET}")
    print(f"    {ASH}• Cache Read Tokens: {CREAM_200}16,800{RESET} {SAND}(92.4% prompt cache hit rate){RESET}")
    print(f"    {ASH}• Active Model:      {CLAY}claude-3-7-sonnet{RESET}")
    print()
    time.sleep(0.8)

    # 5. Thin Divider Line
    print(f"  {BORDER}{'─' * 84}{RESET}")

    # 6. Status Bar matching Screenshot
    status_bar = (
        f"  {CREAM_100}{BOLD}tab{RESET} {ASH}toggle view{RESET}   "
        f"{CREAM_100}{BOLD}ctrl-p{RESET} {ASH}commands{RESET}   "
        f"{CREAM_100}{BOLD}ctrl-o{RESET} {ASH}thinking{RESET}   "
        f"{CREAM_100}{BOLD}esc{RESET} {ASH}quit (run continues){RESET}"
    )
    print(status_bar)
    print()
    time.sleep(1.8)

if __name__ == "__main__":
    main()
