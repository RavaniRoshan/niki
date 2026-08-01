# SWE-bench Evaluation Plan

## Objective
Run NIKI on SWE-bench Verified (25-task pilot, then full 500) to measure real-world bug-fix resolution rate and produce a public benchmark report.

## Status: NOT STARTED (infrastructure blocked)

## Background
- SWE-bench Verified: 500 hand-verified GitHub issues with developer-written test suites
- Evaluation is free (dataset, Docker harness, grading all open-source)
- What costs money: LLM API tokens for NIKI's multi-agent pipeline
- Target: beat Claude-3.5-Sonnet (49.5% Verified) — NIKI's adversarial review + test-driven pipeline designed for this

## What We Built (now deleted — plan preserved here for next attempt)
1. `bench/swe-bench/1_sample.py` — Sample instances from SWE-bench Verified (HuggingFace)
2. `bench/swe-bench/2_run_niki.py` — Per-instance: git clone → config → run NIKI → extract patch
3. `bench/swe-bench/3_eval.sh` — Official swebench Docker harness (gold sanity + eval)
4. `bench/swe-bench/4_report.py` — Parse results.json → RESULTS.md

## Environment Constraints (Why We Stopped)
- Machine: 16-core, 7.5GB RAM (eval recommends 16GB RAM)
- No paid LLM API budget
- Free-tier LLM options exhausted:
  - Groq: 12K TPM limit, NIKI sends 25K tokens → 413 error
  - Google AI Studio: user's key has zero free quota
  - Ollama local (qwen2.5-coder:3b): 3B model too weak, empty/truncated patches
  - NVIDIA NIM (llama-3.3-70b): connects but produces empty unified_diff on complex tasks
  - Other NIM models: JSON parsing errors or 404

## Recommended Next Steps

### Option A: Hosted SWE-bench Platforms (RECOMMENDED)
Run NIKI on managed infrastructure with LLM APIs included:

1. **SWE-bench official leaderboard submission** — submit via PR to `princeton-nlp/SWE-bench`
2. **Meta Manas** — agentic coding platform, may support custom agents
3. **Modal / Replicate** — deploy NIKI as a container, run with your API key
4. **GitHub Actions** — CI-based eval (free for public repos, 2000 min/month)

### Option B: Fix Local Environment
1. Get a valid Google API key with free-tier quota (enable Generative Language API in GCP)
2. Or fund a cheap API ($5-10 OpenAI/Groq credits for 25 instances)
3. Run on machine with ≥16GB RAM

### Option C: Delegate to Research Lab
- Contact SWE-bench maintainers at Princeton NLP
- Offer NIKI as a submission for their leaderboard
- They have compute and LLM access for eval

## Key Files
- Dataset: `SWE-bench/SWE-bench_Verified` on HuggingFace (500 instances)
- Harness: `github.com/SWE-bench/experiments` (MIT license)
- NIKI binary: `target/release/niki` (v0.1.0, 19MB)
- Sandbox image: `niki-sandbox:24.04` (367MB, on local Docker)
- Eval config: `niki.toml` — `extra_packages = []`, Docker backend required

## Prediction Format
```json
{"instance_id": "django__django-16379", "model_name_or_path": "niki-v0.1.0", "model_patch": "<git diff>"}
```
