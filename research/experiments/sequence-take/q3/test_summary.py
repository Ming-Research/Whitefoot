#!/usr/bin/env python3
"""Q3 reduction controls, explicitly run by the temporary experiment workflow."""
import contextlib
import csv
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "summarize.py"
SPEC = importlib.util.spec_from_file_location("sequence_take_summary", SCRIPT)
SUMMARY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SUMMARY)


def write_table(path, rows):
    with path.open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(rows)


class ReductionControls(unittest.TestCase):
    def data(self, root, omit=None, verdict=0, bad_verdict=5):
        rows = []
        for block in range(2):
            for cpus, drivers in (("0", 1), ("0,2,4,6,8,10,12,14", 8)):
                for arm in SUMMARY.Q3_ARMS:
                    if arm == omit:
                        continue
                    rows.append(dict(block=block, pos=1, kind="q3", arm=arm, w=0,
                                     cpus=cpus, drivers=drivers, contexts=64, ops=100,
                                     ns=1000, ops_per_s=100000000, check=7))
        write_table(root / "raw.tsv", rows)
        checks = []
        for block in range(2):
            for arm, code in (("c_t1", verdict), ("c_t2", 0), ("c_tbad", bad_verdict)):
                checks.append(dict(block=block, arm=arm, attempts=1, status=code,
                                   verdict=code, expect="ok", line="synthetic oracle control"))
        write_table(root / "correct.tsv", checks)

    def reduce(self, root):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            status = SUMMARY.main(str(root), smoke=True)
        return status, output.getvalue()

    def test_complete_controls_reduce(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            self.data(root)
            status, output = self.reduce(root)
            self.assertEqual(status, 0, output)
            self.assertIn("Q3 acceptance", output)

    def test_missing_atomic_twin_is_detected(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            self.data(root, omit="T1t")
            status, output = self.reduce(root)
            self.assertEqual(status, 1, output)
            self.assertIn("missing Q3 cells for T1t", output)

    def test_failed_real_time_check_is_detected_even_with_ok_label(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            self.data(root, verdict=4)
            status, output = self.reduce(root)
            self.assertEqual(status, 1, output)
            self.assertIn("required verdict", output)

    def test_wrong_arm_that_passes_is_detected(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            self.data(root, bad_verdict=0)
            status, output = self.reduce(root)
            self.assertEqual(status, 1, output)
            self.assertIn("required verdict", output)


if __name__ == "__main__":
    unittest.main()
