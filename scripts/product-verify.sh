#!/usr/bin/env bash
set -eo pipefail

echo '========================================='
echo '        NIKI PRODUCT VERIFICATION        '
echo '========================================='
echo ''

GREEN='[0;32m'
RED='[0;31m'
YELLOW='[1;33m'
NC='[0m'

pass_check() { echo -e "[${GREEN}PASS${NC}] $1"; echo "$1" >>"$PASSED_LEDGER"; }
fail_check() { echo -e "[${RED}FAIL${NC}] $1"; exit 1; }
skip_check() { echo -e "[${YELLOW}SKIP${NC}] $1"; }

# Every layer that must have actually run for this script to mean anything.
#
# This repo has already shipped a CI job that went green while executing
# nothing: a runner script was missing, the `else` branch called skip_check, and
# the job passed. `pass_check` records into a ledger so the end of the script
# can refuse to say READY when a mandatory layer was skipped instead of run.
# The deliberate skips — the static and suite layers, which are the `check` and
# `test` jobs' gate — are not on this list.
MUST_HAVE_RUN=(
    'Release binary built'
    'CLI smoke'
    'TUI PTY smoke'
    'TUI interaction'
    'TUI Bash Smoke'
    'Agent workflow E2E'
)

PASSED_LEDGER="$(mktemp)"
trap 'rm -f "$PASSED_LEDGER"' EXIT

export NIKI_BIN="${NIKI_BIN:-$PWD/target/release/niki}"

# What this script is for: proving the *product* works — the real binary, the
# real TUI, real user journeys. It is not a second copy of the test suite.
#
# It used to re-run `cargo fmt`, `cargo clippy --all-targets` and the whole
# `cargo test` suite first, and CI ran it behind `needs: test` so all of it ran
# twice. In the workflow that made this job the critical path at 14 minutes, of
# which roughly 9 were a verbatim rerun of a `test` job that had already passed
# a few minutes earlier.
#
# The static and suite layers now run here only when asked, via
# NIKI_VERIFY_FULL=1 — which is how you run this script locally, where being
# the only gate is the point. In CI they are skipped, because the `check` and
# `test` jobs are the gates and they run in parallel with this one.
#
# Skipping is reported, never silent: a skipped layer prints SKIP, and a
# --check that finds nothing to do still fails. The failure mode this repo has
# already shipped — a CI job that goes green while executing nothing — is
# guarded against at the end of this file.
if [ "${NIKI_VERIFY_FULL:-0}" = "1" ]; then
    echo '--- Static Checks (NIKI_VERIFY_FULL=1) ---'
    cargo fmt --check >/dev/null 2>&1 || fail_check 'Formatting'
    pass_check 'Formatting'

    cargo clippy --all-targets -- -D warnings >/dev/null 2>&1 || fail_check 'Clippy'
    pass_check 'Clippy'

    echo '--- Unit & Integration Tests (NIKI_VERIFY_FULL=1) ---'
    cargo test --quiet || fail_check 'Unit & Integration tests'
    pass_check 'Unit tests'
    pass_check 'Integration tests'
else
    skip_check 'Formatting (run by the CI `check` job; set NIKI_VERIFY_FULL=1 to run here)'
    skip_check 'Clippy (run by the CI `check` job; set NIKI_VERIFY_FULL=1 to run here)'
    skip_check 'Unit & integration tests (run by the CI `test` job; set NIKI_VERIFY_FULL=1 to run here)'
fi

# Build release binary for subsequent tests
echo '--- Building Release Binary ---'
cargo build --release --quiet || fail_check 'Release build'
pass_check 'Release binary built'

# 3. CLI PRODUCT SMOKE TESTS
echo '--- CLI Smoke Tests ---'
if [ -f tests/product/runners/cli_smoke.sh ]; then
    bash tests/product/runners/cli_smoke.sh || fail_check 'CLI smoke'
    pass_check 'CLI smoke'
else
    skip_check 'CLI smoke (runner not found)'
fi

# 4-7. REAL TUI / PTY TESTING
echo '--- TUI / PTY Smoke Tests ---'
if command -v pytest >/dev/null 2>&1 && [ -f tests/headless_tui.py ]; then
    python3 -m pytest -c pytest_headless.ini -v tests/headless_tui.py >/dev/null 2>&1 || fail_check 'TUI PTY smoke'
    pass_check 'TUI PTY smoke'
    pass_check 'TUI interaction'
else
    skip_check 'TUI PTY smoke (pytest not found)'
fi

if [ -f tests/tui_smoke/run.sh ]; then
    bash tests/tui_smoke/run.sh --bin "$NIKI_BIN" >/dev/null 2>&1 || fail_check 'TUI Bash Smoke'
    pass_check 'TUI Bash Smoke'
else
    skip_check 'TUI Bash Smoke'
fi

# 7. TUI VISUAL REGRESSION
echo '--- Visual Regression ---'
if [ -f tests/visual/run.sh ]; then
    # We allow visual regression to fail gracefully if VHS isn't installed
    if command -v vhs >/dev/null 2>&1; then
        bash tests/visual/run.sh >/dev/null 2>&1 || fail_check 'Visual regression'
        pass_check 'Visual regression'
    else
        skip_check 'Visual regression (VHS not installed)'
    fi
else
    skip_check 'Visual regression'
fi

# 8-23. END-TO-END AGENT TESTS & WORKFLOWS
echo '--- End-to-End Agent Scenarios ---'
if [ -f tests/product/runners/run_scenarios.sh ]; then
    bash tests/product/runners/run_scenarios.sh || fail_check 'Agent workflow E2E'
    pass_check 'Agent workflow E2E'
else
    skip_check 'Agent workflow E2E'
fi

echo ''
# The vacuity gate lives in its own script so a test can *run* it. Inline, the
# only thing a test could assert was that the word MUST_HAVE_RUN appeared in
# this file, which still passed with the recording line deleted.
if ! ./scripts/require-layers.sh "$PASSED_LEDGER" "${MUST_HAVE_RUN[@]}"; then
    echo -e "${RED}RESULT: NOT VERIFIED${NC}"
    exit 1
fi

echo -e "${GREEN}RESULT: READY${NC}"
