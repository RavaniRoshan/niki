#!/usr/bin/env bash
# The inner development loop.
#
# AGENTS.md tells you how to run the *gate*: format, lint, the whole suite, a
# release build. That is correct and it is also, on a 7.5 GiB host with 42
# integration binaries, tens of minutes. Nobody runs the gate to find out whether
# their edit compiles. They run it once at the end and then sit and wait, which
# is why this repo's iteration is slow: the only loop that exists is the slow one.
#
# This script is the fast loop, and it is built around one question: what is the
# least work that can tell you this change is broken?
#
#   ./scripts/dev-loop.sh changed          # what did I touch, and what does it risk?
#   ./scripts/dev-loop.sh check            # fmt + clippy on the library      (~30s)
#   ./scripts/dev-loop.sh test run_lifecycle   # one binary, serialized      (~1m)
#   ./scripts/dev-loop.sh fast             # every cheap binary             (~3m)
#   ./scripts/dev-loop.sh watch [paths...] # re-run on change, until Ctrl-C
#   ./scripts/dev-loop.sh gate             # the full pre-push gate from AGENTS.md
#
# Everything here runs one cargo command at a time with -j 2. That is the whole
# point: peak RSS is the constraint on this machine, and parallelism is the
# thing that breaks it.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
JOBS="${CARGO_BUILD_JOBS}"

cyan() { printf '\033[36m%s\033[0m\n' "$*"; }
red()  { printf '\033[31m%s\033[0m\n' "$*"; }
grn()  { printf '\033[32m%s\033[0m\n' "$*"; }
dim()  { printf '\033[2m%s\033[0m\n' "$*"; }

# Which integration binaries are worth running when a path changes. Ordered
# most-specific first; the first matching prefix wins for a given binary, but a
# single change can select several groups, so every matching row contributes.
#
# This is a heuristic, not a proof of coverage — the gate exists for that. It is
# here to make the common case (touch the display code, run the display tests)
# take a minute instead of half an hour.
impact() {
  local p="$1" out=""
  case "$p" in
    prompts/*|schemas/*)                out="artifact_contracts embedded_assets" ;;
    src/llm/*)                          out="llm_tool_calls multi_provider failover_chain repair_retry llm_mock_provider" ;;
    src/orchestrator/*)                 out="run_lifecycle pipeline_guards pipeline_degradation cancellation resume_cli provenance" ;;
    src/display/*|src/tui*|src/theme*|src/palette*) out="tui_navigation tui_perf visual_layout_check state_layout" ;;
    src/knowledge/*|src/memory/*)       out="kb_pipeline kb_store history_cache" ;;
    src/repo_intel/*|src/risk/*)        out="repo_intel risk_enumeration structural_index" ;;
    src/sandbox/*)                      out="sandbox_teardown worktree_policy docker_resource_caps security_exec" ;;
    src/runtime/*|src/tools/*)          out="agent_runtime diff_scope repair_retry exec_timeout" ;;
    src/permissions/*|src/safety/*)     out="security_exec secret_redaction" ;;
    src/config/*)                       out="claims claims_audit dist_install" ;;
    .github/workflows/*|deny.toml)      out="dist_install" ;;
    tests/*)                            out="" ;; # a test change: the gate's job
  esac
  printf '%s' "$out"
}

# ── changed ────────────────────────────────────────────────────────────────
cmd_changed() {
  local base
  base="$(git merge-base HEAD origin/master 2>/dev/null || git merge-base HEAD master 2>/dev/null || echo HEAD)"
  local files
  mapfile -t files < <(git diff --name-only "$base"...HEAD 2>/dev/null; git diff --name-only 2>/dev/null)
  [ "${#files[@]}" -gt 0 ] || { dim "no changes vs $base"; return 0; }

  local bin seen=" "
  printf '%s\n' "${files[@]}" | while read -r f; do
    for bin in $(impact "$f"); do
      case "$seen" in *" $bin "*) continue ;; esac
      seen="$seen$bin "
      printf '  %s\n' "$bin"
    done
  done | sort -u
}

# ── check / test / fast ───────────────────────────────────────────────────
cmd_check() {
  dim "fmt"
  cargo fmt --check || return 1
  # Library + bins only. `--all-targets` also checks all 42 test binaries and is
  # the single most expensive lint step in this repo; `dev-loop.sh gate` runs it.
  dim "clippy (lib+bins, -j $JOBS)"
  cargo clippy -j "$JOBS" --lib --bins -- -D warnings || return 1
  cat <<'NOTE'

  check: clean — but this is NOT the pre-push gate.

  It lints the library and binaries only. The tests are not linted, and CI runs
  `clippy --all-targets`, so a lint error in tests/*.rs passes here and fails
  the build. That is not hypothetical: two collapsible-`if` lints in new test
  files went through this check and broke CI.

  Before pushing:  ./scripts/dev-loop.sh gate
NOTE
}

cmd_test() {
  [ $# -ge 1 ] || { red "usage: dev-loop.sh test <binary>"; return 2; }
  local b
  for b in "$@"; do
    if [ -f "tests/${b}.rs" ]; then
      dim "--- $b ---"
      cargo test -j "$JOBS" --test "$b" -- --test-threads=1 || return 1
    else
      dim "--- lib (unit) ---"
      cargo test -j "$JOBS" --lib -- --test-threads=1 || return 1
    fi
  done
}

cmd_fast() { ./scripts/test-layer.sh fast; }

# ── watch ─────────────────────────────────────────────────────────────────
cmd_watch() {
  [ $# -gt 0 ] || set -- src prompts schemas
  local stamp; stamp="$(mktemp)"
  local next=1
  dim "watching: $*   (Ctrl-C to stop)"
  while :; do
    sleep 2
    local changed=""
    local f
    for f in "$@"; do
      [ -e "$f" ] || continue
      if [ -n "$(find "$f" -newer "$stamp" -type f \( -name '*.rs' -o -name '*.md' -o -name '*.json' -o -name '*.toml' \) -print -quit 2>/dev/null)" ]; then
        changed="$f"
        break
      fi
    done
    [ -n "$changed" ] || continue
    find . -path ./target -prune -o -type f -newer "$stamp" -print >/dev/null 2>&1
    touch "$stamp"
    echo
    cyan "── change detected under $changed ──"
    if ! cmd_check; then
      red "check failed — not running tests"
      next=1
      continue
    fi
    local bins; bins="$(impact "$changed")"
    if [ -z "$bins" ]; then
      dim "no binary mapped to $changed — run the gate before pushing"
    else
      dim "running: $bins"
      # shellcheck disable=SC2086
      cmd_test $bins || red "tests failed"
    fi
    grn "── green ──"
  done
}

# ── gate ──────────────────────────────────────────────────────────────────
cmd_gate() {
  cyan "full gate (this is the slow one)"
  cargo fmt --check || return 1
  cargo clippy -j "$JOBS" --all-targets -- -D warnings || return 1
  ./scripts/test-layer.sh all || return 1
  python3 scripts/gen-nextest-groups.py --check || return 1
  grn "gate: green"
}

case "${1:-}" in
  changed) shift; cmd_changed "$@" ;;
  check)   shift; cmd_check "$@" ;;
  test)    shift; cmd_test "$@" ;;
  fast)    shift; cmd_fast "$@" ;;
  watch)   shift; cmd_watch "$@" ;;
  gate)    shift; cmd_gate "$@" ;;
  ""|-h|--help|help) sed -n '2,22p' "$0" ;;
  *) red "unknown command '$1'"; sed -n '2,22p' "$0"; exit 2 ;;
esac
