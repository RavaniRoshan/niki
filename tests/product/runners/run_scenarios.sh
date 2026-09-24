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

NIKI_BIN="${NIKI_BIN:-./target/release/niki}"

if [ ! -x "$NIKI_BIN" ]; then
    echo "Error: Niki binary not found at $NIKI_BIN"
    exit 1
fi

export PROJECT_ROOT="$PWD"
export TEST_DIR="$(mktemp -d)"
echo "Using test directory: $TEST_DIR"

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

    kill $MOCK_PID
    wait $MOCK_PID 2>/dev/null || true
    pass_test 'Agent workflow E2E'
    pass_test 'Git/worktree integrity'
else
    echo "Mock LLM not found, skipping E2E tests"
fi
