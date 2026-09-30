#!/usr/bin/env bash
# The starter's own invariant, checked from CI.
#
# `niki-starter` exists so a first run has something real to fix. The obvious
# way for this project to "tidy itself up" is for somebody to solve the task —
# and then every subsequent student run starts from green, the Coder has nothing
# to do, and the whole thing quietly stops teaching anything.
#
# So: the starter's suite must fail, and it must fail on /health. Solving the
# task is a regression, and this is what turns it into one.
#
#   bash niki-starter/selftest.sh
#
# Exits non-zero if the starter has been solved or is failing for the wrong
# reason. No NIKI binary, no model, no network.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE" || exit 1

RED=$'\033[0;31m'
GREEN=$'\033[0;32m'
NC=$'\033[0m'

echo '===================================='
echo 'NIKI-STARTER — the task must be unsolved'
echo '===================================='
echo

if ! command -v node >/dev/null 2>&1; then
    echo -e "[${RED}node is not installed${NC}]"
    exit 1
fi

fail=0

# ── 1 · The task suite must fail ────────────────────────────────────────
OUT="$(mktemp)"
trap 'rm -f "$OUT"' EXIT
node --test test/server.test.js >"$OUT" 2>&1
status=$?

if [ "$status" -eq 0 ]; then
    echo -e "[${RED}FAIL${NC}] the starter's tests pass — there is no task left"
    echo
    echo "  Revert the change in src/server.js, or give the starter a new"
    echo "  unsolved requirement. A starter that has been solved is a broken"
    echo "  starter, and the first person to notice will be a student whose"
    echo "  run produces nothing and no explanation."
    fail=1
else
    echo -e "[${GREEN}PASS${NC}] the starter's tests fail, as they must"
fi

# ── 2 · They must fail on /health, not on a typo ────────────────────────
if grep -qi 'health' "$OUT"; then
    echo -e "[${GREEN}PASS${NC}] the failure is about /health"
else
    echo -e "[${RED}FAIL${NC}] the tests fail, but not about /health"
    echo
    echo "  They must fail because the endpoint is unimplemented. A failure"
    echo "  that looks like a broken exercise rather than an unfinished one is"
    echo "  worse than no exercise."
    sed 's/^/    /' "$OUT" | head -20
    fail=1
fi

# ── 3 · The starter's own guard must pass ───────────────────────────────
if node --test test/starter-is-red.test.js >/dev/null 2>&1; then
    echo -e "[${GREEN}PASS${NC}] the in-suite guard agrees"
else
    echo -e "[${RED}FAIL${NC}] test/starter-is-red.test.js failed"
    echo "  That guard is what notices a solved starter, in the environment a"
    echo "  student will actually run it in. It should pass here."
    fail=1
fi

# ── 4 · The docs the starter ships must be present ──────────────────────
for f in README.md HONESTY.md TROUBLESHOOTING.md REPORT-GUIDE.md run.sh niki.toml; do
    if [ -s "$f" ]; then
        echo -e "[${GREEN}PASS${NC}] $f"
    else
        echo -e "[${RED}FAIL${NC}] $f is missing or empty"
        fail=1
    fi
done

# ── 5 · …and present in the repository, not only on this disk ───────────
#
# `niki.toml` was missing from the repository for three commits. The
# repository's root `.gitignore` excludes `niki.toml` everywhere — correctly,
# so a developer's real configuration with their keys can never be committed —
# and it caught the starter's own config too. Every check above still passed,
# because the file was sitting right there on disk. It was only CI, on a clean
# checkout, that reported "niki.toml is missing".
#
# A check that runs against the working tree cannot catch "this file was never
# committed". So this one asks git, which is the only thing that can.
for f in README.md HONESTY.md TROUBLESHOOTING.md REPORT-GUIDE.md run.sh selftest.sh niki.toml package.json src/server.js test/server.test.js; do
    if git ls-files --error-unmatch "$f" >/dev/null 2>&1; then
        echo -e "[${GREEN}PASS${NC}] $f is tracked in git"
    else
        echo -e "[${RED}FAIL${NC}] $f exists on disk but is NOT in the repository"
        echo "  Anyone who clones this project will not have it. This is exactly"
        echo "  what happened to niki.toml: the root .gitignore excludes it"
        echo "  everywhere so a developer's real config cannot be committed, and"
        echo "  the starter's own config was caught by the same rule."
        echo "  Add a negation to niki-starter/.gitignore and 'git add -f' it."
        fail=1
    fi
done

echo
if [ "$fail" -ne 0 ]; then
    echo -e "[${RED}the starter is not in a shippable state${NC}]"
    exit 1
fi
echo -e "[${GREEN}the starter is ready to be handed to someone${NC}]"
