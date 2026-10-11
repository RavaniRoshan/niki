#!/bin/sh
# Small-model evaluation harness (M9). Runs the fixed 20-task behavioral
# suite against a configured small LOCAL model (Ollama) and appends the
# measured pass rate to docs/finish/EVALS.md with the model,
# quantization, hardware, and date.
#
# This is OWNER-VERIFY: it needs a real local model, which only the
# owner can run. The agent prepares the harness; it does not run it and
# does not claim a pass rate.
#
# Usage:
#   NIKI_SMALL_MODEL=qwen2.5-coder:3b tools/smallmodel_eval.sh
#
# Prerequisites:
#   - ollama installed and serving (ollama serve)
#   - the model pulled (ollama pull qwen2.5-coder:3b)
#   - nikicode built (go build -o nikicode ./cmd/nikicode)
set -eu

MODEL="${NIKI_SMALL_MODEL:-qwen2.5-coder:3b}"
QUANT="${NIKI_SMALL_QUANT:-unknown}"
HW="${NIKI_SMALL_HW:-$(uname -s)-$(uname -m)}"
DATE="$(date -u +%Y-%m-%d)"
EVALS="docs/finish/EVALS.md"

echo "M9 small-model eval: model=$MODEL quant=$QUANT hw=$HW date=$DATE"

# The 20 fixture tasks are the same fixed suite the deterministic
# behavioral eval runs (internal/eval). Here they are driven against
# the live local model via `nikicode exec`, one prompt per task, and
# each outcome is judged by the real verifier. A task passes when its
# exec run ends verified (exit 0).
TASKS='read-ok write-new edit-ok grep-ok shell-echo repair-shape
stubborn-args plan-edit-denied verifier-fail nil-verifier undo-edit
rewind-turn doom-loop budget-steps capped-output directive-flag
push-denied empty-claim interrupt-turn two-turns'

pass=0
total=0
{
  echo ""
  echo "## Small-model run (M9, owner-run $(date -u))"
  echo ""
  echo "model=$MODEL quant=$QUANT hardware=$HW date=$DATE"
  echo ""
  echo '```'
} >> "$EVALS"

for t in $TASKS; do
  total=$((total + 1))
  # Each fixture task has a prompt file under docs/finish/smallmodel/<id>.txt
  prompt_file="docs/finish/smallmodel/$t.txt"
  if [ ! -f "$prompt_file" ]; then
    echo "$t            SKIP (no fixture prompt)" >> "$EVALS"
    continue
  fi
  if ./nikicode exec --provider ollama --model "$MODEL" "$(cat "$prompt_file")" >/dev/null 2>&1; then
    echo "$t            PASS" >> "$EVALS"
    pass=$((pass + 1))
  else
    echo "$t            FAIL" >> "$EVALS"
  fi
done

rate=$(awk "BEGIN{printf \"%.0f\", ($pass/$total)*100}")
{
  echo '```'
  echo ""
  echo "Pass rate: $pass/$total = ${rate}%"
} >> "$EVALS"

echo "M9 small-model eval complete: $pass/$total = ${rate}% (appended to $EVALS)"
