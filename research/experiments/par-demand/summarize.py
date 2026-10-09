#!/usr/bin/env python3
"""Apply the prospective DESIGN.md bounds; never turn missing evidence into pass.

Native tools have no equivalent paired-twin/re-run decision rule. All quantities
here are ratios (seconds cancel). The max/min spread over candidate and twin
samples, divided by their median, is the conservative interpretation fixed for
this harness before its first measurement. CPU is reported but decides no bound.
"""
import argparse
import csv
import json
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


def attempt_result(arms, width):
    if set(arms) != set(ARMS):
        raise ValueError("missing arm")
    rounds = set(arms["seq"])
    if not rounds or any(set(arm) != rounds for arm in arms.values()):
        raise ValueError("unpaired rounds")
    if rounds != set(range(len(rounds))):
        raise ValueError("missing round")
    wall = {arm: median([pair[0] for pair in rows.values()]) for arm, rows in arms.items()}
    cpu = {arm: median([pair[1] for pair in rows.values()]) for arm, rows in arms.items()}
    twins = [p[0] for arm in ("demand", "twin") for p in arms[arm].values()]
    spread = (max(twins) - min(twins)) / median(twins)
    noise = max(spread, 0.01)
    ratio = wall["demand"] / wall["seq"]
    bound = 1 + noise if width == 1 else 1.02 + noise
    if spread > 0.02:
        status = "inconclusive"
    elif (abs(ratio - 1) <= noise if width == 1 else ratio <= bound):
        status = "pass"
    else:
        status = "exceeds"
    return dict(status=status, ratio=ratio, noise=noise, spread=spread,
                bound=bound, wall_ns=wall, cpu_ns=cpu, rounds=len(rounds))


def summarize(path, inspection=None, sizing=False):
    groups = load(path)
    inspection = inspection or {}
    results = []
    for name, meta in MANIFEST.items():
        for width in WIDTHS:
            first = groups.get((name, width, 1))
            if first is None:
                raise ValueError(f"missing initial cell: {name}/{width}")
            initial = attempt_result(first, width)
            result = dict(workload=name, width=width, attribution=meta["kind"], initial=initial)
            status = initial["status"]
            if status == "exceeds":
                second = groups.get((name, width, 2))
                if second is None:
                    status = "needs-rerun"
                else:
                    result["rerun"] = attempt_result(second, width)
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
    # DESIGN ("The rerun's rule"): the twin spread decides each workload and
    # width on its own, so a noisy width leaves the others' verdicts standing.
    for row in results:
        if row["status"] != "inconclusive" or "reason" in row:
            continue
        noisy = row["initial"]["spread"] > 0.02 or row.get("rerun", {}).get("spread", 0) > 0.02
        row["reason"] = ("twin spread exceeds 2 percent for this width" if noisy
                         else "the rerun came within its bound")
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
