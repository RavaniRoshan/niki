#!/usr/bin/env bash
# Serialized test runner with a RAM preflight.
#
# NIKI's suite has a documented memory ceiling (see AGENTS.md). A bare
# `cargo test` compiles and runs every integration binary with default thread
# counts, which on an 8 GiB-class host OOMs. This script is the sanctioned
# entry point: it runs one binary at a time, caps cargo jobs, and refuses to
# start a heavy binary when free memory is already low.
#
#   ./scripts/test-layer.sh fast          # cheap binaries, may parallelize
#   ./scripts/test-layer.sh heavy         # one at a time, threads=1
#   ./scripts/test-layer.sh heap          # index/KB builders, threads=1
#   ./scripts/test-layer.sh all           # every binary, serialized
#   ./scripts/test-layer.sh run_lifecycle # one named binary
#   ./scripts/test-layer.sh lib           # unit tests only
#
# It works with plain cargo. When cargo-nextest is installed, prefer it — the
# groups in .config/nextest.toml mirror the categories below and give
# per-test process isolation on top.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Never let cargo fan out. Default jobs = nproc, which is what OOMs the box.
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"

# Free RAM (MiB) required before a heavy/heap layer is allowed to start.
# 2600 MB, not 1200: linking a 66k-LOC test binary is the peak allocation in
# this project, and at 1200 MB the check would pass moments before a build
# exhausted the host.
MIN_FREE_MB="${NIKI_MIN_FREE_MB:-2600}"
# When memory is short, wait for it rather than refusing outright. Cargo
# releases its page cache within seconds of a build finishing, so a hard
# refusal fails on a machine that is merely settling.
RECOVER_WAIT_S="${NIKI_RECOVER_WAIT:-180}"

# Binaries that build git fixture repos, worktrees, or measure wall-clock
# render budgets. Concurrent execution starves the timing assertions and
# multiplies peak RSS.
HEAVY=(
  tui_navigation
  tui_perf
  visual_layout_check
  run_lifecycle
  pipeline_guards
  kb_pipeline
  runtime_benchmarks
  worktree_policy
  diff_scope
  sandbox_teardown
  acp_server
  resume_cli
  hooks_lifecycle
  agent_runtime
)

# Binaries that build large in-memory indexes.
HEAP=(
  structural_index
  kb_store
  history_cache
  repo_intel
)

red()   { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
info()  { printf '\033[2m%s\033[0m\n' "$*"; }

free_mb() {
  # Portable across Linux and macOS.
  if command -v free >/dev/null 2>&1; then
    free -m | awk '/^Mem:/ {print $7}'
  else
    vm_stat | awk '/Pages free/ {gsub("\\.", "", $3); print $3/256}'
  fi
}

# Block until free memory clears the threshold, or give up after
# RECOVER_WAIT_S seconds.
wait_for_memory() {
  local need="$1" waited=0 avail
  while :; do
    avail="$(free_mb)"
    [ "${avail:-0}" -ge "$need" ] && return 0
    if [ "$waited" -ge "$RECOVER_WAIT_S" ]; then
      red "only ${avail} MiB free after ${waited}s (need ${need} MiB)"
      return 1
    fi
    [ "$waited" -eq 0 ] && info "waiting for memory: ${avail} MiB free, need ${need} MiB"
    sleep 5
    waited=$((waited + 5))
  done
}

preflight() {
  local need="$1" label="$2" avail
  if ! wait_for_memory "$need"; then
    red "refusing to run the '$label' layer."
    red "Override with NIKI_MIN_FREE_MB=<MiB> if this host is fine."
    return 1
  fi
  info "preflight ok: $(free_mb) MiB available (need ${need} MiB) for '$label'"
  return 0
}

run_binary() {
  local name="$1" threads="${2:-1}"
  info "--- $name (--test-threads=$threads) ---"
  if cargo test --test "$name" -j 2 -- --test-threads="$threads"; then
    green "PASS $name"
    return 0
  fi
  red "FAIL $name"
  return 1
}

in_list() {
  local needle="$1"; shift
  local item
  for item in "$@"; do [ "$item" = "$needle" ] && return 0; done
  return 1
}

run_group() {
  local threads="$1"; shift
  local name failed=0
  preflight "$MIN_FREE_MB" "$1" || return 1
  for name in "$@"; do
    preflight "$MIN_FREE_MB" "$name" || return 1
    run_binary "$name" "$threads" || failed=1
  done
  return $failed
}

layer="${1:-all}"
status=0

case "$layer" in
  lib)
    info "unit tests (--test-threads=1)"
    cargo test --lib -j 2 -- --test-threads=1 || status=1
    ;;
  fast)
    # Every integration binary that is not heavy or heap.
    mapfile -t fast < <(
      for f in tests/*.rs; do
        n="$(basename "$f" .rs)"
        in_list "$n" "${HEAVY[@]}" "${HEAP[@]}" || echo "$n"
      done
    )
    for n in "${fast[@]}"; do run_binary "$n" 1 || status=1; done
    ;;
  heavy)
    run_group 1 "${HEAVY[@]}" || status=1
    ;;
  heap)
    run_group 1 "${HEAP[@]}" || status=1
    ;;
  all)
    # Serialized end to end. This is the gate the release checklist runs.
    cargo test --lib -j 2 -- --test-threads=1 || status=1
    for n in "${HEAVY[@]}" "${HEAP[@]}"; do run_binary "$n" 1 || status=1; done
    mapfile -t fast < <(
      for f in tests/*.rs; do
        n="$(basename "$f" .rs)"
        in_list "$n" "${HEAVY[@]}" "${HEAP[@]}" || echo "$n"
      done
    )
    for n in "${fast[@]}"; do run_binary "$n" 1 || status=1; done
    ;;
  *)
    if [ -f "tests/${layer}.rs" ]; then
      threads=1
      in_list "$layer" "${HEAVY[@]}" "${HEAP[@]}" && preflight "$MIN_FREE_MB" "$layer"
      run_binary "$layer" "$threads" || status=1
    else
      red "unknown layer '$layer'"
      sed -n '3,20p' "$0"
      status=2
    fi
    ;;
esac

if [ "$status" -eq 0 ]; then
  green "layer '$layer': all green"
else
  red "layer '$layer': FAILURES"
fi
exit "$status"
