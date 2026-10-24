#!/usr/bin/env python3
"""Before/after comparison of two compilers on two interpreters (README.md
beside this file): the Halo Lua interpreter running fib, and the wasm
interpreter v2h running CoreMark.

    interp.py build --base-wfc WFC --candidate-wfc WFC --halo DIR --silverfir DIR --out DIR
    interp.py time --out DIR --cpu N [--min-rounds 10] [--max-rounds 30]
    interp.py perf --out DIR --cpu N [--rounds 5]
    interp.py reduce --out DIR [--floor FLOOR]
    interp.py self-test

`build` writes DIR/images: for each interpreter the base and candidate images
and the twin, a byte copy of the base image. `time` runs interleaved rounds
and writes DIR/time.tsv; `perf` runs `perf stat` rounds and writes
DIR/perf.json. `reduce` writes DIR/summary.txt: the timed verdicts by the
hash-map comparison's rule (its functions are used as they are) and the
counter tables. FLOOR is the recorded noise floor of each workload, as
"workload=spread" items separated by commas, and a workload whose twin spread
is above its floor is not counted. Exit status: 0 when the data are complete
and consistent, whatever the verdicts; 1 for missing, malformed or
inconsistent data or floor; 2 for usage.
"""
import argparse
import csv
import hashlib
import io
import itertools
import json
import math
import os
import random
import re
import shutil
import statistics
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "hash-map-compare"))
import compare  # noqa: E402  the hash-map comparison's statistics and rule

Invalid = compare.Invalid

ARMS = compare.ARMS
WORKLOADS = ["fib", "coremark"]
# Halo's fib kernel at N = FIB_N prints fib(FIB_N); CoreMark at COREMARK_ITERATIONS
# iterations of the 2K performance run.
FIB_N = 32
FIB_VALUE = "2178309"
COREMARK_ITERATIONS = 2000
COREMARK_ARGUMENTS = ["0x0", "0x0", "0x66"]
# The component CRCs of the 2K performance run, and the final CRC the
# match-dispatch investigation recorded for its v2h interpreter at 2000
# iterations (research/experiments/match-dispatch/RESULTS.md).
CRC = {"crclist": "0xe714", "crcmatrix": "0x1fd7", "crcstate": "0x8e3a"}
CRC_FINAL = "0x4983"
# Halo's benchmark graph moved from research/ to tests/ between the revisions
# the experiments pinned; the first that exists is used.
HALO_GRAPHS = ["tests/halo-bench/modules.wfg", "research/experiments/halo-bench/modules.wfg"]
HALO_FIB = "research/experiments/halo-bench/kernels/fib.lua"
COREMARK_MODULE = "benchmarks/wasi/coremark/coremark.wasm"
V2H_GENERATOR = os.path.join(HERE, "..", "match-dispatch", "wasm", "gen.py")
RUN_BOUND = 600
RAW_COLUMNS = ["round", "position", "workload", "arm", "ns"]
PERF_EVENTS = ["instructions:u", "cycles:u", "branches:u", "branch-misses:u",
               "L1-dcache-loads:u", "L1-dcache-load-misses:u", "stalled-cycles-backend:u"]
# The rates the counter table reports: label, numerator, denominator, percent.
PERF_RATES = [("IPC", "instructions:u", "cycles:u", False),
              ("branch miss", "branch-misses:u", "branches:u", True),
              ("L1 miss", "L1-dcache-load-misses:u", "L1-dcache-loads:u", True),
              ("backend stall", "stalled-cycles-backend:u", "cycles:u", True)]
# The cell key compare.judge reads its resolution by: not a map size, so the
# default 2%.
RESOLUTION_KEY = None


# ---------------------------------------------------------------- inputs


def sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def run(command, **kwargs):
    print("+ " + " ".join(str(c) for c in command), flush=True)
    subprocess.run([str(c) for c in command], check=True, **kwargs)


def fresh(path):
    if os.path.exists(path):
        shutil.rmtree(path)
    os.makedirs(path)
    return path


def fib_source(text):
    """The Halo fib kernel with its size set to FIB_N."""
    out, changed = re.subn(r"^local N = [0-9]+$", f"local N = {FIB_N}", text, flags=re.M)
    if changed != 1:
        raise Invalid(f"fib kernel: expected one 'local N = <number>' line, found {changed}")
    return out


def first_existing(root, candidates):
    for name in candidates:
        if os.path.exists(os.path.join(root, name)):
            return name
    raise Invalid(f"none of {candidates} exists under {root}")


def build(args):
    images = os.path.join(args.out, "images")
    inputs = os.path.join(args.out, "inputs")
    os.makedirs(images, exist_ok=True)
    os.makedirs(inputs, exist_ok=True)
    halo = os.path.abspath(args.halo)
    graph = first_existing(halo, HALO_GRAPHS)
    with open(os.path.join(halo, HALO_FIB)) as f:
        fib = fib_source(f.read())
    with open(os.path.join(inputs, "fib.lua"), "w") as f:
        f.write(fib)
    module = os.path.join(os.path.abspath(args.silverfir), COREMARK_MODULE)
    if not os.path.exists(module):
        raise Invalid(f"{module} does not exist")
    shutil.copyfile(module, os.path.join(inputs, "coremark.wasm"))
    v2h = os.path.join(inputs, "v2h.wf")
    run([sys.executable, "-B", V2H_GENERATOR, v2h])
    for arm, wfc in (("base", args.base_wfc), ("candidate", args.candidate_wfc)):
        wfc = os.path.abspath(wfc)
        # --full-lto forbids --cache, so Halo is built uncached; v2h gets a
        # fresh cache of its own.
        run([wfc, "--graph", graph, "--entry", "bench", "--full-lto", "-o", os.path.join(images, f"halo-{arm}")],
            cwd=halo)
        run([wfc, v2h, "--cache", fresh(os.path.join(args.out, "caches", arm)), "-o",
             os.path.join(images, f"v2h-{arm}")])
    for kind in ("halo", "v2h"):
        shutil.copyfile(os.path.join(images, f"{kind}-base"), os.path.join(images, f"{kind}-twin"))
    for name in os.listdir(images):
        os.chmod(os.path.join(images, name), 0o755)
    digests = {name: sha256(os.path.join(images, name)) for name in sorted(os.listdir(images))}
    with open(os.path.join(args.out, "manifest.txt"), "a") as f:
        f.write(f"halo graph {graph}\n")
        f.write(f"fib {FIB_N}\ncoremark iterations {COREMARK_ITERATIONS}\n")
        for kind in ("halo", "v2h"):
            if digests[f"{kind}-candidate"] == digests[f"{kind}-base"]:
                f.write(f"{kind} images identical\n")
        for name, digest in digests.items():
            f.write(f"image {name} {digest}\n")
    return 0


# ---------------------------------------------------------------- running


def workload_command(out, workload, arm):
    """argv, stdin path (or None) of one run of a workload by an arm's image."""
    images = os.path.join(out, "images")
    inputs = os.path.join(out, "inputs")
    if workload == "fib":
        return [os.path.join(images, f"halo-{arm}")], os.path.join(inputs, "fib.lua")
    return ([os.path.join(images, f"v2h-{arm}"), os.path.join(inputs, "coremark.wasm")]
            + COREMARK_ARGUMENTS + [str(COREMARK_ITERATIONS)], None)


def validate(workload, text):
    """Checks a run's output against the workload's independent definition."""
    if workload == "fib":
        if text.strip() != FIB_VALUE:
            raise Invalid(f"fib({FIB_N}) printed {text.strip()!r}, expected {FIB_VALUE}")
        return
    fields = re.findall(r"\[0\](crc\w+)\s*:\s*(0x[0-9a-fA-F]+)", text)
    got = dict(fields)
    if len(fields) != 4 or set(got) != set(CRC) | {"crcfinal"}:
        raise Invalid("CoreMark: missing, repeated or malformed CRC fields")
    wrong = {k: got[k] for k, v in CRC.items() if got[k].lower() != v}
    if wrong:
        raise Invalid(f"CoreMark: wrong component CRCs {wrong}")
    if got["crcfinal"].lower() != CRC_FINAL:
        raise Invalid(f"CoreMark: final CRC {got['crcfinal']}, expected {CRC_FINAL} at {COREMARK_ITERATIONS} iterations")


def launch(out, workload, arm, cpu, wrapper=()):
    """One pinned run; returns (wall nanoseconds, output). A failed process or
    an output that does not validate is invalid data."""
    argv, stdin_path = workload_command(out, workload, arm)
    command = (["taskset", "-c", str(cpu), "timeout", "--foreground", "--signal=TERM", "--kill-after=10s",
                f"{RUN_BOUND}s"] + list(wrapper) + argv)
    stdin = open(stdin_path, "rb") if stdin_path else None
    try:
        start = time.perf_counter_ns()
        done = subprocess.run(command, stdin=stdin, capture_output=True, text=True,
                              env=dict(os.environ, LC_ALL="C"))
        elapsed = time.perf_counter_ns() - start
    finally:
        if stdin:
            stdin.close()
    if done.returncode != 0:
        raise Invalid(f"{workload} {arm}: exit {done.returncode}: {done.stderr.strip()[-500:]}")
    validate(workload, done.stdout)
    return elapsed, done.stdout


def append(path, columns, rows):
    fresh_file = not os.path.exists(path)
    with open(path, "a") as f:
        if fresh_file:
            f.write("\t".join(columns) + "\n")
        for row in rows:
            f.write("\t".join(str(v) for v in row) + "\n")


def time_rounds(args):
    raw = os.path.join(args.out, "time.tsv")
    if os.path.exists(raw):
        raise Invalid(f"{raw} exists; time writes a fresh directory")
    with open(os.path.join(args.out, "manifest.txt"), "a") as f:
        f.write(f"time cpu {args.cpu}\ntime rounds {args.min_rounds}..{args.max_rounds}\n")
    # One untimed run of each image first: the first launch of a binary reads
    # it from the disk.
    for workload in WORKLOADS:
        for arm in ARMS:
            launch(args.out, workload, arm, args.cpu)
    collected = []
    for round_ in range(args.max_rounds):
        for index, workload in enumerate(WORKLOADS):
            arms = compare.ORDERS[(round_ + index) % len(compare.ORDERS)]
            for position, arm in enumerate(arms):
                ns, _ = launch(args.out, workload, arm, args.cpu)
                row = [round_, position, workload, arm, ns]
                append(raw, RAW_COLUMNS, [row])
                collected.append(dict(zip(RAW_COLUMNS, row)))
                print(f"round {round_} {workload} {arm}: {ns / 1e9:.3f} s", flush=True)
        done = round_ + 1
        if done >= args.min_rounds:
            ratios = timed_cells(collected)
            unresolved = [w for w in WORKLOADS if compare.judge(*ratios[w][:2], RESOLUTION_KEY)["verdict"] == "unresolved"]
            print(f"round {done}: {len(unresolved)} workloads unresolved", flush=True)
            if not unresolved:
                break
    return 0


# ---------------------------------------------------------------- reducing time


def read_raw(path):
    with open(path) as f:
        header = f.readline().rstrip("\n").split("\t")
        if header != RAW_COLUMNS:
            raise Invalid(f"{path}: columns {header} are not {RAW_COLUMNS}")
        rows = []
        for number, line in enumerate(f, 2):
            fields = line.rstrip("\n").split("\t")
            if len(fields) != len(RAW_COLUMNS):
                raise Invalid(f"{path}:{number}: {len(fields)} fields")
            row = dict(zip(RAW_COLUMNS, fields))
            for name in ("round", "position", "ns"):
                try:
                    row[name] = int(row[name])
                except ValueError:
                    raise Invalid(f"{path}:{number}: {name} {row[name]!r} is not an integer")
            rows.append(row)
    return rows


def timed_cells(rows):
    """Per workload: (candidate ratios, twin ratios, base seconds) per round,
    from the rows' wall times. Every round of a workload holds each arm once,
    and all workloads have the same rounds."""
    times = {}
    for row in rows:
        where = f"round {row['round']} {row['workload']} {row['arm']}"
        if row["workload"] not in WORKLOADS:
            raise Invalid(f"unknown workload {row['workload']!r}")
        if row["arm"] not in ARMS:
            raise Invalid(f"unknown arm {row['arm']!r}")
        if row["ns"] <= 0:
            raise Invalid(f"{where}: no time")
        key = (row["workload"], row["round"], row["arm"])
        if key in times:
            raise Invalid(f"{where}: repeated")
        times[key] = row["ns"]
    if not times:
        raise Invalid("no timed rows")
    cells = {}
    for workload in WORKLOADS:
        rounds = sorted({r for (w, r, _) in times if w == workload})
        if not rounds:
            raise Invalid(f"{workload}: no rows")
        candidate, twin, base = [], [], []
        for round_ in rounds:
            missing = [a for a in ARMS if (workload, round_, a) not in times]
            if missing:
                raise Invalid(f"{workload} round {round_}: no row for {', '.join(missing)}")
            b = times[(workload, round_, "base")]
            candidate.append(times[(workload, round_, "candidate")] / b)
            twin.append(times[(workload, round_, "twin")] / b)
            base.append(b / 1e9)
        cells[workload] = (candidate, twin, base)
    if len({len(c[0]) for c in cells.values()}) != 1:
        raise Invalid("the workloads have different numbers of rounds")
    return cells


def parse_floor(text):
    """FLOOR as {workload: spread}; Invalid when malformed or naming an
    unknown workload."""
    floor = {}
    for item in text.split(","):
        item = item.strip()
        if not item:
            continue
        try:
            name, value = item.split("=")
            spread = float(value)
            if name not in WORKLOADS:
                raise KeyError(name)
        except (ValueError, KeyError):
            raise Invalid(f"floor item {item!r} is not workload=spread")
        if not spread >= 0 or name in floor:
            raise Invalid(f"floor item {item!r} is negative or repeated")
        floor[name] = spread
    if not floor:
        raise Invalid("the floor names no workload")
    return floor


def reduce_time(rows, out, floor=None):
    """Prints and returns every workload's verdict, by the hash-map
    comparison's rule. With floor, a workload whose twin spread is above its
    floor is not counted, whatever its verdict would be; without it, the run
    only measures the floor."""
    ratios = timed_cells(rows)
    if floor is not None:
        missing = [w for w in WORKLOADS if w not in floor]
        if missing:
            raise Invalid("the floor gives no spread for " + ", ".join(missing))
    verdicts, bounds = {}, {}
    print("Timed workloads: ratio = median over rounds of candidate/base wall time;", file=out)
    print("twin = the same for the twin; intervals are distribution-free 95% intervals of the median;", file=out)
    print("noise = the twin interval's half-width; a workload is judged once the twin interval holds 1", file=out)
    print("and noise is below the resolution. spread = median over rounds of |twin/base - 1|, with", file=out)
    print("the upper end of its interval; a workload whose spread is above its floor is not counted.", file=out)
    print(f"{'workload':<10} {'rounds':>6} {'base s':>8} {'ratio':>7} {'interval':>18} {'twin':>7} "
          f"{'twin interval':>18} {'noise':>7} {'res':>5} {'spread':>7} {'upper':>7} {'floor':>7}  verdict", file=out)
    for workload in WORKLOADS:
        candidate, twin, base = ratios[workload]
        reading = compare.judge(candidate, twin, RESOLUTION_KEY)
        spread, interval = compare.twin_spread(twin)
        upper = None if interval is None else interval[1]
        bounds[workload] = upper
        limit = None if floor is None else floor[workload]
        verdict = reading["verdict"]
        if limit is not None and spread > limit:
            verdict = "not counted"
        verdicts[workload] = verdict
        noise = "-" if reading["noise"] is None else f"{reading['noise']:.4f}"
        shown_upper = "-" if upper is None else f"{upper:.4f}"
        shown_limit = "-" if limit is None else f"{limit:.4f}"
        print(f"{workload:<10} {reading['rounds']:>6} {statistics.median(base):>8.3f} {reading['ratio']:>7.4f} "
              f"{compare.fmt_interval(reading['ci']):>18} {reading['twin']:>7.4f} "
              f"{compare.fmt_interval(reading['wi']):>18} {noise:>7} {reading['resolution']:>5.2f} {spread:>7.4f} "
              f"{shown_upper:>7} {shown_limit:>7}  {verdict}", file=out)
    if floor is None:
        print(file=out)
        print("No floor given: no workload was compared with a floor, so these timed verdicts decide", file=out)
        print("nothing; the run measures the floor. Each workload's spread upper end, as a later run's floor:", file=out)
        if any(b is None for b in bounds.values()):
            print("  none: a workload has fewer than six rounds", file=out)
        else:
            print("floor " + ",".join(f"{w}={math.ceil(b * 10000) / 10000:.4f}" for w, b in bounds.items()), file=out)
    return verdicts


# ---------------------------------------------------------------- perf stat


def cpu_set(text):
    cpus = set()
    for part in text.strip().split(","):
        ends = part.split("-")
        if len(ends) == 1:
            cpus.add(int(ends[0]))
        elif len(ends) == 2 and int(ends[0]) <= int(ends[1]):
            cpus.update(range(int(ends[0]), int(ends[1]) + 1))
        else:
            raise Invalid("invalid CPU list: " + text)
    return cpus


def pmu_cpus_from_sysfs(pmu):
    path = f"/sys/bus/event_source/devices/{pmu}/cpus"
    if not os.path.exists(path):
        return None
    with open(path) as f:
        return f.read().strip()


def read_perf(text, events, cpu, pmu_cpus=pmu_cpus_from_sysfs):
    """The counts of one `perf stat -x,` run: ({event: count}, [unsupported
    events]). On a hybrid CPU a generic event is one row per PMU; a PMU whose
    CPUs exclude the pinned CPU is ignored, and the rest are summed. A missing
    or uncounted active counter is invalid data; an unsupported one is
    reported so that the caller can drop the event. Instructions and cycles
    must be counted."""
    counters = {event: [] for event in events}
    for fields in csv.reader(text.splitlines()):
        if not fields or fields[0].lstrip().startswith("#"):
            continue
        if len(fields) < 3:
            raise Invalid(f"malformed perf row {fields}")
        value, unit, name = (field.strip() for field in fields[:3])
        if not value and not unit and not name:
            continue
        if unit == "seconds" and name in ("time elapsed", "user", "sys"):
            continue
        hybrid = re.fullmatch(r"(cpu_core|cpu_atom)/([^/]+)/u", name)
        event = hybrid[2] + ":u" if hybrid else name
        if event not in counters:
            raise Invalid(f"unexpected perf event {name}")
        if hybrid:
            active = pmu_cpus(hybrid[1])
            if active is None:
                raise Invalid(f"cannot establish the CPUs of {hybrid[1]}")
            if cpu not in cpu_set(active):
                continue
        if value == "<not supported>":
            counters[event].append(None)
        elif value == "<not counted>":
            raise Invalid(f"active {name} was not counted")
        else:
            if len(fields) < 5:
                raise Invalid(f"{name}: no runtime or scaling fields")
            count, runtime, percent = float(value), float(fields[3]), float(fields[4].strip().rstrip("%"))
            if not all(math.isfinite(v) for v in (count, runtime, percent)) or count < 0 or runtime <= 0 \
                    or not 0 < percent <= 100:
                raise Invalid(f"{name}: invalid count, runtime or scaling {fields}")
            counters[event].append(count)
    values, unsupported = {}, []
    for event, counts in counters.items():
        if not counts:
            raise Invalid(f"no active counter for {event}")
        if all(count is None for count in counts):
            unsupported.append(event)
        elif any(count is None for count in counts):
            raise Invalid(f"{event} is unsupported on only some of the active PMUs")
        else:
            values[event] = sum(counts)
    for event in ("instructions:u", "cycles:u"):
        if event in unsupported or values.get(event, 0) <= 0:
            raise Invalid(f"{event} is unavailable")
    return values, unsupported


PERF_RUNS = itertools.count()


def perf_once(out, workload, arm, cpu, events):
    csv_path = os.path.join(out, "perf", f"{next(PERF_RUNS):03d}-{workload}-{arm}.csv")
    # timeout stays outside perf: its helper process is no part of the count.
    elapsed, _ = launch(out, workload, arm, cpu,
                        wrapper=["perf", "stat", "-x,", "--no-big-num", "-e", ",".join(events), "-o", csv_path, "--"])
    with open(csv_path) as f:
        return read_perf(f.read(), events, cpu)


def perf_rounds(args):
    perf_dir = os.path.join(args.out, "perf")
    os.makedirs(perf_dir, exist_ok=True)
    result = os.path.join(args.out, "perf.json")
    if os.path.exists(result):
        raise Invalid(f"{result} exists; perf writes a fresh directory")
    with open(os.path.join(args.out, "manifest.txt"), "a") as f:
        f.write(f"perf cpu {args.cpu}\nperf rounds {args.rounds}\n")
    events, dropped = list(PERF_EVENTS), []
    # A real validated run of the base finds which events the host lacks
    # before any measured round; a tiny probe could return <not counted>.
    while True:
        _, unsupported = perf_once(args.out, "fib", "base", args.cpu, events)
        if not unsupported:
            break
        dropped += unsupported
        events = [e for e in events if e not in unsupported]
    runs = []
    for round_ in range(args.rounds):
        for index, workload in enumerate(WORKLOADS):
            arms = compare.ORDERS[(round_ + index) % len(compare.ORDERS)]
            for arm in arms:
                values, unsupported = perf_once(args.out, workload, arm, args.cpu, events)
                if unsupported:
                    raise Invalid("event support changed after discovery: " + ", ".join(unsupported))
                runs.append({"round": round_, "workload": workload, "arm": arm, "counts": values})
                print(f"perf round {round_} {workload} {arm}: {values['instructions:u']:.0f} instructions, "
                      f"{values['cycles:u']:.0f} cycles", flush=True)
    with open(result, "w") as f:
        json.dump({"events": events, "dropped": dropped, "runs": runs}, f, indent=1)
    return 0


def reduce_perf(data, out):
    """Prints the counter tables of one perf.json: per workload and arm the
    median counts and rates, then the candidate and twin ratios to the base
    with their range over the rounds. Five rounds give no 95% interval, so
    nothing is judged: the twin's ratios show the run's own noise."""
    events, runs = data["events"], data["runs"]
    if not events or "instructions:u" not in events or "cycles:u" not in events:
        raise Invalid("perf: no instructions or cycles")
    by = {}
    for run_ in runs:
        if run_["workload"] not in WORKLOADS or run_["arm"] not in ARMS:
            raise Invalid(f"perf: unknown workload or arm in {run_}")
        key = (run_["workload"], run_["arm"], run_["round"])
        if key in by:
            raise Invalid(f"perf: repeated {key}")
        if set(run_["counts"]) != set(events):
            raise Invalid(f"perf: {key} counts {sorted(run_['counts'])} are not {sorted(events)}")
        by[key] = run_["counts"]
    rounds = sorted({r for (_, _, r) in by})
    if not rounds:
        raise Invalid("perf: no runs")
    for workload in WORKLOADS:
        for arm in ARMS:
            for r in rounds:
                if (workload, arm, r) not in by:
                    raise Invalid(f"perf: no run of {workload} {arm} in round {r}")
    print(f"perf stat, user mode, {len(rounds)} rounds; counts are medians over rounds, rates are medians of", file=out)
    print("the per-run rates. Not judged: fewer than six rounds give no 95% interval; the twin's ratios", file=out)
    print("are the run's own noise. Unsupported events: " + (", ".join(data["dropped"]) or "none"), file=out)
    header = f"{'workload':<9} {'arm':<10} {'instructions':>16} {'cycles':>16}"
    for label, numerator, denominator, _ in PERF_RATES:
        if numerator in events and denominator in events:
            header += f" {label:>13}"
    print(header, file=out)
    for workload in WORKLOADS:
        for arm in ARMS:
            counts = [by[(workload, arm, r)] for r in rounds]
            line = (f"{workload:<9} {arm:<10} {statistics.median(c['instructions:u'] for c in counts):>16.0f} "
                    f"{statistics.median(c['cycles:u'] for c in counts):>16.0f}")
            for _, numerator, denominator, percent in PERF_RATES:
                if numerator in events and denominator in events:
                    rate = statistics.median(c[numerator] / c[denominator] for c in counts)
                    line += f" {100 * rate:>12.4f}%" if percent else f" {rate:>13.5f}"
            print(line, file=out)
    print(file=out)
    print("Ratios to the base, median over rounds with [min, max]:", file=out)
    print(f"{'workload':<9} {'arm':<10} {'event':<26} {'ratio':>9} {'range':>22}", file=out)
    for workload in WORKLOADS:
        for arm in ("candidate", "twin"):
            for event in events:
                ratios = [by[(workload, arm, r)][event] / by[(workload, "base", r)][event] for r in rounds]
                print(f"{workload:<9} {arm:<10} {event:<26} {statistics.median(ratios):>9.5f} "
                      f"[{min(ratios):.5f}, {max(ratios):.5f}]", file=out)
    return 0


def reduce_all(directory, out, floor=None):
    manifest = os.path.join(directory, "manifest.txt")
    if os.path.exists(manifest):
        with open(manifest) as f:
            print(f.read().rstrip(), file=out)
        print(file=out)
    reduce_time(read_raw(os.path.join(directory, "time.tsv")), out, floor)
    perf = os.path.join(directory, "perf.json")
    if os.path.exists(perf):
        print(file=out)
        with open(perf) as f:
            reduce_perf(json.load(f), out)
    return 0


# ---------------------------------------------------------------- self-test


def synthetic(rounds, candidate_factor, twin_factor=1.0, noise=0.0, seed=1):
    """Raw rows for both workloads: base 10 s, candidate and twin their factor
    times that, each run scaled by a uniform draw within plus or minus noise."""
    rng = random.Random(seed)
    rows = []
    for round_ in range(rounds):
        for workload in WORKLOADS:
            for position, arm in enumerate(ARMS):
                scale = {"base": 1.0, "candidate": candidate_factor, "twin": twin_factor}[arm]
                scale *= 1 + rng.uniform(-noise, noise)
                rows.append({"round": round_, "position": position, "workload": workload, "arm": arm,
                             "ns": round(10e9 * scale)})
    return rows


def perf_text(counts, pmu="cpu_core"):
    """What `perf stat -x,` writes for the given {event: count} on a hybrid
    CPU: one row per PMU, the atom PMU unsupported."""
    lines = ["# started on Fri Oct 23 12:00:00 2026", ""]
    for event, count in counts.items():
        name = event.split(":")[0]
        lines.append(f"{count},,{pmu}/{name}/u,5000000000,100.00,,")
        lines.append(f"<not supported>,,cpu_atom/{name}/u,0,100.00,,")
    return "\n".join(lines) + "\n"


def self_test():
    failures = []

    def expect(label, got, wanted):
        if got != wanted:
            failures.append(f"{label}: got {got!r}, wanted {wanted!r}")

    def expect_invalid(label, thunk):
        try:
            thunk()
        except Invalid:
            return
        failures.append(f"{label}: accepted")

    sink = open(os.devnull, "w")
    same = {"fib": "no change", "coremark": "no change"}
    expect("equal arms", reduce_time(synthetic(10, 1.0), sink), same)
    expect("slower", reduce_time(synthetic(10, 1.05, noise=0.005), sink),
           {"fib": "regression", "coremark": "regression"})
    expect("faster", reduce_time(synthetic(10, 0.95, noise=0.005), sink),
           {"fib": "improvement", "coremark": "improvement"})
    expect("too few rounds", set(reduce_time(synthetic(5, 1.2), sink).values()), {"unresolved"})
    expect("noisy twin", set(reduce_time(synthetic(10, 1.2, noise=0.2), sink).values()), {"unresolved"})
    expect("offset twin", set(reduce_time(synthetic(10, 1.2, twin_factor=1.05), sink).values()), {"unresolved"})
    # One workload slower and the other not: the verdicts are per workload.
    mixed = [r for r in synthetic(10, 1.05, noise=0.005) if r["workload"] == "fib"] \
        + [r for r in synthetic(10, 1.0, noise=0.005) if r["workload"] == "coremark"]
    expect("per workload", reduce_time(mixed, sink), {"fib": "regression", "coremark": "no change"})

    noisy = synthetic(12, 1.05, noise=0.005)
    expect("below the floor", set(reduce_time(noisy, sink, {w: 0.01 for w in WORKLOADS}).values()), {"regression"})
    expect("above the floor", set(reduce_time(noisy, sink, {w: 0.0001 for w in WORKLOADS}).values()),
           {"not counted"})
    expect_invalid("floor missing a workload", lambda: reduce_time(noisy, sink, {"fib": 0.01}))
    expect("floor parsed", parse_floor("fib=0.012, coremark=0.003"), {"fib": 0.012, "coremark": 0.003})
    for text in ("fib", "fibonacci=0.01", "fib=x", "fib=-1", "fib=0.01,fib=0.02", " , "):
        expect_invalid(f"floor {text!r}", lambda text=text: parse_floor(text))

    good = synthetic(6, 1.0)
    expect_invalid("missing arm", lambda: timed_cells([r for r in good if not (r["round"] == 2 and r["arm"] == "twin"
                                                                               and r["workload"] == "fib")]))
    expect_invalid("duplicate", lambda: timed_cells(good + good[:1]))
    expect_invalid("no time", lambda: timed_cells([dict(r, ns=0) if r["round"] == 0 else r for r in good]))
    expect_invalid("unknown arm", lambda: timed_cells([dict(r, arm="old") for r in good]))
    expect_invalid("a workload with other rounds", lambda: timed_cells(
        [r for r in good if not (r["workload"] == "coremark" and r["round"] == 5)]))

    # The outputs: fib must print its value, CoreMark its CRCs.
    validate("fib", f"{FIB_VALUE}\n")
    expect_invalid("fib value", lambda: validate("fib", "2178308\n"))
    coremark = "".join(f"[0]{k} : {v}\n" for k, v in CRC.items()) + f"[0]crcfinal : {CRC_FINAL}\n"
    validate("coremark", coremark)
    expect_invalid("coremark wrong component", lambda: validate("coremark", coremark.replace("0xe714", "0xe715")))
    expect_invalid("coremark wrong final", lambda: validate("coremark", coremark.replace(CRC_FINAL, "0x4984")))
    expect_invalid("coremark missing field", lambda: validate("coremark", coremark.split("\n", 1)[1]))
    expect("fib source", fib_source("local N = 30\nprint(fib(N))\n"), f"local N = {FIB_N}\nprint(fib(N))\n")
    expect_invalid("fib source without a size", lambda: fib_source("print(1)\n"))

    # perf stat output: the CPU 2 sits in cpu_core, whose rows are summed and
    # whose unsupported atom rows are ignored; a counter that is missing or
    # not counted is invalid, and an unsupported one is reported.
    pmus = {"cpu_core": "0-15", "cpu_atom": "16-31"}.get
    counts = {event: 1000.0 * (i + 1) for i, event in enumerate(PERF_EVENTS)}
    expect("perf parsed", read_perf(perf_text(counts), PERF_EVENTS, 2, pmus), (counts, []))
    expect_invalid("perf event missing", lambda: read_perf(
        perf_text({k: v for k, v in counts.items() if k != "branches:u"}), PERF_EVENTS, 2, pmus))
    expect_invalid("perf not counted", lambda: read_perf(
        perf_text(counts).replace("1000.0,,cpu_core/instructions/u", "<not counted>,,cpu_core/instructions/u"),
        PERF_EVENTS, 2, pmus))
    expect_invalid("perf unknown pmu", lambda: read_perf(perf_text(counts), PERF_EVENTS, 2, lambda pmu: None))
    expect_invalid("perf unexpected event", lambda: read_perf(
        perf_text(dict(counts, **{"page-faults:u": 1.0})), PERF_EVENTS, 2, pmus))
    stalled = perf_text(counts).replace(f"{counts['stalled-cycles-backend:u']},,cpu_core/stalled-cycles-backend/u",
                                        "<not supported>,,cpu_core/stalled-cycles-backend/u")
    values, unsupported = read_perf(stalled, PERF_EVENTS, 2, pmus)
    expect("perf unsupported", unsupported, ["stalled-cycles-backend:u"])
    expect("perf unsupported dropped from values", "stalled-cycles-backend:u" in values, False)
    nocycles = perf_text(counts).replace(f"{counts['cycles:u']},,cpu_core/cycles/u",
                                         "<not supported>,,cpu_core/cycles/u")
    expect_invalid("perf no cycles", lambda: read_perf(nocycles, PERF_EVENTS, 2, pmus))
    # With both PMUs active, the atom PMU's unsupported rows beside the core
    # PMU's counts are a partly unsupported event, not a zero.
    both = {"cpu_core": "0-31", "cpu_atom": "0-31"}.get
    expect_invalid("perf partly unsupported", lambda: read_perf(perf_text(counts), PERF_EVENTS, 2, both))
    # Pinned to an atom CPU the core rows are the ones ignored.
    expect_invalid("perf pinned to atom", lambda: read_perf(perf_text(counts), PERF_EVENTS, 20, pmus))

    def perf_data(candidate_scale, twin_scale):
        runs = []
        for round_ in range(5):
            for workload in WORKLOADS:
                for arm, scale in (("base", 1.0), ("candidate", candidate_scale), ("twin", twin_scale)):
                    runs.append({"round": round_, "workload": workload, "arm": arm,
                                 "counts": {e: c * scale for e, c in counts.items()}})
        return {"events": PERF_EVENTS, "dropped": [], "runs": runs}

    text_out = open(os.devnull, "w")
    expect("perf reduced", reduce_perf(perf_data(0.9, 1.0), text_out), 0)
    expect_invalid("perf missing run", lambda: reduce_perf(
        dict(perf_data(0.9, 1.0), runs=perf_data(0.9, 1.0)["runs"][:-1]), text_out))
    expect_invalid("perf repeated run", lambda: reduce_perf(
        dict(perf_data(0.9, 1.0), runs=perf_data(0.9, 1.0)["runs"] * 2), text_out))
    expect_invalid("perf counts differ from the events", lambda: reduce_perf(
        dict(perf_data(0.9, 1.0), events=PERF_EVENTS[:3]), text_out))
    buffer = io.StringIO()
    reduce_perf(perf_data(0.9, 1.0), buffer)
    expect("perf ratio printed", "0.90000" in buffer.getvalue(), True)
    text_out.close()
    sink.close()
    for failure in failures:
        print("FAIL " + failure)
    print(f"self-test: {'FAILED' if failures else 'passed'}")
    return 1 if failures else 0


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("build")
    p.add_argument("--base-wfc", required=True)
    p.add_argument("--candidate-wfc", required=True)
    p.add_argument("--halo", required=True)
    p.add_argument("--silverfir", required=True)
    p.add_argument("--out", required=True)
    p = sub.add_parser("time")
    p.add_argument("--out", required=True)
    p.add_argument("--cpu", required=True, type=int)
    p.add_argument("--min-rounds", type=int, default=10)
    p.add_argument("--max-rounds", type=int, default=30)
    p = sub.add_parser("perf")
    p.add_argument("--out", required=True)
    p.add_argument("--cpu", required=True, type=int)
    p.add_argument("--rounds", type=int, default=5)
    p = sub.add_parser("reduce")
    p.add_argument("--out", required=True)
    p.add_argument("--floor", default="")
    sub.add_parser("self-test")
    args = parser.parse_args(argv)
    try:
        if args.command == "build":
            return build(args)
        if args.command == "time":
            if args.min_rounds < 6 or args.max_rounds < args.min_rounds:
                parser.error("rounds need 6 <= min-rounds <= max-rounds")
            return time_rounds(args)
        if args.command == "perf":
            if args.rounds < 1:
                parser.error("perf needs at least one round")
            return perf_rounds(args)
        if args.command == "reduce":
            floor = parse_floor(args.floor) if args.floor.strip() else None
            with open(os.path.join(args.out, "summary.txt"), "w") as out:
                status = reduce_all(args.out, out, floor)
            with open(os.path.join(args.out, "summary.txt")) as f:
                print(f.read(), end="")
            return status
        return self_test()
    except (Invalid, subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError) as error:
        print(f"interp.py {args.command}: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
