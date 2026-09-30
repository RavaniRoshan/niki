#!/usr/bin/env python3
"""Check that branch protection and `ci.yml` agree, before a merge finds out.

The required-checks list is a set of **strings**, and GitHub blocks a merge
when a required check has not reported. It does not care whether the check
still exists. So a job that is renamed or deleted here leaves a required name
behind that can never report, and every pull request in the repository is
blocked from then on — with a merge box that says "Expected — waiting for
status check" and no indication of why.

That is the failure this script exists to prevent, and it is a settings-level
thing that no amount of reading `ci.yml` will reveal: the list lives in the
repository's branch protection, and it drifts silently.

## What is required, and why it is here

    python3 scripts/check-required-contexts.py            # offline, uses the
                                                          # committed list
    python3 scripts/check-required-contexts.py --live     # also asks GitHub

`--live` needs `gh` and a token, so it is the one to run after changing branch
protection. The default check is offline and runs in CI, because the direction
that breaks merges is the one that can be decided from the repository alone:
a name this file lists that `ci.yml` no longer defines.

## Regenerating the list

The names are the `name:` fields of the jobs in `.github/workflows/ci.yml`,
with `build-matrix` expanded to one name per matrix target. After changing a
job name, change it here too, and push — then apply the same edit under
Settings -> Branches -> master -> Branch protection rules -> Required status
checks. `tests/ci_contracts.rs` fails if the two drift apart.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CI_YML = REPO / ".github/workflows/ci.yml"

# The contexts the repository's branch protection requires on `master`, as of
# the run this was written against. Keep in step with the settings.
REQUIRED_CONTEXTS: list[str] = [
    "Audit (supply chain)",
    "Build (--no-default-features)",
    "Build aarch64-apple-darwin",
    "Build x86_64-apple-darwin",
    "Build x86_64-unknown-linux-gnu",
    "Check (fmt + clippy)",
    "Consumer journeys",
    "Demo (no key, no container)",
    "E2E Pipeline (Mock LLM)",
    "Headless PTY TUI (NO_COLOR=1)",
    "Headless PTY TUI (colour)",
    "Install path",
    "MSRV (1.88)",
    "Manifest parity",
    "Mega E2E (code must work)",
    "Product Acceptance Suite",
    "Real PTY TUI",
    "Release packaging contract",
    "Tests",
    "Validate x86_64 on Intel Mac",
    "Visual regression (VHS)",
    "Windows (build + smoke)",
]


# Job names built from a matrix. The template is what `ci.yml` contains; each
# expansion is the check name the runner actually reports. Listed rather than
# parsed because the expansion is a property of the *settings* — GitHub needs
# the concrete name, and there is no way to read it back out of the workflow.
MATRIX_EXPANSIONS: dict[str, list[str]] = {
    "Build ${{ matrix.target }}": [
        "Build x86_64-unknown-linux-gnu",
        "Build x86_64-apple-darwin",
        "Build aarch64-apple-darwin",
    ],
    "Headless PTY TUI (${{ matrix.colour }})": [
        "Headless PTY TUI (NO_COLOR=1)",
        "Headless PTY TUI (colour)",
    ],
}


def produced_names(ci_text: str) -> set[str]:
    """Every check name `ci.yml` can report, with matrices expanded.

    Text, not YAML, on purpose. The question is never "is this valid YAML" —
    the Actions runner answers that — it is "does a name this file must
    require still exist", and that is a substring question over the source.
    It also survives YAML features this script does not model.

    Deliberately one-directional. It can prove a required name is gone; it
    cannot prove a *new* job is required, because a new job is exactly what
    nobody has added to the required list yet. `--live` checks that direction
    against the real settings, and CI runs the offline half every push.
    """
    names: set[str] = set()
    for template, expansions in MATRIX_EXPANSIONS.items():
        if template in ci_text:
            names.update(expansions)
    for line in ci_text.splitlines():
        m = re.match(r"^\s+name:\s+(.+?)\s*$", line)
        if not m:
            continue
        raw = m.group(1).strip().strip('"')
        names.update(MATRIX_EXPANSIONS.get(raw, [raw]))
    return names


def main() -> int:
    ci_text = CI_YML.read_text(encoding="utf-8")
    produced = produced_names(ci_text)
    required = set(REQUIRED_CONTEXTS)
    failed = False

    missing = sorted(required - produced)
    if missing:
        failed = True
        print("FAIL: required but not produced by ci.yml:", file=sys.stderr)
        for name in missing:
            print(f"  - {name}", file=sys.stderr)
        print(
            "\n  Every one of these blocks every merge, permanently, because\n"
            "  GitHub waits for a check that can never report.",
            file=sys.stderr,
        )

    if "--list" in sys.argv:
        # One name per line, for `scripts/ci-is-green.sh`. Parsing the prose
        # summary above would be parsing a sentence to get a list.
        for name in sorted(required):
            print(name)
        return 0 if not failed else 1

    if "--live" not in sys.argv:
        # The reverse direction needs the settings, and the settings are not
        # in the repository. `--live` is the only place it can be checked.
        pass

    if not failed:
        print(f"required contexts: {len(required)}, all produced by ci.yml")

    if "--live" in sys.argv:
        out = subprocess.run(
            [
                "gh",
                "api",
                "repos/Roshan-s-labs/niki/branches/master/protection",
                "--jq",
                ".required_status_checks.contexts[]",
            ],
            capture_output=True,
            text=True,
        )
        if out.returncode != 0:
            print(f"could not read branch protection: {out.stderr.strip()}", file=sys.stderr)
            return 1
        live = {line for line in out.stdout.splitlines() if line.strip()}
        drift = sorted(live ^ required)
        if drift:
            failed = True
            print("\nFAIL: branch protection and this file disagree:", file=sys.stderr)
            for name in drift:
                side = "only in settings" if name in live else "only in this file"
                print(f"  - {name} ({side})", file=sys.stderr)
        else:
            print(f"branch protection: {len(live)} contexts, matches this file")

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
