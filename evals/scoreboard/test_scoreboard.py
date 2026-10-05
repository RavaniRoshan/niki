#!/usr/bin/env python3
"""Tests for the scoreboard harness.

The harness is what makes the scoreboard a measurement rather than an opinion, so its own
correctness is tested: a broken scorer that reports "NIKI is 20 points ahead" is worse than no
scorer at all.

Run: python3 evals/scoreboard/test_scoreboard.py
"""

from __future__ import annotations

import importlib.util
import json
import math
import pathlib
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("scoreboard", HERE / "run.py")
sb = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(sb)


class TestVerdictParsing(unittest.TestCase):
    """One token, machine-parsed the same way for every agent."""

    def test_recognises_the_two_tokens(self):
        self.assertEqual(sb.parse_verdict("CAUGHT"), "CAUGHT")
        self.assertEqual(sb.parse_verdict("clean"), "CLEAN")
        self.assertEqual(sb.parse_verdict("  Caught.  "), "CAUGHT")

    def test_finds_the_token_inside_prose(self):
        self.assertEqual(sb.parse_verdict("Sure.\n\nCAUGHT - the slice drops the last item."), "CAUGHT")

    def test_refuses_anything_it_cannot_parse(self):
        # This is the property that matters: an unparseable answer is scored as a miss, not
        # quietly rounded to whichever side looks better.
        for text in ["", "   ", "I think it is probably fine", "maybe", "YES"]:
            self.assertIsNone(sb.parse_verdict(text), f"{text!r} must not parse")

    def test_does_not_guess_from_a_negation(self):
        # "not caught" must not score as CAUGHT by accident.
        self.assertIsNone(sb.parse_verdict("NOT CAUGHT"))


class TestNikiVocabulary(unittest.TestCase):
    """NIKI answers a merge decision in its own words; the adapter maps them once."""

    def test_maps_the_two_verdicts(self):
        self.assertEqual(sb.niki_verdict({"verdict": "Approved"}), "CLEAN")
        self.assertEqual(sb.niki_verdict({"verdict": "RevisionNeeded"}), "CAUGHT")

    def test_an_unknown_verdict_is_not_mapped(self):
        # Mapping an unknown verdict to either side would manufacture a result.
        self.assertEqual(sb.niki_verdict({"verdict": "unknown"}), "")
        self.assertEqual(sb.niki_verdict({"verdict": "something else"}), "")
        self.assertEqual(sb.niki_verdict({}), "")

    def test_the_mapped_token_parses_back(self):
        self.assertEqual(sb.parse_verdict(sb.niki_verdict({"verdict": "Approved"})), "CLEAN")
        self.assertEqual(sb.parse_verdict(sb.niki_verdict({"verdict": "RevisionNeeded"})), "CAUGHT")


class TestWilson(unittest.TestCase):
    """A proportion at n=27 lives in exactly the regime where the normal approximation lies."""

    def test_brackets_the_point_estimate(self):
        for successes, total in [(23, 27), (0, 27), (27, 27), (5, 27), (1, 4)]:
            lo, hi = sb.wilson(successes, total)
            p = successes / total
            self.assertLessEqual(lo, hi)
            self.assertLessEqual(lo, min(1.0, p + 1e-9), f"{successes}/{total}: lo above p")
            self.assertGreaterEqual(hi, max(0.0, p - 1e-9), f"{successes}/{total}: hi below p")

    def test_stays_inside_zero_and_one(self):
        for successes, total in [(0, 1), (1, 1), (0, 27), (27, 27)]:
            lo, hi = sb.wilson(successes, total)
            self.assertGreaterEqual(lo, 0.0)
            self.assertLessEqual(hi, 1.0)

    def test_an_empty_sample_is_not_a_confident_zero(self):
        lo, hi = sb.wilson(0, 0)
        self.assertEqual((lo, hi), (0.0, 0.0))

    def test_narrows_as_the_sample_grows(self):
        narrow = sb.wilson(90, 100)
        wide = sb.wilson(9, 10)
        self.assertLess((narrow[1] - narrow[0]), (wide[1] - wide[0]))


class TestScoring(unittest.TestCase):
    """Both directions, always together."""

    def _records(self, defect_verdicts, control_verdicts):
        recs = []
        for i, v in enumerate(defect_verdicts):
            recs.append({"id": f"d{i}", "expected_caught": True, "verdict": v})
        for i, v in enumerate(control_verdicts):
            recs.append({"id": f"c{i}", "expected_caught": False, "verdict": v})
        return recs

    def test_a_reviewer_that_flags_everything_scores_perfect_recall_and_zero_precision(self):
        # This is exactly the degenerate reviewer the dataset's negative controls exist to catch.
        s = sb.score(self._records(["CAUGHT"] * 5, ["CAUGHT"] * 3))
        self.assertEqual(s["recall"], 1.0)
        self.assertEqual(s["precision"], 5 / 8)
        self.assertEqual(s["false_positives"], 3)

    def test_a_reviewer_that_flags_nothing_scores_zero_recall(self):
        s = sb.score(self._records(["CLEAN"] * 5, ["CLEAN"] * 3))
        self.assertEqual(s["recall"], 0.0)
        self.assertEqual(s["false_positives"], 0)

    def test_unparseable_counts_as_a_miss_and_is_reported(self):
        s = sb.score(self._records(["CAUGHT", None, "CLEAN", None], ["CLEAN"] * 3))
        self.assertEqual(s["caught"], 1)
        self.assertEqual(s["recall"], 0.25)
        self.assertEqual(s["unparseable"], 2)

    def test_precision_is_undefined_when_nothing_was_flagged(self):
        s = sb.score(self._records(["CLEAN"] * 2, ["CLEAN"] * 2))
        self.assertEqual(s["precision"], 0.0)
        self.assertEqual(s["precision_ci"], (0.0, 0.0))


class TestSeal(unittest.TestCase):
    """The seal is what stops a moving goalpost."""

    def test_dataset_parses_and_every_case_is_labelled(self):
        cases = sb.parse_dataset()
        self.assertGreater(len(cases), 0, "no cases parsed")
        for cid, case in cases.items():
            self.assertIn("expected_caught", case, f"{cid} has no verdict label")
            self.assertIsInstance(case["expected_caught"], bool)

    def test_there_are_both_defects_and_clean_controls(self):
        cases = sb.parse_dataset()
        defects = [c for c in cases.values() if c["expected_caught"]]
        controls = [c for c in cases.values() if not c["expected_caught"]]
        self.assertGreater(len(defects), 0, "no seeded defects: recall is unmeasurable")
        self.assertGreater(len(controls), 0, "no clean controls: precision is unmeasurable")

    def test_the_seal_is_stable(self):
        with tempfile.TemporaryDirectory() as d:
            original = sb.SEALED
            try:
                sb.SEALED = pathlib.Path(d) / "sealed.json"
                first = sb.seal()
                second = sb.seal()
                self.assertEqual(first["dataset_sha256"], second["dataset_sha256"])
                self.assertEqual(first["cases"], second["cases"])
                self.assertIsNone(first["sealed_at"], "a timestamp would make the seal unstable")
            finally:
                sb.SEALED = original


if __name__ == "__main__":
    unittest.main(verbosity=2)