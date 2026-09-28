#!/usr/bin/env bash
# The verifiers. Three axes, each one a number you can re-measure.
#
#   ./scripts/verify-product.sh              # everything that needs no key
#   ./scripts/verify-product.sh --live       # also the live eval, needs a local model
#   ./scripts/verify-product.sh --json       # machine-readable, for diffing over time
#
# The axes are the ones a first-time user actually judges the product on:
#
#   FIRST-RUN   can a stranger go from nothing to watching an agent work, and
#               how long does it take? A product that is excellent and cannot be
#               started is not competitive with one that can.
#   SPEED       how long does the loop take? A harness you cannot iterate on is
#               a harness you cannot improve.
#   QUALITY     does it catch a real defect, and does it stay quiet on a clean
#               change? A reviewer that flags everything scores 100% recall and
#               is useless, which is why precision is measured here too.
#
# Every check prints PASS/FAIL and exits non-zero if any failed, so this is a
# gate as well as a report. It is deliberately one file: a verifier suite
# spread across scripts is a verifier suite nobody runs.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

BIN="${NIKI_BIN:-$REPO_ROOT/target/release/niki}"
LIVE=0
JSON=0
for a in "$@"; do
    case "$a" in
        --live) LIVE=1 ;;
        --json) JSON=1 ;;
        -h|--help) sed -n '2,26p' "$0"; exit 0 ;;
    esac
done

PASSED=0
FAILED=0
declare -a RESULTS=()

red()   { printf '\033[31m%s\033[0m\n' "$*"; }
grn()   { printf '\033[32m%s\033[0m\n' "$*"; }
dim()   { printf '\033[2m%s\033[0m\n' "$*"; }
hdr()   { printf '\n\033[1m== %s\033[0m\n' "$*"; }

check() {
    local name="$1" ok="$2" detail="${3:-}"
    if [ "$ok" = "1" ]; then
        PASSED=$((PASSED + 1))
        RESULTS+=("$name|$ok|$detail")
        [ "$JSON" = "1" ] || printf '  PASS  %-46s %s\n' "$name" "$detail"
    else
        FAILED=$((FAILED + 1))
        RESULTS+=("$name|$ok|$detail")
        [ "$JSON" = "1" ] || red "  FAIL  $name  $detail"
    fi
}

measure() {
    # Seconds for one command, as a 2-decimal string.
    local start end
    start=$(date +%s%N)
    "$@" >/dev/null 2>&1
    end=$(date +%s%N)
    awk -v a="$start" -v b="$end" 'BEGIN{printf "%.2f", (b-a)/1000000000}'
}

# ── 0. the binary a user would run ─────────────────────────────────────────
hdr "Build"
if [ ! -x "$BIN" ]; then
    red "no binary at $BIN — cargo build --release first"
    exit 1
fi
ver="$("$BIN" --version 2>/dev/null | head -1)"
check "binary reports a version" "$([ -n "$ver" ] && echo 1 || echo 0)" "$ver"
t=$(measure "$BIN" --version)
check "startup under 0.25s" "$(awk -v t="$t" 'BEGIN{print (t<0.25)?1:0}')" "${t}s"
t=$(measure "$BIN" --help)
check "help under 0.25s" "$(awk -v t="$t" 'BEGIN{print (t<0.25)?1:0}')" "${t}s"

# ── 1. FIRST-RUN ───────────────────────────────────────────────────────────
hdr "First run — a stranger with no key and no container"

DEMO_SECONDS=0
if command -v python3 >/dev/null && [ -f scripts/demo.sh ]; then
    dim "  running scripts/demo.sh end to end…"
    start=$(date +%s%N)
    out="$(env NIKI_BIN="$BIN" NIKI_DEMO_PORT=8123 ./scripts/demo.sh --keep 2>&1)"
    rc=$?
    end=$(date +%s%N)
    DEMO_SECONDS=$(awk -v a="$start" -v b="$end" 'BEGIN{printf "%.0f", (b-a)/1000000000}')

    check "demo reaches a branch" "$([ $rc -eq 0 ] && echo 1 || echo 0)" "${DEMO_SECONDS}s"
    if [ $rc -ne 0 ]; then
        dim "  ── last 15 lines of the demo ──"
        printf '%s\n' "$out" | tail -15 | sed 's/^/  /'
    fi

    # The four agents must all have run, not just one. Without an explicit
    # topology the mock's "low complexity" spec collapses to the solo fast path
    # and the demo shows a single agent doing the work — which is the opposite
    # of what it claims to demonstrate.
    for stage in Planner Coder Tester Reviewer; do
        check "demo ran the $stage stage" \
            "$(printf '%s' "$out" | grep -q "\[$stage\]" && echo 1 || echo 0)"
    done
    check "demo finished under 60s" \
        "$(awk -v t="$DEMO_SECONDS" 'BEGIN{print (t<60)?1:0}')" "${DEMO_SECONDS}s"
else
    check "scripts/demo.sh present" 0 "missing"
fi

# ── 2. SPEED — the loop you actually iterate on ────────────────────────────
hdr "Speed — the feedback loop"
if [ -x scripts/dev-loop.sh ]; then
    t=$(measure ./scripts/dev-loop.sh changed)
    check "dev-loop changed under 5s" "$(awk -v t="$t" 'BEGIN{print (t<5)?1:0}')" "${t}s"
else
    check "scripts/dev-loop.sh present" 0 "missing"
fi

# The gate is the number people feel. Measured, not asserted.
if [ -x scripts/test-layer.sh ]; then
    dim "  measuring the unit-test layer (the cheap half of the gate)…"
    start=$(date +%s%N)
    if ./scripts/test-layer.sh lib >/dev/null 2>&1; then ok=1; else ok=0; fi
    end=$(date +%s%N)
    lib=$(awk -v a="$start" -v b="$end" 'BEGIN{printf "%.0f", (b-a)/1000000000}')
    check "unit tests pass" "$ok" "${lib}s"
    check "unit tests under 300s" "$(awk -v t="$lib" 'BEGIN{print (t<300)?1:0}')" "${lib}s"
else
    check "scripts/test-layer.sh present" 0 "missing"
fi

# ── 3. QUALITY — does it catch defects, and stay quiet otherwise ───────────
hdr "Quality — offline eval"
if [ -x "$BIN" ]; then
    out="$("$BIN" eval 2>&1)"
    recall=$(printf '%s' "$out" | sed -n 's/.*recall \([0-9]*\)%.*/\1/p' | head -1)
    fp=$(printf '%s' "$out" | sed -n 's/.*precision \([0-9]*\)% false positives.*/\1/p' | head -1)
    check "eval runs" "$([ -n "$recall" ] && echo 1 || echo 0)" "recall ${recall:-?}% · ${fp:-?}% false positives"
    # Precision is the half that is easy to fake. A reviewer that flags
    # everything gets 100% recall; the clean-change cases are the only thing
    # that separates a reviewer from a noise source.
    check "no false positives on clean changes" \
        "$([ "$fp" = "0" ] && echo 1 || echo 0)" "${fp:-?}% FP"
    check "recall at least 90%" \
        "$(awk -v r="${recall:-0}" 'BEGIN{print (r>=90)?1:0}')" "${recall:-?}%"
else
    check "binary present" 0 "missing"
fi

# ── 4. Quality — LIVE, against a real model ───────────────────────────────
hdr "Quality — live (real model, real pipeline)"
if [ "$LIVE" = "1" ]; then
    if command -v ollama >/dev/null && ollama list >/dev/null 2>&1; then
        dim "  live eval against $(ollama list 2>/dev/null | sed -n '2p' | awk '{print $1}')…"
        start=$(date +%s%N)
        if out="$("$BIN" eval --live 2>&1)"; then ok=1; else ok=0; fi
        end=$(date +%s%N)
        live=$(awk -v a="$start" -v b="$end" 'BEGIN{printf "%.0f", (b-a)/1000000000}')
        check "live eval completes" "$ok" "${live}s"
        [ "$ok" = "1" ] || printf '%s\n' "$out" | tail -12 | sed 's/^/  /'
    else
        dim "  no local model — skipping (start ollama, or pass a provider key)"
    fi
else
    dim "  skipped (--live)"
fi

# ── report ────────────────────────────────────────────────────────────────
if [ "$JSON" = "1" ]; then
    printf '{\n  "passed": %d,\n  "failed": %d,\n  "checks": [\n' "$PASSED" "$FAILED"
    for i in "${!RESULTS[@]}"; do
        IFS='|' read -r n ok d <<<"${RESULTS[$i]}"
        printf '    {"name": "%s", "ok": %s, "detail": "%s"}%s\n' \
            "$n" "$([ "$ok" = "1" ] && echo true || echo false)" "$d" \
            "$([ "$i" -lt $(( ${#RESULTS[@]} - 1 )) ] && echo ,)"
    done
    printf '  ]\n}\n'
fi

echo
if [ "$FAILED" -eq 0 ]; then
    grn "verify-product: $PASSED passed, 0 failed"
    exit 0
fi
red "verify-product: $PASSED passed, $FAILED FAILED"
exit 1
