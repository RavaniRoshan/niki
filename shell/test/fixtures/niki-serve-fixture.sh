#!/bin/sh
# Runs the engine's scripted runtime. The `fixture-runtime` feature is off by default and its
# module carries a `compile_error!` in release builds, so this script can only replay against a
# binary that was deliberately built with the feature on.
exec "$(dirname "$0")/../../target/debug/niki" serve --fixture "$@"
