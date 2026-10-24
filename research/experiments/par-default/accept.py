#!/usr/bin/env python3
"""Stage-3 acceptance matrix for demand splitting in default --par.

Manual research only: no gate imports this file. It implements the matrix,
order, oracles and reducers registered in
research/investigations/par-default/DESIGN.md, "Stage-3 acceptance
registration", and is retired with that investigation.

It reuses the prototype runner frozen at FROZEN (research/experiments/
par-demand at that commit: runner.c, the oracles, the Makefile's arm and
layout-shift rules, identity.py, the balanced order, the P-core placement
check and the bootstrap reducers). The temporary workflow extracts that
directory beside this one; --frozen names it elsewhere. Only what the
stage-3 matrix adds lives here: the arm set (current-main --par as a
separately built baseline), the whole-process indexed and dispatch programs,
the S3 rules and their rerun discipline.

    accept.py --self-test
    accept.py build-programs --root TREE --candidate WFC --base WFC --build DIR
    accept.py run --build DIR --mode smoke|pilot|main --rounds N --candidate-revision R --base-revision R
    accept.py summarize --build DIR [--inspection FILE]
"""
import argparse
import csv
import hashlib
import json
import os
import platform
import resource
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from statistics import median

FROZEN = "73aaab85a92479ada6bf57fa996e17b163e019ed"
HERE = Path(__file__).resolve().parent

# The prior stage's unified loop/call set, in the frozen manifest's order.
UNIFIED = ("small_constant", "small_split", "recursion", "spine", "hot_helper", "large_helper",
           "mandelbrot", "records", "fir", "stencil", "prefix", "histogram")
# Whole-process programs: each is compiled from one staged source by both
# compilers. which= selects vm.wf's kernel (see select_vm_kernel).
PROGRAMS = {
    "hist256": dict(kind="indexed", source="research/experiments/indexed-reduction-timing/histogram.wf",
                    widths=(1, 2, 4, 8)),
    "hist4096": dict(kind="indexed", source="research/experiments/indexed-reduction-timing/histogram_4096.wf",
                     widths=(1, 2, 4, 8)),
    "vm-loop": dict(kind="dispatch", source="research/experiments/match-dispatch/wf/vm.wf", which=0, widths=(1, 8)),
    "vm-fib": dict(kind="dispatch", source="research/experiments/match-dispatch/wf/vm.wf", which=1, widths=(1, 8)),
    "vm-sieve": dict(kind="dispatch", source="research/experiments/match-dispatch/wf/vm.wf", which=2, widths=(1, 8)),
    "vm-mandel": dict(kind="dispatch", source="research/experiments/match-dispatch/wf/vm.wf", which=3, widths=(1, 8)),
}
VM_SELECTOR = "  let which = 0_u64;"
VM_KERNELS = 4
INDEXED = frozenset(("prefix", "histogram", "hist256", "hist4096"))
# Workloads whose frozen manifest declares a nonhoistable per-repetition decision.
DECISION_WORKLOADS = ("small_split",)
WIDTHS = (1, 2, 4, 8)
# par is current main's --par build; demand is the candidate's --par build.
CORE_ARMS = ("seq", "par", "demand", "twin")
SHIFTS = ("seq-shift64", "seq-shift4160", "seq-shift65664")
PLACEMENT = {1: [2], 2: [2, 4], 4: [2, 4, 6, 8], 8: [2, 4, 6, 8, 10, 12, 14, 1]}
MODES = ("smoke", "pilot", "main")
HIST_K = {"smoke": 2, "pilot": 20, "main": 20}
FIELDS = ("workload", "arm", "width", "round", "attempt", "sample", "wall_ns", "cpu_ns", "count")
GATING = ("pass", "exceeds", "inconclusive")
PROCESS_TIMEOUT = 600

F_SUM = F_MEAS = None


def load_frozen(directory):
    """Import the frozen reducers and runner helpers; refuse anything else."""
    global F_SUM, F_MEAS
    directory = Path(directory).resolve()
    for name in ("summarize.py", "measure.py", "identity.py", "manifest.json", "runner.c"):
        if not (directory / name).is_file():
            raise SystemExit(f"frozen runner missing {name} under {directory}; extract {FROZEN}")
    sys.path.insert(0, str(directory))
    import summarize as frozen_summarize
    import measure as frozen_measure
    if tuple(frozen_summarize.MANIFEST) != UNIFIED:
        raise SystemExit("frozen manifest differs from the registered unified set")
    if (frozen_summarize.BOOTSTRAP_DRAWS, frozen_summarize.BOOTSTRAP_SEED, frozen_summarize.DECISION_NS) != (10_000, 20261010, 1.0):
        raise SystemExit("frozen bootstrap or decision allowance differs from the registration")
    F_SUM, F_MEAS = frozen_summarize, frozen_measure


def cell_widths(name):
    return PROGRAMS[name]["widths"] if name in PROGRAMS else WIDTHS


def cell_arms(name, width):
    return CORE_ARMS + SHIFTS if name in UNIFIED and width == 1 else CORE_ARMS


def all_cells():
    return [(name, width) for name in (*UNIFIED, *PROGRAMS) for width in cell_widths(name)]


def samples(name):
    if name in UNIFIED:
        return ("0", "1")
    return ("k0", "kR") if PROGRAMS[name]["kind"] == "indexed" else ("proc",)


def unified_settings(name, mode):
    meta = F_SUM.MANIFEST[name]
    if "repetitions" not in meta:
        return {}
    smoke = mode == "smoke"
    return dict(WFD_REPETITIONS=str(meta.get("sizing_repetitions", meta["repetitions"]) if smoke else meta["repetitions"]),
                WFD_EXTENT=str(meta.get("sizing_extent", meta["extent"]) if smoke else meta["extent"]))


def decisions_per_call(name, mode):
    if name not in DECISION_WORKLOADS:
        return 0
    meta = F_SUM.MANIFEST[name]
    return meta["decisions_per_repetition"] * int(unified_settings(name, mode)["WFD_REPETITIONS"])


def base_env():
    env = dict(os.environ)
    for variable in ("WF_SPLIT_WORK", "WF_SCHED_REPORT", "WF_PAR_DEMAND", "WFD_REPETITIONS", "WFD_EXTENT", "WF_WORKERS"):
        env.pop(variable, None)
    return env


def pin(pinned, width, command):
    cpus = pinned.get(width)
    return ["taskset", "-c", ",".join(map(str, cpus))] + command if cpus else command


def placement(mode):
    """The registered P-core lists on the 14900K; smoke runs unpinned."""
    if mode == "smoke":
        return {}, dict(placement_policy="unpinned hosted smoke; decides nothing")
    if platform.system() != "Linux" or shutil.which("taskset") is None:
        raise SystemExit("pilot and main require Linux and taskset")
    if "i9-14900K" not in Path("/proc/cpuinfo").read_text():
        raise SystemExit("pilot and main run only on the i9-14900K")
    pinned, topology = F_MEAS.e5c_placement()
    pinned[2] = [2, 4]
    if pinned != PLACEMENT:
        raise SystemExit(f"placement {pinned} differs from the registered lists {PLACEMENT}")
    topology["placement_policy"] = "stage-3 registered P-core lists; never CPU 0; W8 shares CPU 0's core through CPU 1"
    return pinned, topology


def run_process(command, env):
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.monotonic_ns()
    completed = subprocess.run(command, env=env, capture_output=True, timeout=PROCESS_TIMEOUT)
    wall = time.monotonic_ns() - start
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = round((after.ru_utime - before.ru_utime + after.ru_stime - before.ru_stime) * 1e9)
    return completed, wall, cpu


def checksum(completed, label):
    if completed.returncode != 0 or len(completed.stdout) != 8:
        raise RuntimeError(f"{label}: status {completed.returncode}, {len(completed.stdout)} output bytes; "
                           f"stderr {completed.stderr.decode(errors='replace')[-400:]}")
    return int.from_bytes(completed.stdout, "little")


# ---------------------------------------------------------------- construction

def select_vm_kernel(text, which):
    """Keep only the selected kernel's arm of vm.wf's `if which == K` chain.

    With the selector a constant every one of those conditions is decided, and
    the redundant-condition rule [OP-5] rejects a decided condition; this is
    the mechanical repair its diagnostic gives, applied to each arm.
    """
    lines = text.split("\n")
    if lines.count(VM_SELECTOR) != 1:
        raise SystemExit("vm.wf must contain its kernel selector exactly once")
    kept, index, arms = [], 0, set()
    while index < len(lines):
        line = lines[index]
        arm = next((k for k in range(VM_KERNELS) if line == f"  if which == {k}_u64 {{"), None)
        if line == VM_SELECTOR:
            index += 1
            continue
        if arm is None:
            kept.append(line)
            index += 1
            continue
        end = lines.index("  }", index + 1)
        if arm == which:
            body = lines[index + 1:end]
            if not all(row.startswith("    ") or not row for row in body):
                raise SystemExit(f"vm.wf kernel {arm}: arm body is not indented under its test")
            kept.extend(row[2:] for row in body)
        arms.add(arm)
        index = end + 1
    if arms != set(range(VM_KERNELS)):
        raise SystemExit(f"vm.wf kernel chain has arms {sorted(arms)}, expected 0..{VM_KERNELS - 1}")
    return "\n".join(kept)


def build_programs(root, candidate, base, build):
    """Stage each program once; compile it with both compilers; twin is a copy."""
    staged_dir = build / "programs"
    staged_dir.mkdir(parents=True, exist_ok=True)
    for name, meta in PROGRAMS.items():
        text = (root / meta["source"]).read_text()
        if "which" in meta:
            text = select_vm_kernel(text, meta["which"])
        staged = staged_dir / f"{name}.wf"
        staged.write_text(text)
        for arm, compiler, flags in (("seq", candidate, []), ("demand", candidate, ["--par"]), ("par", base, ["--par"])):
            output = build / arm / name
            output.parent.mkdir(parents=True, exist_ok=True)
            subprocess.run([str(compiler), *flags, str(staged), "-o", str(output)], check=True)
        twin = build / "twin" / name
        twin.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(build / "demand" / name, twin)
        if twin.read_bytes() != (build / "demand" / name).read_bytes():
            raise SystemExit(f"{name}: twin copy differs")


def image_hashes(build):
    hashes = {}
    for name, width in all_cells():
        for arm in cell_arms(name, width):
            path = build / arm / name
            if not path.is_file() or not os.access(path, os.X_OK):
                raise SystemExit(f"missing image {arm}/{name}")
            hashes.setdefault(name, {})[arm] = hashlib.sha256(path.read_bytes()).hexdigest()
    for name, arms in hashes.items():
        if arms["demand"] != arms["twin"]:
            raise SystemExit(f"{name}: candidate and twin images differ")
    return hashes


def verify(build, mode, pinned):
    """Every image at every width passes its independent oracle before timing."""
    env = base_env()
    references = {}
    k = HIST_K[mode]
    for name, width in all_cells():
        settings = dict(env, WF_WORKERS=str(width))
        for arm in cell_arms(name, width):
            image = str(build / arm / name)
            if name in UNIFIED:
                completed = subprocess.run(pin(pinned, width, [image, "verify"]), env=settings,
                                           capture_output=True, text=True, timeout=PROCESS_TIMEOUT)
                if completed.returncode != 0 or "oracle PASS" not in completed.stdout:
                    raise SystemExit(f"oracle failed: {arm}/{name} W={width}: {completed.stdout}{completed.stderr}")
            elif PROGRAMS[name]["kind"] == "indexed":
                for repetitions in (0, k):
                    completed = subprocess.run(pin(pinned, width, [image, str(repetitions)]), env=settings,
                                               capture_output=True, timeout=PROCESS_TIMEOUT)
                    value = checksum(completed, f"{arm}/{name} K={repetitions} W={width}")
                    # The ordinary build at width 1 executes in source order: the oracle.
                    expected = references.setdefault(f"{name}:{repetitions}", value)
                    if value != expected:
                        raise SystemExit(f"wrong result: {arm}/{name} K={repetitions} W={width} gave {value}, seq gave {expected}")
            else:
                completed = subprocess.run(pin(pinned, width, [image]), env=settings,
                                           capture_output=True, timeout=PROCESS_TIMEOUT)
                if completed.returncode != 0:
                    raise SystemExit(f"oracle failed: {arm}/{name} W={width} exited {completed.returncode}")
    return references


# ------------------------------------------------------------------ measuring

def runner_rows(build, name, arm, width, round_id, attempt, env, pinned):
    command = [str(build / arm / name), "measure", arm, str(width), str(round_id), str(attempt)]
    completed = subprocess.run(pin(pinned, width, command), env=env, capture_output=True, text=True,
                               timeout=PROCESS_TIMEOUT)
    if completed.returncode != 0:
        raise RuntimeError(f"{arm}/{name} W={width}: {completed.stdout}{completed.stderr}")
    rows = [line.split("\t") for line in completed.stdout.splitlines() if line]
    expected = [[name, arm, str(width), str(round_id), str(attempt), sample] for sample in ("0", "1")]
    if len(rows) != 2 or any(len(row) != len(FIELDS) or row[:6] != want for row, want in zip(rows, expected)):
        raise RuntimeError(f"{arm}/{name} W={width}: malformed runner output {completed.stdout!r}")
    return rows


def program_rows(build, name, arm, width, round_id, attempt, env, pinned, mode, references):
    image = str(build / arm / name)
    rows = []
    if PROGRAMS[name]["kind"] == "indexed":
        k = HIST_K[mode]
        order = ((0, "k0"), (k, "kR")) if round_id % 2 == 0 else ((k, "kR"), (0, "k0"))
        for repetitions, sample in order:
            completed, wall, cpu = run_process(pin(pinned, width, [image, str(repetitions)]), env)
            value = checksum(completed, f"{arm}/{name} K={repetitions}")
            if value != references[f"{name}:{repetitions}"]:
                raise RuntimeError(f"wrong result while timing {arm}/{name} K={repetitions} W={width}")
            rows.append([name, arm, width, round_id, attempt, sample, wall, cpu, 1])
    else:
        completed, wall, cpu = run_process(pin(pinned, width, [image]), env)
        if completed.returncode != 0:
            raise RuntimeError(f"wrong result while timing {arm}/{name} W={width}: exit {completed.returncode}")
        rows.append([name, arm, width, round_id, attempt, "proc", wall, cpu, 1])
    return rows


def measure_attempt(build, mode, rounds, attempt, selected, pinned, references):
    """Rotate and reverse workloads, widths and arms by round (frozen order)."""
    path = build / "measurements.tsv"
    env = base_env()
    names = [*UNIFIED, *PROGRAMS]
    with path.open("a") as output:
        for round_id in range(rounds):
            for name in F_MEAS.balanced_order(names, round_id):
                for width in F_MEAS.balanced_order(cell_widths(name), round_id):
                    if (name, width) not in selected:
                        continue
                    arms = F_MEAS.balanced_order(cell_arms(name, width), round_id, WIDTHS.index(width))
                    settings = dict(env, WF_WORKERS=str(width))
                    if name in UNIFIED:
                        settings.update(unified_settings(name, mode))
                    for arm in arms:
                        print(f"attempt={attempt} round={round_id} {name} W={width} {arm}", flush=True)
                        if name in UNIFIED:
                            rows = runner_rows(build, name, arm, width, round_id, attempt, settings, pinned)
                        else:
                            rows = program_rows(build, name, arm, width, round_id, attempt, settings, pinned, mode, references)
                        for row in rows:
                            output.write("\t".join(map(str, row)) + "\n")
                        output.flush()


# ------------------------------------------------------------------- reducing

def load_rows(path, k):
    """Group the decisive quantity by cell, attempt, arm and round.

    Runner workloads decide on the second call (sample 1) and report the
    first; indexed programs decide on (K=k process - K=0 process) / k;
    dispatch programs on the whole process. Malformed, duplicate, unpaired or
    extent-changing rows raise instead of being dropped.
    """
    raw, extents = {}, {}
    cells = set(all_cells())
    with Path(path).open() as stream:
        for row in csv.reader(stream, delimiter="\t"):
            if len(row) != len(FIELDS):
                raise ValueError(f"malformed measurement: {row}")
            name, arm, sample = row[0], row[1], row[5]
            width, round_id, attempt, wall, cpu, count = (int(row[i]) for i in (2, 3, 4, 6, 7, 8))
            if ((name, width) not in cells or arm not in cell_arms(name, width) or sample not in samples(name)
                    or attempt not in (1, 2) or round_id < 0 or wall <= 0 or cpu < 0 or count <= 0):
                raise ValueError(f"invalid measurement: {row}")
            if extents.setdefault((name, sample), count) != count:
                raise ValueError(f"comparison extent changed: {name}")
            key = (name, width, attempt)
            slot = raw.setdefault(key, {}).setdefault(arm, {}).setdefault(round_id, {})
            if sample in slot:
                raise ValueError(f"duplicate measurement: {row}")
            slot[sample] = (wall, cpu)
    groups, first = {}, {}
    for (name, width, attempt), arms in raw.items():
        expected = cell_arms(name, width)
        if set(arms) != set(expected):
            raise ValueError(f"missing arm in {name} W={width} attempt {attempt}")
        rounds = set(arms["seq"])
        if rounds != set(range(len(rounds))) or any(set(rows) != rounds for rows in arms.values()):
            raise ValueError(f"unpaired or missing rounds in {name} W={width} attempt {attempt}")
        for arm, rows in arms.items():
            for round_id, values in rows.items():
                if set(values) != set(samples(name)):
                    raise ValueError(f"missing sample in {name}/{arm} W={width} round {round_id}")
                if name in UNIFIED:
                    decisive = values["1"]
                    first.setdefault((name, width, attempt), {}).setdefault(arm, {})[round_id] = values["0"]
                elif PROGRAMS[name]["kind"] == "indexed":
                    decisive = tuple((values["kR"][i] - values["k0"][i]) / k for i in (0, 1))
                else:
                    decisive = values["proc"]
                groups.setdefault((name, width, attempt), {}).setdefault(arm, {})[round_id] = decisive
    return groups, first


def floor_radius(result):
    """The workload's layout floor: the widest shifted-seq displacement at W1."""
    controls = result.get("layout_controls") or {}
    if not controls or result["status"] == "void":
        return None
    return max(max(abs(q["interval"][0] - 1), abs(q["interval"][1] - 1)) for q in controls.values())


def evaluate(name, width, arms, floor, decisions=0):
    """One attempt of one cell under the registered S3 rules."""
    q, upper = F_SUM.quantity, F_SUM.upper_bound
    expected = cell_arms(name, width)
    if set(arms) != set(expected):
        raise ValueError(f"missing arm in {name} W={width}")
    rounds = sorted(arms["seq"])
    walls = {arm: [arms[arm][r][0] for r in rounds] for arm in expected}
    cpus = {arm: [arms[arm][r][1] for r in rounds] for arm in expected}
    result = dict(workload=name, width=width, rounds=len(rounds))
    if min(min(values) for values in walls.values()) <= 0:
        # A nonpositive differenced histogram time leaves the ratio undefined.
        result.update(status="inconclusive", reason="nonpositive differenced wall time",
                      rules={"S3-seq": dict(status="inconclusive")})
        return result

    def ratio(numerator, denominator, source=walls):
        return [a / b for a, b in zip(source[numerator], source[denominator])]
    twin = q(ratio("twin", "demand"))
    void = not twin["interval"][0] <= 1 <= twin["interval"][1]
    over_seq = ratio("demand", "seq")
    rules = {}
    if decisions:
        bound = 1 + max(0.02, decisions * F_SUM.DECISION_NS / median(walls["seq"]))
        rules["S3-decision"] = upper(over_seq, bound)
    else:
        rules["S3-seq"] = F_SUM.floor_bound(over_seq, floor)
    par_over_seq = q(ratio("par", "seq"))
    rules["S3-par"] = upper(ratio("demand", "par"), 1.05)
    if par_over_seq["interval"][1] >= 1:
        rules["S3-par"].update(status="not-applicable", reason="current --par does not speed this cell up")
    if min(cpus["seq"]) <= 0:
        rules["S3-cpu"] = dict(status="inconclusive", reason="nonpositive sequential CPU")
    else:
        margins = [(cpu - 1.1 * seq_cpu - 0.1 * max(0, seq_wall - wall) * width) / seq_cpu
                   for cpu, seq_cpu, seq_wall, wall in zip(cpus["demand"], cpus["seq"], walls["seq"], walls["demand"])]
        rules["S3-cpu"] = upper(margins, 0)
        if q(over_seq)["interval"][1] < 1:
            rules["S3-cpu"].update(reading=rules["S3-cpu"]["status"], status="reported",
                                   reason="the candidate is demonstrably faster than sequential here")
    if void:
        for rule in rules.values():
            rule["status"] = "void"
    statuses = {rule["status"] for rule in rules.values() if rule["status"] in GATING}
    result.update(status="void" if void else "exceeds" if "exceeds" in statuses else
                  "inconclusive" if "inconclusive" in statuses else "pass",
                  rules=rules, twin_wall_ratio=twin,
                  wall_ns={arm: q(values) for arm, values in walls.items()},
                  cpu_ns={arm: q(values) for arm, values in cpus.items()},
                  wall_over_seq={arm: q(ratio(arm, "seq")) for arm in expected},
                  cpu_over_seq={arm: q(ratio(arm, "seq", cpus)) for arm in expected if min(cpus["seq"]) > 0},
                  layout_floor_radius=floor)
    if set(SHIFTS) <= set(expected):
        result["layout_controls"] = {arm: q(ratio(arm, "seq")) for arm in SHIFTS}
    return result


def combine(initial, rerun):
    """Frozen rerun discipline: a fail needs two exceeding attempts."""
    verdicts = {}
    for key, rule in initial["rules"].items():
        status = rule["status"]
        if status == "exceeds":
            status = "needs-rerun" if rerun is None else "fail" if rerun["rules"][key]["status"] == "exceeds" else "inconclusive"
        elif rerun is not None and rerun["rules"][key]["status"] != status:
            status = "inconclusive"
        if initial["status"] == "void" or (rerun or {}).get("status") == "void":
            status = "void"
        verdicts[key] = status
    return verdicts


def check_inspection(inspection):
    if inspection is None:
        return {}
    names = {*UNIFIED, *PROGRAMS}
    if set(inspection) != names:
        raise ValueError("inspection must name every workload")
    for name, record in inspection.items():
        if type(record.get("hot_work_survives")) is not bool or not record.get("evidence"):
            raise ValueError(f"inspection record for {name} needs hot_work_survives and evidence")
    return inspection


def reduce(groups, first, mode, rounds, decisions, inspection=None, selected=None):
    inspection = check_inspection(inspection)
    floors, cells = {}, []
    for name, width in all_cells():
        if selected is not None and (name, width) not in selected:
            continue
        key = (name, width, 1)
        if key not in groups:
            raise ValueError(f"missing cell {name} W={width}")
        initial = evaluate(name, width, groups[key], floors.get(name), decisions.get(name, 0))
        if name in UNIFIED and width == 1:
            floors[name] = floor_radius(initial)
            initial["layout_floor_radius"] = floors[name]
        rerun = None
        if (name, width, 2) in groups:
            rerun = evaluate(name, width, groups[name, width, 2], floors.get(name), decisions.get(name, 0))
        for attempt in (initial, rerun):
            if attempt is not None and attempt["rounds"] != rounds:
                raise ValueError(f"{name} W={width}: {attempt['rounds']} rounds, registered {rounds}")
        verdicts = combine(initial, rerun)
        inspected = inspection.get(name, {}).get("hot_work_survives") is True
        for rule, status in verdicts.items():
            if mode != "main" and status not in ("not-applicable", "reported"):
                verdicts[rule] = f"{mode}-only"
            elif status == "pass" and not inspected:
                verdicts[rule] = "pass-uninspected"
        cell = dict(workload=name, width=width, indexed=name in INDEXED, initial=initial, verdicts=verdicts)
        if rerun is not None:
            cell["rerun"] = rerun
        if (name, width, 1) in first:
            cell["first_call_wall_ns"] = {arm: F_SUM.quantity([v[0] for _, v in sorted(rows.items())])
                                          for arm, rows in first[name, width, 1].items()}
        cells.append(cell)
    return cells


def overall(cells, mode):
    if mode != "main":
        return dict(verdict="none", reason=f"{mode} decides no verdict")
    gating = [status for cell in cells for status in cell["verdicts"].values()
              if status not in ("not-applicable", "reported")]
    verdict = ("reject" if "fail" in gating else
               "accept" if gating and all(status == "pass" for status in gating) else "undecided")
    useful = [f"{c['workload']}:{c['width']}" for c in cells if c["indexed"] and c["width"] > 1
              and c["verdicts"].get("S3-seq") in ("pass", "pass-uninspected")]
    return dict(verdict=verdict,
                failed_rules=[f"{c['workload']}:{c['width']}:{rule}" for c in cells
                              for rule, status in c["verdicts"].items() if status == "fail"],
                indexed_useful_cells=useful,
                indexed_reading=("useful parallel indexed cells exist" if useful else
                                 "no indexed parallel cell meets the sequential bound: the sequential alternative returns to the owner"))


def recommended_rounds(cells):
    """Frozen E6b sizing rule over the gating rules: smallest even n in 6..30."""
    sizing = [dict(initial=dict(rounds=cell["initial"]["rounds"],
                                rules={key: rule for key, rule in cell["initial"]["rules"].items()
                                       if rule["status"] in GATING and "bound" in rule}))
              for cell in cells]
    return F_SUM.e4_round_count(sizing, step=2)


def summarize(build, inspection=None):
    identity = json.loads((build / "identity.json").read_text())
    mode, rounds = identity["mode"], identity["rounds"]
    groups, first = load_rows(build / "measurements.tsv", HIST_K[mode])
    decisions = {name: decisions_per_call(name, mode) for name in DECISION_WORKLOADS}
    cells = reduce(groups, first, mode, rounds, decisions, inspection)
    result = dict(mode=mode, rounds=rounds, candidate=identity["candidate_revision"], base=identity["base_revision"],
                  frozen_runner=FROZEN, cells=cells, overall=overall(cells, mode),
                  inspection="provided" if inspection else "missing: passes are uninspected")
    if mode == "pilot":
        n = recommended_rounds(cells)
        per_round = identity["elapsed_seconds"]["attempt1"] / rounds
        result["pilot"] = dict(decisive_rounds=n, seconds_per_round=per_round,
                               main_timing_seconds_without_reruns=per_round * n,
                               main_timing_seconds_if_every_cell_reruns=2 * per_round * n)
    return result


def table(summary):
    lines = [f"mode={summary['mode']} rounds={summary['rounds']} candidate={summary['candidate']} base={summary['base']}",
             f"overall={json.dumps(summary['overall'])}"]
    if "pilot" in summary:
        lines.append(f"pilot={json.dumps(summary['pilot'])}")
    lines.append("workload\tW\tstatus\tdemand/seq\tdemand/par\tpar/seq\ttwin/demand\tverdicts")
    for cell in summary["cells"]:
        initial = cell["initial"]
        if "wall_over_seq" not in initial:
            lines.append(f"{cell['workload']}\t{cell['width']}\t{initial['status']}\t-\t-\t-\t-\t{cell['verdicts']}")
            continue
        def show(value):
            return f"{value['median']:.4f} [{value['interval'][0]:.4f}, {value['interval'][1]:.4f}]"
        demand_par = initial["rules"]["S3-par"]
        lines.append("\t".join([cell["workload"], str(cell["width"]), initial["status"],
                                show(initial["wall_over_seq"]["demand"]), show(demand_par) if "median" in demand_par else "-",
                                show(initial["wall_over_seq"]["par"]), show(initial["twin_wall_ratio"]),
                                json.dumps(cell["verdicts"])]))
    return "\n".join(lines) + "\n"


def registered_rounds(mode, rounds):
    if mode == "smoke" and rounds == 2 or mode == "pilot" and rounds == 6:
        return rounds
    if mode == "main" and 6 <= rounds <= 30 and rounds % 2 == 0:
        return rounds
    raise SystemExit("rounds: smoke 2, pilot 6, main an even count in 6..30 frozen from the pilot")


def run(args):
    build = args.build.resolve()
    rounds = registered_rounds(args.mode, args.rounds)
    if (build / "measurements.tsv").exists():
        raise SystemExit("measurements.tsv exists; a run starts from a fresh build directory")
    pinned, topology = placement(args.mode)
    hashes = image_hashes(build)
    layout = F_MEAS.layout_identity(build, {name: F_SUM.MANIFEST[name] for name in UNIFIED})
    references = verify(build, args.mode, pinned)
    identity = dict(mode=args.mode, rounds=rounds, hist_k=HIST_K[args.mode], frozen_runner=FROZEN,
                    candidate_revision=args.candidate_revision, base_revision=args.base_revision,
                    host=platform.uname()._asdict(), pinned={str(k): v for k, v in pinned.items()},
                    topology=topology, hashes=hashes, layout=layout, references=references,
                    manifest=F_SUM.MANIFEST, programs=PROGRAMS, elapsed_seconds={})
    (build / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    start = time.monotonic()
    measure_attempt(build, args.mode, rounds, 1, set(all_cells()), pinned, references)
    identity["elapsed_seconds"]["attempt1"] = time.monotonic() - start
    (build / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    if args.mode == "main":
        initial = summarize(build)
        rerun = {(c["workload"], c["width"]) for c in initial["cells"] if c["initial"]["status"] == "exceeds"}
        identity["rerun_cells"] = sorted(f"{name}:{width}" for name, width in rerun)
        if rerun:
            start = time.monotonic()
            measure_attempt(build, args.mode, rounds, 2, rerun, pinned, references)
            identity["elapsed_seconds"]["attempt2"] = time.monotonic() - start
        (build / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    write_summary(build, None)


def write_summary(build, inspection):
    summary = summarize(build, inspection)
    (build / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    (build / "summary.txt").write_text(table(summary))
    print(table(summary))


# ------------------------------------------------------------------ self-test

def self_test():
    """Each S3 rule detects a representative wrong result; malformed data raises."""
    def cell(name, width, scale, rounds=6, cpu=None, twin=1.0, par=1.0):
        result = {}
        for arm in cell_arms(name, width):
            factor = dict(seq=1.0, par=par, demand=scale, twin=scale * twin).get(arm, 1.0)
            cpu_factor = (cpu or {}).get(arm, factor)
            result[arm] = {r: (1e8 * factor * (1 + 0.0005 * ((r * 7 + len(arm)) % 5 - 2)),
                               1e8 * cpu_factor * (1 + 0.0005 * ((r * 3 + len(arm)) % 5 - 2))) for r in range(rounds)}
        return result

    def expect(condition, message):
        if not condition:
            raise AssertionError(message)

    if PLACEMENT != {**F_MEAS.E5C_PLACEMENT, 2: [2, 4]}:
        raise AssertionError("registered placement differs from the frozen 6b lists")
    same = evaluate("fir", 2, cell("fir", 2, 1.0), 0.005)
    expect(same["rules"]["S3-seq"]["status"] == "pass", "equal arms inside the layout floor must pass S3-seq")
    expect(evaluate("fir", 2, cell("fir", 2, 0.98), None)["rules"]["S3-seq"]["status"] == "pass",
           "a candidate 2% faster than sequential must pass S3-seq without a floor")
    expect(same["rules"]["S3-par"]["status"] == "not-applicable", "par without speedup must not apply")
    slow = evaluate("fir", 2, cell("fir", 2, 1.10), None)
    expect(slow["rules"]["S3-seq"]["status"] == "exceeds", "a 10% slower candidate must exceed S3-seq")
    expect(combine(slow, slow)["S3-seq"] == "fail", "two exceeding attempts must fail")
    expect(combine(slow, same)["S3-seq"] == "inconclusive", "a disagreeing rerun is inconclusive")
    expect(combine(slow, None)["S3-seq"] == "needs-rerun", "one exceeding attempt needs a rerun")
    floor = evaluate("fir", 4, cell("fir", 4, 1.004), 0.008)
    expect(floor["rules"]["S3-seq"]["status"] == "pass", "an interval inside the floor must pass")
    expect(evaluate("fir", 4, cell("fir", 4, 1.004), None)["rules"]["S3-seq"]["status"] == "exceeds",
           "without a floor, 1.004 must exceed 1.00")
    expect(evaluate("fir", 4, cell("fir", 4, 1.004), 0.002)["rules"]["S3-seq"]["status"] == "exceeds",
           "an interval above a narrower floor must exceed")
    void = evaluate("fir", 2, cell("fir", 2, 1.0, twin=1.05), None)
    expect(void["status"] == "void" and all(r["status"] == "void" for r in void["rules"].values()),
           "a disagreeing twin must void the cell")
    lost = evaluate("mandelbrot", 8, cell("mandelbrot", 8, 0.60, par=0.50), None)
    expect(lost["rules"]["S3-par"]["status"] == "exceeds", "losing 20% of current --par speedup must exceed S3-par")
    kept = evaluate("mandelbrot", 8, cell("mandelbrot", 8, 0.51, par=0.50), None)
    expect(kept["rules"]["S3-par"]["status"] == "pass", "a 2% loss of speedup must pass S3-par")
    hot = evaluate("spine", 8, cell("spine", 8, 1.0, cpu=dict(demand=3.0)), None)
    expect(hot["rules"]["S3-cpu"]["status"] == "exceeds", "triple CPU without speedup must exceed S3-cpu")
    fast = evaluate("spine", 8, cell("spine", 8, 0.5, cpu=dict(demand=3.0)), None)
    expect(fast["rules"]["S3-cpu"]["status"] == "reported" and fast["rules"]["S3-cpu"]["reading"] == "exceeds",
           "CPU above the bound is reported, not gating, where the candidate is faster")
    decision = evaluate("small_split", 4, cell("small_split", 4, 1.015), None, decisions=1)
    expect(decision["rules"]["S3-decision"]["status"] == "pass", "1.5% is inside the 2% decision allowance")
    decision = evaluate("small_split", 4, cell("small_split", 4, 1.05), None, decisions=1)
    expect(decision["rules"]["S3-decision"]["status"] == "exceeds", "5% exceeds the decision allowance")
    shifted = evaluate("fir", 1, cell("fir", 1, 1.0), None)
    expect(floor_radius(shifted) is not None and floor_radius(shifted) < 0.01, "W1 cells carry their layout floor")
    expect(recommended_rounds([dict(initial=same)]) in range(6, 31, 2), "pilot recommends an even round count")
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "rows.tsv"
        good = [["hist256", arm, 2, r, 1, s, 3000 if s == "kR" else 1000, 900, 1]
                for arm in CORE_ARMS for r in range(2) for s in ("k0", "kR")]
        def write(rows):
            path.write_text("".join("\t".join(map(str, row)) + "\n" for row in rows))
        write(good)
        groups, _ = load_rows(path, 2)
        expect(groups["hist256", 2, 1]["seq"][0] == (1000.0, 0.0), "indexed rows difference K=k and K=0 per k")
        for broken, reason in ((good[1:], "a missing sample"), (good + good[:1], "a duplicate row"),
                               ([row for row in good if row[1] != "twin"], "a missing arm"),
                               ([row for row in good if row[3] != 0], "a missing round"),
                               ([[*good[0][:8], 2], *good[1:]], "a changed extent")):
            write(broken)
            try:
                load_rows(path, 2)
            except ValueError:
                continue
            raise AssertionError(f"load_rows accepted {reason}")
        negative = [[*row[:6], 500 if row[5] == "kR" else row[6], *row[7:]] for row in good]
        write(negative)
        groups, _ = load_rows(path, 2)
        expect(evaluate("hist256", 2, groups["hist256", 2, 1], None)["status"] == "inconclusive",
               "a nonpositive differenced time is inconclusive")

    # Kernel selection keeps exactly the chosen arm, and refuses a chain it
    # does not recognize instead of staging a program with the wrong kernel.
    vm = (HERE.parents[1] / "match-dispatch" / "wf" / "vm.wf").read_text()
    for which in range(VM_KERNELS):
        staged = select_vm_kernel(vm, which)
        expect("which" not in staged,
               f"kernel {which}: the selector survives staging")
        for other, builder in enumerate(("build_loop", "build_fib", "build_sieve", "build_mandel")):
            calls = staged.count(f"set ok = {builder}(code: &code);")
            expect(calls == (other == which), f"kernel {which}: {builder} staged {calls} times")
    for broken, reason in ((vm.replace("  if which == 3_u64 {", "  if which == 4_u64 {"), "a missing arm"),
                           (vm.replace(VM_SELECTOR, ""), "no selector")):
        try:
            select_vm_kernel(broken, 0)
        except SystemExit:
            continue
        raise AssertionError(f"select_vm_kernel accepted {reason}")
    print("accept.py self-test: pass")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--frozen", type=Path, default=HERE.parent / "par-demand",
                        help=f"the par-demand runner directory extracted from {FROZEN}")
    parser.add_argument("--self-test", action="store_true")
    commands = parser.add_subparsers(dest="command")
    programs = commands.add_parser("build-programs")
    programs.add_argument("--root", type=Path, required=True)
    programs.add_argument("--candidate", type=Path, required=True)
    programs.add_argument("--base", type=Path, required=True)
    programs.add_argument("--build", type=Path, required=True)
    measure = commands.add_parser("run")
    measure.add_argument("--build", type=Path, required=True)
    measure.add_argument("--mode", choices=MODES, required=True)
    measure.add_argument("--rounds", type=int, required=True)
    measure.add_argument("--candidate-revision", required=True)
    measure.add_argument("--base-revision", required=True)
    reduce_parser = commands.add_parser("summarize")
    reduce_parser.add_argument("--build", type=Path, required=True)
    reduce_parser.add_argument("--inspection", type=Path)
    args = parser.parse_args()
    load_frozen(args.frozen)
    if args.self_test:
        self_test()
    elif args.command == "build-programs":
        build_programs(args.root.resolve(), args.candidate.resolve(), args.base.resolve(), args.build.resolve())
    elif args.command == "run":
        run(args)
    elif args.command == "summarize":
        inspection = json.loads(args.inspection.read_text()) if args.inspection else None
        write_summary(args.build.resolve(), inspection)
    else:
        parser.error("choose --self-test or a command")


if __name__ == "__main__":
    main()
