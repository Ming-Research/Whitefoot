"""Manual workflow controls: a wrong ratio, disagreeing twin, straddling interval
or missing input cannot pass."""
import tempfile
import unittest
from pathlib import Path
from summarize import ARMS, WIDTHS, MANIFEST, load, summarize

class VerdictTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "rows.tsv"
        self.inspection = {name: dict(hot_work_survives=True, evidence="controlled test image",
                                    check_compiles_to="load and branch") for name in MANIFEST}

    def write(self, ratio=1.0, twin=None, attempts=(1,), one_ratio=1.0, twin_widths=WIDTHS, rounds=2, swing=0.0):
        lines = []
        for name in MANIFEST:
            for width in WIDTHS:
                for attempt in attempts:
                    for arm in ARMS:
                        for round_id in range(rounds):
                            for sample in (0, 1):
                                factor = 1.0
                                if arm in ("demand", "twin"):
                                    factor = one_ratio if width == 1 else ratio
                                    factor *= 1 + swing * (round_id % 2 * 2 - 1)
                                    if twin is not None and arm == "twin" and width in twin_widths: factor *= twin
                                value = int(1000000000 * factor)
                                lines.append(f"{name}\t{arm}\t{width}\t{round_id}\t{attempt}\t{sample}\t{value}\t{value}\t1\n")
        self.path.write_text("".join(lines))

    def test_pass_requires_an_inspected_surviving_site(self):
        self.write()
        self.assertEqual({r["status"] for r in summarize(self.path, self.inspection)}, {"pass"})
        self.assertEqual({r["status"] for r in summarize(self.path)}, {"inconclusive"})
        self.assertEqual({r["status"] for r in summarize(self.path, self.inspection, sizing=True)}, {"inconclusive"})

    def test_exceeded_bound_needs_one_rerun_then_fails(self):
        self.write(ratio=1.04)
        self.assertIn("needs-rerun", {r["status"] for r in summarize(self.path, self.inspection)})
        self.write(ratio=1.04, attempts=(1, 2))
        self.assertIn("fail", {r["status"] for r in summarize(self.path, self.inspection)})

    def test_one_worker_is_two_sided_and_a_disagreeing_twin_voids_the_cell(self):
        self.write(one_ratio=0.97, attempts=(1, 2))
        self.assertTrue(any(r["width"] == 1 and r["status"] == "fail" for r in summarize(self.path, self.inspection)))
        self.write(ratio=1.3, twin=1.05, attempts=(1, 2))
        self.assertEqual({r["status"] for r in summarize(self.path, self.inspection)}, {"void"})

    def test_a_void_width_leaves_the_other_widths_verdicts(self):
        self.write(ratio=1.3, twin=1.05, attempts=(1, 2), twin_widths=(1,))
        verdicts = {r["width"]: r["status"] for r in summarize(self.path, self.inspection) if r["workload"] == "small_split"}
        self.assertEqual(verdicts, {1: "void", 4: "fail", 8: "fail"})

    def test_an_interval_straddling_the_bound_decides_nothing(self):
        # Rounds alternate 1.02 * (1 -/+ 0.03): the median sits on the bound
        # and the interval reaches both sides of it.
        self.write(ratio=1.02, rounds=10, swing=0.03)
        verdicts = {(r["workload"], r["width"]): r["status"] for r in summarize(self.path, self.inspection)}
        self.assertEqual(verdicts[("large_helper", 4)], "inconclusive")
        self.write(ratio=0.99, rounds=10, swing=0.005)
        verdicts = {(r["workload"], r["width"]): r["status"] for r in summarize(self.path, self.inspection)}
        self.assertEqual(verdicts[("large_helper", 4)], "pass")

    def test_a_decision_point_may_cost_one_nanosecond_per_execution(self):
        # Every arm takes 1 s here; small_split's 200,000,000 decisions allow
        # 0.2 of it, while a workload without decision points keeps 1.02.
        self.write(ratio=1.1, attempts=(1, 2))
        verdicts = {(r["workload"], r["width"]): r["status"] for r in summarize(self.path, self.inspection)}
        self.assertEqual(verdicts[("small_split", 4)], "pass")
        self.assertEqual(verdicts[("large_helper", 4)], "fail")

    def test_missing_and_duplicate_rows_are_errors(self):
        self.write()
        lines = self.path.read_text().splitlines(keepends=True)
        self.path.write_text("".join(lines[:-1]))
        with self.assertRaises(ValueError): summarize(self.path, self.inspection)
        self.path.write_text("".join(lines + [lines[-1]]))
        with self.assertRaises(ValueError): load(self.path)

if __name__ == "__main__":
    unittest.main()
