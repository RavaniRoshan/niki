#!/usr/bin/env python3
"""Count `todo!` / `unimplemented!` in production code, not in test modules.

Filtering by `grep -v 'cfg(test)'` is the obvious approach and it is wrong: it
drops the attribute's own line and nothing else, so every `unimplemented!()`
inside `mod tests` — which is where a deliberately-unreachable trait stub lives
— counts as a production defect. On this tree that was 3 false positives out
of 3.

So: track the line each `#[cfg(test)] mod ... {` opens and ignore everything
from there to its matching close. Prints the count; the detail is the caller's
job.
"""
import re
import sys
from pathlib import Path

SRC = Path(__file__).resolve().parent.parent / "src"
CALL = re.compile(r"\b(todo!|unimplemented!)\(")
CFG_TEST = re.compile(r"^\s*#\[cfg\(test\)\]")
MOD_OPEN = re.compile(r"^\s*(pub(\([^)]*\))?\s+)?mod\s+\w+")

count = 0
for path in sorted(SRC.rglob("*.rs")):
    lines = path.read_text().splitlines()
    depth = 0          # inside a cfg(test) module
    skip_depth: list[int] = []
    for i, line in enumerate(lines):
        if depth == 0 and CFG_TEST.match(line):
            # Find the `mod` this attribute belongs to and start skipping.
            for j in range(i, min(i + 4, len(lines))):
                if MOD_OPEN.match(lines[j]):
                    depth = 1
                    skip_depth = [j]
                    break
            continue
        if depth:
            depth += line.count("{") - line.count("}")
            if depth <= 0:
                depth = 0
            continue
        if CALL.search(line):
            count += 1
            print(f"  {path.relative_to(SRC.parent)}:{i+1}: {line.strip()[:80]}", file=sys.stderr)

print(count)
