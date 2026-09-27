#!/usr/bin/env python3
"""Classify a canary-gate run and decide pass/fail.

A survived canary means the suite cannot detect that defect. That is a gate
failure -- unless the corpus already declares it a known blind spot, in which
case it is reported rather than counted as a regression. Reporting it is the
point: a blind spot nobody wrote down is indistinguishable from a test suite
that has quietly stopped testing anything.

Split out of `canary-gate.sh` so the gate logic is readable and independently
runnable against a stored `results.json`.
"""
import json
import sys


def main(path: str) -> int:
    with open(path) as fh:
        report = json.load(fh)
    canaries = report["canaries"]

    def survived(c):
        return c["outcome"] == "survived"

    unexpected = [
        c for c in canaries
        if survived(c)
        and c.get("expect_kill", True)
        and not c.get("known_surviving")
        and not c.get("equivalent")
    ]
    expected = [c for c in canaries if survived(c)]

    if expected:
        print()
        print("  survivors:")
        for c in expected:
            if c.get("equivalent"):
                why = "declared equivalent — the mutation has no observable effect"
            elif c.get("known_surviving"):
                why = "declared known blind spot — recorded in the corpus with the reason"
            else:
                why = "UNEXPECTED — the suite does not detect this defect"
            print(f"    - {c['id']}  ({why})")

    holdout = [c for c in canaries if c.get("split") == "holdout" and not c.get("equivalent")]
    if holdout:
        killed = sum(1 for c in holdout if c["outcome"] == "killed")
        rate = killed / len(holdout)
        print()
        print(f"  HELD-OUT kill rate: {rate:.2f} ({killed}/{len(holdout)})")
        unexp = [c["id"] for c in holdout if c in unexpected]
        if unexp:
            print(f"  held-out regressions: {', '.join(unexp)}")
        # Below 0.95 on the honest split the corpus is not measuring much,
        # even when nothing technically regressed.
        if rate < 0.95 and not unexpected:
            print()
            print("  NOTE: held-out kill rate is below the 0.95 target. The corpus needs more")
            print("        canaries in the holdout split before the number means anything.")

    return 1 if unexpected else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
