# NIKI Behavioral Evals (docs/finish/EVALS.md)

## Fixture suite (deterministic providers, 2026-10-11)

Pass rate: 20/20 = 100%

```
read-ok            PASS
write-new          PASS
edit-ok            PASS
grep-ok            PASS
shell-echo         PASS
repair-shape       PASS
stubborn-args      PASS
plan-edit-denied   PASS
verifier-fail      PASS
nil-verifier       PASS
undo-edit          PASS
rewind-turn        PASS
doom-loop          PASS
budget-steps       PASS
capped-output      PASS
directive-flag     PASS
push-denied        PASS
empty-claim        PASS
interrupt-turn     PASS
two-turns          PASS
```

## Small-model suite (M9) — AWAITS OWNER RUN

The 20-task small-local-model suite has NOT been run by the agent: it
requires a real local model (e.g. Ollama `qwen2.5-coder:3b`), which
only the owner can run. **No pass rate is claimed.**

Run it with the prepared harness:

```
NIKI_SMALL_MODEL=qwen2.5-coder:3b \
NIKI_SMALL_QUANT=Q4_K_M \
NIKI_SMALL_HW="$(uname -s)-$(uname -m)" \
  tools/smallmodel_eval.sh
```

The script (`tools/smallmodel_eval.sh`) drives the same 20 fixed
fixture tasks against the live local model via `nikicode exec
--provider ollama`, judges each by the real verifier, and appends the
measured pass rate with model, quantization, hardware, and date to
this file. Prerequisites: `ollama serve`, the model pulled, and
`nikicode` built. The small-model profile ships only at or above the
Phase 0 threshold, evaluated from that recorded rate.

