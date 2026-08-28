#!/usr/bin/env bash
# Render all marketing assets from VHS tapes.
#
# Builds the release binary, then runs each tape in assets/tapes/ and
# verifies the output file exists and is non-zero. Exits non-zero if any
# tape fails.
#
# Usage: scripts/render_assets.sh [tape-name]
#   With no arguments, renders every tape.
#   With an argument, renders only tapes matching that substring.

set -euo pipefail

cd "$(dirname "$0")/.."

# Build the release binary (the tapes invoke it directly).
echo "=== Building release binary ==="
cargo build --release

# Discover tapes.
TAPES=()
for tape in assets/tapes/*.tape; do
    name="$(basename "$tape" .tape)"
    if [ "${1:-}" = "" ] || [[ "$name" == *"$1"* ]]; then
        TAPES+=("$tape")
    fi
done

if [ ${#TAPES[@]} -eq 0 ]; then
    echo "No tapes matched."
    exit 1
fi

echo "=== Rendering ${#TAPES[@]} tape(s) ==="
fail=0
for tape in "${TAPES[@]}"; do
    name="$(basename "$tape" .tape)"
    # Clean up any orphan .png/ directory from a prior run that didn't
    # finish (VHS creates these when the final assembly step fails).
    target="$(grep -E '^Output ' "$tape" | awk '{print $2}')"
    if [ -d "$target" ]; then
        rm -rf "$target"
    fi

    echo "--- $name ---"
    if ! vhs "$tape" >/dev/null 2>&1; then
        echo "  FAILED to record"
        fail=$((fail + 1))
        continue
    fi

    if [ ! -s "$target" ]; then
        echo "  FAILED: $target is empty or missing"
        fail=$((fail + 1))
        continue
    fi

    size=$(stat -c %s "$target" 2>/dev/null || stat -f %z "$target" 2>/dev/null)
    echo "  OK: $target ($size bytes)"
done

if [ $fail -ne 0 ]; then
    echo "=== $fail tape(s) failed ==="
    exit 1
fi

echo "=== All ${#TAPES[@]} tape(s) rendered ==="
