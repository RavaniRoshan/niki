#!/usr/bin/env bash
# Regenerate the demo-state fixture used by capture / docs tests.
#
# Usage: scripts/dump_demo_state.sh
#
# Builds the release binary (if needed) and writes the JSON snapshot of
# the canonical demo state to tests/fixtures/demo_state.json. Diff the
# output against git to spot unintended changes.

set -euo pipefail
cd "$(dirname "$0")/.."

mkdir -p tests/fixtures

# Build only if the binary is older than the source tree.
if [[ ! -x target/release/niki ]] || \
   [[ -n "$(find src/display/capture.rs -newer target/release/niki 2>/dev/null | head -1)" ]]; then
    cargo build --release
fi

./target/release/niki run 'Add a /health endpoint' \
    --backend worktree --tui --demo --dump-state \
    > tests/fixtures/demo_state.json

echo "Wrote tests/fixtures/demo_state.json ($(wc -c < tests/fixtures/demo_state.json) bytes)"
