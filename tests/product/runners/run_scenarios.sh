#!/usr/bin/env bash
set -eo pipefail

echo '===================================='
echo 'NIKI PRODUCT ACCEPTANCE SCENARIOS'
echo '===================================='

GREEN='[0;32m'
RED='[0;31m'
NC='[0m'

pass_test() { echo -e "[${GREEN}PASS${NC}] $1"; }
fail_test() { echo -e "[${RED}FAIL${NC}] $1"; exit 1; }

# The integrity assertions, shared with run_scenarios_selftest.sh.
# shellcheck source=lib_integrity.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib_integrity.sh"

NIKI_BIN="${NIKI_BIN:-./target/release/niki}"

# The run happens inside a scratch directory (see `cd "$TEST_DIR"` below), so a
# relative binary path stops resolving the moment we get there. CI and
# product-verify.sh both export an absolute path, which hid this; running the
# script directly — the way a contributor does — died with a bare
# "No such file or directory" on a file that was sitting right there.
case "$NIKI_BIN" in
    /*) ;;
    *) NIKI_BIN="$PWD/$NIKI_BIN" ;;
esac

if [ ! -x "$NIKI_BIN" ]; then
    echo "Error: Niki binary not found at $NIKI_BIN"
    echo "Build it first:  cargo build --release"
    echo "or point at one:  NIKI_BIN=/path/to/niki $0"
    exit 1
fi

export PROJECT_ROOT="$PWD"
export TEST_DIR="$(mktemp -d)"
echo "Using test directory: $TEST_DIR"

# Hermetic environment.
#
# `NikiConfig::load` (src/config/types.rs:1737) merges a global
# `~/.config/niki/niki.toml` *under* the project one, so anything a developer has
# in their personal config — a base_url, a default model, a saved theme — is
# merged in before the run starts. That makes this script's verdict a function
# of the machine as well as the repository: a developer's personal config can
# turn a green acceptance run red, and the failure reads as a product bug.
#
# The same class of leak the journey suite calls out in tests/journeys.rs
# ("a journey exercises a first-run machine rather than inheriting whatever the
# test runner happens to export"). HOME is redirected into the scratch dir so
# the global config cannot be found, and the run reads only what this repository
# ships.
export HOME="$TEST_DIR/.home"
mkdir -p "$HOME"
unset XDG_CONFIG_HOME
unset ANTHROPIC_BASE_URL OPENAI_BASE_URL GOOGLE_BASE_URL
unset ANTHROPIC_MODEL OPENAI_MODEL GOOGLE_MODEL

cleanup() {
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

# Basic E2E Task (Mocked LLM)
echo 'Running Agent workflow E2E...'

# Ensure the mock LLM is available
if [ -f tests/integration/mock_llm.py ]; then
    python3 tests/integration/mock_llm.py &
    MOCK_PID=$!

    # Wait for mock to be ready
    for i in {1..20}; do
        if curl -s http://localhost:8080/health >/dev/null 2>&1; then
            break
        fi
        sleep 0.5
    done

    # Run a test task
    cd "$TEST_DIR"
    git init >/dev/null
    git config user.email "test@niki.dev"
    git config user.name "Test"
    echo 'console.log("hello");' > index.js
    git add . && git commit -m "initial" >/dev/null

    # The starting point, recorded BEFORE the run. NIKI checks out the branch it
    # creates, so reading `HEAD` afterwards gives the new branch's own commit and
    # every "did anything change?" comparison below silently compares a commit to
    # itself — which is how a check that never fails gets written.
    START_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
    BASE_REF="$(git rev-parse HEAD)"
    START_BRANCH_SHA="$BASE_REF"

    # Create config for mock provider
    cp "$PROJECT_ROOT/tests/integration/niki.test.toml" "$TEST_DIR/niki.toml"

    # Execute Nikki
    export ANTHROPIC_API_KEY="mock-key"
    export OPENAI_API_KEY="mock-key"
    $NIKI_BIN run 'Add a /health endpoint to this application' --backend worktree --quiet --project "$TEST_DIR" > "$TEST_DIR/niki_out.log" 2>&1 || {
        cat "$TEST_DIR/niki_out.log"
        kill $MOCK_PID
        fail_test 'Agent workflow E2E failed to execute successfully'
    }

    # Verification
    if [ ! -d "$TEST_DIR/.niki" ]; then
        kill $MOCK_PID
        fail_test 'No .niki artifacts directory created'
    fi

    # ── Git / worktree integrity ────────────────────────────────────────
    #
    # This block used to be a bare `pass_test 'Git/worktree integrity'` with no
    # assertion anywhere near it. It reported a pass for a property nothing
    # measured, which is the failure mode this repository's own rule names: a
    # check that has only ever been green is not known to be a check.
    #
    # The assertions live in lib_integrity.sh so that
    # run_scenarios_selftest.sh can run exactly these checks against
    # deliberately broken repositories and assert each one goes red. Do not
    # inline them back into this file — the selftest is the thing that makes the
    # gate trustworthy, and inlining would silently disown it.
    #
    # What the product promises — README "Output is a git branch", and that
    # committed branches are never rewritten — is only true if all of this holds.
    if INTEGRITY_SUMMARY="$(assert_git_integrity "$TEST_DIR" "$BASE_REF" "$START_BRANCH" "$START_BRANCH_SHA" 2>&1)"; then
        pass_test "Git/worktree integrity (${INTEGRITY_SUMMARY#INTEGRITY OK: })"
    else
        echo "$INTEGRITY_SUMMARY"
        kill $MOCK_PID
        fail_test 'Git/worktree integrity'
    fi

    kill $MOCK_PID
    wait $MOCK_PID 2>/dev/null || true
    pass_test 'Agent workflow E2E'
else
    # A missing mock is a broken repository, not an environment fact: the file
    # is tracked in git. Exiting 0 here would let the entire product E2E vanish
    # from CI and still read as a pass.
    echo "Error: tests/integration/mock_llm.py is missing (it is tracked in git)."
    fail_test 'Mock LLM not found — the product E2E cannot run and must not report success'
fi
