#!/usr/bin/env bash
# The shell suite, on an engine that can actually be driven.
#
# Why this exists: `shell/test/fixture-loop.test.tsx` drives the **real binary** end to end
# through the scripted fixture runtime, which lives behind the `fixture-runtime` cargo feature.
# A plain `cargo build` does not enable it, so `niki serve --fixture` does not exist, the probe
# at the top of that file returns false, and the three tests skip. The suite then reports green
# with the only leg that exercises the engine missing.
#
# So the build is part of the test command, not a thing you were supposed to remember.
#
#   scripts/test-shell.sh                 # the whole suite
#   scripts/test-shell.sh test/pty.test.ts
#
# `fixture-runtime` carries a `compile_error!` in non-debug builds, so this is a debug build by
# construction and can never produce a shipped binary that replays a script instead of running
# the engine.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

JOBS="${NIKI_TEST_JOBS:-2}"

if [ "${NIKI_SKIP_ENGINE_BUILD:-0}" = "1" ]; then
    :
else
    echo "building the engine with the fixture runtime (-j $JOBS)…"
    CARGO_BUILD_JOBS="$JOBS" cargo build -j "$JOBS" --features fixture-runtime || {
        echo "the engine build failed; the shell suite would skip its real-binary leg" >&2
        exit 1
    }
fi

if [ ! -x target/debug/niki ]; then
    echo "target/debug/niki is missing after the build" >&2
    exit 1
fi
if ! ./target/debug/niki serve --fixture --help 2>&1 | grep -q -- '--fixture'; then
    echo "target/debug/niki has no --fixture: the build did not enable fixture-runtime," >&2
    echo "so shell/test/fixture-loop.test.tsx would skip instead of running." >&2
    exit 1
fi

echo "fixture runtime: present"
cd shell
exec npx vitest run --reporter=dot "$@"