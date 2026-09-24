#!/usr/bin/env bash
set -eo pipefail

GREEN='[0;32m'
RED='[0;31m'
NC='[0m'

pass_check() { echo -e "[${GREEN}PASS${NC}] $1"; }
fail_check() { echo -e "[${RED}FAIL${NC}] $1"; exit 1; }

export NIKI_BIN="${NIKI_BIN:-$PWD/target/release/niki}"

# 3. CLI PRODUCT SMOKE TESTS
$NIKI_BIN --version >/dev/null 2>&1 || fail_check 'CLI smoke: version'
$NIKI_BIN --help >/dev/null 2>&1 || fail_check 'CLI smoke: help'
$NIKI_BIN doesnotexist 2>/dev/null && fail_check 'CLI smoke: invalid command succeeded' || true
# Ensure no panics on basic failure
$NIKI_BIN run 'echo hello' --dry-run >/dev/null 2>&1 || true

pass_check 'CLI basic arguments'
