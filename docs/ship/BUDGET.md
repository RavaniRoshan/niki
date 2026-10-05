# NIKI — ship + perform: BUDGET

Tracks evaluation spend against approved budgets per PACK.md protocol.

---

## Approved Budget Ceiling

- **Owner Approved Budget**: USD 25.00 (Standard pilot & dev calibration budget ceiling)
- **Hard Gate**: `niki bench run --budget-usd <AMOUNT>` enforces that estimated run cost cannot exceed the passed budget.

---

## Cost Estimates (Measured Pilot Models)

| Model | Provider | Est. Input / 1M | Est. Output / 1M | Est. Cost / Task-Trial |
|---|---|---|---|---|
| `qwen2.5-coder:3b` | Ollama (Local) | $0.00 | $0.00 | $0.00 (Compute only) |
| `qwen2.5-coder:7b` | OpenRouter / DeepInfra | $0.06 | $0.12 | ~$0.003 - $0.015 |
| `deepseek-chat` / `deepseek-coder` | DeepSeek | $0.14 | $0.28 | ~$0.01 - $0.04 |
| `claude-3-5-haiku` | Anthropic | $0.80 | $4.00 | ~$0.05 - $0.15 |

---

## Pilot Plan (10 Tasks x 1 Trial)

- 10 tasks selected from `bench/splits/tb2_split.json` (DEV split).
- 1 trial per task.
- Projected pilot cost on open-weight candidate: < USD 0.50.

---

## Run Ledger

| Date | Run ID | Benchmark | Split / Tasks | Trials | Model | Estimated | Actual Spend | Status |
|---|---|---|---|---|---|---|---|---|
| 2026-10-05 | dry-run-01 | TB 2.1 | Pilot (10 tasks) | 1 | mock / open | $2.00 | $0.00 | Dry run ok |
