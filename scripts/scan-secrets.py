#!/usr/bin/env python3
"""Scan tracked files for credential-shaped strings, including split ones.

The G5 check this replaces used one `grep -E` over the raw bytes of each file.
That has two holes, and a live NVIDIA API key walked through both of them:

1. **No vendor coverage.** The pattern listed `sk-ant-`, `sk-`, `ghp_` and
   `AKIA`. An `nvapi` key — a whole provider, one this project explicitly
   supports — matched nothing.

2. **Split literals.** The key was committed as two adjacent string
   literals, concatenated at runtime:

       splice(&[
           "nvapi-ROTATED",
           "ROTATED",
       ])

   No regex over raw bytes can see that. The two halves are individually too
   short to match, and joining them is the compiler's job, not the scanner's.

So this scans each file twice: once raw, and once with every string literal's
contents spliced together in source order. The second pass is the one that
sees a split secret.

## Why this file exists rather than a grep line

Because a gate that has only ever been green is not known to be a gate.
`tests/secret_scan_can_fail.rs` feeds this scanner a known credential, split
across two literals exactly the way the real one was, and asserts it is
found — so the check is known to be able to fail.

## Usage

    scripts/scan-secrets.py                 # scan tracked files
    scripts/scan-secrets.py --stdin         # read the blob to test from stdin
    scripts/scan-secrets.py --path FILE     # scan one path, for the tests

Exit 0 = clean. Exit 1 = at least one finding, printed to stdout as
`path:line: <matched-prefix>` with the match itself never echoed in full.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys

# Vendor prefixes, each with the character class that follows it. Lengths are
# set to the real shapes: short enough that a word cannot match, long enough
# that a truncated paste does not either.
PATTERNS: dict[str, re.Pattern[str]] = {
    "openai/anthropic (sk-)": re.compile(r"\bsk-ant-[A-Za-z0-9_-]{20,}"),
    "openai (sk-)": re.compile(r"\bsk-[A-Za-z0-9]{32,}"),
    "openai project (sk-proj-)": re.compile(r"\bsk-proj-[A-Za-z0-9_-]{20,}"),
    "openrouter (sk-or-v1-)": re.compile(r"\bsk-or-v1-[a-f0-9]{32,}"),
    "nvidia (nvapi-)": re.compile(r"\bnvapi-[A-Za-z0-9_-]{20,}"),
    "github (ghp_/gho_/ghs_)": re.compile(r"\bgh[pos]_[A-Za-z0-9]{30,}"),
    "aws (AKIA)": re.compile(r"\bAKIA[0-9A-Z]{16}"),
    "google (AIza)": re.compile(r"\bAIza[A-Za-z0-9_-]{35}"),
    "huggingface (hf_)": re.compile(r"\bhf_[A-Za-z0-9]{34,}"),
    "stripe (sk_live_)": re.compile(r"\bsk_live_[A-Za-z0-9]{16,}"),
    "slack (xox)": re.compile(r"\bxox[baprs]-[A-Za-z0-9-]{10,}"),
}

# Any string literal. Deliberately permissive: a false positive here costs a
# re-scan, and a false negative costs a credential.
LITERAL = re.compile(r"""(?:"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`(?:[^`\\]|\\.)*`)""", re.S)


def splice_literals(text: str) -> str:
    """Return every string literal's contents, concatenated in source order.

    Not "replace each literal with its contents" — that keeps the source text
    *between* literals, so two literals on separate lines stay separated by a
    newline and the credential is still broken across the join. That is
    exactly the shape this exists to catch, so the text between them has to go
    too.

    Joining every literal with no separator is deliberately more aggressive
    than the compiler. `splice(&["nvapi-2Xc…", "dU8Up…"])` is an array, and
    nothing concatenates those at compile time; the runtime does. A scanner
    that only modelled literal adjacency would miss it. False positives here
    cost an allowlist entry; false negatives cost a credential, so this errs
    hard toward reporting.
    """
    return "".join(m.group(0)[1:-1] for m in LITERAL.finditer(text))


def scan_text(text: str) -> list[tuple[str, str, int]]:
    """Return `[(pattern_name, matched_prefix, line)]` for one blob."""
    findings: list[tuple[str, str, int]] = []
    seen: set[tuple[str, str]] = set()

    for pass_name, blob in (("raw", text), ("spliced", splice_literals(text))):
        for name, pattern in PATTERNS.items():
            for match in pattern.finditer(blob):
                matched = match.group(0)
                if (name, matched) in seen:
                    continue
                seen.add((name, matched))
                line = blob.count("\n", 0, match.start()) + 1
                suffix = f" (as {pass_name} text)" if pass_name == "spliced" else ""
                findings.append((f"{name}{suffix}", matched[:12] + "…", line))
    return findings


def tracked_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [p for p in out.split("\0") if p]


# Directories whose contents are credential-shaped *by design*: the redaction
# corpus the binary needs at runtime, and test fixtures. `tests_legacy/` is
# the path `tests/` had before a rename, so it appears in older diffs and
# would otherwise be the only thing the history scan reports.
EXCLUDED_PREFIXES = (
    "tests/",
    "tests_legacy/",
    "target/",
    ".git/",
    ".evidence/",
    ".odw/",
    "examples/",
)

# Individual files allowed to hold a credential-shaped string, each with the
# reason it cannot be moved out. This is deliberately a per-file list rather
# than a per-line one: a finer allowlist invites the kind of edit nobody
# re-reads, and one whole file is a boundary a human can actually check.
#
# `tests/test_secret_scan_can_fail.rs` pins this list's exact contents, so
# adding an entry here fails a test that has to be updated deliberately.
ALLOWED_FILES: dict[str, str] = {
    "src/cli/doctor.rs": (
        "redaction_corpus() — niki doctor runs this at runtime to check that "
        "provider keys are masked, so the shape has to exist in the binary. "
        "The values are fabricated; one of them was a real key until the "
        "split-literal scan below caught it."
    ),
}


def allowed_reason(path: str) -> str | None:
    return ALLOWED_FILES.get(path)


def scan_diff(diff: str) -> list[tuple[str, str, str]]:
    """Scan the *added* lines of a unified diff, honouring the file path.

    Scanning raw history with no path context reports every fixture in
    `tests/`, which holds credential-shaped strings on purpose. A gate that is
    always red gets switched off, and then it protects nothing — which is what
    the previous one did.

    So each hunk is attributed to its file from the `+++ b/<path>` header,
    excluded paths are skipped, and allowlisted files are reported rather than
    counted.
    """
    findings: list[tuple[str, str, str]] = []
    path = ""
    added: list[str] = []

    def flush() -> None:
        if not added:
            return
        blob = "\n".join(added)
        # Deliberately **not** consulting ALLOWED_FILES here. The allowlist
        # says "this file holds a fabricated shape *now*", which is true after
        # the fix and was false before it — and the real key is in exactly that
        # file's history. Applying it here would hide the finding the history
        # scan exists to produce.
        if path and not path.startswith(EXCLUDED_PREFIXES):
            for name, preview, _ in scan_text(blob):
                findings.append((path, name, preview))
        added.clear()

    for line in diff.splitlines():
        if line.startswith("+++ "):
            flush()
            path = line[4:].strip()
            # `git diff` writes `+++ b/<path>`; the prefix is not part of the
            # path, and leaving it on makes every exclusion silently fail.
            if path.startswith(("b/", "a/")):
                path = path[2:]
            if path == "/dev/null":
                path = ""
            continue
        if line.startswith("--- ") or line.startswith("diff --git"):
            continue
        if line.startswith("@@"):
            flush()
            continue
        if line.startswith("+") and not line.startswith("+++"):
            added.append(line[1:])
        elif line.startswith(" "):
            continue
    flush()
    return findings


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stdin", action="store_true", help="scan the blob on stdin")
    parser.add_argument(
        "--diff",
        action="store_true",
        help="scan a unified diff on stdin: added lines only, path-aware",
    )
    parser.add_argument("--path", help="scan this one path instead of the tracked set")
    args = parser.parse_args()

    if args.diff:
        findings = scan_diff(sys.stdin.read())
        for path, name, preview in findings:
            print(f"{path}: {name}: {preview}")
        return 1 if findings else 0

    if args.stdin:
        findings = scan_text(sys.stdin.read())
        for name, preview, line in findings:
            print(f"<stdin>:{line}: {name}: {preview}")
        return 1 if findings else 0

    paths = [args.path] if args.path else tracked_files()
    total = 0
    for path in paths:
        if not args.path and path.startswith(EXCLUDED_PREFIXES):
            continue
        try:
            with open(path, encoding="utf-8", errors="replace") as handle:
                text = handle.read()
        except (OSError, IsADirectoryError):
            continue
        for name, preview, line in scan_text(text):
            reason = allowed_reason(path)
            if reason is not None:
                print(
                    f"{path}:{line}: allowed — {name}: {preview}\n"
                    f"    (allowlisted: {reason})",
                    file=sys.stderr,
                )
                continue
            # The full match is never printed. A scanner that echoes the
            # secret it found has just leaked it into CI logs.
            print(f"{path}:{line}: {name}: {preview}")
            total += 1

    print(f"scanned {len(paths)} path(s); {total} finding(s)", file=sys.stderr)
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main())
