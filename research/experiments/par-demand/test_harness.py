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
from summarize import (ARMS, E2_ARMS, WIDTHS, MANIFEST, load, summarize,
                       attempt_result_e2)
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
