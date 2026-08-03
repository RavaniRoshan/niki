#!/usr/bin/env python3
"""Generate SWE-bench predictions by running NIKI on verified instances."""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def load_instances(count: int, split: str, instance_ids: str | None) -> list[dict]:
    """Load instances from SWE-bench dataset."""
    from swebench.harness.utils import load_swebench_dataset

    ids = instance_ids.split(",") if instance_ids else None
    print(f"Loading {split} split" + (f" ({len(ids)} instances)" if ids else ""))
    return load_swebench_dataset(split=split, instance_ids=ids)[:count]


def setup_repo(repo_url: str, base_commit: str, work_dir: Path) -> Path:
    """Clone repo and checkout at base_commit."""
    repo_name = repo_url.split("/")[-1]
    if repo_name.endswith(".git"):
        repo_name = repo_name[:-4]
    repo_dir = work_dir / repo_name

    if repo_dir.exists():
        shutil.rmtree(repo_dir)

    subprocess.run(
        ["git", "clone", "--quiet", repo_url, str(repo_dir)],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["git", "checkout", "--quiet", base_commit],
        cwd=repo_dir,
        check=True,
        capture_output=True,
    )
    return repo_dir


def run_niki(repo_dir: Path, problem: str, api_key: str, model: str, max_rounds: int) -> str | None:
    """Run NIKI and extract the patch from git diff."""
    # Initialize git tracking in the repo if not already a git repo
    if not (repo_dir / ".git").exists():
        subprocess.run(["git", "init"], cwd=repo_dir, capture_output=True)
        subprocess.run(["git", "add", "-A"], cwd=repo_dir, capture_output=True)
        subprocess.run(["git", "commit", "-m", "initial"], cwd=repo_dir, capture_output=True)

    # Copy niki.toml from project root so NIKI uses the OpenRouter config
    project_root = Path(__file__).parent.parent
    src_niki_toml = project_root / "niki.toml"
    dst_niki_toml = repo_dir / "niki.toml"
    if src_niki_toml.exists() and not dst_niki_toml.exists():
        shutil.copy2(src_niki_toml, dst_niki_toml)

    # Remove .niki from previous runs
    niki_dir = repo_dir / ".niki"
    if niki_dir.exists():
        shutil.rmtree(niki_dir)

    env = os.environ.copy()
    env["OPENAI_API_KEY"] = api_key
    env["OPENAI_BASE_URL"] = "https://openrouter.ai/api/v1"

    # Run NIKI from the project root with --project pointing at the repo
    niki_bin = Path(__file__).parent.parent / "target" / "release" / "niki"
    if not niki_bin.exists():
        niki_bin = Path(__file__).parent.parent / "target" / "debug" / "niki"
    if not niki_bin.exists():
        print("  ERROR: niki binary not found, building...")
        subprocess.run(["cargo", "build", "--release"], cwd=Path(__file__).parent.parent, check=True)

    cmd = [
        str(niki_bin),
        "run",
        "--backend", "worktree",
        "--max-rounds", str(max_rounds),
        "--quiet",
        "--project", str(repo_dir),
        problem,
    ]

    try:
        result = subprocess.run(
            cmd,
            cwd=str(repo_dir),
            env=env,
            capture_output=True,
            text=True,
            timeout=300,
        )
        if result.returncode != 0:
            print(f"  NIKI error (exit {result.returncode}): {result.stderr[:200] if result.stderr else '(no stderr)'}")
    except subprocess.TimeoutExpired:
        print("  NIKI timed out (300s)")
        return None

    # Extract patch: try NIKI's recorded diff, fall back to git diff
    diff = extract_diff(repo_dir)
    return diff


def extract_diff(repo_dir: Path) -> str | None:
    """Extract the patch NIKI produced: try the recorded diff, fall back to git diff."""
    # Try .niki/tasks/*/changes.patch
    tasks_dir = repo_dir / ".niki" / "tasks"
    if tasks_dir.exists():
        patches = sorted(tasks_dir.glob("*/changes.patch"), key=lambda p: p.stat().st_mtime, reverse=True)
        for patch_file in patches:
            content = patch_file.read_text().strip()
            if content:
                return content

    # Fall back to git diff HEAD (NIKI commits its changes)
    result = subprocess.run(
        ["git", "diff", "HEAD~1"],
        cwd=repo_dir,
        capture_output=True,
        text=True,
    )
    if result.returncode == 0 and result.stdout.strip():
        return result.stdout.strip()

    # Fall back to unstaged diff
    result = subprocess.run(
        ["git", "diff"],
        cwd=repo_dir,
        capture_output=True,
        text=True,
    )
    if result.returncode == 0 and result.stdout.strip():
        return result.stdout.strip()

    return None


def main():
    parser = argparse.ArgumentParser(description="Generate SWE-bench predictions using NIKI")
    parser.add_argument("--count", type=int, default=3, help="Number of instances to process")
    parser.add_argument("--split", default="test", help="Dataset split (default: test)")
    parser.add_argument("--instance-ids", default=None, help="Comma-separated instance IDs to use")
    parser.add_argument("--output-dir", default="bench/predictions/niki-v0.1.0", help="Output directory")
    parser.add_argument("--model", default="nvidia/nemotron-3-ultra-550b-a55b:free", help="Model name")
    parser.add_argument("--max-rounds", type=int, default=3, help="Max revision rounds")
    parser.add_argument("--api-key", default=None, help="OpenRouter API key (or OPENAI_API_KEY env)")
    parser.add_argument("--work-dir", default=None, help="Working directory for repo clones (default: /tmp/swebench)")
    args = parser.parse_args()

    api_key = args.api_key or os.environ.get("OPENAI_API_KEY", "")
    if not api_key:
        print("ERROR: --api-key or OPENAI_API_KEY env var required")
        sys.exit(1)

    work_dir = Path(args.work_dir or "/tmp/swebench")
    work_dir.mkdir(parents=True, exist_ok=True)
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    instances = load_instances(args.count, args.split, args.instance_ids)
    print(f"Processing {len(instances)} instances\n")

    predictions = []
    results = []

    for i, inst in enumerate(instances, 1):
        iid = inst["instance_id"]
        repo_url = f"https://github.com/{inst['repo']}.git"
        print(f"[{i}/{len(instances)}] {iid} ({inst['repo']})")

        # Clone and checkout
        print(f"  Cloning {inst['repo']} at {inst['base_commit'][:12]}...")
        try:
            repo_dir = setup_repo(repo_url, inst["base_commit"], work_dir)
        except Exception as e:
            print(f"  Clone failed: {e}")
            results.append({"instance_id": iid, "status": "clone_failed", "error": str(e)})
            continue

        # Run NIKI
        print(f"  Running NIKI (model: {args.model})...")
        t0 = time.time()
        patch = run_niki(repo_dir, inst["problem_statement"], api_key, args.model, args.max_rounds)
        elapsed = time.time() - t0
        print(f"  Done in {elapsed:.1f}s")

        if patch:
            predictions.append({
                "instance_id": iid,
                "model_patch": patch,
                "model_name_or_path": "niki",
            })
            results.append({"instance_id": iid, "status": "success", "patch_len": len(patch), "elapsed": elapsed})
            print(f"  Patch: {len(patch)} chars")
        else:
            predictions.append({
                "instance_id": iid,
                "model_patch": "",
                "model_name_or_path": "niki",
            })
            results.append({"instance_id": iid, "status": "empty_patch", "elapsed": elapsed})
            print("  Empty patch")

    # Write predictions.json
    predictions_file = output_dir / "predictions.json"
    with open(predictions_file, "w") as f:
        json.dump(predictions, f, indent=2)
    print(f"\nWrote {len(predictions)} predictions to {predictions_file}")

    # Write instances.jsonl
    instances_file = output_dir / "instances.jsonl"
    with open(instances_file, "w") as f:
        for inst in instances:
            row = {k: inst.get(k, "") for k in [
                "instance_id", "repo", "base_commit", "patch", "test_patch",
                "problem_statement", "hints_text", "version", "environment_setup_commit",
            ]}
            # Arrays need to be serialized as strings for swebench harness
            if isinstance(row.get("FAIL_TO_PASS"), list):
                row["FAIL_TO_PASS"] = json.dumps(row["FAIL_TO_PASS"])
            else:
                row["FAIL_TO_PASS"] = inst.get("FAIL_TO_PASS", "[]")
            if isinstance(row.get("PASS_TO_PASS"), list):
                row["PASS_TO_PASS"] = json.dumps(row["PASS_TO_PASS"])
            else:
                row["PASS_TO_PASS"] = inst.get("PASS_TO_PASS", "[]")
            f.write(json.dumps(row) + "\n")
    print(f"Wrote instances to {instances_file}")

    # Summary
    successes = sum(1 for r in results if r["status"] == "success")
    print(f"\nSummary: {successes}/{len(results)} produced patches")

    # Print results table
    for r in results:
        status = r["status"]
        marker = "+" if status == "success" else "-"
        elapsed = r.get("elapsed", 0)
        patch_info = f" ({r.get('patch_len', 0)} chars)" if status == "success" else ""
        print(f"  {marker} {r['instance_id']}: {status}{patch_info} [{elapsed:.1f}s]")


if __name__ == "__main__":
    main()
