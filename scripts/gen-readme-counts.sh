#!/usr/bin/env bash
# Keep hand-typed test counts out of the README.
#
#   scripts/gen-readme-counts.sh          # print the measured counts
#   scripts/gen-readme-counts.sh --check  # exit non-zero if the README states one
#
# Why this exists: the README said "946 unit tests · ~540 integration tests across 53 binaries"
# for several releases while the real numbers were 1129, ~1100 and 131. Nothing compared them, so
# nothing noticed. This repository already has a badge in that shape — `tests/claims.rs` exists
# because a status badge stayed green while describing a version that no longer existed.
#
# **The README does not state a count, and this is why.** A number here can only be right if
# something regenerates it on every change, and the two obvious candidates both lie:
#
#   * counting declarations gives 360 shell tests against 418 that actually run, because some are
#     generated inside loops and `.each` blocks;
#   * counting a run records one machine on one day and goes stale the moment a test is added.
#
# So the gate's job is the negative: **a hand-written count may not come back.** The measured
# values are still printed, for anyone who wants them, and they are labelled as what they are.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

CHECK=0
[ "${1:-}" = "--check" ] && CHECK=1

count() { grep -rhoaE "$1" "${@:2}" 2>/dev/null | wc -l | tr -d ' '; }

UNIT="$(count '#\[(tokio::)?test' src --include='*.rs')"
INTEGRATION="$(count '#\[(tokio::)?test' tests --include='*.rs')"
BINARIES="$(find tests -maxdepth 1 -name '*.rs' | wc -l | tr -d ' ')"
SHELL_FILES="$(find shell/test -maxdepth 1 \( -name '*.ts' -o -name '*.tsx' \) | wc -l | tr -d ' ')"
SHELL_DECLARED="$(grep -rhoaE '^[[:space:]]*(it|test)(\.[a-zA-Z]+)*\(' shell/test --include='*.ts' --include='*.tsx' | wc -l | tr -d ' ')"
CANARIES="$(grep -c '^\[\[canary\]\]' mutants/canaries.toml 2>/dev/null || true)"
CANARIES="${CANARIES:-0}"

SUMMARY="measured now — ${UNIT} unit, ${INTEGRATION} integration across ${BINARIES} binaries, ${SHELL_DECLARED} shell declared across ${SHELL_FILES} files, ${CANARIES} canaries (declared counts: lower bounds, since some shell tests are generated)"

if [ "$CHECK" -eq 1 ]; then
    # Any of these shapes is a number somebody typed and will forget to update.
    # Only the shapes that are a claim about how big the suite is. "8/8 tests passed" in a sample
    # transcript and "entry points, tests, risk signals" in a description are prose, not counts,
    # and a gate that flags prose trains people to disable gates.
    HITS="$(grep -nE '[0-9,]+ (unit tests|integration tests|shell tests|test binaries|binaries|canaries)' README.md || true)"
    if [ -n "$HITS" ]; then
        echo "README states a test count. Nothing regenerates it, so it will be stale:" >&2
        echo "$HITS" >&2
        echo >&2
        echo "Either drop the number, or point at a script that produces it:" >&2
        echo "  scripts/gen-readme-counts.sh" >&2
        exit 1
    fi
    echo "README states no hand-typed test count."
    exit 0
fi

echo "$SUMMARY"
