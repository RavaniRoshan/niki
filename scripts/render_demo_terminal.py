#!/usr/bin/env python3
"""
NIKI Terminal Showcase Script for VHS Recording.
Renders the complete visual design language, micro-interactions,
and multi-agent choreography according to token.md specifications.
"""

import sys
import time

# --- ANSI Color Codes (Matching token.md) ---
RESET = "\033[0m"
BOLD = "\033[1m"
DIM = "\033[2m"
ITALIC = "\033[3m"

CLAY = "\033[38;2;204;120;92m"       # #cc785c
LIGHT_CLAY = "\033[38;2;212;139;112m" # #d48b70
SAND = "\033[38;2;212;163;115m"      # #d4a373
CREAM_100 = "\033[38;2;250;248;245m" # #faf8f5 (Bright Hero)
CREAM_200 = "\033[38;2;243;239;234m" # #f3efea (Body Text)
CREAM_300 = "\033[38;2;230;223;213m" # #e6dfd5 (Dim Labels)
ASH = "\033[38;2;138;132;128m"       # #8a8480 (Muted Meta)
BORDER = "\033[38;2;56;51;48m"        # #383330 (Card Dividers)
BG_SURFACE = "\033[48;2;32;29;29m"   # #201d1d (Card Surface)
THINKING_GREEN = "\033[38;2;78;190;130m" # #4ebe82 (Spinner Only)
SUCCESS_GREEN = "\033[38;2;52;211;153m"  # #34d399 (Checkmark Only)
AMBER = "\033[38;2;224;159;62m"      # #e09f3e (Reviewer / Prompt)

SPINNER_FRAMES = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]

def type_text(text: str, speed: float = 0.035):
    for ch in text:
        sys.stdout.write(ch)
        sys.stdout.flush()
        time.sleep(speed)

def print_banner():
    sys.stdout.write("\033[2J\033[H") # Clear screen
    print()
    logo = f"""  {CLAY}███╗   ██╗██╗██╗  ██╗██╗{RESET}
  {CLAY}████╗  ██║██║██║ ██╔╝██║{RESET}
  {LIGHT_CLAY}██╔██╗ ██║██║█████╔╝ ██║{RESET}
  {SAND}██║╚██╗██║██║██╔═██╗ ██║{RESET}
  {CREAM_300}██║ ╚████║██║██║  ██╗██║{RESET}
  {ASH}╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝╚═╝{RESET}"""
    print(logo)
    print()
    print(f"  {CREAM_100}{BOLD}NIKI{RESET} {ASH}v0.4.0{RESET} {ASH}· Autonomous Multi-Agent Engineering Architecture{RESET}")
    print(f"  {ASH}Workspace:{RESET} {CREAM_200}~/projects/payment-service{RESET} {ASH}· Branch:{RESET} {SAND}niki/feat-rate-limiter{RESET} {ASH}· Engine:{RESET} {CLAY}claude-3-7-sonnet{RESET}")
    print(f"  {BORDER}{'─' * 74}{RESET}")
    print()

def simulate_spinner(prefix: str, verb: str, duration: float = 1.4):
    frames = len(SPINNER_FRAMES)
    steps = int(duration / 0.07)
    for i in range(steps):
        glyph = SPINNER_FRAMES[i % frames]
        sys.stdout.write(f"\r  {THINKING_GREEN}∴ {glyph}{RESET} {SAND}{prefix}{RESET} {ASH}· {verb}...{RESET}   ")
        sys.stdout.flush()
        time.sleep(0.07)
    sys.stdout.write(f"\r  {SUCCESS_GREEN}✓{RESET} {SAND}{prefix}{RESET} {CREAM_300}· completed{RESET}                   \n")
    sys.stdout.flush()

def main():
    print_banner()
    time.sleep(0.6)

    # 1. First User Prompt
    sys.stdout.write(f"  {CLAY}❯{RESET} ")
    sys.stdout.flush()
    time.sleep(0.4)
    prompt = "Implement tiered token-bucket rate limiter with Redis backend and hermetic tests"
    type_text(prompt, 0.032)
    print()
    time.sleep(0.3)
    print()

    # 2. Planner Agent
    print(f"  {SAND}◈ Planner{RESET} {CREAM_300}Decomposing architecture into 4 verified task stages:{RESET}")
    print(f"    {ASH}1. Define `TokenBucketLimiter` struct with atomic Redis Lua script{RESET}")
    print(f"    {ASH}2. Attach Actix-web / Axum extraction middleware with IP tiering{RESET}")
    print(f"    {ASH}3. Author hermetic unit tests with mocked Redis client{RESET}")
    print(f"    {ASH}4. Run adversarial Red-team security audit on header bypass risks{RESET}")
    print()
    time.sleep(0.6)

    # 3. Coder Agent with Dynamic Thinking
    simulate_spinner(f"{CLAY}⟠ Coder{RESET}", "Synthesizing solution", 1.2)
    time.sleep(0.2)
    print()

    # Code Display Box
    print(f"  {BORDER}┌─ {SAND}src/middleware/rate_limit.rs{BORDER} {'─' * 41}┐{RESET}")
    code_lines = [
        f"  {BORDER}│{RESET} {CLAY}use{RESET} std::sync::Arc;                                               {BORDER}│{RESET}",
        f"  {BORDER}│{RESET} {CLAY}use{RESET} redis::aio::ConnectionPool;                                    {BORDER}│{RESET}",
        f"  {BORDER}│{RESET}                                                                  {BORDER}│{RESET}",
        f"  {BORDER}│{RESET} {CREAM_300}#[derive(Clone)]{RESET}                                                  {BORDER}│{RESET}",
        f"  {BORDER}│{RESET} {CLAY}pub struct{RESET} {CREAM_100}TokenBucketLimiter{RESET} {{                                    {BORDER}│{RESET}",
        f"  {BORDER}│{RESET}     pool: {SAND}Arc<ConnectionPool>{RESET},                                       {BORDER}│{RESET}",
        f"  {BORDER}│{RESET}     capacity: {LIGHT_CLAY}u32{RESET},                                                {BORDER}│{RESET}",
        f"  {BORDER}│{RESET}     refill_rate_per_sec: {LIGHT_CLAY}u32{RESET},                                     {BORDER}│{RESET}",
        f"  {BORDER}│{RESET} }}                                                                {BORDER}│{RESET}",
        f"  {BORDER}│{RESET}                                                                  {BORDER}│{RESET}",
        f"  {BORDER}│{RESET} {CLAY}impl{RESET} {CREAM_100}TokenBucketLimiter{RESET} {{                                           {BORDER}│{RESET}",
        f"  {BORDER}│{RESET}     {CLAY}pub async fn{RESET} {SAND}check_limit{RESET}(&{CLAY}self{RESET}, client_id: &{LIGHT_CLAY}str{RESET}) -> {SAND}Result<bool>{RESET} {{  {BORDER}│{RESET}",
        f"  {BORDER}│{RESET}         {ASH}// Atomic Lua token bucket evaluation script{RESET}                  {BORDER}│{RESET}",
        f"  {BORDER}│{RESET}         {CLAY}let{RESET} allowed: {LIGHT_CLAY}bool{RESET} = {CLAY}self{RESET}.eval_lua(client_id, {CLAY}self{RESET}.capacity).{CLAY}await{RESET}?;     {BORDER}│{RESET}",
        f"  {BORDER}│{RESET}         {CLAY}Ok{RESET}(allowed)                                                {BORDER}│{RESET}",
        f"  {BORDER}│{RESET}     }}                                                            {BORDER}│{RESET}",
        f"  {BORDER}│{RESET} }}                                                                {BORDER}│{RESET}",
    ]
    for line in code_lines:
        print(line)
    print(f"  {BORDER}└{'─' * 70}┘{RESET}")
    print()
    time.sleep(0.6)

    # 4. Tester Agent
    simulate_spinner(f"{CREAM_300}◉ Tester{RESET}", "Running hermetic verification suite", 1.1)
    print(f"    {SUCCESS_GREEN}✓{RESET} {CREAM_200}test_rate_limiter_allows_under_burst_capacity{RESET} {ASH}... ok (2ms){RESET}")
    print(f"    {SUCCESS_GREEN}✓{RESET} {CREAM_200}test_rate_limiter_rejects_exceeded_quota{RESET} {ASH}... ok (3ms){RESET}")
    print(f"    {SUCCESS_GREEN}✓{RESET} {CREAM_200}test_token_bucket_refills_at_steady_state{RESET} {ASH}... ok (15ms){RESET}")
    print(f"    {ASH}Result: {SUCCESS_GREEN}3 passed{ASH} · 0 failed · 0 warnings · Memory hermetic{RESET}")
    print()
    time.sleep(0.6)

    # 5. Reviewer & Red Team
    print(f"  {AMBER}◆ Reviewer{RESET} {SUCCESS_GREEN}✓ Verified:{RESET} {CREAM_200}Adversarial audit clear — zero header spoofing vectors.{RESET}")
    print(f"    {ASH}Audit Verdict: {SUCCESS_GREEN}ACCEPT{ASH} · Confidence: {CREAM_100}0.99{ASH} · Checkpoint: {SAND}chk-7f9a2{RESET}")
    print()
    time.sleep(0.7)

    # 6. Slash Command /cost Demo
    sys.stdout.write(f"  {CLAY}❯{RESET} ")
    sys.stdout.flush()
    time.sleep(0.3)
    type_text("/cost", 0.04)
    print()
    time.sleep(0.2)
    print()
    print(f"  {SAND}{BOLD}Session Economics:{RESET}")
    print(f"    {ASH}• Total Spend:       {SUCCESS_GREEN}$0.0038 USD{RESET}")
    print(f"    {ASH}• Input Tokens:      {CREAM_200}2,420{RESET}")
    print(f"    {ASH}• Output Tokens:     {CREAM_200}890{RESET}")
    print(f"    {ASH}• Cache Read Tokens: {CREAM_200}14,200{RESET} {SAND}(85.4% prompt cache hit rate){RESET}")
    print(f"    {ASH}• Active Model:      {CLAY}claude-3-7-sonnet{RESET}")
    print()
    time.sleep(0.7)

    # 7. Global Thinking Toggle (Ctrl+O)
    sys.stdout.write(f"  {ASH}Pressing {CREAM_100}Ctrl+O{ASH}...{RESET}\n")
    time.sleep(0.3)
    print(f"  {THINKING_GREEN}∴ Expanded all thinking traces (Ctrl+O){RESET}")
    print(f"    {SAND}∴ Deductive Reasoning Chain:{RESET}")
    print(f"      {ASH}1. Invariant: Redis EVALSHA avoids script re-transmission overhead.{RESET}")
    print(f"      {ASH}2. Invariant: Subprocess signals isolated in PGID(0) sandbox.{RESET}")
    print(f"      {ASH}3. Invariant: Atomic rollback checkpoint recorded at git HEAD.{RESET}")
    print()
    time.sleep(0.8)

    # 8. Footer Status Bar
    print(f"  {BORDER}{'─' * 74}{RESET}")
    status_bar = f"  {SAND}[sandbox: worktree]{RESET} {ASH}· tab view · ctrl+o thinking · /rewind rollback ·{RESET} {SUCCESS_GREEN}✓ 0 defects{RESET}"
    print(status_bar)
    print()
    time.sleep(1.2)

if __name__ == "__main__":
    main()
