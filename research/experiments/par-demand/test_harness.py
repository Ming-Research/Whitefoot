"""Manual workflow controls: a wrong ratio, disagreeing twin, straddling interval
or missing input cannot pass."""
import tempfile
import unittest
from unittest.mock import patch
import json
import measure
import contextlib
import io
from pathlib import Path
from summarize import (ARMS, E2_ARMS, E3_ARMS, E4_ARMS, E3_WIDTHS, WIDTHS, MANIFEST, E5A_ARMS, E5A_PADDING, E5A_MANIFEST, layout_floor, load, summarize,
                       attempt_result_e2, attempt_result_e3, attempt_result_e4, cause_verdict, e3_round_count, e4_round_count)
from measure import cpu_list, performance_cores, demand_setting

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



class Experiment2Tests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "rows.tsv"
        self.inspection = {name: dict(hot_work_survives=True, evidence="controlled E2 images",
                                    check_compiles_to="load and branch") for name in MANIFEST}

    def write(self, changes=None, attempts=(1,), rerun_changes=None):
        # Independently chosen baseline: parallel wall 1/2 seq; CPU 1 seq.
        # H3 allows 1.3 CPU at W=4, 1.5 at W=8. The first call carries an
        # extra 70 ms of CPU above wall that never enters a steady verdict.
        values = {"seq": (1.0, 1.0), "par": (0.5, 1.0),
                  "demand": (0.5, 1.0), "idle1": (0.5, 1.0), "twin": (0.5, 1.0)}
        lines = []
        for name in MANIFEST:
            for width in WIDTHS:
                for attempt in attempts:
                    selected = dict(values)
                    selected.update((rerun_changes if attempt == 2 and rerun_changes is not None else changes) or {})
                    if "demand" in (changes or {}) and "twin" not in (changes or {}):
                        selected["twin"] = selected["demand"]
                    for arm, (wall, cpu) in selected.items():
                        for r in range(2):
                            for sample in (0, 1):
                                first_cpu = wall + 0.07 if sample == 0 else cpu
                                lines.append(f"{name}\t{arm}\t{width}\t{r}\t{attempt}\t{sample}\t{int(wall * 1e9)}\t{int(first_cpu * 1e9)}\t1\n")
        self.path.write_text("".join(lines))

    def row(self, width=4, name="large_helper", inspection=None):
        return next(r for r in summarize(self.path, self.inspection if inspection is None else inspection,
                                         experiment=2) if r["workload"] == name and r["width"] == width)

    def test_h1_exceed_for_each_candidate_needs_exactly_one_rerun(self):
        self.write({"demand": (1.04, 1.0)})
        self.assertEqual(self.row()["verdicts"]["E2-H1-demand"], "needs-rerun")
        self.write({"demand": (1.04, 1.0)}, attempts=(1, 2))
        self.assertEqual(self.row()["verdicts"]["E2-H1-demand"], "fail")
        self.write({"idle1": (1.04, 1.0)}, attempts=(1, 2))
        self.assertEqual(self.row()["verdicts"]["E2-H1-idle1"], "fail")
        self.write({"demand": (1.04, 1.0)}, attempts=(1, 2), rerun_changes={"demand": (0.5, 1.0)})
        self.assertEqual(self.row()["verdicts"]["E2-H1-demand"], "inconclusive")

    def test_h1_keeps_the_prospective_decision_point_allowance(self):
        self.write({"demand": (1.1, 1.0), "idle1": (1.1, 1.0)}, attempts=(1, 2))
        for arm in ("demand", "idle1"):
            self.assertEqual(self.row(name="small_split")["verdicts"][f"E2-H1-{arm}"], "pass")
            self.assertEqual(self.row()["verdicts"][f"E2-H1-{arm}"], "fail")

    def test_keep_detects_speedup_loss_for_each_candidate(self):
        for arm in ("demand", "idle1"):
            self.write({arm: (0.54, 1.0)}, attempts=(1, 2))
            row = self.row()
            self.assertEqual(row["verdicts"][f"E2-keep-{arm}"], "fail")
            self.assertEqual(row["verdicts"][f"E2-H1-{arm}"], "pass")
        self.write({"par": (1.0, 1.0), "demand": (1.1, 1.0)}, attempts=(1, 2))
        self.assertEqual(self.row()["verdicts"]["E2-keep-demand"], "not-applicable")

    def test_h3_detects_waste_credits_only_time_saved_and_scales_by_width(self):
        for arm in ("demand", "idle1"):
            self.write({arm: (0.5, 1.4)}, attempts=(1, 2))
            row = self.row()
            self.assertEqual(row["verdicts"][f"E2-H3-{arm}"], "fail")
            self.assertAlmostEqual(row["initial"]["h3"][arm]["median"], 0.1)
            self.assertEqual(self.row(width=8)["verdicts"][f"E2-H3-{arm}"], "pass")
        self.write({"demand": (1.2, 1.2)}, attempts=(1, 2))
        self.assertAlmostEqual(self.row()["initial"]["h3"]["demand"]["median"], 0.1)
        self.assertIn("par", self.row()["initial"]["h3"])

    def test_idle_detects_wall_loss_even_when_cpu_improves(self):
        self.write({"idle1": (0.515, 0.8)}, attempts=(1, 2))
        row = self.row()
        self.assertEqual(row["verdicts"]["E2-idle"], "fail")
        self.assertAlmostEqual(row["initial"]["idle_cpu_ratio"]["median"], 0.8)
        self.assertEqual(row["initial"]["idle_cpu_change_ns"]["median"], -200000000)

    def test_twin_voids_every_rule_and_a_void_rerun_cannot_fail(self):
        self.write({"demand": (1.2, 2.0), "twin": (1.25, 2.0)}, attempts=(1, 2))
        row = self.row()
        self.assertEqual(set(row["verdicts"].values()), {"void"})
        self.write({"idle1": (1.2, 2.0)}, attempts=(1, 2),
                   rerun_changes={"idle1": (1.2, 2.0), "twin": (0.52, 1.0)})
        self.assertEqual(self.row()["status"], "void")

    def test_controls_inspection_and_startup(self):
        self.write()
        row = self.row()
        self.assertEqual(row["status"], "pass")
        self.assertEqual(row["initial"]["first_call_cpu_above_wall_ns"]["idle1"]["median"], 70000000)
        self.assertEqual(row["initial"]["wall_over_seq"]["par"]["interval"], [0.5, 0.5])
        self.assertEqual(self.row(inspection={})["status"], "inconclusive")
        self.write({"demand": (2.0, 3.0), "idle1": (2.0, 3.0)}, attempts=(1, 2))
        for name in ("spine", "small_constant"):
            self.assertEqual(self.row(name=name)["status"], "not-applicable")
        self.assertEqual(self.row(width=1)["status"], "not-applicable")
        self.inspection["large_helper"]["optimized_away_in_both"] = True
        self.assertEqual(self.row()["status"], "not-applicable")

    def test_paired_statistics_and_straddling_do_not_use_ratio_of_medians(self):
        arms = {arm: {r: pair for r, pair in enumerate(((100, 100), (200, 200), (1000, 1000)))} for arm in E2_ARMS}
        arms["demand"] = arms["twin"] = {0: (200, 100), 1: (200, 200), 2: (500, 1000)}
        result = attempt_result_e2(arms, 4)
        self.assertEqual(result["wall_over_seq"]["demand"]["median"], 1.0)
        self.assertEqual(result["rules"]["E2-H1-demand"]["status"], "inconclusive")
        self.assertEqual(result["rules"]["E2-keep-demand"]["status"], "not-applicable")

    def test_missing_first_call_arm_and_changed_comparison_count_raise(self):
        self.write()
        lines = self.path.read_text().splitlines(keepends=True)
        first = next(i for i, line in enumerate(lines) if line.split("\t")[5] == "0")
        self.path.write_text("".join(lines[:first] + lines[first + 1:]))
        with self.assertRaises(ValueError): self.row()
        self.write()
        lines = self.path.read_text().splitlines(keepends=True)
        self.path.write_text("".join(line for line in lines if "\tidle1\t" not in line))
        with self.assertRaises(ValueError): self.row()
        self.path.write_text("".join(lines[:-1] + [lines[-1].rsplit("\t", 1)[0] + "\t2\n"]))
        with self.assertRaises(ValueError): self.row()


class Experiment3Tests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "rows.tsv"

    def write(self, changes=None, rounds=6):
        values = {arm: (0.5, 1.4) for arm in E3_ARMS}
        values["seq"] = (1.0, 1.0)
        values.update(changes or {})
        lines = []
        for name in MANIFEST:
            for width in E3_WIDTHS:
                for arm, (wall, cpu) in values.items():
                    for r in range(rounds):
                        for sample in (0, 1):
                            lines.append(f"{name}\t{arm}\t{width}\t{r}\t1\t{sample}\t{int(wall * 1e9)}\t{int(cpu * 1e9)}\t1\n")
        self.path.write_text("".join(lines))
        sample_dir = self.path.parent / "sizing-e3"
        sample_dir.mkdir(exist_ok=True)
        (sample_dir / "measurements.tsv").write_text("".join(lines))

    def test_neutral_rejects_registered_causes_and_reports_h3_without_bounds(self):
        self.write()
        result = summarize(self.path, experiment=3)
        self.assertEqual({r["verdict"] for r in result["causes"]}, {"rejected"})
        cell = next(c for c in result["cells"] if c["width"] == 4)
        self.assertAlmostEqual(cell["h3"]["demand"]["median"], 0.1)
        self.assertEqual(cell["wall_over_demand"]["seq"]["interval"], [2, 2])
        self.assertNotIn("rules", cell)

    def test_improvement_supports_but_regression_does_not(self):
        self.write({arm: (0.4, 1.0) for arm in ("order", "seed", "extent", "dedup")})
        result = summarize(self.path, experiment=3)
        self.assertEqual({r["verdict"] for r in result["causes"]}, {"supported"})
        self.write({arm: (0.7, 2.0) for arm in ("order", "seed", "extent", "dedup")})
        self.assertEqual({r["verdict"] for r in summarize(self.path, experiment=3)["causes"]}, {"undecided"})

    def test_extent_needs_wall_and_cpu_and_dedup_uses_margin_overlap(self):
        self.write({"extent": (0.4, 1.4), "dedup": (0.5, 1.2)})
        cell = summarize(self.path, experiment=3)["cells"][0]
        self.assertEqual(cause_verdict(cell, "extent"), "undecided")
        cell["h3"]["dedup"]["interval"] = [0.05, 0.2]
        self.assertEqual(cause_verdict(cell, "dedup"), "rejected")

    def test_void_and_sizing_never_support_or_reject_a_cause(self):
        self.write({"twin": (0.6, 1.4)})
        result = summarize(self.path, experiment=3)
        self.assertEqual({c["status"] for c in result["cells"]}, {"void"})
        self.assertEqual({r["verdict"] for r in result["causes"]}, {"undecided"})
        self.write()
        result = summarize(self.path, sizing=True, experiment=3)
        self.assertEqual(result["decisive_rounds"], 6)
        self.assertEqual({r["verdict"] for r in result["causes"]}, {"undecided"})
        for cell in result["cells"]:
            cell["wall_over_demand"]["order"] = dict(median=1, interval=[0.5, 1.5])
        self.assertEqual(e3_round_count(result["cells"]), 30)
        self.write(rounds=5)
        with self.assertRaises(ValueError): summarize(self.path, sizing=True, experiment=3)

    def test_missing_wrong_unpaired_and_rerun_evidence_raise(self):
        self.write()
        original = self.path.read_text().splitlines(keepends=True)
        variants = [original[:-1], original + [original[0]],
                    [line for line in original if "\tseed\t" not in line],
                    [original[0].replace("\t4\t0\t1\t", "\t1\t0\t1\t")] + original[1:],
                    [original[0].replace("\t4\t0\t1\t", "\t4\t0\t2\t")] + original[1:],
                    [original[0].rsplit("\t", 1)[0] + "\t2\n"] + original[1:]]
        for lines in variants:
            self.path.write_text("".join(lines))
            with self.assertRaises(ValueError): summarize(self.path, experiment=3)
        arms = {arm: {0: (100, 100), 1: (200, 100), 2: (1000, 100)} for arm in E3_ARMS}
        arms["order"] = {0: (200, 100), 1: (200, 100), 2: (500, 100)}
        self.assertEqual(attempt_result_e3(arms, 4)["wall_over_demand"]["order"]["median"], 1)
        del arms["order"][1]
        with self.assertRaises(ValueError): attempt_result_e3(arms, 4)
        self.write()
        (self.path.parent / "sizing-e3/measurements.tsv").unlink()
        with self.assertRaisesRegex(ValueError, "sizing evidence"):
            summarize(self.path, experiment=3)
        self.write(rounds=7)
        with self.assertRaisesRegex(ValueError, "six rounds"):
            summarize(self.path, experiment=3)

    def test_measurement_sizes_then_freezes_and_counters_are_unjudged(self):
        build = Path(self.directory.name)
        for arm in E3_ARMS:
            (build / arm).mkdir()
            for name in MANIFEST:
                (build / arm / name).write_bytes((name + ("demand" if arm == "twin" else arm)).encode())
        observed = []
        def fake_run(command, **kwargs):
            if command[0] == "git":
                return type("Result", (), {"stdout": "fixture\n"})()
            image, mode, arm, width, round_id, attempt = command[3:]
            self.assertEqual(mode, "measure")
            self.assertEqual(attempt, "1")
            self.assertIn(int(width), E3_WIDTHS)
            self.assertEqual(kwargs["env"]["WF_PAR_DEMAND"], demand_setting(3, arm))
            observed.append((arm, width, round_id, kwargs["env"].get("WF_SCHED_REPORT")))
            for sample in (0, 1):
                kwargs["stdout"].write(f"{Path(image).name}\t{arm}\t{width}\t{round_id}\t1\t{sample}\t100\t100\t1\n")
        def invoke(extra=()):
            with patch("sys.argv", ["measure.py", "--build", str(build), "--experiment", "3", *extra]), \
                 patch.object(measure, "run", side_effect=fake_run), \
                 patch.object(measure.platform, "system", return_value="Linux"), \
                 patch.object(measure.shutil, "which", return_value="/usr/bin/taskset"), \
                 patch.object(measure, "performance_cores", return_value=(list(range(8)), {})), \
                 contextlib.redirect_stdout(io.StringIO()):
                measure.main()
        invoke()
        count = len(MANIFEST) * len(E3_ARMS) * len(E3_WIDTHS)
        self.assertEqual(len(observed), count * 12)
        self.assertTrue(all(report is None for _, _, _, report in observed))
        self.assertEqual(json.loads((build / "identity.json").read_text())["rounds"], 6)
        self.assertTrue((build / "sizing-e3/measurements.tsv").exists())
        summary = (build / "summary.json").read_bytes()
        observed.clear()
        (build / "counter-build").touch()
        invoke(("--instrumented",))
        self.assertEqual(len(observed), count)
        self.assertTrue(all(report == "2" for _, _, _, report in observed))
        self.assertEqual((build / "summary.json").read_bytes(), summary)
        with self.assertRaises(SystemExit): invoke()


class Experiment4Tests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "rows.tsv"
        self.inspection = {name: dict(hot_work_survives=True, evidence="controlled E4 images",
                                    check_compiles_to="load and branch") for name in MANIFEST}

    def write(self, changes=None, attempts=(1,), rerun_changes=None, rounds=6):
        baseline = {"seq": (1.0, 1.0), "par": (0.5, 1.0),
                    "demand": (0.5, 1.0), "static": (0.5, 1.0), "twin": (0.5, 1.0)}
        lines = []
        for name in MANIFEST:
            for width in WIDTHS:
                for attempt in attempts:
                    changes_for_attempt = rerun_changes if attempt == 2 and rerun_changes is not None else changes
                    selected = dict(baseline)
                    selected.update(changes_for_attempt or {})
                    if "twin" not in (changes_for_attempt or {}):
                        selected["twin"] = selected["demand"]
                    for arm, (wall, cpu) in selected.items():
                        for r in range(rounds):
                            for sample in (0, 1):
                                first_cpu = wall + 0.07 if sample == 0 else cpu
                                lines.append(f"{name}\t{arm}\t{width}\t{r}\t{attempt}\t{sample}\t{int(wall * 1e9)}\t{int(first_cpu * 1e9)}\t1\n")
        self.path.write_text("".join(lines))
        sample_dir = self.path.parent / "sizing-e4"
        sample_dir.mkdir(exist_ok=True)
        # Saved sample always has exactly six rounds and no reruns. Constant
        # fixtures select n=6, independent of their passing/failing ratios.
        sample_lines = [line for line in lines if line.split("\t")[4] == "1" and int(line.split("\t")[3]) < 6]
        (sample_dir / "measurements.tsv").write_text("".join(sample_lines))

    def row(self, width=4, name="large_helper", inspection=None):
        result = summarize(self.path, self.inspection if inspection is None else inspection, experiment=4)
        return next(cell for cell in result["cells"] if cell["workload"] == name and cell["width"] == width)

    def test_seq_is_literal_with_no_allowance_and_one_rerun(self):
        self.write({"par": (1, 1), "static": (1, 1), "demand": (1, 1)})
        for width in WIDTHS:
            self.assertEqual(self.row(width=width)["verdicts"]["E4-seq"], "pass")
        self.write({"demand": (1.01, 1)})
        self.assertEqual(self.row(name="small_split")["verdicts"]["E4-seq"], "needs-rerun")
        self.write({"demand": (1.01, 1)}, attempts=(1, 2))
        for width in WIDTHS:
            self.assertEqual(self.row(width=width, name="small_split")["verdicts"]["E4-seq"], "fail")
        self.write({"demand": (1.01, 1)}, attempts=(1, 2), rerun_changes={"demand": (0.5, 1)})
        self.assertEqual(self.row()["verdicts"]["E4-seq"], "inconclusive")

    def test_par_and_gain_each_keep_speedups_and_require_a_reference_gain(self):
        for rule, reference in (("E4-par", "par"), ("E4-gain", "static")):
            self.write({"demand": (0.525, 1)})
            self.assertEqual(self.row()["verdicts"][rule], "pass")
            self.write({reference: (0.4, 1)})
            self.assertEqual(self.row()["verdicts"][rule], "needs-rerun")
            self.write({reference: (0.4, 1)}, attempts=(1, 2))
            self.assertEqual(self.row()["verdicts"][rule], "fail")
            self.assertEqual(self.row()["verdicts"]["E4-seq"], "pass")
            self.write({reference: (1, 1)}, attempts=(1, 2))
            self.assertEqual(self.row()["verdicts"][rule], "not-applicable")

    def test_h3_keeps_cpu_margin_width_and_only_saved_wall_credit(self):
        self.write({"demand": (0.5, 1.4)})
        self.assertEqual(self.row()["verdicts"]["E4-H3"], "needs-rerun")
        self.write({"demand": (0.5, 1.4)}, attempts=(1, 2))
        self.assertEqual(self.row()["verdicts"]["E4-H3"], "fail")
        self.assertAlmostEqual(self.row()["initial"]["h3"]["demand"]["median"], 0.1)
        self.assertEqual(self.row(width=8)["verdicts"]["E4-H3"], "pass")
        self.write({"demand": (1.2, 1.2)}, attempts=(1, 2))
        self.assertAlmostEqual(self.row()["initial"]["h3"]["demand"]["median"], 0.1)
        self.assertEqual(set(self.row()["initial"]["h3"]), {"par", "demand", "static"})

    def test_each_rule_needs_inspection_and_twin_agreement(self):
        self.write()
        self.assertEqual(set(self.row()["verdicts"].values()), {"pass"})
        for field, wrong in (("hot_work_survives", False), ("evidence", ""), ("check_compiles_to", "")):
            missing = {"large_helper": dict(self.inspection["large_helper"])}
            del missing["large_helper"][field]
            self.assertEqual(set(self.row(inspection=missing)["verdicts"].values()), {"inconclusive"})
            missing["large_helper"][field] = wrong
            self.assertEqual(set(self.row(inspection=missing)["verdicts"].values()), {"inconclusive"})
        self.assertEqual(set(self.row(inspection={})["verdicts"].values()), {"inconclusive"})
        self.write({"twin": (0.6, 1)})
        self.assertEqual(set(self.row()["verdicts"].values()), {"void"})
        self.write({"demand": (1.2, 2)}, attempts=(1, 2),
                   rerun_changes={"demand": (1.2, 2), "twin": (1.3, 2)})
        self.assertEqual(self.row()["status"], "void")

    def test_controls_startup_pairing_and_straddling(self):
        self.write({"demand": (2, 3)}, attempts=(1, 2))
        for name in ("spine", "small_constant"):
            self.assertEqual(self.row(name=name)["status"], "not-applicable")
        self.inspection["large_helper"]["optimized_away_in_both"] = True
        self.assertEqual(self.row()["status"], "not-applicable")
        arms = {arm: {0: (100, 100), 1: (200, 200), 2: (1000, 1000)} for arm in E4_ARMS}
        arms["demand"] = arms["twin"] = {0: (200, 100), 1: (200, 200), 2: (500, 1000)}
        result = attempt_result_e4(arms, 4)
        self.assertEqual(result["wall_over_seq"]["demand"]["median"], 1)
        self.assertEqual(result["rules"]["E4-seq"]["status"], "inconclusive")
        del arms["static"][1]
        with self.assertRaises(ValueError): attempt_result_e4(arms, 4)
        self.write()
        self.assertEqual(self.row()["initial"]["first_call_cpu_above_wall_ns"]["demand"]["median"], 70000000)

    def test_wrong_missing_and_changed_round_evidence_raise(self):
        self.write()
        original = self.path.read_text().splitlines(keepends=True)
        variants = [original[:-1], original + [original[0]],
                    [line for line in original if "\tstatic\t" not in line],
                    [line for line in original if line.split("\t")[5] != "0"],
                    [original[0].rsplit("\t", 1)[0] + "\t2\n"] + original[1:],
                    [original[0].replace("\t1\t0\t1\t", "\t1\t0\t3\t")] + original[1:]]
        for lines in variants:
            self.path.write_text("".join(lines))
            with self.assertRaises(ValueError): self.row()
        self.write(attempts=(1, 2))
        rows = [line.split("\t") for line in self.path.read_text().splitlines()]
        for row in rows:
            if row[4] == "2":
                row[-1] = "2"
        self.path.write_text("".join("\t".join(row) + "\n" for row in rows))
        with self.assertRaisesRegex(ValueError, "comparison extent changed"): self.row()
        self.write()
        (self.path.parent / "sizing-e4/measurements.tsv").unlink()
        with self.assertRaisesRegex(ValueError, "sizing evidence"): self.row()
        self.write(rounds=7)
        with self.assertRaisesRegex(ValueError, "frozen"): self.row()
        self.write(rounds=5)
        with self.assertRaisesRegex(ValueError, "six rounds"): self.row()

    def test_sizing_freezes_six_to_thirty_and_cannot_pass(self):
        self.write()
        sample = summarize(self.path, sizing=True, experiment=4)
        self.assertEqual(sample["decisive_rounds"], 6)
        self.assertEqual({cell["status"] for cell in sample["cells"]}, {"inconclusive", "not-applicable"})
        cells = sample["cells"]
        rule = next(cell for cell in cells if cell["workload"] == "large_helper")["initial"]["rules"]["E4-seq"]
        rule.update(median=1, interval=[0.5, 1.5])
        self.assertEqual(e4_round_count(cells), 30)
        # 0.03 * sqrt(6/n) <= 0.02 first holds at n=14.
        rule.update(median=1, interval=[0.985, 1.015])
        self.assertEqual(e4_round_count(cells), 14)
        self.write({"demand": (1.2, 2)})
        sample = summarize(self.path, sizing=True, experiment=4)
        self.assertEqual(next(cell for cell in sample["cells"] if cell["workload"] == "large_helper")["status"], "inconclusive")
        self.write(attempts=(1, 2))
        with self.assertRaisesRegex(ValueError, "without reruns"):
            summarize(self.path, sizing=True, experiment=4)

    def test_driver_sizes_then_runs_decisive_rounds_and_reruns_once(self):
        build = self.path.parent
        for arm in E4_ARMS:
            (build / arm).mkdir()
            for name in MANIFEST:
                (build / arm / name).write_bytes((name + ("demand" if arm == "twin" else arm)).encode())
        observed = []
        def fake_run(command, **kwargs):
            if command[0] == "git":
                return type("Result", (), {"stdout": "fixture\n"})()
            image, mode, arm, width, round_id, attempt = command[3:]
            name = Path(image).name
            self.assertEqual(mode, "measure")
            self.assertEqual(command[2], ",".join(map(str, range(int(width)))))
            self.assertEqual(kwargs["env"]["WF_PAR_DEMAND"], demand_setting(4, arm))
            if "repetitions" in MANIFEST[name]:
                self.assertEqual(kwargs["env"]["WFD_REPETITIONS"], str(MANIFEST[name]["repetitions"]))
            observed.append((name, arm, width, round_id, attempt))
            # Only small_split W=4 exceeds; its twin remains identical.
            wall = 110 if name == "small_split" and width == "4" and arm in ("demand", "twin") else 100
            for sample in (0, 1):
                kwargs["stdout"].write(f"{name}\t{arm}\t{width}\t{round_id}\t{attempt}\t{sample}\t{wall}\t100\t1\n")
        with patch("sys.argv", ["measure.py", "--build", str(build), "--experiment", "4"]), \
             patch.object(measure, "run", side_effect=fake_run), \
             patch.object(measure.platform, "system", return_value="Linux"), \
             patch.object(measure.shutil, "which", return_value="/usr/bin/taskset"), \
             patch.object(measure, "performance_cores", return_value=(list(range(8)), {})), \
             contextlib.redirect_stdout(io.StringIO()):
            measure.main()
        count = len(MANIFEST) * len(E4_ARMS) * len(WIDTHS)
        self.assertEqual(len(observed), count * 12 + len(E4_ARMS) * 6)
        reruns = [row for row in observed if row[-1] == "2"]
        self.assertEqual({(row[0], row[2]) for row in reruns}, {("small_split", "4")})
        identity = json.loads((build / "identity.json").read_text())
        self.assertEqual((identity["rounds"], identity["sizing_rounds"]), (6, 6))
        sample = json.loads((build / "sizing-e4/summary.json").read_text())
        self.assertEqual(sample["decisive_rounds"], 6)
        cells = json.loads((build / "summary.json").read_text())["cells"]
        self.assertEqual(next(cell for cell in cells if cell["workload"] == "small_split" and cell["width"] == 4)["status"], "fail")
        for extra in (("--rounds", "30"), ("--instrumented",)):
            with patch("sys.argv", ["measure.py", "--build", str(build), "--experiment", "4", *extra]), \
                 contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                measure.main()


class Experiment5aTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / "measurements.tsv"

    def write(self, changes=None, rounds=6):
        values = {arm: 1.0 for arm in E5A_ARMS}
        values.update(changes or {})
        lines = []
        for name in E5A_MANIFEST:
            for arm, factor in values.items():
                for r in range(rounds):
                    for sample in (0, 1):
                        wall = int(1e9 * factor)
                        lines.append(f"{name}\t{arm}\t1\t{r}\t1\t{sample}\t{wall}\t1000000000\t1\n")
        self.path.write_text("".join(lines))
        sample_dir = self.path.parent / "sizing-e5a"
        sample_dir.mkdir(exist_ok=True)
        (sample_dir / "measurements.tsv").write_text("".join(lines))

    def test_layout_reading_separates_quiet_floor_and_inconclusive(self):
        self.write()
        result = summarize(self.path, experiment="5a")
        self.assertEqual(result["layout_verdict"], "reject-image-layout")
        self.assertEqual(result["decisive_rounds"], 6)
        self.write({"seq-shift64": 1.0016})
        self.assertEqual(summarize(self.path, experiment="5a")["layout_verdict"], "instrument-floor")
        self.write({"seq-shift64": 1.0012})
        self.assertEqual(summarize(self.path, experiment="5a")["layout_verdict"], "inconclusive")
        self.assertEqual(layout_floor([dict(median=1, interval=[0.998, 1.002])]), "instrument-floor")
        self.assertEqual(layout_floor([dict(median=1, interval=[0.9989, 1.0011])]), "inconclusive")
        # A large W1 loss is reported beside controls, never counted as a
        # layout control itself. A disagreeing twin prevents either ruling.
        self.write({"demand": 1.1, "twin": 1.1, "par": 1.05})
        self.assertEqual(summarize(self.path, experiment="5a")["layout_verdict"], "reject-image-layout")
        self.write({"twin": 1.01, "seq-shift64": 1.002})
        self.assertEqual(summarize(self.path, experiment="5a")["layout_verdict"], "inconclusive")
        self.assertEqual(summarize(self.path, sizing=True, experiment="5a")["layout_verdict"], "inconclusive")

    def test_missing_changed_wrong_width_and_rerun_evidence_raise(self):
        self.write()
        original = self.path.read_text().splitlines(keepends=True)
        variants = [original[:-1], original + [original[0]],
                    [line for line in original if "\tseq-shift4160\t" not in line],
                    [line for line in original if line.split("\t")[5] != "0"],
                    [original[0].replace("\t1\t0\t1\t", "\t4\t0\t1\t")] + original[1:],
                    [original[0].replace("\t1\t0\t1\t", "\t1\t0\t2\t")] + original[1:],
                    [original[0].rsplit("\t", 1)[0] + "\t2\n"] + original[1:]]
        for lines in variants:
            self.path.write_text("".join(lines))
            with self.assertRaises(ValueError): summarize(self.path, experiment="5a")
        self.write()
        (self.path.parent / "sizing-e5a/measurements.tsv").unlink()
        with self.assertRaisesRegex(ValueError, "sizing evidence"):
            summarize(self.path, experiment="5a")
        self.write(rounds=5)
        with self.assertRaisesRegex(ValueError, "six rounds"):
            summarize(self.path, sizing=True, experiment="5a")
        self.write(rounds=7)
        sample = self.path.parent / "sizing-e5a/measurements.tsv"
        sample.write_text("".join(line for line in self.path.read_text().splitlines(keepends=True)
                                 if int(line.split("\t")[3]) < 6))
        with self.assertRaisesRegex(ValueError, "frozen"):
            summarize(self.path, experiment="5a")

    def test_round_ratios_are_not_a_ratio_of_arm_medians(self):
        self.write()
        rows = [line.split("\t") for line in self.path.read_text().splitlines()]
        for row in rows:
            if row[0] == "records" and row[1] in ("seq", "seq-shift64"):
                values = (100, 1000, 10000) if row[1] == "seq" else (200, 5000, 1000)
                row[6] = str(values[int(row[3]) % 3])
        self.path.write_text("".join("\t".join(row) + "\n" for row in rows))
        result = summarize(self.path, sizing=True, experiment="5a")
        cell = next(cell for cell in result["cells"] if cell["workload"] == "records")
        # Paired ratios repeat 2, 5, 0.1, median 2; arm medians are both
        # 1000 and their ratio would incorrectly be 1.
        self.assertEqual(cell["controls"]["seq-shift64"]["median"], 2)

    def test_driver_pins_cpu_two_sizes_and_freezes_without_reruns(self):
        build = self.path.parent
        for arm in E5A_ARMS:
            (build / arm).mkdir()
            for name in E5A_MANIFEST:
                (build / arm / name).write_bytes((name + ("demand" if arm == "twin" else arm)).encode())
        observed = []
        def fake_run(command, **kwargs):
            if command[0] == "git":
                return type("Result", (), {"stdout": "fixture\n"})()
            self.assertEqual(command[:3], ["taskset", "-c", "2"])
            image, mode, arm, width, r, attempt = command[3:]
            self.assertEqual((mode, width, attempt), ("measure", "1", "1"))
            self.assertEqual(kwargs["env"]["WF_PAR_DEMAND"], demand_setting("5a", arm))
            observed.append((Path(image).name, arm, r))
            for sample in (0, 1):
                kwargs["stdout"].write(f"{Path(image).name}\t{arm}\t1\t{r}\t1\t{sample}\t100\t100\t1\n")
        def invoke(cores=None, extra=()):
            topology = dict(performance_cpus=list(range(16)))
            with patch("sys.argv", ["measure.py", "--build", str(build), "--experiment", "5a", *extra]), \
                 patch.object(measure, "run", side_effect=fake_run), \
                 patch.object(measure.platform, "system", return_value="Linux"), \
                 patch.object(measure.shutil, "which", return_value="/usr/bin/taskset"), \
                 patch.object(measure, "performance_cores", return_value=(list(range(0, 16, 2)) if cores is None else cores, topology)), \
                 patch.object(measure, "layout_identity", return_value={"fixture": "text shifted"}), \
                 contextlib.redirect_stdout(io.StringIO()):
                measure.main()
        invoke()
        self.assertEqual(len(observed), len(E5A_MANIFEST) * len(E5A_ARMS) * 12)
        self.assertNotEqual([r[1] for r in observed[:7]], [r[1] for r in observed[42:49]])
        identity = json.loads((build / "identity.json").read_text())
        self.assertEqual(identity["pinned"], {"1": [2]})
        self.assertEqual((identity["rounds"], identity["sizing_rounds"]), (6, 6))
        self.assertTrue((build / "sizing-e5a/measurements.tsv").exists())
        self.assertEqual(json.loads((build / "summary.json").read_text())["layout_verdict"], "reject-image-layout")
        with self.assertRaisesRegex(ValueError, "CPU 2"):
            invoke(cores=[0, 4, 6, 8, 10, 12, 14, 16])
        for extra in (("--rounds", "30"), ("--instrumented",)):
            with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
                invoke(extra=extra)

    def test_linked_padding_must_shift_the_reused_object(self):
        build = self.path.parent
        (build / "seq").mkdir()
        (build / "seq/records.o").write_bytes(b"same WF object")
        def fake_nm(command, **kwargs):
            path = Path(command[-1])
            offset = 0 if path.suffix == ".o" else 0x4000 + E5A_PADDING.get(path.parent.name, 0)
            return type("Result", (), {"stdout": f"{offset:x} T wf_bench_records\n"})()
        with patch.object(measure, "run", side_effect=fake_nm):
            identity = measure.layout_identity(build, {"records": {}})
        self.assertEqual(identity["records"]["linked_text_offsets"]["seq-shift4160"]["wf_bench_records"], 0x4000 + 4160)
        with patch.object(measure, "run", return_value=type("Result", (), {"stdout": "4000 T wf_bench_records\n"})()), \
             self.assertRaisesRegex(ValueError, "did not shift"):
            measure.layout_identity(build, {"records": {}})


class PerformanceCoreTests(unittest.TestCase):
    def topology(self, count=8, hybrid=True):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        root = Path(directory.name)
        for core in range(count + (2 if hybrid else 0)):
            path = root / f"cpu{core * 2}" / "topology"
            path.mkdir(parents=True)
            (path / "thread_siblings_list").write_text(f"{core * 2}-{core * 2 + 1}\n")
        performance = root / "p-cpus"
        if hybrid:
            performance.write_text(f"0-{count * 2 - 1}\n")
        return root, performance

    def test_hybrid_pins_exclude_smt_and_efficiency_and_record_sources(self):
        root, performance = self.topology()
        cores, identity = performance_cores(root, performance, set(range(20)))
        self.assertEqual(cores, list(range(0, 16, 2)))
        self.assertEqual(identity["files"][str(performance)], "0-15\n")
        self.assertIn(str(root / "cpu0/topology/thread_siblings_list"), identity["files"])
        self.assertEqual(cpu_list("0-3,8,10-12"), {0, 1, 2, 3, 8, 10, 11, 12})

    def test_fewer_than_eight_p_cores_or_restricted_affinity_is_refused(self):
        root, performance = self.topology(count=7)
        with self.assertRaises(ValueError): performance_cores(root, performance, set(range(18)))
        root, performance = self.topology()
        with self.assertRaises(ValueError): performance_cores(root, performance, set(range(14)))

    def test_missing_p_core_file_falls_back_to_distinct_cores(self):
        root, performance = self.topology(hybrid=False)
        cores, identity = performance_cores(root, performance, set(range(16)))
        self.assertEqual(cores, list(range(0, 16, 2)))
        self.assertIsNone(identity["performance_cpus"])
        self.assertEqual({arm: demand_setting(2, arm) for arm in E2_ARMS},
                         dict(seq="off-never-request", par="off-never-request", demand="on", idle1="on", twin="on"))
        self.assertEqual({demand_setting(1, arm) for arm in ARMS}, {"off-never-request"})
        self.assertEqual({arm: demand_setting(4, arm) for arm in E4_ARMS},
                         dict(seq="off-never-request", par="off-never-request", demand="on", static="on", twin="on"))


class MeasurementModeTests(unittest.TestCase):
    def test_experiment2_default_rounds_arm_settings_and_pin_identity(self):
        # Pure Python driver control: no executable is built or launched. The
        # synthetic runner emits a known passing paired matrix; all subprocess
        # calls, Linux selection and taskset lookup are replaced by fixtures.
        with tempfile.TemporaryDirectory() as directory:
            build = Path(directory)
            for arm in E2_ARMS:
                (build / arm).mkdir()
                for name in MANIFEST:
                    (build / arm / name).write_bytes((name + ("demand" if arm == "twin" else arm)).encode())
            observed = []
            def fake_run(command, **kwargs):
                if command[:2] == ["git", "rev-parse"]:
                    return type("Result", (), {"stdout": "fixture-revision\n"})()
                if command[:2] == ["git", "status"]:
                    return type("Result", (), {"stdout": ""})()
                self.assertEqual(command[:2], ["taskset", "-c"])
                image, mode, arm, width, round_id, attempt = command[3:]
                self.assertEqual(mode, "measure")
                self.assertEqual(attempt, "1")
                cpus = list(range(0, int(width) * 2, 2))
                self.assertEqual(command[2], ",".join(map(str, cpus)))
                expected = "on" if arm in ("demand", "idle1", "twin") else "off-never-request"
                self.assertEqual(kwargs["env"]["WF_PAR_DEMAND"], expected)
                name = Path(image).name
                observed.append((name, arm, width, round_id))
                wall = 1000000000 if arm == "seq" else 500000000
                for sample in (0, 1):
                    kwargs["stdout"].write(f"{name}\t{arm}\t{width}\t{round_id}\t{attempt}\t{sample}\t{wall}\t1000000000\t1\n")
            topology = {"files": {"/proc/cpuinfo": "model name: fixture\n"}}
            with patch("sys.argv", ["measure.py", "--build", str(build), "--experiment", "2"]), \
                 patch.object(measure, "run", side_effect=fake_run), \
                 patch.object(measure.platform, "system", return_value="Linux"), \
                 patch.object(measure.shutil, "which", return_value="/usr/bin/taskset"), \
                 patch.object(measure, "performance_cores", return_value=(list(range(0, 16, 2)), topology)):
                with contextlib.redirect_stdout(io.StringIO()):
                    measure.main()
            identity = json.loads((build / "identity.json").read_text())
            self.assertEqual(identity["rounds"], 30)
            self.assertEqual(identity["pinned"], {"1": [0], "4": [0, 2, 4, 6], "8": list(range(0, 16, 2))})
            self.assertEqual(identity["topology"], topology)
            self.assertEqual(len(observed), len(MANIFEST) * len(E2_ARMS) * len(WIDTHS) * 30)
            self.assertEqual({row[3] for row in observed}, set(map(str, range(30))))



if __name__ == "__main__":
    unittest.main()
