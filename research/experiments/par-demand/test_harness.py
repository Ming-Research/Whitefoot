"""Manual workflow controls: a wrong ratio, noisy twin or missing input cannot pass."""
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

    def write(self, ratio=1.0, twin=None, attempts=(1,), one_ratio=1.0, twin_widths=WIDTHS):
        lines = []
        for name in MANIFEST:
            for width in WIDTHS:
                for attempt in attempts:
                    for arm in ARMS:
                        for round_id in range(2):
                            for sample in (0, 1):
                                factor = 1.0
                                if arm in ("demand", "twin"):
                                    factor = one_ratio if width == 1 else ratio
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

    def test_one_worker_is_two_sided_and_twins_over_two_percent_decide_nothing(self):
        self.write(one_ratio=0.97, attempts=(1, 2))
        self.assertTrue(any(r["width"] == 1 and r["status"] == "fail" for r in summarize(self.path, self.inspection)))
        self.write(ratio=1.3, twin=1.05, attempts=(1, 2))
        self.assertEqual({r["status"] for r in summarize(self.path, self.inspection)}, {"inconclusive"})

    def test_a_noisy_width_leaves_the_other_widths_verdicts(self):
        self.write(ratio=1.3, twin=1.05, attempts=(1, 2), twin_widths=(1,))
        verdicts = {r["width"]: r["status"] for r in summarize(self.path, self.inspection) if r["workload"] == "small_split"}
        self.assertEqual(verdicts, {1: "inconclusive", 4: "fail", 8: "fail"})

    def test_missing_and_duplicate_rows_are_errors(self):
        self.write()
        lines = self.path.read_text().splitlines(keepends=True)
        self.path.write_text("".join(lines[:-1]))
        with self.assertRaises(ValueError): summarize(self.path, self.inspection)
        self.path.write_text("".join(lines + [lines[-1]]))
        with self.assertRaises(ValueError): load(self.path)

if __name__ == "__main__":
    unittest.main()
