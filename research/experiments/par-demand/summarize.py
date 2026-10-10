#!/usr/bin/env python3
"""Apply the prospective DESIGN.md bounds; never turn missing evidence into pass.

Native tools have no equivalent paired-twin/re-run decision rule. All quantities
here are ratios (seconds cancel). Each cell is judged by a bootstrap interval of
the median paired round ratio (DESIGN.md, "The paired noise rule"), and a twin
that disagrees with its byte-identical candidate voids the cell. CPU is
reported in experiment 1 and judges H3 in experiment 2.
"""
import argparse
import csv
import json
import random
from functools import lru_cache
from pathlib import Path
from statistics import median

HERE = Path(__file__).resolve().parent
MANIFEST = json.loads((HERE / "manifest.json").read_text())
ARMS = ("seq", "demand", "par", "twin")
E2_ARMS = ("seq", "par", "demand", "idle1", "twin")
WIDTHS = (1, 4, 8)
FIELDS = ("workload", "arm", "width", "round", "attempt", "sample", "wall_ns", "cpu_ns", "count")


def load(path, experiment=1, sample_index=1):
    expected_arms = E2_ARMS if experiment == 2 else ARMS
    counts = {}
    groups = {}
    seen = set()
    with Path(path).open() as stream:
        for row in csv.reader(stream, delimiter="\t"):
            if len(row) != len(FIELDS):
                raise ValueError(f"malformed measurement: {row}")
            name, arm = row[:2]
            width, round_id, attempt, sample, wall, cpu, count = map(int, row[2:])
            if (name not in MANIFEST or arm not in expected_arms or width not in WIDTHS
                    or round_id < 0 or attempt not in (1, 2) or sample not in (0, 1)
                    or min(wall, cpu, count) <= 0):
                raise ValueError(f"invalid measurement: {row}")
            key = (name, width, attempt, arm, round_id, sample)
            if key in seen:
                raise ValueError(f"duplicate measurement: {key}")
            seen.add(key)
            if experiment == 2:
                cell = (name, width, attempt)
                if counts.setdefault(cell, count) != count:
                    raise ValueError(f"comparison extent changed: {cell}")
            if sample == sample_index:  # second call judges; first reports startup
                groups.setdefault((name, width, attempt), {}).setdefault(arm, {})[round_id] = (wall, cpu)
    if experiment == 2:
        for key in seen:
            if key[:-1] + (1 - key[-1],) not in seen:
                raise ValueError(f"missing first/second call: {key}")
    return groups


# The owner's allowance (DESIGN, "The allowance for tiny decision points"):
# each decision point may cost the wider of 1 ns or 2 percent per execution.
DECISION_NS = 1.0


BOOTSTRAP_DRAWS = 10_000
BOOTSTRAP_SEED = 20261010


def median_interval(values):
    """The 95 percent bootstrap interval of the median, reproducible by seed."""
    return _median_interval(tuple(values), BOOTSTRAP_SEED, BOOTSTRAP_DRAWS)


@lru_cache(maxsize=1024)
def _median_interval(values, seed, count):
    # The E2 report reuses the same paired quantities in several rules. Cache
    # only the deterministic calculation; no observation or verdict is cached.
    generator = random.Random(seed)
    draws = sorted(median(generator.choices(values, k=len(values))) for _ in range(count))
    return draws[int(0.025 * count)], draws[int(0.975 * count) - 1]


def attempt_result(arms, width, decisions=0):
    if set(arms) != set(ARMS):
        raise ValueError("missing arm")
    rounds = set(arms["seq"])
    if not rounds or any(set(arm) != rounds for arm in arms.values()):
        raise ValueError("unpaired rounds")
    if rounds != set(range(len(rounds))):
        raise ValueError("missing round")
    order = sorted(rounds)
    wall = {arm: median([pair[0] for pair in rows.values()]) for arm, rows in arms.items()}
    cpu = {arm: median([pair[1] for pair in rows.values()]) for arm, rows in arms.items()}
    paired = [arms["demand"][r][0] / arms["seq"][r][0] for r in order]
    twins = [arms["twin"][r][0] / arms["demand"][r][0] for r in order]
    ratio = median(paired)
    low, high = median_interval(paired)
    twin_low, twin_high = median_interval(twins)
    allowance = max(0.02, decisions * DECISION_NS / wall["seq"])
    # One worker runs the sequential clone, so it keeps a two-sided band.
    floor, bound = (0.98, 1.02) if width == 1 else (0.0, 1 + allowance)
    if not twin_low <= 1 <= twin_high:
        status = "void"
    elif floor <= low and high <= bound:
        status = "pass"
    elif low > bound or high < floor:
        status = "exceeds"
    else:
        status = "inconclusive"
    return dict(status=status, ratio=ratio, interval=[low, high], twin_interval=[twin_low, twin_high],
                floor=floor, bound=bound, wall_ns=wall, cpu_ns=cpu, rounds=len(rounds))


def summarize(path, inspection=None, sizing=False, experiment=1):
    if experiment == 2:
        return summarize_e2(path, inspection, sizing)
    groups = load(path)
    inspection = inspection or {}
    results = []
    for name, meta in MANIFEST.items():
        for width in WIDTHS:
            first = groups.get((name, width, 1))
            if first is None:
                raise ValueError(f"missing initial cell: {name}/{width}")
            repetitions = meta.get("sizing_repetitions", meta.get("repetitions", 0)) if sizing else meta.get("repetitions", 0)
            decisions = meta.get("decisions_per_repetition", 0) * repetitions
            initial = attempt_result(first, width, decisions)
            result = dict(workload=name, width=width, attribution=meta["kind"], initial=initial)
            status = initial["status"]
            if status == "exceeds":
                second = groups.get((name, width, 2))
                if second is None:
                    status = "needs-rerun"
                else:
                    result["rerun"] = attempt_result(second, width, decisions)
                    rerun_status = result["rerun"]["status"]
                    status = "fail" if rerun_status == "exceeds" else "inconclusive"
            checked = inspection.get(name, {})
            # An inspected site record must say what survived and cite the
            # assembly/artifact. Pruned sites must retain the hot workload;
            # the removed request check itself is their expected result.
            inspected = checked.get("hot_work_survives") is True and bool(checked.get("evidence")) and bool(checked.get("check_compiles_to"))
            if status == "pass" and (not inspected or sizing):
                status = "inconclusive"
                result["reason"] = "hosted sizing only" if sizing else "optimized hot-site inspection missing or work optimized away"
            result["status"] = status
            results.append(result)
    # DESIGN ("The paired noise rule"): each workload and width is judged on
    # its own interval, so a noisy cell leaves the others' verdicts standing.
    for row in results:
        if "reason" in row:
            continue
        if row["status"] == "void" or row.get("rerun", {}).get("status") == "void":
            row["status"] = "void"
            row["reason"] = "the twin's interval against its identical candidate excludes 1"
        elif row["status"] == "inconclusive":
            row["reason"] = ("the rerun came within its bound" if "rerun" in row
                             else "the interval straddles the bound")
    return results


def quantity(values):
    return dict(median=median(values), interval=list(median_interval(values)))


def upper_bound(values, bound):
    result = quantity(values)
    low, high = result["interval"]
    result.update(bound=bound, status=("pass" if high <= bound else
                                      "exceeds" if low > bound else "inconclusive"))
    return result


def attempt_result_e2(arms, width, decisions=0, startup=None, excluded=False):
    if set(arms) != set(E2_ARMS):
        raise ValueError("missing arm")
    rounds = set(arms["seq"])
    if not rounds or any(set(rows) != rounds for rows in arms.values()):
        raise ValueError("unpaired rounds")
    if rounds != set(range(len(rounds))):
        raise ValueError("missing round")
    order = sorted(rounds)
    walls = {arm: [arms[arm][r][0] for r in order] for arm in E2_ARMS}
    cpus = {arm: [arms[arm][r][1] for r in order] for arm in E2_ARMS}
    def ratios(source, numerator, denominator):
        return [a / b for a, b in zip(source[numerator], source[denominator])]
    wall_ratios = {arm: quantity(ratios(walls, arm, "seq")) for arm in E2_ARMS}
    cpu_ratios = {arm: quantity(ratios(cpus, arm, "seq")) for arm in E2_ARMS}
    twin = quantity(ratios(walls, "twin", "demand"))
    bound = max(1.02, 1 + decisions * DECISION_NS / median(walls["seq"]))
    h3 = {arm: [(cpu - 1.1 * seq_cpu - 0.1 * max(0, seq_wall - wall) * width) / seq_cpu
                for cpu, seq_cpu, seq_wall, wall in zip(cpus[arm], cpus["seq"], walls["seq"], walls[arm])]
          for arm in ("par", "demand", "idle1")}
    keep = {arm: ratios(walls, arm, "par") for arm in ("demand", "idle1")}
    idle_wall = ratios(walls, "idle1", "demand")
    rules = {}
    for arm in ("demand", "idle1"):
        rules[f"E2-H1-{arm}"] = upper_bound(ratios(walls, arm, "seq"), bound)
        rules[f"E2-keep-{arm}"] = upper_bound(keep[arm], 1.05)
        if wall_ratios["par"]["interval"][1] >= 1:
            rules[f"E2-keep-{arm}"].update(status="not-applicable", reason="par/seq interval is not wholly below 1")
        rules[f"E2-H3-{arm}"] = upper_bound(h3[arm], 0)
    rules["E2-idle"] = upper_bound(idle_wall, 1.02)
    void = not twin["interval"][0] <= 1 <= twin["interval"][1]
    for result in rules.values():
        if void:
            result["status"] = "void"
        elif excluded or width == 1:
            result["status"] = "not-applicable"
            result["reason"] = "reported control; decides nothing"
    statuses = {r["status"] for r in rules.values()}
    status = ("void" if void else "not-applicable" if excluded or width == 1 else
              "exceeds" if "exceeds" in statuses else "inconclusive" if "inconclusive" in statuses else "pass")
    result = dict(status=status, rules=rules, rounds=len(rounds), twin_wall_ratio=twin,
                  wall_ns={arm: quantity(values) for arm, values in walls.items()},
                  cpu_ns={arm: quantity(values) for arm, values in cpus.items()},
                  wall_over_seq=wall_ratios, cpu_over_seq=cpu_ratios,
                  wall_over_par={arm: quantity(values) for arm, values in keep.items()},
                  h3={arm: quantity(values) for arm, values in h3.items()},
                  idle_wall_ratio=quantity(idle_wall),
                  idle_cpu_ratio=quantity(ratios(cpus, "idle1", "demand")),
                  idle_cpu_change_ns=quantity([a - b for a, b in zip(cpus["idle1"], cpus["demand"])]))
    if startup is not None:
        if set(startup) != set(E2_ARMS) or any(set(rows) != rounds for rows in startup.values()):
            raise ValueError("unpaired first calls")
        result["first_call_cpu_above_wall_ns"] = {
            arm: quantity([startup[arm][r][1] - startup[arm][r][0] for r in order]) for arm in E2_ARMS}
    return result


def e2_excluded(name, checked):
    # small_constant is prospectively excluded; inspection can identify more
    # cells whose timed work is optimized away in both builds.
    return name in ("spine", "small_constant") or checked.get("optimized_away_in_both") is True


def summarize_e2(path, inspection=None, sizing=False):
    groups = load(path, experiment=2)
    startup = load(path, experiment=2, sample_index=0)
    inspection = inspection or {}
    results = []
    for name, meta in MANIFEST.items():
        checked = inspection.get(name, {})
        excluded = e2_excluded(name, checked)
        inspected = (checked.get("hot_work_survives") is True and bool(checked.get("evidence"))
                     and bool(checked.get("check_compiles_to")))
        for width in WIDTHS:
            cell = (name, width, 1)
            if cell not in groups:
                raise ValueError(f"missing initial cell: {name}/{width}")
            repetitions = meta.get("sizing_repetitions", meta.get("repetitions", 0)) if sizing else meta.get("repetitions", 0)
            decisions = meta.get("decisions_per_repetition", 0) * repetitions
            initial = attempt_result_e2(groups[cell], width, decisions, startup[cell], excluded)
            result = dict(workload=name, width=width, attribution=meta["kind"], initial=initial)
            second = groups.get((name, width, 2))
            if second is not None:
                result["rerun"] = attempt_result_e2(second, width, decisions, startup[(name, width, 2)], excluded)
            verdicts = {}
            for key, rule in initial["rules"].items():
                status = rule["status"]
                if status == "exceeds":
                    status = "needs-rerun" if second is None else (
                        "fail" if result["rerun"]["rules"][key]["status"] == "exceeds" else "inconclusive")
                if initial["status"] == "void" or result.get("rerun", {}).get("status") == "void":
                    status = "void"
                elif status == "pass" and (not inspected or sizing):
                    status = "inconclusive"
                verdicts[key] = status
            statuses = set(verdicts.values())
            result["verdicts"] = verdicts
            result["status"] = next((s for s in ("void", "fail", "needs-rerun", "inconclusive", "pass") if s in statuses), "not-applicable")
            if excluded or width == 1:
                result["reason"] = "spine belongs to stage 4" if name == "spine" else (
                    "timed work optimized away" if excluded else "one-worker control; rules apply only at four and eight")
            elif not inspected or sizing:
                result["reason"] = "hosted sizing only" if sizing else "optimized hot-site inspection missing"
            results.append(result)
    return results


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("measurements", type=Path)
    parser.add_argument("--inspection", type=Path)
    parser.add_argument("--sizing", action="store_true")
    parser.add_argument("--experiment", type=int, choices=(1, 2), default=1)
    args = parser.parse_args()
    inspection = json.loads(args.inspection.read_text()) if args.inspection and args.inspection.exists() else {}
    rows = summarize(args.measurements, inspection, args.sizing, args.experiment)
    print(json.dumps(rows, indent=2))
    # Experiment verdicts are data, never a language or build gate. A malformed
    # or incomplete matrix raises instead, failing the manual workflow.


if __name__ == "__main__":
    main()
