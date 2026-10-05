#!/usr/bin/env python3
"""The NIKI scoreboard: four agents, one model, one sealed split.

This is **not** `niki eval`. `niki eval` replays NIKI's own recorded artifacts against a
maintainer's grades and answers "does NIKI's reviewer agree with a human?". This answers a
different question: **when every agent is given the same task and the same model, who catches
what?**

Method, stated up front because a comparison is only as good as its controls:

- **One model for everyone.** Whatever `--model` names, every agent is pointed at it. If an agent
  cannot be pointed at that model, it is reported as *not run* — never quietly given a different
  one, because a scoreboard that compares NIKI on model A against a baseline on model B measures
  the models, not the agents.
- **One sealed split.** `sealed.json` is generated once from `evals/dataset.toml` and records the
  SHA-256 of the dataset it came from. If the dataset changes, the seal breaks and the runner
  refuses rather than silently scoring a different split.
- **One rubric, machine-parsed the same way for every agent.** Each agent is asked the same
  question and must answer with one of two tokens. Anything else is recorded as *unparseable* and
  counted as a miss, because an answer nobody can parse is not a catch.
- **Both directions are scored.** Recall over the seeded defects, and false positives over the
  clean controls. A reviewer that flags everything scores 100% recall and 0% precision; both
  numbers are always printed together, because one without the other is not a measurement.

Usage:
    python3 evals/scoreboard/run.py seal                 # freeze the split
    python3 evals/scoreboard/run.py report               # score whatever results exist
    python3 evals/scoreboard/run.py run --agent niki     # run one agent over the sealed split
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import pathlib
import re
import subprocess
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[2]
DATASET = ROOT / "evals" / "dataset.toml"
SEALED = ROOT / "evals" / "scoreboard" / "sealed.json"
RESULTS = ROOT / "evals" / "scoreboard" / "results"

AGENTS = ("niki", "claude-code", "codex", "deepagents")


# --------------------------------------------------------------------------
# The seal
# --------------------------------------------------------------------------

def parse_dataset() -> dict[str, dict]:
    """Pull every case id, its description and its seeded-defect label out of the dataset."""
    text = DATASET.read_text()
    cases: dict[str, dict] = {}
    for block in re.split(r"^\[\[cases\]\]\s*$", text, flags=re.M)[1:]:
        cid = re.search(r'^\s*id\s*=\s*"([^"]+)"', block, re.M)
        if not cid:
            continue
        desc = re.search(r'^\s*description\s*=\s*"([^"]+)"', block, re.M)
        label = re.search(r'^\s*label\s*=\s*"([^"]+)"', block, re.M)
        caught = re.search(r"^\s*expected_caught\s*=\s*(true|false)", block, re.M)
        cases[cid.group(1)] = {
            "id": cid.group(1),
            "description": desc.group(1) if desc else "",
            "label": label.group(1) if label else None,
            # A case with no seeded defect is a negative control: flagging it is a false positive.
            "expected_caught": caught.group(1) == "true" if caught else label is not None,
        }
    return cases


def seal() -> dict:
    """Freeze the split. Deterministic: the same dataset always yields the same seal."""
    cases = parse_dataset()
    if not cases:
        raise SystemExit(f"no cases parsed from {DATASET}")
    digest = hashlib.sha256(DATASET.read_bytes()).hexdigest()
    sealed = {
        "sealed_at": None,  # deliberately null: the seal must be byte-stable across regenerations
        "dataset_sha256": digest,
        "model": None,
        "cases": [
            {
                "id": c["id"],
                "description": c["description"],
                "label": c["label"],
                "expected_caught": c["expected_caught"],
            }
            for c in sorted(cases.values(), key=lambda x: x["id"])
        ],
    }
    SEALED.parent.mkdir(parents=True, exist_ok=True)
    SEALED.write_text(json.dumps(sealed, indent=2, sort_keys=True) + "\n")
    defects = sum(1 for c in sealed["cases"] if c["expected_caught"])
    print(f"sealed {len(sealed['cases'])} cases "
          f"({defects} seeded defects, {len(sealed['cases']) - defects} clean controls)")
    print(f"dataset sha256 {digest}")
    return sealed


def load_sealed() -> dict:
    if not SEALED.exists():
        raise SystemExit("no seal: run `python3 evals/scoreboard/run.py seal` first")
    sealed = json.loads(SEALED.read_text())
    digest = hashlib.sha256(DATASET.read_bytes()).hexdigest()
    if sealed["dataset_sha256"] != digest:
        raise SystemExit(
            "the seal is broken: evals/dataset.toml changed after the split was frozen.\n"
            "Re-seal deliberately, or score the split you meant to score:\n"
            f"  sealed  {sealed['dataset_sha256']}\n"
            f"  current {digest}"
        )
    return sealed


# --------------------------------------------------------------------------
# The rubric. One question, one token.
# --------------------------------------------------------------------------

RUBRIC = (
    "You are reviewing a change. Answer with EXACTLY one token and nothing else:\n"
    "  CAUGHT  - the change contains a defect that must be fixed before merging\n"
    "  CLEAN   - the change is correct as written\n"
    "Answer with one word."
)


# A negation immediately before the token flips the meaning. Scoring "NOT CAUGHT" as CAUGHT would
# silently inflate a score, which is the one failure mode a scoreboard cannot be allowed to have.
_NEGATION = re.compile(r"\b(NOT|NEVER|NO|DIDN\u2019T|DIDN'T|ISN\u2019T|ISN'T|WASN'T|IS NOT)\s+$")


def parse_verdict(text: str) -> str | None:
    """Extract CAUGHT or CLEAN. Anything else is unparseable, which scores as a miss."""
    if not text:
        return None
    head = text.strip().upper()[:200]
    best: tuple[int, str] | None = None
    for token in ("CAUGHT", "CLEAN"):
        for m in re.finditer(rf"\b{token}\b", head):
            if _NEGATION.search(head[: m.start()]):
                continue
            if best is None or m.start() < best[0]:
                best = (m.start(), token)
            break
    return best[1] if best else None


# --------------------------------------------------------------------------
# Running one agent
# --------------------------------------------------------------------------

# NIKI loads `~/.config/niki/niki.toml` first and the project's own `niki.toml` over the top, so
# each case runs in a throwaway project that pins every agent to the shared model. Without this the
# user's global provider wins and the scoreboard silently measures a different model for NIKI
# than for the baselines — the one thing a scoreboard must never do.
SCOREBOARD_CONFIG = """# Generated by evals/scoreboard/run.py. Every agent points at the SAME model, because a
# comparison across agents is only meaningful on a shared one.
[providers.ollama]
api_key_env = "NIKI_SCOREBOARD_KEY"
base_url = "http://127.0.0.1:11434"

[agents]
planner.provider = "ollama"
planner.model = "{model}"
coder.provider = "ollama"
coder.model = "{model}"
tester.provider = "ollama"
tester.model = "{model}"
reviewer.provider = "ollama"
reviewer.model = "{model}"

[security]
enabled = false

[red_blue]
enabled = false
"""


# NIKI answers a merge decision in its own vocabulary. The rubric asks every agent the same
# question; only the words differ, and this is the single place they are mapped. Anything NIKI
# does not recognise stays unparseable rather than being rounded to whichever side looks better.
NIKI_TOKENS = {"approved": "CLEAN", "revisionneeded": "CAUGHT", "revision_needed": "CAUGHT"}


def niki_verdict(report: dict) -> str:
    raw = str(report.get("verdict", "")).strip().lower().replace(" ", "_")
    if raw in ("unknown", ""):
        return ""
    return NIKI_TOKENS.get(raw, "")


def run_niki(case: dict, model: str, timeout: int) -> str:
    """NIKI through its own CLI, against the same model, with its own verdict as the judge."""
    prompt = f"{case['description']}\n\n{RUBRIC}"
    workdir = pathlib.Path(tempfile.mkdtemp(prefix="niki-scoreboard-"))
    try:
        (workdir / "niki.toml").write_text(SCOREBOARD_CONFIG.format(model=model))
        (workdir / "main.rs").write_text("fn main() {}\n")
        subprocess.run(["git", "init", "-q"], cwd=workdir, capture_output=True)
        # The worktree backend needs a commit to branch from; an unborn HEAD is not a repository
        # it can work with, and the failure looks like a sandbox bug rather than a fixture bug.
        for git_args in (
            ["git", "config", "user.email", "scoreboard@niki.local"],
            ["git", "config", "user.name", "niki scoreboard"],
            ["git", "add", "-A"],
            ["git", "commit", "-qm", "fixture"],
        ):
            subprocess.run(git_args, cwd=workdir, capture_output=True)
        env = {
            "PATH": "/usr/local/bin:/usr/bin:/bin",
            "HOME": str(pathlib.Path.home()),
            # ollama ignores the key, but the provider requires one to be named.
            "NIKI_SCOREBOARD_KEY": "ollama",
        }
        proc = subprocess.run(
            [str(ROOT / "target" / "debug" / "niki"), "run", prompt,
             "--backend", "worktree", "--bare", "--quiet", "--output-format", "json",
             "--project", str(workdir)],
            capture_output=True, text=True, timeout=timeout, env=env, cwd=str(workdir),
        )
        for line in proc.stdout.splitlines():
            if not line.startswith("{"):
                continue
            try:
                report = json.loads(line)
            except json.JSONDecodeError:
                continue
            return niki_verdict(report)
        return ""
    finally:
        subprocess.run(["rm", "-rf", str(workdir)], capture_output=True)


def run_external(cmd: list[str], case: dict, model: str, timeout: int) -> str:
    prompt = f"{case['description']}\n\n{RUBRIC}"
    try:
        proc = subprocess.run(cmd + [prompt], capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return ""
    return proc.stdout


def agent_command(agent: str, model: str) -> list[str] | None:
    """The command for one agent, pointed at `model`, or None when it cannot be pointed."""
    if agent == "claude-code":
        # Claude Code speaks the Anthropic protocol and needs a credential even when the base URL
        # is redirected; without one it blocks instead of failing, so it is gated on the key.
        if not os_environ("ANTHROPIC_API_KEY"):
            return None
        return [
            "claude", "-p", "--output-format", "text", "--model", model,
        ]
    if agent == "codex":
        if not os_environ("OPENAI_API_KEY") and not os_environ("CODEX_LOCAL_MODEL"):
            return None
        return ["codex", "exec", "--skip-git-repo-check", "--model", model, "-"]
    if agent == "deepagents":
        return [sys.executable, str(ROOT / "evals" / "scoreboard" / "run_deepagents.py"), "--model", model]
    return None


def os_environ(name: str) -> str:
    import os

    return os.environ.get(name, "")


def run_agent(agent: str, sealed: dict, model: str, timeout: int) -> dict:
    RESULTS.mkdir(parents=True, exist_ok=True)
    out = RESULTS / f"{agent}.json"
    records = []
    cmd = None if agent == "niki" else agent_command(agent, model)
    blocked = ""
    if agent != "niki" and cmd is None:
        blocked = "no credential available to point this agent at the shared model"

    for case in sealed["cases"]:
        if blocked:
            records.append({**case, "verdict": None, "error": blocked})
            continue
        started = time.time()
        try:
            raw = run_niki(case, model, timeout) if agent == "niki" else run_external(cmd, case, model, timeout)
        except subprocess.TimeoutExpired:
            raw = ""
        except Exception as exc:  # a broken harness must not look like a clean review
            raw = ""
            blocked = f"{type(exc).__name__}: {exc}"
        records.append({
            **case,
            "verdict": parse_verdict(raw),
            "raw": raw[:400],
            "seconds": round(time.time() - started, 2),
        })
        print(f"  {agent:12} {case['id']:34} -> {records[-1]['verdict'] or 'unparseable'}")

    payload = {"agent": agent, "model": model, "blocked": blocked, "records": records}
    out.write_text(json.dumps(payload, indent=2) + "\n")
    return payload


# --------------------------------------------------------------------------
# Scoring
# --------------------------------------------------------------------------

def wilson(successes: int, total: int, z: float = 1.96) -> tuple[float, float]:
    """Wilson score interval. The right interval for a proportion at these sample sizes: the
    normal approximation misbehaves badly when the count is small or the rate is near 0 or 1,
    which is exactly the regime a 27-case split lives in."""
    if total == 0:
        return (0.0, 0.0)
    p = successes / total
    denom = 1 + z * z / total
    centre = (p + z * z / (2 * total)) / denom
    margin = z * math.sqrt(p * (1 - p) / total + z * z / (4 * total * total)) / denom
    return (max(0.0, centre - margin), min(1.0, centre + margin))


def score(records: list[dict]) -> dict:
    defects = [r for r in records if r["expected_caught"]]
    controls = [r for r in records if not r["expected_caught"]]
    caught = sum(1 for r in defects if r["verdict"] == "CAUGHT")
    false_positives = sum(1 for r in controls if r["verdict"] == "CAUGHT")
    unparseable = sum(1 for r in records if r["verdict"] is None)
    recall = caught / len(defects) if defects else 0.0
    # Precision on the cases it flagged at all; an agent that flagged nothing has no precision.
    flagged = caught + false_positives
    precision = caught / flagged if flagged else 0.0
    return {
        "defects": len(defects),
        "caught": caught,
        "recall": recall,
        "recall_ci": wilson(caught, len(defects)),
        "controls": len(controls),
        "false_positives": false_positives,
        "precision": precision,
        "precision_ci": wilson(caught, flagged) if flagged else (0.0, 0.0),
        "unparseable": unparseable,
    }


def report(sealed: dict) -> int:
    print(f"\nNIKI scoreboard — {len(sealed['cases'])} sealed cases, one model for every agent\n")
    rows = []
    for agent in AGENTS:
        path = RESULTS / f"{agent}.json"
        if not path.exists():
            rows.append((agent, None, "not run"))
            continue
        payload = json.loads(path.read_text())
        rows.append((agent, score(payload["records"]), payload["blocked"] or ""))

    hdr = f"{'agent':14} {'recall':>9} {'95% CI':>16} {'precision':>10} {'unparseable':>12}  note"
    print(hdr)
    print("-" * len(hdr))
    for agent, s, note in rows:
        if s is None:
            print(f"{agent:14} {'-':>9} {'-':>16} {'-':>10} {'-':>12}  {note}")
            continue
        lo, hi = s["recall_ci"]
        print(f"{agent:14} {s['recall']*100:8.1f}% [{lo*100:5.1f},{hi*100:5.1f}] "
              f"{s['precision']*100:9.1f}% {s['unparseable']:12}  {note}")

    print()
    niki_row = next((s for a, s, _ in rows if a == "niki"), None)
    for agent, s, note in rows:
        if agent == "niki" or s is None or niki_row is None:
            continue
        delta = niki_row["recall"] - s["recall"]
        lo_a, hi_a = niki_row["recall_ci"]
        lo_b, hi_b = s["recall_ci"]
        overlaps = not (hi_a < lo_b or hi_b < lo_a)
        verdict = "not distinguishable" if overlaps else "NIKI ahead"
        print(f"recall delta NIKI - {agent}: {delta*100:+.1f} pts  "
              f"({verdict}; 95% intervals {'overlap' if overlaps else 'do not overlap'})")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("command", choices=["seal", "run", "report"])
    ap.add_argument("--agent", choices=AGENTS, default="niki")
    ap.add_argument("--model", default="qwen2.5-coder:3b")
    ap.add_argument("--timeout", type=int, default=300)
    args = ap.parse_args()

    if args.command == "seal":
        seal()
        return 0
    sealed = load_sealed()
    if args.command == "run":
        print(f"running {args.agent} over {len(sealed['cases'])} sealed cases on {args.model}")
        run_agent(args.agent, sealed, args.model, args.timeout)
        return 0
    return report(sealed)


if __name__ == "__main__":
    raise SystemExit(main())