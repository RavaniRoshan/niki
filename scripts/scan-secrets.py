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
           "nvapi-2XcDwy…"   # first half,
           "dU8Up1X9…"   # second half,
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


def splice_literals(text: str, max_gap: int = 200) -> str:
    """Concatenate the contents of *nearby* string literals.

    A secret is written as two adjacent literals — the real one was
    `"nvapi-2XcDwy…",` / `"dU8Up1X9…"` on consecutive lines, about 20 characters
    apart — so joining a bounded window catches it.

    An earlier version joined **every** literal in the file with nothing
    between them. That found the key, and it found itself: in
    `src/cli/doctor.rs` it joined the corpus canary `"sk-ant-api03-AAAAAA"` to a
    backticked commit id in a doc comment a thousand characters later, and
    reported

        sk-ant-api03-AAAAAA<commit-id>nvapi-<fn>NAME

    as a credential — one real prefix, a git sha, a function name and a label,
    spliced from three unrelated literals. Written out in full that example
    would itself match this scanner's `sk-ant-` rule, so it is elided here.

    Beyond `max_gap` the run is broken with a newline, which no credential shape
    can cross. Erring wide: a secret split by more than 200 characters of source
    is not something anyone writes.
    """
    out: list[str] = []
    prev_end: int | None = None
    for m in LITERAL.finditer(text):
        if prev_end is not None and m.start() - prev_end > max_gap:
            out.append("\n")
        out.append(m.group(0)[1:-1])
        prev_end = m.end()
    return "".join(out)


def scan_text(text: str) -> list[tuple[str, str, int]]:
    """Return `[(pattern_name, matched_prefix, line)]` for one blob."""
    findings: list[tuple[str, str, int]] = []
    seen: set[tuple[str, str]] = set()

    for pass_name, blob in (("raw", text), ("spliced", splice_literals(text))):
        for literal in KNOWN_SAFE_LITERALS:
            blob = blob.replace(literal, "[known-safe-fixture]")
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

# Values we author that are credential-*shaped* and are not credentials.
#
# Value-level on purpose. An earlier version allowlisted whole *files* — which
# meant `src/cli/doctor.rs` was unscannable in the tree, and a real key pasted
# into the redaction corpus would have been invisible to both passes. Naming
# the literals instead means a real key in that file still trips the scan; only
# the shapes we wrote are exempt.
#
# Every entry is a fixture from `redaction_corpus()` in `src/cli/doctor.rs`, or
# a canary the test suite also quotes in `BLOCKERS.md` and `EVIDENCE.md` — which
# is why the *history* scan needs them: `tests/` is excluded by path, but a doc
# that quotes a fixture is not.
#
# Fixtures that live **only** under `tests/` are deliberately absent. Exempting
# them here would make `tests/secret_scan_can_fail.rs` vacuous: it asserts the
# scanner finds canaries in the test suite, and an allowlist entry containing
# those same canaries means the scanner is only finding them because it was
# told not to.
CORPUS = "redaction_corpus() fixture in src/cli/doctor.rs"

KNOWN_SAFE_LITERALS: dict[str, str] = {
    # Complete runtime values only. Never a fragment.
    #
    # An earlier version of this list included the *halves* the corpus
    # assembles with `splice()`. That was wrong in a way the can-fail test
    # caught: `"AbCdEfGhIjKlMnOpQrSt"` is a substring of a perfectly good
    # Anthropic-shaped key, so exempting it silently disabled detection of any
    # key containing that run — and an allowlist that can hide a real key is
    # worse than no allowlist. The joined form is exempt instead, which cannot
    # mask anything else.
    #
    # `nvapi-A1b2C3d4E5f6G7h8I9j0` is the one half listed on its own, and it is
    # a complete key shape in the source: `nvapi` plus 24 characters, which the
    # rule matches on its own.
    "AIzaSyA0123456789012345678901234567890A": CORPUS,
    "AKIAIOSFODNN7EXAMPLE": CORPUS,
    "ghp_012345678901234567890123456789012345": CORPUS,
    "hf_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789": CORPUS,
    "nvapi-A1b2C3d4E5f6G7h8I9j0": "complete NVIDIA corpus sample, first literal",
    "nvapi-A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8S9t0U1v2W3x4Y5z6": CORPUS,
    "sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA": CORPUS,
    "sk-proj-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA": CORPUS,
    "sk-ant-api03-AAAAAANVIDIAnvapi-ROTATEDROTATED2XcDwyksXdofV7sVL25dBPV2AWS": (
        "NOT a fixture and NOT a credential: an artefact of the spliced pass "
        "over historical revisions of src/cli/doctor.rs, where this scanner's "
        "own incident report sat next to the corpus labels. Assembled from a "
        "corpus canary, the label \"NVIDIA\", the redaction placeholder, and "
        "the tail of the key's first half inside a doc comment. Listed as one "
        "complete string so the exemption cannot mask a different key."
    ),
    "sk-canary0123456789abcdefghijklmnop": (
        "canary in tests/providers_catalogue_echo.rs and "
        "tests/pipe_to_early_reader.rs — quoted in the reports that cite them"
    ),
}


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
        # The value-level KNOWN_SAFE_LITERALS strip already ran inside
        # `scan_text`, so the redaction corpus and the test canaries are gone
        # from `blob` before anything is matched — in the tree pass *and* in
        # the history pass, on purpose. An earlier version exempted whole
        # files here, which would have hidden the real key: it lived in
        # exactly the file that was exempt.
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
            # The full match is never printed. A scanner that echoes the
            # secret it found has just leaked it into CI logs.
            print(f"{path}:{line}: {name}: {preview}")
            total += 1

    print(f"scanned {len(paths)} path(s); {total} finding(s)", file=sys.stderr)
    return 1 if total else 0


if __name__ == "__main__":
    sys.exit(main())
