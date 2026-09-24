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

pass_check() { echo -e "[${GREEN}PASS${NC}] $1"; }
fail_check() { echo -e "[${RED}FAIL${NC}] $1"; exit 1; }
skip_check() { echo -e "[${YELLOW}SKIP${NC}] $1"; }

export NIKI_BIN="${NIKI_BIN:-$PWD/target/release/niki}"

# Layer A: Static checks
echo '--- Static Checks ---'
cargo fmt --check >/dev/null 2>&1 || fail_check 'Formatting'
pass_check 'Formatting'

cargo clippy --all-targets -- -D warnings >/dev/null 2>&1 || fail_check 'Clippy'
pass_check 'Clippy'

# Layer B & C: Unit and Integration tests
echo '--- Unit & Integration Tests ---'
cargo test --quiet || fail_check 'Unit & Integration tests'
pass_check 'Unit tests'
pass_check 'Integration tests'

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
echo -e "${GREEN}RESULT: READY${NC}"
