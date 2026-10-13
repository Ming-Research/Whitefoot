#!/usr/bin/env python3
"""Apply the prospective DESIGN.md bounds; never turn missing evidence into pass.

Native tools have no equivalent paired-twin/re-run decision rule. All quantities
here are ratios (seconds cancel). Each cell is judged by a bootstrap interval of
the median paired round ratio (DESIGN.md, "The paired noise rule"), and a twin
that disagrees with its byte-identical candidate voids the cell. CPU is
reported in experiment 1 and judges H3 in experiments 2, 4 and 5c.
"""
import argparse
import csv
import hashlib
import json
import random
from identity import validate_admission
from inspection import validate_inspection
from functools import lru_cache
from pathlib import Path
from statistics import median

HERE = Path(__file__).resolve().parent
MANIFEST = json.loads((HERE / "manifest.json").read_text())
ARMS = ("seq", "demand", "par", "twin")
E2_ARMS = ("seq", "par", "demand", "idle1", "twin")
E3_ARMS = ("seq", "par", "demand", "twin", "order", "seed", "extent", "dedup")
E4_ARMS = ("seq", "par", "demand", "static", "twin")
E5C_ARMS = ("seq", "par", "demand", "unversioned", "twin")
E6_CORE_ARMS = ("seq", "par", "demand", "unversioned", "previous", "twin")
E6_SWEEP = ("stencil", "histogram")
# Three nonzero shifts; ordinary seq is the zero-padding baseline.
E5A_PADDING = {"seq-shift64": 64, "seq-shift4160": 4160, "seq-shift65664": 65664}
E5A_ARMS = ("seq", "par", "demand", "twin", *E5A_PADDING)
E6_ARMS = (*E6_CORE_ARMS, *E5A_PADDING)
# Experiment 6b reruns 6's matrix with the both-sides call grain (DESIGN.md,
# "Experiment 6b"). Its previous arm is the one-sided ablation; spine W4/W8
# also run the shifted-seq controls that give their registered floor.
E6B = "6b"
E6_FAMILY = (6, E6B)
E6B_FLOOR_WIDE = (("spine", 4), ("spine", 8))
# The 25 cells experiment 6's replayed reduction qualified as pass
# (inspection-38179220701.json); only these carry E6b-keep.
E6B_PROTECTED = frozenset(
    [("small_split", w) for w in (1, 4, 8)] + [("recursion", w) for w in (1, 4, 8)]
    + [("spine", 1)] + [("hot_helper", w) for w in (1, 4, 8)] + [("large_helper", 4)]
    + [("mandelbrot", w) for w in (1, 4, 8)] + [("records", w) for w in (1, 4)]
    + [("fir", 8)] + [("stencil", w) for w in (1, 2, 8)] + [("prefix", w) for w in (1, 8)]
    + [("histogram", w) for w in (2, 4, 8)])
E5A_MANIFEST = {name: MANIFEST[name] for name in
                ("mandelbrot", "records", "fir", "stencil", "prefix", "histogram")}
E3_WIDTHS = (4, 8)
WIDTHS = (1, 4, 8)
FIELDS = ("workload", "arm", "width", "round", "attempt", "sample", "wall_ns", "cpu_ns", "count")


def parse_experiment(value):
    return value if value in ("5a", "5c", E6B) else int(value)


def experiment_matrix(experiment):
    if experiment in E6_FAMILY:
        return E6_ARMS, (1, 2, 4, 8), MANIFEST
    if experiment == "5a":
        return E5A_ARMS, (1,), E5A_MANIFEST
    arms = E5C_ARMS if experiment == "5c" else E4_ARMS if experiment == 4 else E3_ARMS if experiment == 3 else E2_ARMS if experiment == 2 else ARMS
    return arms, E3_WIDTHS if experiment == 3 else WIDTHS, MANIFEST


def cell_widths(experiment, name):
    return ((1, 2, 4, 8) if name in E6_SWEEP else WIDTHS) if experiment in E6_FAMILY else experiment_matrix(experiment)[1]


def cell_arms(experiment, width, name=None):
    if experiment == E6B and (name, width) in E6B_FLOOR_WIDE:
        return E6_ARMS
    return (E6_ARMS if width == 1 else E6_CORE_ARMS) if experiment in E6_FAMILY else experiment_matrix(experiment)[0]


def parse_cells(value, experiment):
    """Validate workload:width selection; empty keeps the complete matrix."""
    _, _, manifest = experiment_matrix(experiment)
    complete = frozenset((name, width) for name in manifest for width in cell_widths(experiment, name))
    if value == "":
        return complete
    if experiment not in E6_FAMILY:
        raise ValueError("--cells requires experiment 6 or 6b")
    selected = set()
    for token in value.split(","):
        fields = token.split(":")
        if len(fields) != 2 or not fields[0] or not fields[1].isascii() or not fields[1].isdigit():
            raise ValueError(f"malformed cell {token!r}; expected workload:width")
        cell = (fields[0], int(fields[1]))
        if cell not in complete:
            raise ValueError(f"unknown experiment {experiment} cell: {token}")
        if cell in selected:
            raise ValueError(f"duplicate cell: {token}")
        selected.add(cell)
    return frozenset(selected)


def cell_names(cells, experiment):
    """Canonical manifest order for recorded selection, independent of CLI order."""
    return [f"{name}:{width}" for name in experiment_matrix(experiment)[2]
            for width in cell_widths(experiment, name) if (name, width) in cells]


def reduction_cells(path, experiment, cells, sizing):
    # Sizing always freezes the full matrix, including every diagnostic.
    if sizing:
        if cells is not None:
            parse_cells(cells, experiment)
        return parse_cells("", experiment)
    identity_path = Path(path).parent / "identity.json"
    identity = json.loads(identity_path.read_text()) if identity_path.exists() else {}
    if not isinstance(identity, dict):
        raise ValueError("malformed measured identity")
    recorded = identity.get("selected_cells")
    if recorded is not None:
        if not isinstance(recorded, list) or not recorded or any(not isinstance(cell, str) for cell in recorded):
            raise ValueError("malformed selected_cells in measured identity")
        measured = parse_cells(",".join(recorded), experiment)
        if cells is None:
            return measured
        selected = parse_cells(cells, experiment)
        if selected != measured:
            raise ValueError("requested cells differ from selected_cells in measured identity")
        return selected
    # Archived identities without a selection retain explicit reducer selection.
    return parse_cells("" if cells is None else cells, experiment)


def load(path, experiment=1, sample_index=1):
    expected_arms, expected_widths, manifest = experiment_matrix(experiment)
    counts = {}
    groups = {}
    seen = set()
    with Path(path).open() as stream:
        for row in csv.reader(stream, delimiter="\t"):
            if len(row) != len(FIELDS):
                raise ValueError(f"malformed measurement: {row}")
            name, arm = row[:2]
            width, round_id, attempt, sample, wall, cpu, count = map(int, row[2:])
            if (name not in manifest or arm not in expected_arms or width not in cell_widths(experiment, name) or arm not in cell_arms(experiment, width, name)
                    or round_id < 0 or attempt not in (1, 2) or sample not in (0, 1)
                    or min(wall, cpu, count) <= 0):
                raise ValueError(f"invalid measurement: {row}")
            if experiment in (3, "5a") and attempt != 1:
                raise ValueError(f"experiment {experiment} has no rerun attempt")
            key = (name, width, attempt, arm, round_id, sample)
            if key in seen:
                raise ValueError(f"duplicate measurement: {key}")
            seen.add(key)
            if experiment in (2, 3, 4, "5a", "5c", *E6_FAMILY):
                cell = name if experiment in E6_FAMILY else (name, width) if experiment in (4, "5a", "5c") else (name, width, attempt)
                if counts.setdefault(cell, count) != count:
                    raise ValueError(f"comparison extent changed: {cell}")
            if sample == sample_index:  # second call judges; first reports startup
                groups.setdefault((name, width, attempt), {}).setdefault(arm, {})[round_id] = (wall, cpu)
    if experiment in (2, 3, 4, "5a", "5c", *E6_FAMILY):
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


def bound_inspection(path, inspection, experiment):
    build = Path(path).parent
    try:
        identity = json.loads((build / 'identity.json').read_text())
    except (OSError, ValueError) as error:
        raise ValueError('inspection requires measured build identity.json') from error
    if not isinstance(identity, dict):
        raise ValueError('inspection measured build identity.json is malformed')
    arms, _, manifest = experiment_matrix(experiment)
    return validate_inspection(build, build / 'inspection.json' if inspection is None else inspection,
                               identity.get('revision'), manifest, arms)


def summarize(path, inspection=None, sizing=False, experiment=1, cells=None):
    if experiment in E6_FAMILY:
        return summarize_e6(path, inspection, sizing, experiment, cells)
    if cells:
        parse_cells(cells, experiment)
    if not sizing:
        inspection = bound_inspection(path, inspection, experiment)
    if experiment == "5a":
        return summarize_e5a(path, sizing)
    if experiment in (4, "5c"):
        return summarize_e4(path, inspection, sizing, experiment)
    if experiment == 3:
        return summarize_e3(path, sizing)
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
    return summarize_rule_cells(path, inspection, sizing, experiment=2)


def summarize_rule_cells(path, inspection, sizing, experiment):
    groups = load(path, experiment=experiment)
    startup = load(path, experiment=experiment, sample_index=0)
    inspection = inspection or {}
    results = []
    for name, meta in MANIFEST.items():
        checked = inspection.get(name, {})
        excluded = e2_excluded(name, checked)
        inspected = (checked.get("hot_work_survives") is True and bool(checked.get("evidence"))
                     and bool(checked.get("check_compiles_to")))
        if experiment == "5c":
            # Inspection must cover every versioned sequential clone against
            # the corresponding seq body, even when the diff is nonempty.
            inspected = inspected and checked.get("all_versioned_clones_compared") is True and bool(
                checked.get("normalized_disassembly_evidence"))
        for width in WIDTHS:
            cell = (name, width, 1)
            if cell not in groups:
                raise ValueError(f"missing initial cell: {name}/{width}")
            repetitions = meta.get("sizing_repetitions", meta.get("repetitions", 0)) if sizing else meta.get("repetitions", 0)
            decisions = meta.get("decisions_per_repetition", 0) * repetitions
            evaluate = (lambda arms, first_calls: attempt_result_e4(arms, width, first_calls, excluded, experiment)) if experiment in (4, "5c") else (
                lambda arms, first_calls: attempt_result_e2(arms, width, decisions, first_calls, excluded))
            initial = evaluate(groups[cell], startup[cell])
            result = dict(workload=name, width=width, attribution=meta["kind"], initial=initial)
            second = groups.get((name, width, 2))
            if second is not None:
                result["rerun"] = evaluate(second, startup[(name, width, 2)])
            verdicts = {}
            for key, rule in initial["rules"].items():
                status = rule["status"]
                if status == "exceeds":
                    status = "needs-rerun" if second is None else (
                        "fail" if result["rerun"]["rules"][key]["status"] == "exceeds" else "inconclusive")
                elif second is not None and result["rerun"]["rules"][key]["status"] != status:
                    # A cell rerun covers every rule, even when a different
                    # rule triggered it. Disagreement cannot preserve a pass.
                    status = "inconclusive"
                if initial["status"] == "void" or result.get("rerun", {}).get("status") == "void":
                    status = "void"
                elif experiment in (4, "5c") and sizing and status != "not-applicable":
                    status = "inconclusive"  # the sample selects n, never a verdict
                elif status == "pass" and (not inspected or sizing):
                    status = "inconclusive"
                verdicts[key] = status
            statuses = set(verdicts.values())
            result["verdicts"] = verdicts
            result["status"] = next((s for s in ("void", "fail", "needs-rerun", "inconclusive", "pass") if s in statuses), "not-applicable")
            if experiment == "5c":
                result["e5a_layout_spread"] = e5a_observed_spread(name)
                if name == "small_split" and width in (4, 8):
                    no_decision = (inspected and checked.get("walker_has_per_iteration_decision") is False
                                   and bool(checked.get("normalized_disassembly_evidence")))
                    # Apply E4's rerun discipline to a persistent loss. A
                    # missing image inspection cannot falsify an attribution.
                    result["decision_whole_cause"] = (
                        "falsified" if not sizing and no_decision and verdicts["E4-seq"] == "fail"
                        else "undecided")
                if excluded:
                    result["e4_control"] = "E4 rules decide nothing; E5c-keep still applies"
            if (excluded and experiment != "5c") or (experiment == 2 and width == 1):
                result["reason"] = "spine belongs to stage 4" if name == "spine" else (
                    "timed work optimized away" if excluded else "one-worker control; rules apply only at four and eight")
            elif not inspected or sizing:
                result["reason"] = "sizing only" if sizing else "optimized hot-site inspection missing"
            results.append(result)
    return results


def attempt_result_e4(arms, width, startup=None, excluded=False, experiment=4):
    expected = E5C_ARMS if experiment == "5c" else E4_ARMS
    reference_arm = "unversioned" if experiment == "5c" else "static"
    if set(arms) != set(expected):
        raise ValueError(f"missing experiment-{experiment} arm")
    rounds = set(arms["seq"])
    if not rounds or rounds != set(range(len(rounds))) or any(set(rows) != rounds for rows in arms.values()):
        raise ValueError(f"unpaired or missing experiment-{experiment} rounds")
    order = sorted(rounds)
    walls = {arm: [arms[arm][r][0] for r in order] for arm in expected}
    cpus = {arm: [arms[arm][r][1] for r in order] for arm in expected}
    def ratios(source, numerator, denominator):
        return [a / b for a, b in zip(source[numerator], source[denominator])]
    wall_ratios = {arm: quantity(ratios(walls, arm, "seq")) for arm in expected}
    twin = quantity(ratios(walls, "twin", "demand"))
    margins = {arm: [(cpu - 1.1 * seq_cpu - 0.1 * max(0, seq_wall - wall) * width) / seq_cpu
                     for cpu, seq_cpu, seq_wall, wall in zip(cpus[arm], cpus["seq"], walls["seq"], walls[arm])]
               for arm in ("par", "demand", reference_arm)}
    rules = {"E4-seq": upper_bound(ratios(walls, "demand", "seq"), 1.00),
             "E4-H3": upper_bound(margins["demand"], 0)}
    comparisons = {}
    for key, reference in (("E4-par", "par"), ("E5c-keep" if experiment == "5c" else "E4-gain", reference_arm)):
        values = ratios(walls, "demand", reference)
        comparisons[reference] = quantity(values)
        rules[key] = upper_bound(values, 1.05)
        if key != "E5c-keep" and wall_ratios[reference]["interval"][1] >= 1:
            rules[key].update(status="not-applicable", reason=f"{reference}/seq interval is not wholly below 1")
    void = not twin["interval"][0] <= 1 <= twin["interval"][1]
    for key, rule in rules.items():
        if void:
            rule["status"] = "void"
        elif excluded and key != "E5c-keep":
            rule.update(status="not-applicable", reason="reported control; decides nothing")
    statuses = {rule["status"] for rule in rules.values()}
    status = ("void" if void else "not-applicable" if excluded and experiment != "5c" else
              "exceeds" if "exceeds" in statuses else "inconclusive" if "inconclusive" in statuses else "pass")
    result = dict(status=status, rules=rules, rounds=len(rounds), twin_wall_ratio=twin,
                  wall_ns={arm: quantity(values) for arm, values in walls.items()},
                  cpu_ns={arm: quantity(values) for arm, values in cpus.items()},
                  wall_over_seq=wall_ratios,
                  cpu_over_seq={arm: quantity(ratios(cpus, arm, "seq")) for arm in expected},
                  wall_over_par={"demand": comparisons["par"]},
                  h3={arm: quantity(values) for arm, values in margins.items()})
    result[f"wall_over_{reference_arm}"] = {"demand": comparisons[reference_arm]}
    if startup is not None:
        if set(startup) != set(expected) or any(set(rows) != rounds for rows in startup.values()):
            raise ValueError("unpaired first calls")
        result["first_call_cpu_above_wall_ns"] = {
            arm: quantity([startup[arm][r][1] - startup[arm][r][0] for r in order]) for arm in expected}
    return result


def e4_round_count(cells, step=1):
    """Smallest n in 6..30 (every step-th) meeting the projected widths, else 30."""
    if any(cell["initial"]["rounds"] != 6 or "rerun" in cell for cell in cells):
        raise ValueError("E4 sizing rule requires exactly six rounds without reruns")
    quantities = [(rule["interval"][1] - rule["interval"][0],
                   max(0.02, abs(rule["median"] - rule["bound"])))
                  for cell in cells for rule in cell["initial"]["rules"].values()
                  if rule["status"] != "not-applicable"]
    return next((n for n in range(6, 31, step) if all(width * (6 / n) ** 0.5 <= target
                                                     for width, target in quantities)), 30)


def summarize_e4(path, inspection=None, sizing=False, experiment=4):
    # Freeze sizing using measurements and prospective controls only. Later
    # optimized-code inspection must not change the already selected count.
    cells = summarize_rule_cells(path, None if sizing else inspection, sizing, experiment=experiment)
    if sizing:
        count = e4_round_count(cells)
    else:
        sample_path = Path(path).parent / f"sizing-e{experiment}" / "measurements.tsv"
        if not sample_path.exists():
            raise ValueError(f"missing experiment {experiment} six-round sizing evidence")
        sample = summarize_e4(sample_path, sizing=True, experiment=experiment)
        count = sample["decisive_rounds"]
        if any(attempt["rounds"] != count for cell in cells
               for attempt in (cell["initial"], *([cell["rerun"]] if "rerun" in cell else []))):
            raise ValueError("decisive rounds differ from the count frozen by sizing")
    result = dict(experiment=experiment, sizing=sizing, cells=cells, decisive_rounds=count)
    if not sizing:
        result["sizing_evidence"] = str(sample_path)
    return result



def floor_bound(values, radius):
    """E6b-seq: pass at or below 1.00, or wholly inside the cell's own floor."""
    result = quantity(values)
    low, high = result["interval"]
    inside = radius is not None and 1 - radius <= low and high <= 1 + radius
    result.update(bound=1.00, floor_radius=radius,
                  status="pass" if high <= 1.00 or inside else
                  "exceeds" if low > 1 + (radius or 0) else "inconclusive")
    if inside and high > 1.00:
        result["reading"] = "at the floor"
    return result


def e6b_rules(name, width, ratios, floor):
    """Exactly the registered E6b criteria for one cell; the rest is reported."""
    rules = {}
    if (name, width) in E6B_PROTECTED:
        for reference in ("unversioned", "previous"):
            rules[f"E6b-keep-{reference}"] = upper_bound(ratios("demand", reference), 1.05)
    if (name, width) == ("recursion", 8):
        rules["E6b-recursion"] = upper_bound(ratios("demand", "par"), 1.05)
    if (name, width) == ("fir", 1):
        rules["E6b-fir-seq"] = upper_bound(ratios("demand"), 1.05)
    elif (name, width) in E6B_FLOOR_WIDE:
        rules["E6b-seq"] = floor_bound(ratios("demand"), floor)
    return rules


def attempt_result_e6(arms, width, startup=None, name=None, experiment=6):
    expected = cell_arms(experiment, width, name)
    if set(arms) != set(expected):
        raise ValueError(f"missing experiment-{experiment} arm")
    rounds = set(arms["seq"])
    if not rounds or rounds != set(range(len(rounds))) or any(set(rows) != rounds for rows in arms.values()):
        raise ValueError("unpaired or missing experiment-6 rounds")
    order = sorted(rounds)
    walls = {arm: [arms[arm][r][0] for r in order] for arm in expected}
    cpus = {arm: [arms[arm][r][1] for r in order] for arm in expected}
    def ratios(source, numerator, denominator="seq"):
        return [a / b for a, b in zip(source[numerator], source[denominator])]
    twin = quantity(ratios(walls, "twin", "demand"))
    void = not twin["interval"][0] <= 1 <= twin["interval"][1]
    literal = upper_bound(ratios(walls, "demand"), 1.00)
    controls = {arm: quantity(ratios(walls, arm)) for arm in E5A_PADDING} if set(E5A_PADDING) <= set(expected) else {}
    floor = max((max(abs(q["interval"][0] - 1), abs(q["interval"][1] - 1))
                 for q in controls.values()), default=None)
    if experiment == E6B:
        rules = e6b_rules(name, width, lambda numerator, denominator="seq": ratios(walls, numerator, denominator),
                          None if void else floor)
    else:
        rules = {f"E6-keep-{reference}": upper_bound(ratios(walls, "demand", reference), 1.05)
                 for reference in ("unversioned", "previous")}
        # Preserve par speedups. The preregistered recursion W8 check is applied
        # unconditionally by summarize_e6, even if this run's par interval is noisy.
        rules["E6-par"] = upper_bound(ratios(walls, "demand", "par"), 1.05)
        if quantity(ratios(walls, "par"))["interval"][1] >= 1:
            rules["E6-par"].update(status="not-applicable", reason="par/seq interval is not wholly below 1")
    floor_status = ("void" if void else "at the floor" if floor is not None and
                    1 - floor <= literal["interval"][0] and literal["interval"][1] <= 1 + floor
                    else "outside the floor" if floor is not None else "not measured at this width")
    margins = {arm: quantity([(cpu - 1.1 * seq_cpu - 0.1 * max(0, seq_wall - wall) * width) / seq_cpu
                             for cpu, seq_cpu, seq_wall, wall in zip(cpus[arm], cpus["seq"], walls["seq"], walls[arm])])
               for arm in ("par", "demand", "unversioned", "previous")}
    for rule in rules.values():
        if void:
            rule["status"] = "void"
    statuses = {rule["status"] for rule in rules.values()}
    result = dict(status="void" if void else "exceeds" if "exceeds" in statuses else
                  "inconclusive" if "inconclusive" in statuses else "pass" if rules else "not-applicable",
                  rounds=len(rounds), rules=rules, twin_wall_ratio=twin,
                  wall_ns={arm: quantity(values) for arm, values in walls.items()},
                  cpu_ns={arm: quantity(values) for arm, values in cpus.items()},
                  wall_over_seq={arm: quantity(ratios(walls, arm)) for arm in expected},
                  cpu_over_seq={arm: quantity(ratios(cpus, arm)) for arm in expected},
                  literal_seq_diagnostic=literal,
                  layout_floor=dict(status=floor_status, radius=None if void else floor,
                                    band=None if floor is None or void else [1 - floor, 1 + floor], controls=controls),
                  h3=margins, h3_policy="reported; no E6 gate or rerun; scaling evidence is reported separately")
    if experiment == E6B:
        # Reported, never gated: E6b fixes exactly its registered rules.
        result["reported_demand_ratios"] = {reference: quantity(ratios(walls, "demand", reference))
                                            for reference in ("par", "previous", "unversioned")}
        if width == 1 and name != "fir":
            result["floor_seq_diagnostic"] = floor_bound(ratios(walls, "demand"), None if void else floor)
            if void:
                result["floor_seq_diagnostic"]["status"] = "void"
    if startup is not None:
        if set(startup) != set(expected) or any(set(rows) != rounds for rows in startup.values()):
            raise ValueError("unpaired first calls")
        result["first_call_wall_ns"] = {arm: quantity([startup[arm][r][0] for r in order]) for arm in expected}
        result["first_call_cpu_ns"] = {arm: quantity([startup[arm][r][1] for r in order]) for arm in expected}
        result["first_call_cpu_above_wall_ns"] = {
            arm: quantity([startup[arm][r][1] - startup[arm][r][0] for r in order]) for arm in expected}
    return result


def e6_attempt(name, arms, width, startup=None, experiment=6):
    result = attempt_result_e6(arms, width, startup, name, experiment)
    if experiment == E6B:
        return result
    if name in ("small_constant", "spine") and result["status"] != "void":
        result["rules"]["E6-par"].update(status="not-applicable", reason="reported E4 control; par rule decides nothing")
    if name == "recursion" and width == 8 and result["status"] != "void":
        rule = result["rules"]["E6-par"]
        low, high = rule["interval"]
        rule["status"] = "pass" if high <= 1.05 else "exceeds" if low > 1.05 else "inconclusive"
        rule.pop("reason", None)
    if result["status"] != "void":
        statuses = {r["status"] for r in result["rules"].values()}
        result["status"] = "exceeds" if "exceeds" in statuses else "inconclusive" if "inconclusive" in statuses else "pass"
    return result


def e6_sweep(groups, cells, sizing=False):
    """Cross-width pairing is same-round and same attempt; never pool reruns.

    Only selected initial widths are reported; reruns need not cover adjacent widths, so they
    report their raw wall/CPU but do not fabricate a paired scaling interval.
    """
    by_cell = {(cell["workload"], cell["width"]): cell for cell in cells}
    reports = []
    for name in E6_SWEEP:
        rows = []
        for width in (1, 2, 4, 8):
            if (name, width) not in by_cell:
                continue
            cell = by_cell[name, width]
            rows.append(dict(width=width, validity=cell["status"],
                             inspection="missing" if cell.get("reason") == "optimized hot-site inspection missing" else
                             "sizing only" if sizing else "provided", wall_ns=cell["initial"]["wall_ns"],
                             cpu_ns=cell["initial"]["cpu_ns"], h3=cell["initial"]["h3"],
                             rerun=cell.get("rerun")))
        comparisons = []
        for lower, higher in ((1, 2), (2, 4), (4, 8)):
            if any((name, width) not in by_cell for width in (lower, higher)):
                continue
            a, b = groups[name, lower, 1], groups[name, higher, 1]
            for arm in E6_CORE_ARMS:
                rounds = sorted(a[arm])
                if set(rounds) != set(b[arm]):
                    raise ValueError("unpaired sweep rounds")
                wall = quantity([b[arm][r][0] / a[arm][r][0] for r in rounds])
                cpu = quantity([b[arm][r][1] / a[arm][r][1] for r in rounds])
                void = any(by_cell[name, w]["status"] == "void" for w in (lower, higher))
                comparisons.append(dict(arm=arm, lower_width=lower, higher_width=higher,
                                        higher_over_lower_wall=wall, higher_over_lower_cpu=cpu,
                                        scaling="sizing only" if sizing else "void" if void else "demonstrated faster" if wall["interval"][1] < 1 else
                                        "demonstrated slower" if wall["interval"][0] > 1 else "uncertain"))
        if not rows:
            continue
        reports.append(dict(workload=name, widths=rows, adjacent_widths=comparisons,
                            interpretation="scaling observations require surviving-work inspection before attributing CPU tradeoffs"))
    return reports



def construction_identity(build):
    evidence_path = build / "sequential-identity.json"
    if not evidence_path.exists():
        raise ValueError("missing experiment 6 sequential identity construction evidence")
    evidence = json.loads(evidence_path.read_text())
    records = evidence.get("workloads", {})
    if evidence.get("status") != "pass" or set(records) != set(MANIFEST) or any(
            record.get("status") != "pass" or 1 not in record.get("covered_widths", [])
            for record in records.values()) or not {1, 4, 8} <= set(records["small_split"].get("covered_widths", [])):
        raise ValueError("failed or incomplete experiment 6 sequential identity construction evidence")
    for name, record in records.items():
        try:
            admission = validate_admission(record)
        except ValueError as error:
            raise ValueError(f"invalid sequential identity admission: {name}: {error}") from error
        if admission.get("status") != "pass":
            raise ValueError(f"failed sequential identity admission: {name}")
    required = {f"{arm}/{name}{suffix}" for name in MANIFEST for arm in ("seq", "demand")
                for suffix in ("", ".ll", ".o", ".disassembly")}
    hashes = evidence.get("hashes", {})
    if not required <= set(hashes):
        raise ValueError("missing sequential identity artifact hashes")
    for filename, digest in hashes.items():
        artifact = build / filename
        if Path(filename).is_absolute() or ".." in Path(filename).parts:
            raise ValueError("invalid sequential identity artifact path")
        if not artifact.is_file() or hashlib.sha256(artifact.read_bytes()).hexdigest() != digest:
            raise ValueError(f"sequential identity artifact changed: {filename}")
    return evidence

def e6b_construction(build):
    """E6b-identity: option-D identity plus spine's sequentialized hand-off.

    Spine's timed entry must make no pool query (its only hand-off point now
    lowers to the ordinary computation), while recursion keeps its one query
    (its balanced hand-off stays). Anything else stops timing.
    """
    evidence = construction_identity(build)
    pools = {name: evidence["workloads"][name].get("dispatch", {}).get("pool_decisions_per_timed_call")
             for name in ("spine", "recursion")}
    if pools != {"spine": 0, "recursion": 1}:
        raise ValueError(f"experiment 6b construction: spine must make 0 and recursion 1 pool queries per timed call, observed {pools}")
    return evidence


def summarize_e6(path, inspection=None, sizing=False, experiment=6, cells=None):
    selected = reduction_cells(path, experiment, cells, sizing)
    groups = load(path, experiment=experiment)
    startup = load(path, experiment=experiment, sample_index=0)
    if not sizing and any((name, width) not in selected for name, width, _ in groups):
        raise ValueError("measurement contains an unselected cell")
    if sizing and any(attempt != 1 for _, _, attempt in groups):
        raise ValueError("E6 sizing rule requires exactly six rounds without reruns")
    if not sizing:
        evidence_path = Path(path).parent / "sequential-identity.json"
        construction = (e6b_construction if experiment == E6B else construction_identity)(Path(path).parent)
        inspection = bound_inspection(path, inspection, experiment)
    inspection = inspection or {}
    cells = []
    for name, meta in MANIFEST.items():
        checked = inspection.get(name, {})
        inspected = (checked.get("hot_work_survives") is True and bool(checked.get("evidence"))
                     and bool(checked.get("check_compiles_to")))
        for width in cell_widths(experiment, name):
            if (name, width) not in selected:
                continue
            key = (name, width, 1)
            if key not in groups:
                raise ValueError(f"missing experiment {experiment} cell: {name}/{width}")
            initial = e6_attempt(name, groups[key], width, startup[key], experiment)
            cell = dict(workload=name, width=width, attribution=meta["kind"], initial=initial)
            second = groups.get((name, width, 2))
            if second is not None:
                cell["rerun"] = e6_attempt(name, second, width, startup[name, width, 2], experiment)
            verdicts = {}
            for key, rule in initial["rules"].items():
                status = rule["status"]
                if status == "exceeds":
                    status = "needs-rerun" if second is None else "fail" if cell["rerun"]["rules"][key]["status"] == "exceeds" else "inconclusive"
                elif second is not None and cell["rerun"]["rules"][key]["status"] != status:
                    status = "inconclusive"
                if initial["status"] == "void" or cell.get("rerun", {}).get("status") == "void":
                    status = "void"
                elif sizing and status != "not-applicable":
                    status = "inconclusive"
                elif status == "pass" and not inspected:
                    status = "inconclusive"
                verdicts[key] = status
            if not inspected or sizing:
                cell["reason"] = ("sizing only" if sizing else
                    "optimized hot work or intended site did not survive" if checked.get("hot_work_survives") is False else
                    "optimized hot-site inspection missing")
            # A cell with no registered E6b rule still reports its twin.
            void = "void" in verdicts.values() or not verdicts and "void" in (
                initial["status"], cell.get("rerun", {}).get("status"))
            if void:
                for attempt in (initial, *([cell["rerun"]] if "rerun" in cell else [])):
                    attempt["layout_floor"].update(status="void", radius=None, band=None)
            cell["verdicts"] = verdicts
            cell["status"] = "void" if void else next((s for s in ("fail", "needs-rerun", "inconclusive", "pass")
                                                       if s in verdicts.values()), "not-applicable")
            cells.append(cell)
    if sizing:
        # Floor controls and the literal diagnostic participate in sizing,
        # but neither diagnostic nor H3 can trigger a decisive rerun.
        sizing_cells = []
        for cell in cells:
            initial = cell["initial"]
            rules = dict(initial["rules"], literal=initial["literal_seq_diagnostic"])
            if "floor_seq_diagnostic" in initial:
                rules["floor_seq_diagnostic"] = initial["floor_seq_diagnostic"]
            rules.update({arm: dict(q, bound=1, status="intervals")
                          for arm, q in initial["layout_floor"]["controls"].items()})
            sizing_cells.append(dict(initial=dict(initial, rules=rules)))
        # E6b fixes an even count so every arm pair is order-balanced.
        count = e4_round_count(sizing_cells, step=2 if experiment == E6B else 1)
    else:
        sample_path = Path(path).parent / f"sizing-e{experiment}" / "measurements.tsv"
        if not sample_path.exists():
            raise ValueError(f"missing experiment {experiment} six-round sizing evidence")
        count = summarize_e6(sample_path, sizing=True, experiment=experiment)["decisive_rounds"]
        if any(attempt["rounds"] != count for cell in cells
               for attempt in (cell["initial"], *([cell["rerun"]] if "rerun" in cell else []))):
            raise ValueError("decisive rounds differ from the count frozen by sizing")
    result = dict(experiment=experiment, sizing=sizing, cells=cells, decisive_rounds=count,
                  selected_cells=cell_names(selected, experiment),
                  sweep=e6_sweep(groups, cells, sizing),
                  h3_policy="reported for every cell; inspect adjacent-width speedup before interpreting CPU margin")
    if not sizing:
        result.update(sizing_evidence=str(sample_path), identity_evidence=str(evidence_path))
        result["construction_admission"] = {name: dict(
            status=record["admission"]["status"], computation_status=record["computation_status"],
            category_counts=record["admission"]["category_counts"],
            cold_support_boundary=record["admission"]["cold_support_boundary"],
            entry_decisions=record["admission"]["entry_decisions"],
            observed_differences=[{key: failure[key] for key in
                ("category", "function", "reason", "instruction", "seq", "demand") if key in failure}
                for failure in record["failures"]]) for name, record in construction["workloads"].items()}
        from entry_dispatch import report as entry_report, validate_proof
        validate_proof(Path(path).parent)
        entry_path = Path(path).parent / "entry-dispatch-measurements.json"
        result["entry_dispatch"] = entry_report(json.loads(entry_path.read_text()), count)
        result["entry_dispatch_evidence"] = str(entry_path)
    return result

# Rounded spreads transcribed from DESIGN.md, "Experiment 5a's results".
# This run was globally inconclusive (records' twin voided its cell); these
# controls cannot alter any literal E4-seq verdict or establish a micro floor.
E5A_OBSERVED_SPREADS = {
    "mandelbrot": (0.004611, 0.004012, 0.002904),
    "records": (0.002021, 0.001588, 0.002010),
    "fir": (0.000895, 0.001901, 0.000965),
    "stencil": (0.001220, 0.001888, 0.001295),
    "prefix": (0.004408, 0.005413, 0.005881),
    "histogram": (0.006751, 0.003918, 0.002170),
}


def e5a_observed_spread(name):
    spreads = E5A_OBSERVED_SPREADS.get(name)
    return dict(source="research/investigations/par-demand/DESIGN.md#experiment-5as-results",
                overall_reading="inconclusive", measured_width=1, measured_cpu=2,
                workload_controls=dict(zip(E5A_PADDING, spreads)) if spreads is not None else None,
                workload_cell="void" if name == "records" else "observations" if spreads else "not-measured",
                panel_max_spread=max(max(values) for values in E5A_OBSERVED_SPREADS.values()),
                interpretation="reported beside the literal E4-seq verdict; no threshold changed")


def layout_floor(controls):
    """Prospective E5a spread: displacement or bootstrap half-width."""
    spreads = [max((q["interval"][1] - q["interval"][0]) / 2,
                   abs(q["median"] - 1)) for q in controls]
    if not spreads:
        raise ValueError("missing layout controls")
    return ("reject-image-layout" if all(s < 0.001 for s in spreads) else
            "instrument-floor" if any(s >= 0.0015 for s in spreads) else "inconclusive")


def summarize_e5a(path, sizing=False):
    groups = load(path, experiment="5a")
    startup = load(path, experiment="5a", sample_index=0)
    cells = []
    for name in E5A_MANIFEST:
        key = (name, 1, 1)
        if key not in groups:
            raise ValueError(f"missing experiment 5a cell: {name}/1")
        arms = groups[key]
        if set(arms) != set(E5A_ARMS):
            raise ValueError("missing experiment-5a arm")
        rounds = set(arms["seq"])
        if not rounds or rounds != set(range(len(rounds))) or any(set(rows) != rounds for rows in arms.values()):
            raise ValueError("unpaired or missing experiment-5a rounds")
        order = sorted(rounds)
        def ratios(index, numerator, denominator="seq"):
            return quantity([arms[numerator][r][index] / arms[denominator][r][index] for r in order])
        twin = ratios(0, "twin", "demand")
        walls = {arm: ratios(0, arm) for arm in E5A_ARMS}
        controls = {arm: dict(walls[arm], padding_bytes=size,
                             half_width=(walls[arm]["interval"][1] - walls[arm]["interval"][0]) / 2,
                             displacement=abs(walls[arm]["median"] - 1))
                    for arm, size in E5A_PADDING.items()}
        status = "void" if not twin["interval"][0] <= 1 <= twin["interval"][1] else "intervals"
        # Shape matches the E4 sizing rule; 1 is the comparison point and
        # every wall ratio, including the twin, participates in selecting n.
        initial = dict(rounds=len(rounds), rules={arm: dict(q, bound=1, status="intervals")
                                                 for arm, q in walls.items()})
        cells.append(dict(workload=name, width=1, status=status, initial=initial,
                          wall_over_seq=walls, cpu_over_seq={arm: ratios(1, arm) for arm in E5A_ARMS},
                          wall_ns={arm: quantity([arms[arm][r][0] for r in order]) for arm in E5A_ARMS},
                          cpu_ns={arm: quantity([arms[arm][r][1] for r in order]) for arm in E5A_ARMS},
                          first_call_cpu_above_wall_ns={arm: quantity([startup[key][arm][r][1] - startup[key][arm][r][0]
                                                                      for r in order]) for arm in E5A_ARMS},
                          twin_wall_ratio=twin, controls=controls))
    if sizing:
        count = e4_round_count(cells)
    else:
        sample_path = Path(path).parent / "sizing-e5a" / "measurements.tsv"
        if not sample_path.exists():
            raise ValueError("missing experiment 5a six-round sizing evidence")
        count = summarize_e5a(sample_path, sizing=True)["decisive_rounds"]
        if any(cell["initial"]["rounds"] != count for cell in cells):
            raise ValueError("decisive rounds differ from the count frozen by sizing")
    verdict = "inconclusive" if sizing or any(c["status"] == "void" for c in cells) else layout_floor(
        [q for c in cells for q in c["controls"].values()])
    result = dict(experiment="5a", sizing=sizing, cells=cells, decisive_rounds=count,
                  layout_verdict=verdict)
    if not sizing:
        result["sizing_evidence"] = str(sample_path)
    return result


def attempt_result_e3(arms, width):
    if set(arms) != set(E3_ARMS):
        raise ValueError("missing experiment-3 arm")
    rounds = set(arms["demand"])
    if not rounds or rounds != set(range(len(rounds))) or any(set(rows) != rounds for rows in arms.values()):
        raise ValueError("unpaired or missing experiment-3 rounds")
    order = sorted(rounds)
    wall = {arm: [arms[arm][r][0] / arms["demand"][r][0] for r in order] for arm in E3_ARMS}
    cpu = {arm: [arms[arm][r][1] / arms["demand"][r][1] for r in order] for arm in E3_ARMS}
    margins = {arm: [(arms[arm][r][1] - 1.1 * arms["seq"][r][1]
                     - 0.1 * max(0, arms["seq"][r][0] - arms[arm][r][0]) * width)
                    / arms["seq"][r][1] for r in order] for arm in E3_ARMS}
    twin = quantity(wall["twin"])
    return dict(rounds=len(rounds), status="intervals" if twin["interval"][0] <= 1 <= twin["interval"][1] else "void",
                wall_over_demand={arm: quantity(values) for arm, values in wall.items()},
                cpu_over_demand={arm: quantity(values) for arm, values in cpu.items()},
                h3={arm: quantity(values) for arm, values in margins.items()}, twin_wall_ratio=twin,
                h3_change={arm: quantity([a - b for a, b in zip(values, margins["demand"])])
                           for arm, values in margins.items()})


def cause_verdict(cell, arm, sizing=False):
    if sizing or cell["status"] == "void":
        return "undecided"
    if arm == "dedup":
        low, high = cell["h3"][arm]["interval"]
        base_low, base_high = cell["h3"]["demand"]["interval"]
        if max(low, base_low) <= min(high, base_high):
            return "rejected"
        return "supported" if high < base_low and cell["h3_change"][arm]["interval"][1] < 0 else "undecided"
    intervals = [cell["wall_over_demand"][arm]["interval"]]
    if arm == "extent":
        intervals.append(cell["cpu_over_demand"][arm]["interval"])
    if all(low <= 1 <= high for low, high in intervals):
        return "rejected"
    return "supported" if all(high < 1 for _, high in intervals) else "undecided"


def e3_round_count(cells):
    # Project the six-round interval widths by sqrt(6/n), as a sizing rule,
    # never a verdict. Freeze n before collecting any decisive observations.
    if any(cell["rounds"] != 6 for cell in cells):
        raise ValueError("experiment 3 sizing requires exactly six rounds")
    quantities = []
    for cell in cells:
        for metric, bound in (("wall_over_demand", 1), ("cpu_over_demand", 1), ("h3", 0)):
            for arm in E3_ARMS:
                value = cell[metric][arm]
                quantities.append((value["interval"][1] - value["interval"][0],
                                   max(0.02, abs(value["median"] - bound))))
    return next((n for n in range(6, 31) if all(width * (6 / n) ** 0.5 <= target
                                               for width, target in quantities)), 30)


def summarize_e3(path, sizing=False):
    groups = load(path, experiment=3)
    cells = []
    for name in MANIFEST:
        for width in E3_WIDTHS:
            if (name, width, 1) not in groups:
                raise ValueError(f"missing initial cell: {name}/{width}")
            cell = attempt_result_e3(groups[name, width, 1], width)
            cell.update(workload=name, width=width)
            cells.append(cell)
    if len({cell["rounds"] for cell in cells}) != 1:
        raise ValueError("experiment 3 cells must have the same frozen round count")
    if not sizing:
        sample_path = Path(path).parent / "sizing-e3" / "measurements.tsv"
        if not sample_path.exists():
            raise ValueError("missing experiment 3 six-round sizing evidence")
        sample = summarize_e3(sample_path, sizing=True)
        if any(cell["rounds"] != sample["decisive_rounds"] for cell in cells):
            raise ValueError("decisive rounds differ from the count frozen by sizing")
    causes = []
    for arm, names, widths in (("order", ("mandelbrot",), (8,)),
                               ("seed", ("mandelbrot", "recursion", "stencil"), E3_WIDTHS),
                               ("extent", ("stencil", "histogram"), E3_WIDTHS),
                               ("dedup", ("stencil", "histogram"), E3_WIDTHS)):
        for cell in cells:
            name, width = cell["workload"], cell["width"]
            if name not in names or width not in widths:
                continue
            if arm == "dedup" and (name, width) not in (("stencil", 4), ("histogram", 8)):
                continue
            causes.append(dict(cause=arm, workload=name, width=width,
                               verdict=cause_verdict(cell, arm, sizing),
                               evidence=cell["status"]))
    result = dict(experiment=3, sizing=sizing, cells=cells, causes=causes)
    if sizing:
        result["decisive_rounds"] = e3_round_count(cells)
    else:
        result["sizing_evidence"] = str(sample_path)
        result["decisive_rounds"] = sample["decisive_rounds"]
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("measurements", type=Path)
    parser.add_argument("--inspection", type=Path)
    parser.add_argument("--sizing", action="store_true")
    parser.add_argument("--experiment", type=parse_experiment, choices=(1, 2, 3, 4, "5a", "5c", 6, E6B), default=1)
    parser.add_argument("--cells", help="comma-separated workload:width cells (6/6b); empty keeps all; omitted reads measured identity")
    args = parser.parse_args()
    if args.sizing:
        inspection = None  # Sizing is unqualified and never needs an inspection.
    else:
        inspection = args.inspection  # Missing files fail in the common validator.
    rows = summarize(args.measurements, inspection, args.sizing, args.experiment, args.cells)
    print(json.dumps(rows, indent=2))
    # Experiment verdicts are data, never a language or build gate. A malformed
    # or incomplete matrix raises instead, failing the manual workflow.


if __name__ == "__main__":
    main()
