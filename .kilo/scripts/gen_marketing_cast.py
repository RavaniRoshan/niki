#!/usr/bin/env python3
"""Generate demo.cast for the edb05249 NIKI run using real artifact data.

Emits one bulk event per pipeline milestone with realistic timing.
Real e429 durations: Planner 15s, Coder 10s, Tester 37s, Red 73s,
Reviewer 77s, completion at 212s total.
"""
import json

OUT = "/home/shiva/projects/niki/marketing-assets-export/demo.cast"
TS0 = 1785744810

W, H = 118, 34

def fg(r, g, b): return f"\x1b[38;2;{r};{g};{b}m"
def bg(r, g, b): return f"\x1b[48;2;{r};{g};{b}m"
def bold():      return "\x1b[1m"
def reset():     return "\x1b[0m"

BLUE   = fg(177,185,249)
PURPLE = fg(198,160,246)
GREEN  = fg(78,186,101)
AMBER  = fg(255,193,7)
PINK   = fg(255,99,132)
DIM    = fg(102,102,102)
BORDER = fg(58,58,58)
BRIGHT = fg(255,255,255)
WHITE  = fg(204,204,204)

events = []
def emit(t, text):
    events.append((round(t, 3), "o", text))

# Timing model (seconds, from real e429 run)
BANNER_T = 0.0
PLANNER_START = 2.5
PLANNER_DONE = 15.0
CODER_START = 15.0
CODER_DONE = 25.0
TESTER_START = 25.0
TESTER_DONE = 62.0
RED_START = 62.0
RED_DONE = 135.0
REVIEWER_START = 135.0
REVIEWER_DONE = 212.0
COMPLETION_T = 212.0

# ── Banner ──────────────────────────────────────────────────────────────
t = BANNER_T
banner_lines = [
    f" {BORDER}┌{'─'*67}┐{reset()}",
    f" {BORDER}│{' '*67}│{reset()}",
    f" {BORDER}│ {BLUE}◆{reset()} {PURPLE}⚡{reset()} {GREEN}●{reset()} {AMBER}◆{reset()} {bold()}{WHITE}NIKI{reset()}                                  │{BORDER}",
    f" {BORDER}│{' '*67}│{reset()}",
    f" {BORDER}│ {WHITE}\"Add a function square(n: i32) -> i32 that returns n squared\"{reset()} │{BORDER}",
    f" {BORDER}│{' '*67}│{reset()}",
    f" {BORDER}│ Project   {WHITE}niki{reset()}{'/'*44}{BORDER}",
    f" {BORDER}│ Pipeline  Planner → Coder → Tester → Reviewer{' '*24}{BORDER}",
    f" {BORDER}│ Models    {WHITE}claude-sonnet-4-20250514{reset()}{' '*24}{BORDER}",
    f" {BORDER}│ Task ID   {WHITE}edb05249{reset()}{' '*58}{BORDER}",
    f" {BORDER}│{' '*67}│{reset()}",
    f" {BORDER}└{'─'*67}┘{reset()}",
]
emit(t, "\n".join(banner_lines) + "\n")

# ── Agent block helper ────────────────────────────────────────────────
def agent_block(start_t, done_t, color, icon, name, content,
                in_tok, out_tok, summary, is_red=False):
    t = start_t

    # Agent start header
    header = f"\r\n{color}{bold()}{icon} {bold()}{name}{reset()}                                      {DIM}⣿{reset()}\r\n"
    header += f"{DIM} ─────────────────────────────────────────────────────────────────────────{reset()}\r\n"
    emit(t, header)
    t += 0.2

    # Streamed content — emit as 4 chunks at evenly spaced timestamps
    lines = content.split("\n")
    n = len(lines)
    for i in range(0, n, 4):
        chunk = "\n".join(lines[i:i+4]) + "\r\n"
        pct = (i / max(n, 1)) * (done_t - start_t - 0.4) + start_t + 0.2
        t = min(start_t + 0.2 + pct, done_t - 0.1)
        emit(t, chunk)

    # Done summary
    t = done_t
    ktok = (in_tok + out_tok) / 1000.0
    check_icon = "✓" if not is_red else "✓"
    check_color = GREEN if not is_red else PINK
    done_line = (f"\r\n{color}{bold()}{icon} {bold()}{name}{reset()}   "
                 f"{check_color}{bold()}{check_icon}{reset()}  {done_t - start_t:.0f}s  "
                 f"{in_tok:,}/{out_tok} ({ktok:.0f}k tok) · cost n/a — {summary}\r\n")
    emit(t, done_line)

    return done_t

# ── Planner ─────────────────────────────────────────────────────────────
spec_json = json.dumps({
    "summary": "Add a simple square function that takes an i32 and returns its square (n * n)",
    "approach": "Add the square function to src/lib.rs as a public function. This is a simple utility function that doesn't require a new module. The function will be straightforward: pub fn square(n: i32) -> i32 { n * n }",
    "files_to_modify": [{"path": "src/lib.rs", "action": "modify", "description": "Add the square function as a public function in the library root"}],
    "acceptance_criteria": [
        "Function square(n: i32) -> i32 exists and is publicly accessible",
        "square(2) returns 4",
        "square(0) returns 0",
        "square(-3) returns 9",
        "square(10) returns 100",
        "Code compiles without errors (cargo build passes)"
    ],
    "constraints": [
        "Must use i32 as specified in the task",
        "Must handle negative numbers correctly (square of negative is positive)",
        "Must not overflow for valid i32 inputs (overflow is expected for large inputs, per i32 semantics)"
    ],
    "estimated_complexity": "low"
}, indent=2)

t = agent_block(PLANNER_START, PLANNER_DONE, BLUE, "◆", "Planner",
    spec_json, 63457, 707,
    "Spec: 1 file to modify · Add square(n: i32) -> i32 to src/lib.rs")

# ── Coder ───────────────────────────────────────────────────────────────
diff = """diff --git a/src/lib.rs b/src/lib.rs
index 22c7571..951a35e 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -82,3 +82,17 @@ pub fn resolve_asset(rel: &str) -> std::path::PathBuf {
     }
     std::path::PathBuf::from(rel)
 }
+
+/// Returns the square of the given integer.
+///
+/// # Examples
+///
+/// ```
+/// assert_eq!(square(2), 4);
+/// assert_eq!(square(0), 0);
+/// assert_eq!(square(-3), 9);
+/// assert_eq!(square(10), 100);
+/// ```
+pub fn square(n: i32) -> i32 {
+    n * n
+}"""

t = agent_block(CODER_START, CODER_DONE, PURPLE, "⚡", "Coder",
    diff, 64302, 520,
    "Changed 1 file · src/lib.rs [modified]")

# ── Tester ──────────────────────────────────────────────────────────────
testlog = """     Running unittests src/lib.rs
running 8 tests
test tests::test_square_positive ... ok
test tests::test_square_zero ... ok
test tests::test_square_negative ... ok
test tests::test_square_larger_positive ... ok
test tests::test_square_public_accessibility ... ok
test tests::test_square_i32_type ... ok
test tests::test_square_negative_one ... ok
test tests::test_square_one ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured
running 3 tests
test tests::test_square_large_positive ... ok
test tests::test_square_overflow_behavior ... ok
test tests::test_cargo_build ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured
   Doc-tests running 4 tests
test src/lib.rs - square (line 87) ... ok
test src/lib.rs - square (line 91) ... ok
test src/lib.rs - square (line 95) ... ok
test src/lib.rs - square (line 99) ... ok

test result: ok. 4 passed; 0 failed; 0 ignored"""

t = agent_block(TESTER_START, TESTER_DONE, GREEN, "●", "Tester",
    testlog, 63621, 1589,
    "11/11 tests passed · 100% coverage")

# ── Red ──────────────────────────────────────────────────────────────────
red = """Red agent — independent adversarial probe (saw only spec + diff + tests)

R1 [Major · SpecDeviation] ✓ upheld? no, Refuted
  The constraint 'Must not overflow for valid i32 inputs' is violated for all
  inputs with absolute value > 46340 (including i32::MAX and i32::MIN), yet the
  parenthetical calls this 'expected behavior'.
  → Refuted: the spec explicitly states overflow is expected for i32; plain n*n
    is exactly what was requested. No spec deviation.

R2 [Major · TestGap] ✓ Upheld
  No test explicitly exercises square(i32::MIN) (-2147483648), the most dangerous
  input because its absolute value exceeds i32::MAX.
  → Add `#[test] fn test_square_i32_min() { let r = square(i32::MIN); /* panic
    in debug, wrap in release */ }`

R3 [Major · TestGap] ✓ Upheld
  'test_square_overflow_behavior' passes but its assertions are not visible —
  may not validate the debug-vs-release divergence the tester notes identify.
  → Inspect and strengthen using `#[cfg(debug_assertions)]` / `catch_unwind`.

R4 [Minor · Logic] ✓ Upheld
  Doc examples only cover inputs in [-46340, 46340] (2,0,-3,10), so `cargo test
  --doc` cannot catch regressions in debug-mode panic behavior.

R5 [Nit · Style] ✓ Upheld
  The function lacks a `# Panics` section — standard practice for public APIs
  that can panic (debug mode for |n| > 46340)."""

t = agent_block(RED_START, RED_DONE, PINK, "✗", "Red",
    red, 65070, 2574,
    "5 challenge(s) · 1 refuted, 4 upheld", is_red=True)

# ── Reviewer ─────────────────────────────────────────────────────────────
review = """Reviewer verdict: APPROVED

Quality scores:
  correctness      10/10
  code_quality      9/10
  test_coverage     7/10
  spec_adherence   10/10

Overall: The implementation correctly satisfies all acceptance criteria:
  square(2)=4, square(0)=0, square(-3)=9, square(10)=100 — all pass, the
  function is public, compiles without errors, and uses i32 as specified.

Red reconciliation:
  R1 Refuted   — spec explicitly accepts i32 overflow semantics; no deviation.
  R2 Upheld    — test gap for i32::MIN; improvement, not a defect.
  R3 Upheld    — overflow-test assertions not visible; improvement, not a defect.
  R4 Upheld    — doc examples don't exercise overflow path; improvement.
  R5 Upheld    — missing `# Panics` doc section; nit.

Guidance: the upheld challenges are quality improvements to test coverage and
documentation, not bugs. The implementation matches the specification exactly.

Changes requested: None · Verdict: APPROVED"""

t = agent_block(REVIEWER_START, REVIEWER_DONE, AMBER, "◆", "Reviewer",
    review, 66526, 2679,
    "Approved · correctness 10/10 · quality 9/10 · coverage 7/10")

# ── Completion screen ────────────────────────────────────────────────────
completion = f"""{GREEN}{bold()}✨ {bold()}Task Completed Successfully{reset()}

   {DIM}Task ID:{reset()}     edb05249-be9c-48ac-9fa9-06051d8f472e
   {DIM}Revisions:{reset()}   0
   {DIM}Branch:{reset()}    {WHITE}niki/edb05249{reset()}
   {DIM}Patch:{reset()}     {DIM}.niki/tasks/edb05249.../changes.patch{reset()}
   {DIM}Report:{reset()}    {DIM}.niki/tasks/edb05249.../report.md{reset()}

   {DIM}Tokens:{reset()}    322,976 in / 8,069 out (331.0k tok) · 212.1s · cost n/a for model

   {DIM}Next:{reset()}     {WHITE}git checkout niki/edb05249{reset()}   {DIM}(review the branch){reset()}
"""
emit(COMPLETION_T, completion)

# ── Write cast ───────────────────────────────────────────────────────────
header = json.dumps({
    "version": 2, "width": W, "height": H,
    "timestamp": TS0,
    "env": {"SHELL": "/bin/bash", "TERM": "xterm-256color", "COLORTERM": "truecolor"},
    "title": "niki-demo-square"
}, separators=(",", ":"))

with open(OUT, "w") as f:
    f.write(header + "\n")
    for (ts, kind, data) in events:
        rec = json.dumps([ts, kind, data], ensure_ascii=False)
        f.write(rec + "\n")

print(f"wrote {OUT}: {len(events)} events, final t={round(events[-1][0],1)}s")
