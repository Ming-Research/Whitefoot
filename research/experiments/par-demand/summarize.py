#!/usr/bin/env python3
"""Apply the prospective DESIGN.md bounds; never turn missing evidence into pass.

Native tools have no equivalent paired-twin/re-run decision rule. All quantities
here are ratios (seconds cancel). Each cell is judged by a bootstrap interval of
the median paired round ratio (DESIGN.md, "The paired noise rule"), and a twin
that disagrees with its byte-identical candidate voids the cell. CPU is
reported but decides no bound.
"""
import argparse
import csv
import json
import random
from pathlib import Path
from statistics import median

HERE = Path(__file__).resolve().parent
MANIFEST = json.loads((HERE / "manifest.json").read_text())
ARMS = ("seq", "demand", "par", "twin")
WIDTHS = (1, 4, 8)
FIELDS = ("workload", "arm", "width", "round", "attempt", "sample", "wall_ns", "cpu_ns", "count")


def load(path):
    groups = {}
    seen = set()
    with Path(path).open() as stream:
        for row in csv.reader(stream, delimiter="\t"):
            if len(row) != len(FIELDS):
                raise ValueError(f"malformed measurement: {row}")
            name, arm = row[:2]
            width, round_id, attempt, sample, wall, cpu, count = map(int, row[2:])
            if (name not in MANIFEST or arm not in ARMS or width not in WIDTHS
                    or round_id < 0 or attempt not in (1, 2) or sample not in (0, 1)
                    or min(wall, cpu, count) <= 0):
                raise ValueError(f"invalid measurement: {row}")
            key = (name, width, attempt, arm, round_id, sample)
            if key in seen:
                raise ValueError(f"duplicate measurement: {key}")
            seen.add(key)
            if sample:  # sample 0 is the same workload's warm-up, never a verdict row
                groups.setdefault((name, width, attempt), {}).setdefault(arm, {})[round_id] = (wall, cpu)
    return groups


# The owner's allowance (DESIGN, "The allowance for tiny decision points"):
# each decision point may cost the wider of 1 ns or 2 percent per execution.
DECISION_NS = 1.0


BOOTSTRAP_DRAWS = 10_000
BOOTSTRAP_SEED = 20261010


def median_interval(values):
    """The 95 percent bootstrap interval of the median, reproducible by seed."""
    generator = random.Random(BOOTSTRAP_SEED)
    draws = sorted(median(generator.choices(values, k=len(values))) for _ in range(BOOTSTRAP_DRAWS))
    return draws[int(0.025 * BOOTSTRAP_DRAWS)], draws[int(0.975 * BOOTSTRAP_DRAWS) - 1]


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


def summarize(path, inspection=None, sizing=False):
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("measurements", type=Path)
    parser.add_argument("--inspection", type=Path)
    parser.add_argument("--sizing", action="store_true")
    args = parser.parse_args()
    inspection = json.loads(args.inspection.read_text()) if args.inspection and args.inspection.exists() else {}
    rows = summarize(args.measurements, inspection, args.sizing)
    print(json.dumps(rows, indent=2))
    # Experiment verdicts are data, never a language or build gate. A malformed
    # or incomplete matrix raises instead, failing the manual workflow.


if __name__ == "__main__":
    main()
