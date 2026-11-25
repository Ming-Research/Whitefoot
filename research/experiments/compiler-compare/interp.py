#!/usr/bin/env python3
"""Before/after comparison of two compilers on two interpreters (README.md
beside this file): the Halo Lua interpreter running fib, and the wasm
interpreter v2h running CoreMark.

    interp.py build --base-wfc WFC --candidate-wfc WFC --silverfir DIR --out DIR
                    [--workloads fib,coremark] [--halo DIR] [--iterations 2000]
                    [--base-generator GEN] [--candidate-generator GEN]
    interp.py time --out DIR --cpu N [--min-rounds 10] [--max-rounds 30]
                   [--workloads W,...] [--trial]
    interp.py perf --out DIR --cpu N [--rounds 5] [--workloads W,...]
    interp.py reduce --out DIR [--floor FLOOR] [--workloads W,...]
                     [--residual-limit SECONDS]
    interp.py self-test

`build` writes DIR/images: for each interpreter the base and candidate images
and the twin, a byte copy of the base image. The two arms may be built by the
same compiler from two generators (two sources of the wasm interpreter), or by
two compilers from one generator. `time` runs interleaved rounds and writes
DIR/time.tsv: each run's wall time and the user and sys CPU time the kernel
accounted to its processes. `perf` runs `perf stat` rounds and writes
DIR/perf.json, the counts of each run with its wall time, counting window, CPU
time and page faults. `reduce` writes DIR/summary.txt: the timed verdicts by the
hash-map comparison's rule (its functions are used as they are), the wall
time's residual over user+sys CPU time, and the counter tables. FLOOR is the
recorded noise floor of each workload, as "workload=spread" items separated by
commas, and a workload whose twin spread is above its floor is not counted.
A residual limit makes a workload whose median residual is above it invalid
data. `--trial` lets `time` run 1 to 5 rounds, which judge nothing. Exit status:
0 when the data are complete and consistent, whatever the verdicts; 1 for
missing, malformed or inconsistent data or floor, or a residual above the
limit; 2 for usage.
"""
import argparse
import collections
import contextlib
import csv
import hashlib
import io
import itertools
import json
import math
import os
import random
import re
import resource
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import threading
import time
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "hash-map-compare"))
import compare  # noqa: E402  the hash-map comparison's statistics and rule

Invalid = compare.Invalid

ARMS = compare.ARMS
# The workloads of a run, selected by --workloads: the functions below loop
# over WORKLOADS, which select_workloads changes in place.
ALL_WORKLOADS = ["fib", "coremark"]
WORKLOADS = list(ALL_WORKLOADS)
# Halo's fib kernel at N = FIB_N prints fib(FIB_N); CoreMark at COREMARK_ITERATIONS
# iterations of the 2K performance run, unless `build` was given another count
# (inputs/coremark-iterations).
FIB_N = 32
FIB_VALUE = "2178309"
COREMARK_ITERATIONS = 2000
COREMARK_ARGUMENTS = ["0x0", "0x0", "0x66"]
# The component CRCs of the 2K performance run, which hold at every iteration
# count, and the final CRC the match-dispatch investigation recorded for its
# v2h interpreter at 2000 iterations (research/experiments/match-dispatch/
# RESULTS.md) and at 20000. At another count no independent final CRC is
# known: every launch must agree with the first one of the run (FINAL_SEEN).
CRC = {"crclist": "0xe714", "crcmatrix": "0x1fd7", "crcstate": "0x8e3a"}
CRC_FINAL = {2000: "0x4983", 20000: "0x382f"}
FINAL_SEEN = {}
# Halo's benchmark graph moved from research/ to tests/ between the revisions
# the experiments pinned; the first that exists is used.
HALO_GRAPHS = ["tests/halo-bench/modules.wfg", "research/experiments/halo-bench/modules.wfg"]
HALO_FIB = "research/experiments/halo-bench/kernels/fib.lua"
COREMARK_MODULE = "benchmarks/wasi/coremark/coremark.wasm"
V2H_GENERATOR = os.path.join(HERE, "..", "match-dispatch", "wasm", "gen.py")
RUN_BOUND = 600
RAW_COLUMNS = ["round", "position", "workload", "arm", "ns", "user_ns", "sys_ns"]
# What a perf run records beside its counts (perf.json "usage"): the launch's
# wall time, perf's counting window, the user and sys CPU time of every process
# of the launch, and its page faults.
USAGE_FIELDS = ["wall_ns", "window_ns", "user_ns", "sys_ns", "minor_faults", "major_faults"]
PERF_EVENTS = ["instructions:u", "cycles:u", "branches:u", "branch-misses:u",
               "L1-dcache-loads:u", "L1-dcache-load-misses:u", "stalled-cycles-backend:u"]
# The rates the counter table reports: label, numerator, denominator, percent.
PERF_RATES = [("IPC", "instructions:u", "cycles:u", False),
              ("branch miss", "branch-misses:u", "branches:u", True),
              ("L1 miss", "L1-dcache-load-misses:u", "L1-dcache-loads:u", True),
              ("backend stall", "stalled-cycles-backend:u", "cycles:u", True)]
# The table of a perf run's wall time, CPU time and page faults: label, the
# value of one run's usage (seconds, or a count), and the digits printed.
USAGE_COLUMNS = [("wall", lambda u: u["wall_ns"] / 1e9, 4),
                 ("window", lambda u: u["window_ns"] / 1e9, 4),
                 ("outside", lambda u: (u["wall_ns"] - u["window_ns"]) / 1e9, 4),
                 ("user", lambda u: u["user_ns"] / 1e9, 4),
                 ("sys", lambda u: u["sys_ns"] / 1e9, 4),
                 ("faults", lambda u: u["minor_faults"] + u["major_faults"], 0)]
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


def select_workloads(text):
    """Sets WORKLOADS to the comma-separated names in TEXT, in the order of
    ALL_WORKLOADS."""
    names = [name.strip() for name in text.split(",") if name.strip()]
    unknown = [name for name in names if name not in ALL_WORKLOADS]
    if not names or unknown or len(set(names)) != len(names):
        raise Invalid(f"workloads {text!r}: expected a list without repeats of {', '.join(ALL_WORKLOADS)}")
    WORKLOADS[:] = [name for name in ALL_WORKLOADS if name in names]


def coremark_iterations(out):
    """The iteration count `build` wrote for DIR, or the default for a
    directory laid out by hand (the self-test's)."""
    path = os.path.join(os.path.abspath(out), "inputs", "coremark-iterations")
    if not os.path.exists(path):
        return COREMARK_ITERATIONS
    with open(path) as f:
        return int(f.read())


def build(args):
    images = os.path.join(args.out, "images")
    inputs = os.path.join(args.out, "inputs")
    os.makedirs(images, exist_ok=True)
    os.makedirs(inputs, exist_ok=True)
    kinds = []
    if "fib" in WORKLOADS:
        if not args.halo:
            raise Invalid("the fib workload needs --halo")
        halo = os.path.abspath(args.halo)
        graph = first_existing(halo, HALO_GRAPHS)
        with open(os.path.join(halo, HALO_FIB)) as f:
            fib = fib_source(f.read())
        with open(os.path.join(inputs, "fib.lua"), "w") as f:
            f.write(fib)
        kinds.append("halo")
    if "coremark" in WORKLOADS:
        module = os.path.join(os.path.abspath(args.silverfir), COREMARK_MODULE)
        if not os.path.exists(module):
            raise Invalid(f"{module} does not exist")
        shutil.copyfile(module, os.path.join(inputs, "coremark.wasm"))
        with open(os.path.join(inputs, "coremark-iterations"), "w") as f:
            f.write(str(args.iterations))
        kinds.append("v2h")
    for arm, wfc, generator in (("base", args.base_wfc, args.base_generator),
                                ("candidate", args.candidate_wfc, args.candidate_generator)):
        wfc = os.path.abspath(wfc)
        if "halo" in kinds:
            # --full-lto forbids --cache, so Halo is built uncached.
            run([wfc, "--graph", graph, "--entry", "bench", "--full-lto", "-o",
                 os.path.join(images, f"halo-{arm}")], cwd=halo)
        if "v2h" in kinds:
            # Each arm's interpreter source stays in inputs: the sampler maps
            # a handler's symbol to its instruction by that arm's source.
            v2h = os.path.join(inputs, f"v2h-{arm}.wf")
            run([sys.executable, "-B", generator, v2h])
            run([wfc, v2h, "--cache", fresh(os.path.join(args.out, "caches", arm)), "-o",
                 os.path.join(images, f"v2h-{arm}")])
    for kind in kinds:
        shutil.copyfile(os.path.join(images, f"{kind}-base"), os.path.join(images, f"{kind}-twin"))
    for name in os.listdir(images):
        os.chmod(os.path.join(images, name), 0o755)
    digests = {name: sha256(os.path.join(images, name)) for name in sorted(os.listdir(images))}
    with open(os.path.join(args.out, "manifest.txt"), "a") as f:
        if "halo" in kinds:
            f.write(f"halo graph {graph}\nfib {FIB_N}\n")
        if "v2h" in kinds:
            f.write(f"coremark iterations {args.iterations}\n")
        for kind in kinds:
            if digests[f"{kind}-candidate"] == digests[f"{kind}-base"]:
                f.write(f"{kind} images identical\n")
        for name, digest in digests.items():
            f.write(f"image {name} {digest}\n")
    return 0


# ---------------------------------------------------------------- running


def workload_command(out, workload, arm):
    """argv, stdin path (or None) of one run of a workload by an arm's image.

    The interpreter opens its module relative to its working directory and
    rejects an absolute path, so a run starts in the inputs directory."""
    out = os.path.abspath(out)
    images = os.path.join(out, "images")
    inputs = os.path.join(out, "inputs")
    if workload == "fib":
        return [os.path.join(images, f"halo-{arm}")], os.path.join(inputs, "fib.lua")
    return ([os.path.join(images, f"v2h-{arm}"), "coremark.wasm"]
            + COREMARK_ARGUMENTS + [str(coremark_iterations(out))], None)


def validate(workload, text, iterations=COREMARK_ITERATIONS, seen=None):
    """Checks a run's output against the workload's independent definition.
    At an iteration count without a known final CRC, SEEN (a dict) holds the
    final CRC of the first launch, which every later launch must equal."""
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
    final = got["crcfinal"].lower()
    if iterations in CRC_FINAL:
        if final != CRC_FINAL[iterations]:
            raise Invalid(f"CoreMark: final CRC {got['crcfinal']}, expected {CRC_FINAL[iterations]} "
                          f"at {iterations} iterations")
    elif seen is not None:
        first = seen.setdefault(iterations, final)
        if final != first:
            raise Invalid(f"CoreMark: final CRC {got['crcfinal']} differs from the run's first, {first}, "
                          f"at {iterations} iterations (no independent value is known at that count)")


# One run of a workload: its wall time, its output and the resource usage of
# every process of the launch (see usage_between).
Launch = collections.namedtuple("Launch", "wall_ns stdout usage")


def usage_between(before, after):
    """CPU time (nanoseconds) and page faults of the child processes waited for
    between two getrusage(RUSAGE_CHILDREN) readings: the kernel's accounting
    for every process of the launch, the wrappers included. Its user and sys
    times are sampled by the scheduler tick, so only their sum is exact."""
    return {"user_ns": round((after.ru_utime - before.ru_utime) * 1e9),
            "sys_ns": round((after.ru_stime - before.ru_stime) * 1e9),
            "minor_faults": after.ru_minflt - before.ru_minflt,
            "major_faults": after.ru_majflt - before.ru_majflt}


def spawn(command, cwd, stdin_path=None, env=None):
    """Runs COMMAND to its end and returns (wall_ns, returncode, stdout,
    stderr, usage, expired). The wall time is from just before the spawn to the
    return of the blocking wait; the usage is that of every child process
    waited for in between (see usage_between). Nothing in the timed span
    polls: the output is read until its end and the wait is a plain waitpid. The
    run's bound is a timer thread that kills the process group, outside the
    span's path; a helper that waited for the child in steps would put the
    exit's time on the grid of its own poll interval."""
    stdin = open(stdin_path, "rb") if stdin_path else None
    expired = threading.Event()
    try:
        before = resource.getrusage(resource.RUSAGE_CHILDREN)
        start = time.perf_counter_ns()
        child = subprocess.Popen(command, stdin=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
                                 cwd=cwd, env=env, start_new_session=True)

        def stop():
            expired.set()
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

        timer = threading.Timer(RUN_BOUND, stop)
        timer.start()
        try:
            stdout, stderr = child.communicate()
        finally:
            timer.cancel()
        elapsed = time.perf_counter_ns() - start
        after = resource.getrusage(resource.RUSAGE_CHILDREN)
    finally:
        if stdin:
            stdin.close()
    return elapsed, child.returncode, stdout, stderr, usage_between(before, after), expired.is_set()


def launch(out, workload, arm, cpu, wrapper=()):
    """One pinned run. A failed process or an output that does not validate is
    invalid data. Between the timer and the workload there is taskset, which
    replaces itself with the next program, and a wrapper the caller names
    (perf stat); the run goes through no `timeout`."""
    argv, stdin_path = workload_command(out, workload, arm)
    elapsed, returncode, stdout, stderr, usage, expired = spawn(
        ["taskset", "-c", str(cpu)] + list(wrapper) + argv, os.path.join(out, "inputs"), stdin_path,
        dict(os.environ, LC_ALL="C"))
    if expired:
        raise Invalid(f"{workload} {arm}: exceeded {RUN_BOUND} s")
    if returncode != 0:
        raise Invalid(f"{workload} {arm}: exit {returncode}: {stderr.strip()[-500:]}")
    validate(workload, stdout, coremark_iterations(out), FINAL_SEEN.setdefault(os.path.abspath(out), {}))
    return Launch(elapsed, stdout, usage)


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
                launched = launch(args.out, workload, arm, args.cpu)
                ns = launched.wall_ns
                row = [round_, position, workload, arm, ns, launched.usage["user_ns"], launched.usage["sys_ns"]]
                append(raw, RAW_COLUMNS, [row])
                collected.append(dict(zip(RAW_COLUMNS, row)))
                print(f"round {round_} {workload} {arm}: {ns / 1e9:.4f} s, cpu "
                      f"{(launched.usage['user_ns'] + launched.usage['sys_ns']) / 1e9:.4f} s", flush=True)
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
            for name in ("round", "position", "ns", "user_ns", "sys_ns"):
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


def residual_table(residuals, out, limit, what):
    """Prints, per workload, the median and maximum over its runs of the wall
    time minus the user and sys CPU time the kernel accounted to the same
    launch, and returns the workloads whose median is above LIMIT (none when
    LIMIT is None). RESIDUALS is {workload: [seconds]}. The workloads are
    pinned and CPU bound, so the residual is what the launch spent outside the
    process's own CPU time: process start-up, and anything a waiter adds to
    the wall time. A wall time that moves in steps of a poll interval puts
    that interval's remainder here."""
    print(f"Wall time minus user+sys CPU time of the same {what} (seconds); a median above the limit makes the", file=out)
    print("wall times invalid: something outside the workload's own time is in them.", file=out)
    print(f"{'workload':<10} {'runs':>5} {'median':>9} {'max':>9} {'limit':>9}  residual", file=out)
    flagged = []
    for workload in WORKLOADS:
        values = residuals[workload]
        median = statistics.median(values)
        above = limit is not None and median > limit
        if above:
            flagged.append(workload)
        shown = "-" if limit is None else f"{limit:.4f}"
        print(f"{workload:<10} {len(values):>5} {median:>9.4f} {max(values):>9.4f} {shown:>9}  "
              f"{'ABOVE THE LIMIT' if above else 'ok' if limit is not None else 'not checked'}", file=out)
    return flagged


def time_residuals(rows):
    """{workload: [wall - user - sys in seconds, per run]} of the time rows."""
    residuals = {w: [] for w in WORKLOADS}
    for row in rows:
        if row["workload"] in residuals:
            residuals[row["workload"]].append((row["ns"] - row["user_ns"] - row["sys_ns"]) / 1e9)
    for workload, values in residuals.items():
        if not values:
            raise Invalid(f"{workload}: no timed rows")
    return residuals


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
    events], window). On a hybrid CPU a generic event is one row per PMU; a PMU
    whose CPUs exclude the pinned CPU is ignored, and the rest are summed. A
    missing or uncounted active counter is invalid data; an unsupported one is
    reported so that the caller can drop the event. Instructions and cycles
    must be counted. The window is perf's counting window in nanoseconds: the
    longest time a counted row was enabled, its runtime over its scaling."""
    counters = {event: [] for event in events}
    windows = []
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
            windows.append(runtime * 100 / percent)
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
    return values, unsupported, round(max(windows))


PERF_RUNS = itertools.count()


def perf_once(out, workload, arm, cpu, events):
    """({event: count}, [unsupported events], usage) of one run under perf stat;
    the usage has USAGE_FIELDS."""
    csv_path = os.path.join(os.path.abspath(out), "perf", f"{next(PERF_RUNS):03d}-{workload}-{arm}.csv")
    # perf is the only wrapper between taskset and the workload; the launch
    # has no timeout helper.
    launched = launch(out, workload, arm, cpu,
                      wrapper=["perf", "stat", "-x,", "--no-big-num", "-e", ",".join(events), "-o", csv_path, "--"])
    with open(csv_path) as f:
        values, unsupported, window = read_perf(f.read(), events, cpu)
    return values, unsupported, dict(launched.usage, wall_ns=launched.wall_ns, window_ns=window)


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
        _, unsupported, _ = perf_once(args.out, WORKLOADS[0], "base", args.cpu, events)
        if not unsupported:
            break
        dropped += unsupported
        events = [e for e in events if e not in unsupported]
    runs = []
    for round_ in range(args.rounds):
        for index, workload in enumerate(WORKLOADS):
            arms = compare.ORDERS[(round_ + index) % len(compare.ORDERS)]
            for arm in arms:
                values, unsupported, usage = perf_once(args.out, workload, arm, args.cpu, events)
                if unsupported:
                    raise Invalid("event support changed after discovery: " + ", ".join(unsupported))
                runs.append({"round": round_, "workload": workload, "arm": arm, "counts": values, "usage": usage})
                print(f"perf round {round_} {workload} {arm}: {values['instructions:u']:.0f} instructions, "
                      f"{values['cycles:u']:.0f} cycles, wall {usage['wall_ns'] / 1e9:.3f} s, "
                      f"window {usage['window_ns'] / 1e9:.3f} s, user {usage['user_ns'] / 1e9:.3f} s, "
                      f"sys {usage['sys_ns'] / 1e9:.3f} s, faults {usage['minor_faults'] + usage['major_faults']}",
                      flush=True)
    with open(result, "w") as f:
        json.dump({"events": events, "dropped": dropped, "runs": runs}, f, indent=1)
    return 0


def ratio_range(pairs):
    """The median of a / b over the (a, b) pairs with its [min, max], as the
    tables print them; no ratio when some b is not above 0."""
    if not all(b > 0 for _, b in pairs):
        return f"{'n/a':>9} [a base value is 0]"
    ratios = [a / b for a, b in pairs]
    return f"{statistics.median(ratios):>9.5f} [{min(ratios):.5f}, {max(ratios):.5f}]"


def reduce_perf(data, out, residual_limit=None):
    """Prints the tables of one perf.json: per workload and arm the median
    counts and rates, then the candidate and twin ratios to the base with
    their range over the rounds, then the same runs' wall time, CPU time and
    page faults. Five rounds give no 95% interval, so nothing is judged: the
    twin's ratios show the run's own noise. Returns 1 when a workload's wall
    time minus CPU time is above RESIDUAL_LIMIT, else 0."""
    events, runs = data["events"], data["runs"]
    if not events or "instructions:u" not in events or "cycles:u" not in events:
        raise Invalid("perf: no instructions or cycles")
    by, used = {}, {}
    for run_ in runs:
        if run_["workload"] not in WORKLOADS or run_["arm"] not in ARMS:
            raise Invalid(f"perf: unknown workload or arm in {run_}")
        key = (run_["workload"], run_["arm"], run_["round"])
        if key in by:
            raise Invalid(f"perf: repeated {key}")
        if set(run_["counts"]) != set(events):
            raise Invalid(f"perf: {key} counts {sorted(run_['counts'])} are not {sorted(events)}")
        usage = run_.get("usage")
        if not isinstance(usage, dict) or set(usage) != set(USAGE_FIELDS):
            raise Invalid(f"perf: {key} usage {usage!r} does not have exactly {USAGE_FIELDS}")
        if not all(isinstance(v, (int, float)) and math.isfinite(v) and v >= 0 for v in usage.values()) \
                or usage["wall_ns"] <= 0 or usage["window_ns"] <= 0:
            raise Invalid(f"perf: {key} has invalid usage {usage}")
        by[key] = run_["counts"]
        used[key] = usage
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
                pairs = [(by[(workload, arm, r)][event], by[(workload, "base", r)][event]) for r in rounds]
                print(f"{workload:<9} {arm:<10} {event:<26} {ratio_range(pairs)}", file=out)
    print(file=out)
    print("Wall time, CPU time and page faults of the same runs, medians over rounds (seconds, faults a count).", file=out)
    print("Wall, user, sys and faults are those of the whole launch: taskset, perf stat and the", file=out)
    print("workload, the same wrapper for every arm. Window is perf's counting window; outside is wall - window", file=out)
    print("of each run.", file=out)
    header = f"{'workload':<9} {'arm':<10}" + "".join(f" {label:>10}" for label, _, _ in USAGE_COLUMNS)
    print(header, file=out)
    for workload in WORKLOADS:
        for arm in ARMS:
            line = f"{workload:<9} {arm:<10}"
            for _, value, digits in USAGE_COLUMNS:
                line += f" {statistics.median(value(used[(workload, arm, r)]) for r in rounds):>10.{digits}f}"
            print(line, file=out)
    print(file=out)
    print("Differences to the base (arm - base) and ratios (arm / base), median over rounds with [min, max]:", file=out)
    print(f"{'workload':<9} {'arm':<10} {'quantity':<8} {'difference':>34} {'ratio':>26}", file=out)
    for workload in WORKLOADS:
        for arm in ("candidate", "twin"):
            for label, value, digits in USAGE_COLUMNS:
                pairs = [(value(used[(workload, arm, r)]), value(used[(workload, "base", r)])) for r in rounds]
                diffs = [a - b for a, b in pairs]
                text = (f"{statistics.median(diffs):>12.{digits + 2}f} "
                        f"[{min(diffs):.{digits + 2}f}, {max(diffs):.{digits + 2}f}]")
                print(f"{workload:<9} {arm:<10} {label:<8} {text:>34} {ratio_range(pairs):>26}", file=out)
    print(file=out)
    residuals = {w: [(u["wall_ns"] - u["user_ns"] - u["sys_ns"]) / 1e9
                     for (w2, _, _), u in used.items() if w2 == w] for w in WORKLOADS}
    return 1 if residual_table(residuals, out, residual_limit, "perf runs") else 0


def reduce_all(directory, out, floor=None, residual_limit=None):
    manifest = os.path.join(directory, "manifest.txt")
    if os.path.exists(manifest):
        with open(manifest) as f:
            print(f.read().rstrip(), file=out)
        print(file=out)
    rows = read_raw(os.path.join(directory, "time.tsv"))
    reduce_time(rows, out, floor)
    print(file=out)
    status = 1 if residual_table(time_residuals(rows), out, residual_limit, "timed runs") else 0
    perf = os.path.join(directory, "perf.json")
    if os.path.exists(perf):
        print(file=out)
        with open(perf) as f:
            status |= reduce_perf(json.load(f), out, residual_limit)
    return status


# ---------------------------------------------------------------- wall clock probe

# The launch chains the probe times: the instrument's own (taskset, then the
# program), the same with the helper the instrument used to put around every
# run, and the program alone.
WALLCLOCK_CHAINS = [("bare", []),
                    ("taskset", ["taskset", "-c", "{cpu}"]),
                    ("taskset+timeout", ["taskset", "-c", "{cpu}", "timeout", "--foreground", "--signal=TERM",
                                         "--kill-after=10s", "600s"])]
WALLCLOCK_SECONDS = 0.21


def counting_command(seconds):
    """A command that spends about SECONDS of user CPU time in a counted loop
    (a loop on a clock may make a system call each time round), the count
    calibrated by this process on the same interpreter."""
    start = time.perf_counter()
    i = 0
    while i < 2000000:
        i += 1
    rate = 2000000 / (time.perf_counter() - start)
    return [sys.executable, "-c", f"i = 0\nwhile i < {round(rate * seconds)}: i += 1"]


def wallclock_rows(cpu, runs, stand_ins):
    """[(stand-in, chain, wall_s, cpu_s)] of RUNS launches of each stand-in
    under each chain, in rotating chain order so that a drift of the host
    falls on every chain alike."""
    rows = []
    for number in range(runs):
        for label, command in stand_ins:
            for index in range(len(WALLCLOCK_CHAINS)):
                chain, prefix = WALLCLOCK_CHAINS[(number + index) % len(WALLCLOCK_CHAINS)]
                wrapper = [part.format(cpu=cpu) for part in prefix]
                wall, code, _, error, usage, expired = spawn(wrapper + command, None)
                if expired or code != 0:
                    raise Invalid(f"wallclock {label} under {chain}: exit {code}: {error.strip()[-300:]}")
                rows.append((label, chain, wall / 1e9, (usage["user_ns"] + usage["sys_ns"]) / 1e9))
    return rows


def wallclock_table(rows, out):
    """Prints, per stand-in and chain, how many distinct wall readings there
    are at 1 ms, their minimum, median and maximum, and the median and maximum
    of wall - (user+sys). A wall time on a grid shows as few distinct readings
    all above the stand-in's own time, in the chain that has the waiter."""
    print("Wall times of two stand-ins launched under each chain (seconds); distinct = readings at 1 ms;", file=out)
    print("residual = wall - (user+sys) of the same launch; it measures the launch only for the counting", file=out)
    print("stand-in, whose time is CPU time: the sleeper uses none, so its residual is its own time.", file=out)
    print(f"{'stand-in':<9} {'chain':<16} {'runs':>4} {'distinct':>8} {'min':>8} {'median':>8} {'max':>8} "
          f"{'res med':>8} {'res max':>8}", file=out)
    for label in dict.fromkeys(r[0] for r in rows):
        for chain, _ in WALLCLOCK_CHAINS:
            mine = [(w, c) for (l, ch, w, c) in rows if l == label and ch == chain]
            walls = [w for w, _ in mine]
            residuals = [w - c for w, c in mine]
            print(f"{label:<9} {chain:<16} {len(mine):>4} {len({round(w, 3) for w in walls}):>8} "
                  f"{min(walls):>8.4f} {statistics.median(walls):>8.4f} {max(walls):>8.4f} "
                  f"{statistics.median(residuals):>8.4f} {max(residuals):>8.4f}", file=out)


def wallclock_hosts(out):
    """The host's facts a grid's cause can be read from: which `timeout` and
    `taskset` run, their versions, and the system."""
    for command in (["bash", "-c", "type -a timeout taskset"],
                    ["bash", "-c", "readlink -f \"$(command -v timeout)\""],
                    ["timeout", "--version"], ["taskset", "--version"], ["uname", "-srm"],
                    ["sh", "-c", "grep -E '^(PRETTY_NAME|VERSION)=' /etc/os-release"]):
        print("$ " + " ".join(command), file=out)
        try:
            done = subprocess.run(command, capture_output=True, text=True)
            print((done.stdout + done.stderr).rstrip(), file=out)
        except OSError as error:
            print(f"cannot run: {error}", file=out)


def wallclock(args):
    """The `wallclock` command: names what puts a program's wall time on a
    grid, by launching a stand-in that sleeps 0.21 s and one that counts for
    0.21 s under each chain."""
    os.makedirs(args.out, exist_ok=True)
    stand_ins = [("sleep", ["sh", "-c", f"sleep {WALLCLOCK_SECONDS}"]),
                 ("count", counting_command(WALLCLOCK_SECONDS))]
    with open(os.path.join(args.out, "wallclock.txt"), "w") as out:
        wallclock_hosts(out)
        print(file=out)
        wallclock_table(wallclock_rows(args.cpu, args.runs, stand_ins), out)
    with open(os.path.join(args.out, "wallclock.txt")) as f:
        print(f.read(), end="")
    return 0


# ---------------------------------------------------------------- self-test


def synthetic(rounds, candidate_factor, twin_factor=1.0, noise=0.0, seed=1, gap_ns=10_000_000):
    """Raw rows for both workloads: base 10 s, candidate and twin their factor
    times that, each run scaled by a uniform draw within plus or minus noise;
    each run's user and sys CPU time sum to its wall time less gap_ns."""
    rng = random.Random(seed)
    rows = []
    for round_ in range(rounds):
        for workload in WORKLOADS:
            for position, arm in enumerate(ARMS):
                scale = {"base": 1.0, "candidate": candidate_factor, "twin": twin_factor}[arm]
                scale *= 1 + rng.uniform(-noise, noise)
                ns = round(10e9 * scale)
                rows.append({"round": round_, "position": position, "workload": workload, "arm": arm,
                             "ns": ns, "user_ns": ns - gap_ns - 1_000_000, "sys_ns": 1_000_000})
    return rows


def perf_text(counts, pmu="cpu_core", runtime=5000000000, percent="100.00"):
    """What `perf stat -x,` writes for the given {event: count} on a hybrid
    CPU: one row per PMU, the atom PMU unsupported."""
    lines = ["# started on Fri Oct 23 12:00:00 2026", ""]
    for event, count in counts.items():
        name = event.split(":")[0]
        lines.append(f"{count},,{pmu}/{name}/u,{runtime},{percent},,")
        lines.append(f"<not supported>,,cpu_atom/{name}/u,0,100.00,,")
    return "\n".join(lines) + "\n"


STAND_IN_OUTPUTS = {"halo": f"{FIB_VALUE}\n",
                    "v2h": "".join(f"[0]{k} : {v}\n" for k, v in CRC.items()) + f"[0]crcfinal : {CRC_FINAL[2000]}\n"}


@contextlib.contextmanager
def stand_in_images(commands):
    """A directory laid out like the output of `build`, whose images run the
    shell command given for their arm (the base's, for an arm without one) and
    print their workload's expected output."""
    out = tempfile.mkdtemp(prefix="interp-self-test-")
    try:
        os.makedirs(os.path.join(out, "images"))
        os.makedirs(os.path.join(out, "inputs"))
        for name in ("fib.lua", "coremark.wasm"):
            open(os.path.join(out, "inputs", name), "w").close()
        for kind, text in STAND_IN_OUTPUTS.items():
            for arm in ARMS:
                path = os.path.join(out, "images", f"{kind}-{arm}")
                with open(path, "w") as f:
                    f.write(f"#!/bin/sh\n{commands.get(arm, commands['base'])}\ncat <<'EOF'\n{text}EOF\n")
                os.chmod(path, 0o755)
        yield out
    finally:
        shutil.rmtree(out)


def timing_harness_test(expect):
    """`time` on stand-in images of known speeds, through the real launch path:
    the base and its twin sleep 0.05 s, the candidate 0.2 s, and each prints
    its workload's expected output. The rows must show the candidate about
    four times the base and the twin equal to it, so that an image launched
    for the wrong arm, or a row written under the wrong label, is found."""
    if sys.platform != "linux":
        print("self-test: the timing harness is checked on Linux only (taskset)")
        return
    try:
        with stand_in_images({"base": "sleep 0.05", "candidate": "sleep 0.2"}) as out:
            args = argparse.Namespace(out=out, cpu=max(os.sched_getaffinity(0)), min_rounds=6, max_rounds=6)
            with contextlib.redirect_stdout(io.StringIO()):
                time_rounds(args)
            cells = timed_cells(read_raw(os.path.join(out, "time.tsv")))
    except (Invalid, subprocess.CalledProcessError, OSError) as error:
        expect("timing harness runs", f"failed: {error}", "ran")
        return
    for workload in WORKLOADS:
        candidate, twin, _ = cells[workload]
        expect(f"{workload} candidate is the slow arm", statistics.median(candidate) > 2, True)
        expect(f"{workload} twin equals the base", 0.7 < statistics.median(twin) < 1.4, True)


def usage_harness_test(expect):
    """`launch` on stand-in images of known resource use, through the real
    launch path: the base sleeps (wall time and almost no CPU time), the
    candidate touches 40 MB and then burns user CPU time, and the twin reads
    /dev/zero in large blocks, which is kernel time. The usage must show each image's own resource: CPU time
    far below the wall time for the sleeper, user above sys for the burner,
    sys above user for the caller, and page faults counted."""
    if sys.platform != "linux":
        print("self-test: the usage harness is checked on Linux only (taskset, dd)")
        return
    # A counted loop: a loop on a clock would make a system call each time round.
    burn = f'"{sys.executable}" -c \'b = b"x" * 40000000\ni = 0\nwhile i < 15000000: i += 1\''
    # Large reads of /dev/zero: the kernel clears the buffer, and dd itself does
    # next to nothing, so the time is sys on a fast host too.
    calls = "dd if=/dev/zero of=/dev/null bs=1M count=10000 2>/dev/null"
    try:
        with stand_in_images({"base": "sleep 0.3", "candidate": burn, "twin": calls}) as out:
            cpu = max(os.sched_getaffinity(0))
            sleeper, burner, caller = (launch(out, "fib", arm, cpu) for arm in ("base", "candidate", "twin"))
    except (Invalid, subprocess.CalledProcessError, OSError) as error:
        expect("usage harness runs", f"failed: {error}", "ran")
        return
    cpu_time = lambda run_: run_.usage["user_ns"] + run_.usage["sys_ns"]
    expect("sleeper wall time", sleeper.wall_ns >= 0.3e9, True)
    expect("sleeper uses little CPU", cpu_time(sleeper) < sleeper.wall_ns / 2, True)
    expect("burner user time", burner.usage["user_ns"] >= 0.1e9, True)
    expect("burner user above sys", burner.usage["user_ns"] > burner.usage["sys_ns"], True)
    expect("caller sys time", caller.usage["sys_ns"] >= 0.05e9, True)
    expect("caller sys above user", caller.usage["sys_ns"] > caller.usage["user_ns"], True)
    expect("sleeper page faults counted", sleeper.usage["minor_faults"] > 0, True)
    expect("burner page faults above the sleeper's", burner.usage["minor_faults"] > sleeper.usage["minor_faults"], True)


def perf_pipeline_test(expect):
    """`perf` and `reduce` end to end on stand-in images and a stand-in `perf`
    script that runs the command and writes a CSV whose counting window is the
    command's own run time, on a CPU that is not hybrid: the base sleeps 0.05 s
    and the candidate 0.2 s. perf.json must hold every run with its usage, the
    window must be the CSV's, inside the wall time and about the run time of
    the image, and the time tables must print."""
    if sys.platform != "linux":
        print("self-test: the perf pipeline is checked on Linux only (taskset, date)")
        return
    script = """#!/bin/sh
while [ "$1" != -- ]; do
    case "$1" in -e) events=$2; shift ;; -o) csv=$2; shift ;; esac
    shift
done
shift
start=$(date +%s%N)
"$@" || exit $?
window=$(( $(date +%s%N) - start ))
{
    echo "# started on a stand-in"
    echo
    i=0
    for event in $(echo "$events" | tr , ' '); do
        i=$((i + 1))
        echo "$((i * 1000)),,$event,$window,100.00,,"
    done
} > "$csv"
"""
    try:
        with stand_in_images({"base": "sleep 0.05", "candidate": "sleep 0.2"}) as out:
            bin_dir = os.path.join(out, "bin")
            os.makedirs(bin_dir)
            with open(os.path.join(bin_dir, "perf"), "w") as f:
                f.write(script)
            os.chmod(os.path.join(bin_dir, "perf"), 0o755)
            args = argparse.Namespace(out=out, cpu=max(os.sched_getaffinity(0)), rounds=2)
            with mock.patch.dict(os.environ, {"PATH": bin_dir + os.pathsep + os.environ["PATH"]}), \
                    contextlib.redirect_stdout(io.StringIO()):
                perf_rounds(args)
            with open(os.path.join(out, "perf.json")) as f:
                data = json.load(f)
            report = io.StringIO()
            status = reduce_perf(data, report)
    except (Invalid, subprocess.CalledProcessError, OSError) as error:
        expect("perf pipeline runs", f"failed: {error}", "ran")
        return
    runs = data["runs"]
    expect("perf runs recorded", len(runs), 2 * len(WORKLOADS) * len(ARMS))
    expect("perf usage fields", all(set(r["usage"]) == set(USAGE_FIELDS) for r in runs), True)
    expect("perf window is the CSV's, inside the wall time",
           all(0 < r["usage"]["window_ns"] < r["usage"]["wall_ns"] for r in runs), True)
    expect("perf window covers the image's run", all(r["usage"]["window_ns"] >= 0.05e9 for r in runs), True)
    window = {arm: statistics.median(r["usage"]["window_ns"] for r in runs if r["arm"] == arm)
              for arm in ("base", "candidate")}
    expect("perf window follows the arm", window["candidate"] > 2 * window["base"], True)
    expect("perf reduced", status, 0)
    tail = report.getvalue().split("Wall time, CPU time and page faults")[1].splitlines()
    differences = tail[tail.index(next(line for line in tail if line.startswith("Differences"))) + 2:]
    differences = differences[:differences.index("")]
    expect("perf difference rows", len(differences), 2 * 2 * len(USAGE_COLUMNS))


def rounds_error(min_rounds, max_rounds, trial):
    """Why a `time` round range is refused, or None. A judged run needs six
    rounds for a 95% interval; a trial runs one to five and judges nothing."""
    if max_rounds < min_rounds or min_rounds < 1:
        return "rounds need 1 <= min-rounds <= max-rounds"
    if trial:
        return None if max_rounds <= 5 else "a trial runs at most 5 rounds"
    return None if min_rounds >= 6 else "rounds need 6 <= min-rounds <= max-rounds (--trial allows fewer)"


def launch_path_test(expect, expect_invalid):
    """The launch path on stand-in images, with a `timeout` first on PATH that
    fails when run: a launch that goes through `timeout` fails the test. An
    image that sleeps 0.21 s must show a wall time that is its own: a waiter
    that polled every 0.1 s would read it as 0.3 s. A run past its bound is
    killed, and `time` writes the CPU time columns."""
    global RUN_BOUND
    if sys.platform != "linux":
        print("self-test: the launch path is checked on Linux only (taskset)")
        return
    try:
        with stand_in_images({"base": "sleep 0.21"}) as out:
            bin_dir = os.path.join(out, "bin")
            os.makedirs(bin_dir)
            shim = os.path.join(bin_dir, "timeout")
            with open(shim, "w") as f:
                f.write("#!/bin/sh\necho 'timeout was run by the launch' >&2\nexit 97\n")
            os.chmod(shim, 0o755)
            cpu = max(os.sched_getaffinity(0))
            with mock.patch.dict(os.environ, {"PATH": bin_dir + os.pathsep + os.environ["PATH"]}):
                walls = [launch(out, "fib", "base", cpu).wall_ns / 1e9 for _ in range(3)]
                args = argparse.Namespace(out=out, cpu=cpu, min_rounds=2, max_rounds=2)
                with contextlib.redirect_stdout(io.StringIO()):
                    time_rounds(args)
                rows = read_raw(os.path.join(out, "time.tsv"))
                sleeper = os.path.join(out, "images", "v2h-base")
                with open(sleeper, "w") as f:
                    f.write("#!/bin/sh\nsleep 30\n")
                bound, RUN_BOUND = RUN_BOUND, 0.3
                try:
                    started = time.monotonic()
                    expect_invalid("a run past its bound", lambda: launch(out, "coremark", "base", cpu), "exceeded")
                    expect("a run past its bound is killed", time.monotonic() - started < 10, True)
                finally:
                    RUN_BOUND = bound
    except (Invalid, subprocess.CalledProcessError, OSError) as error:
        expect("launch path runs", f"failed: {error}", "ran")
        return
    expect("wall time is the image's own, with no waiter's step", all(0.21 <= w < 0.28 for w in walls), True)
    expect("time rows written for both workloads", len(rows), 2 * len(WORKLOADS) * len(ARMS))
    expect("time rows hold CPU time", all(r["user_ns"] + r["sys_ns"] < r["ns"] for r in rows), True)


def wallclock_test(expect):
    """`wallclock` on this host, two runs of each stand-in: every chain must
    appear for both stand-ins, and the instrument's own chain must read the
    sleeper at its own time and the counter's wall within its CPU time."""
    if sys.platform != "linux":
        print("self-test: the wall clock probe is checked on Linux only (taskset, timeout)")
        return
    try:
        stand_ins = [("sleep", ["sh", "-c", f"sleep {WALLCLOCK_SECONDS}"]), ("count", counting_command(WALLCLOCK_SECONDS))]
        rows = wallclock_rows(max(os.sched_getaffinity(0)), 2, stand_ins)
        table = io.StringIO()
        wallclock_table(rows, table)
    except (Invalid, subprocess.CalledProcessError, OSError) as error:
        expect("wallclock probe runs", f"failed: {error}", "ran")
        return
    expect("wallclock rows", len(rows), 2 * len(stand_ins) * len(WALLCLOCK_CHAINS))
    expect("wallclock table rows", len([l for l in table.getvalue().splitlines()
                                        if l.split()[:1] in (["sleep"], ["count"])]),
           len(stand_ins) * len(WALLCLOCK_CHAINS))
    sleeper = [w for (l, c, w, _) in rows if l == "sleep" and c == "taskset"]
    expect("wallclock sleeper under taskset", all(WALLCLOCK_SECONDS <= w < 0.28 for w in sleeper), True)
    counter = [(w, c) for (l, ch, w, c) in rows if l == "count" and ch == "taskset"]
    expect("wallclock counter's CPU time is most of its wall time", all(c > 0.5 * w for w, c in counter), True)


def self_test():
    failures = []

    def expect(label, got, wanted):
        if got != wanted:
            failures.append(f"{label}: got {got!r}, wanted {wanted!r}")

    def expect_invalid(label, thunk, reason):
        try:
            thunk()
        except Invalid as error:
            if reason not in str(error):
                failures.append(f"{label}: refused for {str(error)!r}, wanted a reason with {reason!r}")
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
    # The residual: runs whose CPU time falls 0.01 s short of the wall time
    # pass a 0.05 s limit; a wall time 0.1 s above its CPU time (a waiter's
    # poll interval) is flagged by it, workload by workload.
    expect("residual within the limit",
           residual_table(time_residuals(synthetic(6, 1.0)), sink, 0.05, "timed runs"), [])
    expect("residual above the limit",
           residual_table(time_residuals(synthetic(6, 1.0, gap_ns=100_000_000)), sink, 0.05, "timed runs"),
           ["fib", "coremark"])
    expect("residual without a limit flags nothing",
           residual_table(time_residuals(synthetic(6, 1.0, gap_ns=100_000_000)), sink, None, "timed runs"), [])
    one_workload = [r if r["workload"] != "fib" else dict(r, user_ns=r["user_ns"] - 90_000_000)
                    for r in synthetic(6, 1.0)]
    expect("residual per workload", residual_table(time_residuals(one_workload), sink, 0.05, "timed runs"), ["fib"])
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
    expect_invalid("floor missing a workload", lambda: reduce_time(noisy, sink, {"fib": 0.01}),
                   "gives no spread for coremark")
    expect("floor parsed", parse_floor("fib=0.012, coremark=0.003"), {"fib": 0.012, "coremark": 0.003})
    for text, reason in (("fib", "is not workload=spread"), ("fibonacci=0.01", "is not workload=spread"),
                         ("fib=x", "is not workload=spread"), ("fib=-1", "negative or repeated"),
                         ("fib=0.01,fib=0.02", "negative or repeated"), (" , ", "names no workload")):
        expect_invalid(f"floor {text!r}", lambda text=text: parse_floor(text), reason)

    good = synthetic(6, 1.0)
    expect_invalid("missing arm", lambda: timed_cells([r for r in good if not (r["round"] == 2 and r["arm"] == "twin"
                                                                               and r["workload"] == "fib")]),
                   "fib round 2: no row for twin")
    expect_invalid("duplicate", lambda: timed_cells(good + good[:1]), "repeated")
    expect_invalid("no time", lambda: timed_cells([dict(r, ns=0) if r["round"] == 0 else r for r in good]),
                   "no time")
    expect_invalid("unknown arm", lambda: timed_cells([dict(r, arm="old") for r in good]), "unknown arm")
    expect_invalid("a workload with other rounds", lambda: timed_cells(
        [r for r in good if not (r["workload"] == "coremark" and r["round"] == 5)]),
                   "different numbers of rounds")

    # The outputs: fib must print its value, CoreMark its CRCs.
    validate("fib", f"{FIB_VALUE}\n")
    expect_invalid("fib value", lambda: validate("fib", "2178308\n"), "expected 2178309")
    coremark = "".join(f"[0]{k} : {v}\n" for k, v in CRC.items()) + f"[0]crcfinal : {CRC_FINAL[2000]}\n"
    validate("coremark", coremark)
    expect_invalid("coremark wrong component", lambda: validate("coremark", coremark.replace("0xe714", "0xe715")),
                   "wrong component CRCs")
    expect_invalid("coremark wrong final", lambda: validate("coremark", coremark.replace(CRC_FINAL[2000], "0x4984")),
                   "final CRC")
    # At 20000 iterations the final CRC is another known value; at a count with
    # none, a launch must equal the run's first, and a differing one is refused.
    validate("coremark", coremark.replace(CRC_FINAL[2000], CRC_FINAL[20000]), 20000)
    expect_invalid("coremark 2000's final CRC at 20000",
                   lambda: validate("coremark", coremark, 20000), "expected 0x382f at 20000")
    seen = {}
    validate("coremark", coremark, 20, seen)
    validate("coremark", coremark, 20, seen)
    expect_invalid("coremark final CRC differing at an unlisted count",
                   lambda: validate("coremark", coremark.replace(CRC_FINAL[2000], "0x4984"), 20, seen),
                   "differs from the run's first")
    expect_invalid("coremark wrong component CRC at an unlisted count",
                   lambda: validate("coremark", coremark.replace("0xe714", "0xe715"), 20, {}), "wrong component CRCs")
    expect_invalid("coremark missing field", lambda: validate("coremark", coremark.split("\n", 1)[1]),
                   "missing, repeated or malformed")
    expect("fib source", fib_source("local N = 30\nprint(fib(N))\n"), f"local N = {FIB_N}\nprint(fib(N))\n")
    expect_invalid("fib source without a size", lambda: fib_source("print(1)\n"), "expected one")

    # perf stat output: the CPU 2 sits in cpu_core, whose rows are summed and
    # whose unsupported atom rows are ignored; a counter that is missing or
    # not counted is invalid, and an unsupported one is reported.
    pmus = {"cpu_core": "0-15", "cpu_atom": "16-31"}.get
    counts = {event: 1000.0 * (i + 1) for i, event in enumerate(PERF_EVENTS)}
    expect("perf parsed", read_perf(perf_text(counts), PERF_EVENTS, 2, pmus), (counts, [], 5000000000))
    # The window is the enabled time: the runtime over the scaling, and the
    # longest of the counted rows.
    expect("perf window scaled", read_perf(perf_text(counts, runtime=2500000000, percent="50.00"),
                                           PERF_EVENTS, 2, pmus)[2], 5000000000)
    longest = perf_text(counts).replace(f"{counts['stalled-cycles-backend:u']},,cpu_core/stalled-cycles-backend/u,5000000000",
                                        f"{counts['stalled-cycles-backend:u']},,cpu_core/stalled-cycles-backend/u,6000000000")
    expect("perf window longest", read_perf(longest, PERF_EVENTS, 2, pmus)[2], 6000000000)
    expect_invalid("perf event missing", lambda: read_perf(
        perf_text({k: v for k, v in counts.items() if k != "branches:u"}), PERF_EVENTS, 2, pmus),
                   "no active counter for branches:u")
    expect_invalid("perf not counted", lambda: read_perf(
        perf_text(counts).replace("1000.0,,cpu_core/instructions/u", "<not counted>,,cpu_core/instructions/u"),
        PERF_EVENTS, 2, pmus), "was not counted")
    expect_invalid("perf unknown pmu", lambda: read_perf(perf_text(counts), PERF_EVENTS, 2, lambda pmu: None),
                   "cannot establish the CPUs")
    expect_invalid("perf unexpected event", lambda: read_perf(
        perf_text(dict(counts, **{"page-faults:u": 1.0})), PERF_EVENTS, 2, pmus), "unexpected perf event")
    stalled = perf_text(counts).replace(f"{counts['stalled-cycles-backend:u']},,cpu_core/stalled-cycles-backend/u",
                                        "<not supported>,,cpu_core/stalled-cycles-backend/u")
    values, unsupported, _ = read_perf(stalled, PERF_EVENTS, 2, pmus)
    expect("perf unsupported", unsupported, ["stalled-cycles-backend:u"])
    expect("perf unsupported dropped from values", "stalled-cycles-backend:u" in values, False)
    nocycles = perf_text(counts).replace(f"{counts['cycles:u']},,cpu_core/cycles/u",
                                         "<not supported>,,cpu_core/cycles/u")
    expect_invalid("perf no cycles", lambda: read_perf(nocycles, PERF_EVENTS, 2, pmus), "cycles:u is unavailable")
    # With both PMUs active, the atom PMU's unsupported rows beside the core
    # PMU's counts are a partly unsupported event, not a zero.
    both = {"cpu_core": "0-31", "cpu_atom": "0-31"}.get
    expect_invalid("perf partly unsupported", lambda: read_perf(perf_text(counts), PERF_EVENTS, 2, both),
                   "unsupported on only some")
    # Pinned to an atom CPU the core rows are the ones ignored, and the
    # atom rows count nothing.
    expect_invalid("perf pinned to atom", lambda: read_perf(perf_text(counts), PERF_EVENTS, 20, pmus),
                   "instructions:u is unavailable")

    def perf_usage(scale):
        # Base: wall 0.3 s, window 0.2 s, user 0.19 s, sys 0.01 s, 1024 faults;
        # the times scale with the arm, the minor faults too.
        return {"wall_ns": round(300e6 * scale), "window_ns": round(200e6 * scale), "user_ns": round(190e6 * scale),
                "sys_ns": round(10e6 * scale), "minor_faults": round(1000 * scale), "major_faults": 24}

    def perf_data(candidate_scale, twin_scale):
        runs = []
        for round_ in range(5):
            for workload in WORKLOADS:
                for arm, scale in (("base", 1.0), ("candidate", candidate_scale), ("twin", twin_scale)):
                    runs.append({"round": round_, "workload": workload, "arm": arm,
                                 "counts": {e: c * scale for e, c in counts.items()}, "usage": perf_usage(scale)})
        return {"events": PERF_EVENTS, "dropped": [], "runs": runs}

    text_out = open(os.devnull, "w")
    expect("perf reduced", reduce_perf(perf_data(0.9, 1.0), text_out), 0)
    expect_invalid("perf missing run", lambda: reduce_perf(
        dict(perf_data(0.9, 1.0), runs=perf_data(0.9, 1.0)["runs"][:-1]), text_out),
                   "no run of coremark twin in round 4")
    expect_invalid("perf repeated run", lambda: reduce_perf(
        dict(perf_data(0.9, 1.0), runs=perf_data(0.9, 1.0)["runs"] * 2), text_out), "repeated")
    expect_invalid("perf counts differ from the events", lambda: reduce_perf(
        dict(perf_data(0.9, 1.0), events=PERF_EVENTS[:3]), text_out), "counts")
    buffer = io.StringIO()
    reduce_perf(perf_data(0.9, 1.0), buffer)
    expect("perf ratio printed", "0.90000" in buffer.getvalue(), True)

    # The time table: each column holds its own quantity, and the differences
    # and ratios are those of the candidate to the base.
    def table_row(heading, *words):
        lines = buffer.getvalue().split(heading)[1].splitlines()
        return next(line.split()[len(words):] for line in lines if line.split()[:len(words)] == list(words))

    expect("perf base row", table_row("Wall time, CPU time", "fib", "base"),
           ["0.3000", "0.2000", "0.1000", "0.1900", "0.0100", "1024"])
    expect("perf candidate row", table_row("Wall time, CPU time", "fib", "candidate"),
           ["0.2700", "0.1800", "0.0900", "0.1710", "0.0090", "924"])
    expect("perf candidate user difference", table_row("Differences to the base", "fib", "candidate", "user"),
           ["-0.019000", "[-0.019000,", "-0.019000]", "0.90000", "[0.90000,", "0.90000]"])
    expect("perf candidate faults difference", table_row("Differences to the base", "fib", "candidate", "faults"),
           ["-100.00", "[-100.00,", "-100.00]", "0.90234", "[0.90234,", "0.90234]"])
    zero = perf_data(0.9, 1.0)
    for run_ in zero["runs"]:
        run_["counts"]["branch-misses:u"] = 0.0
    zero_report = io.StringIO()
    expect("perf zero base count", reduce_perf(zero, zero_report), 0)
    expect("perf zero base count is not a ratio", "n/a [a base value is 0]" in zero_report.getvalue(), True)
    expect_invalid("perf run without usage", lambda: reduce_perf(
        dict(perf_data(0.9, 1.0), runs=[{k: v for k, v in r.items() if k != "usage"} for r in perf_data(0.9, 1.0)["runs"]]),
        text_out), "usage")
    expect_invalid("perf usage with an unknown field", lambda: reduce_perf(
        dict(perf_data(0.9, 1.0), runs=[dict(r, usage=dict(r["usage"], extra=1)) for r in perf_data(0.9, 1.0)["runs"]]),
        text_out), "does not have exactly")
    expect_invalid("perf usage without a window", lambda: reduce_perf(
        dict(perf_data(0.9, 1.0), runs=[dict(r, usage=dict(r["usage"], window_ns=0)) for r in perf_data(0.9, 1.0)["runs"]]),
        text_out), "invalid usage")
    # The perf table's residual: wall 0.3 s against 0.2 s of CPU time is 0.1 s.
    expect("perf residual flagged", reduce_perf(perf_data(0.9, 1.0), io.StringIO(), 0.05), 1)
    expect("perf residual within the limit", reduce_perf(perf_data(0.9, 1.0), io.StringIO(), 0.2), 0)
    expect("perf residual without a limit", reduce_perf(perf_data(0.9, 1.0), io.StringIO()), 0)
    expect("a judged run needs six rounds", rounds_error(5, 5, False) is not None, True)
    expect("a judged run of six rounds", rounds_error(6, 30, False), None)
    expect("a trial of two rounds", rounds_error(2, 2, True), None)
    expect("a trial of six rounds", rounds_error(6, 6, True) is not None, True)
    expect("rounds out of order", rounds_error(8, 6, False) is not None, True)
    timing_harness_test(expect)
    usage_harness_test(expect)
    launch_path_test(expect, expect_invalid)
    wallclock_test(expect)
    perf_pipeline_test(expect)
    text_out.close()
    sink.close()
    for failure in failures:
        print("FAIL " + failure)
    print(f"self-test: {'FAILED' if failures else 'passed'}")
    return 1 if failures else 0


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    workloads = ",".join(ALL_WORKLOADS)
    p = sub.add_parser("build")
    p.add_argument("--base-wfc", required=True)
    p.add_argument("--candidate-wfc", required=True)
    p.add_argument("--halo", help="a Halo-wf checkout; needed by the fib workload")
    p.add_argument("--silverfir", required=True)
    p.add_argument("--base-generator", default=V2H_GENERATOR, help="the wasm interpreter's generator for the base arm")
    p.add_argument("--candidate-generator", default=V2H_GENERATOR,
                   help="the wasm interpreter's generator for the candidate arm")
    p.add_argument("--iterations", type=int, default=COREMARK_ITERATIONS, help="CoreMark iterations")
    p.add_argument("--workloads", default=workloads)
    p.add_argument("--out", required=True)
    p = sub.add_parser("time")
    p.add_argument("--out", required=True)
    p.add_argument("--cpu", required=True, type=int)
    p.add_argument("--min-rounds", type=int, default=10)
    p.add_argument("--max-rounds", type=int, default=30)
    p.add_argument("--trial", action="store_true", help="allow 1 to 5 rounds; nothing is judged")
    p.add_argument("--workloads", default=workloads)
    p = sub.add_parser("perf")
    p.add_argument("--out", required=True)
    p.add_argument("--cpu", required=True, type=int)
    p.add_argument("--rounds", type=int, default=5)
    p.add_argument("--workloads", default=workloads)
    p = sub.add_parser("wallclock")
    p.add_argument("--out", required=True)
    p.add_argument("--cpu", required=True, type=int)
    p.add_argument("--runs", type=int, default=20)
    p = sub.add_parser("reduce")
    p.add_argument("--out", required=True)
    p.add_argument("--floor", default="")
    p.add_argument("--residual-limit", type=float, default=None,
                   help="seconds; a workload whose median wall - (user + sys) is above it is invalid data")
    p.add_argument("--workloads", default=workloads)
    sub.add_parser("self-test")
    args = parser.parse_args(argv)
    try:
        if args.command not in ("self-test", "wallclock"):
            select_workloads(args.workloads)
        if args.command == "wallclock":
            if args.runs < 1:
                parser.error("wallclock needs at least one run")
            return wallclock(args)
        if args.command == "build":
            if args.iterations < 1:
                parser.error("iterations must be at least 1")
            return build(args)
        if args.command == "time":
            problem = rounds_error(args.min_rounds, args.max_rounds, args.trial)
            if problem:
                parser.error(problem)
            return time_rounds(args)
        if args.command == "perf":
            if args.rounds < 1:
                parser.error("perf needs at least one round")
            return perf_rounds(args)
        if args.command == "reduce":
            floor = parse_floor(args.floor) if args.floor.strip() else None
            with open(os.path.join(args.out, "summary.txt"), "w") as out:
                status = reduce_all(args.out, out, floor, args.residual_limit)
            with open(os.path.join(args.out, "summary.txt")) as f:
                print(f.read(), end="")
            return status
        return self_test()
    except (Invalid, subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError) as error:
        print(f"interp.py {args.command}: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
