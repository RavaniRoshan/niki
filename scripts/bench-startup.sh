#!/usr/bin/env bash
# NIKI startup-time benchmark (POSIX). See scripts/bench-startup.ps1 for the
# Windows variant; both prefer hyperfine and fall back to a plain loop.
set -euo pipefail

RUNS="${RUNS:-50}"
EXE="${EXE:-./target/release/niki}"

[ -x "$EXE" ] || { echo "binary not found at $EXE — build with cargo build --release first" >&2; exit 1; }

echo "== NIKI startup benchmark: $EXE =="
"$EXE" --version

if command -v hyperfine >/dev/null 2>&1; then
  echo "[hyperfine]"
  exec hyperfine -N --warmup 10 --min-runs "$RUNS" "$EXE --version"
fi

echo "[fallback loop] install hyperfine for distribution stats"
samples=()
for _ in $(seq "$RUNS"); do
  t=$( { /usr/bin/time -f '%e' "$EXE" --version >/dev/null; } 2>&1 )
  samples+=("$(awk -v t="$t" 'BEGIN{printf "%.1f", t*1000}')")
done
printf '%s\n' "${samples[@]}" | sort -n | awk -v n="$RUNS" '
  { a[NR]=$1 }
  END {
    printf "  min=%.1fms median=%.1fms p95=%.1fms\n", a[1], a[int(n/2)+1], a[int(n*0.95)]
    printf "  budget check (<100 ms launch): %s\n", (a[int(n/2)+1] < 100 ? "PASS" : "FAIL")
  }'
