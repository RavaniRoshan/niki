# SWE-bench External Evaluation — Research Report

## Executive Summary

You don't need to run SWE-bench locally. The best option is **sb-cli** (official, free) which lets you submit predictions and get results on SWE-bench's cloud infrastructure. For your NIKI agent, the workflow is: run NIKI locally to generate patches → submit via sb-cli → get results in ~20 minutes. No Docker, no 120GB disk, no 16GB RAM needed.

---

## Top Recommendation: sb-cli (Official, Free)

**What it is:** Official SWE-bench cloud evaluation tool. You submit predictions, they run evaluation on their AWS infrastructure.

**How it works:**
```bash
pip install sb-cli
sb login                          # sets SWEBENCH_API_KEY
sb-cli submit swe-bench_verified test \
  --predictions_path preds.json \
  --run_id niki-v0.1.0 \
  --wait_for_evaluation \
  --gen_report
```

**Key details:**
- **Free** — no cost for evaluation itself
- **Predictions format:** JSON list `[{"instance_id": "...", "model_patch": "<git diff>", "model_name_or_path": "niki-v0.1.0"}]`
- **Subsets:** `swe-bench_verified` (500 problems), `swe-bench_lite` (300), `swe-bench-m` (Multimodal)
- **Test split quotas:** Limited (~1 run/month for test splits, refreshes every 30 days)
- **Results:** ~20 minutes (limited by slowest instance)
- **Leaderboard submission:** Requires reasoning traces (`trajs/`) + technical report — mandatory since July 2024

**Your workflow:**
1. Run NIKI on each SWE-bench Verified instance locally (one at a time, extract patch)
2. Generate `preds.json` in sb-cli format
3. `sb-cli submit swe-bench_verified test --predictions_path preds.json`
4. Get resolution rate

**Source:** https://www.swebench.com/sb-cli/, https://github.com/SWE-bench/sb-cli

---

## Option 2: Modal (Cloud-Assisted, Free Tier Available)

**What it is:** `--modal true` flag in SWE-bench evaluation harness offloads Docker evaluation to Modal's serverless compute.

**How it works:**
```bash
python -m swebench.harness.run_evaluation \
  --predictions_path preds.json \
  --swe_bench_tasks <tasks_file> \
  --modal true
```

**Key details:**
- You still run the CLI locally, but Docker containers execute in Modal's cloud
- Modal has a free tier ($30/month credit)
- Not fully managed — you orchestrate from your machine

**Source:** https://www.swebench.com/SWE-bench/guides/evaluation/

---

## Option 3: GitHub Actions CI (Free for Public Repos)

**What it is:** Run SWE-bench evaluation in GitHub Actions CI. Public repos get unlimited free minutes.

**Key details:**
- GitHub-hosted runners: 4 vCPU / 16 GB RAM (public repos)
- 6-hour max job duration per run
- `greynewell/swe-bench-pro-action` exists for SWE-bench Pro (not original SWE-bench)
- LLM API costs are separate (you pay for NIKI's inference)
- Need to generate predictions locally first, then submit for grading

**Limitation:** No official GitHub Action for original SWE-bench (only Pro variant). Would need custom workflow.

**Source:** https://github.com/greynewell/swe-bench-pro-action, https://docs.github.com/en/billing/concepts/product-billing/github-actions

---

## Option 4: Benchlist (Third-Party, ~$53/run)

**What it is:** Hosted runner for SWE-bench Verified with cryptographically signed results.

**Key details:**
- REST API: `POST https://benchlist.ai/api/v1/run`
- Platform fee: ~$25 + inference costs ~$28 = **~$53 total**
- Produces Ed25519 signed results with optional Ethereum ZK anchor
- Also offers self-hosted `benchlist-runner` pip package

**Source:** https://benchlist.ai/articles/swe-bench-verified

---

## Option 5: CoreWeave Sandboxes

**What it is:** Cloud sandboxes for running SWE-bench evaluation. Pulls pre-built images from Epoch AI's GHCR registry.

**Source:** https://docs.coreweave.com/products/sandboxes/client/guides/swebench

---

## What Doesn't Work

| Platform | Status | Why |
|----------|--------|-----|
| Meta Manus | ❌ Not SWE-bench | General-purpose agent (GAIA benchmark), no SWE-bench evaluation feature |
| Scale AI SWE-bench Pro | ❌ Different benchmark | 1,865 tasks across 41 repos — separate from original SWE-bench |
| open-compass/SWE-bench-server | ❌ Self-hosted | Repo returns 404; was never a hosted service |

---

## Recommended Next Step

1. **Generate patches locally:** Run NIKI on 25 SWE-bench Verified instances (one at a time, extract `model_patch` from each run)
2. **Format predictions:** Create `preds.json` in sb-cli format
3. **Submit:** `sb-cli submit swe-bench_verified test --predictions_path preds.json`
4. **Get results:** ~20 minutes, free, official evaluation

The bottleneck is still generating the patches (NVIDIA NIM produced empty diffs). To solve that, you need either:
- A working LLM provider (fix Google API key, or get $5-10 OpenAI/Groq credits)
- Or use sb-cli with a different agent's predictions first to validate the pipeline
