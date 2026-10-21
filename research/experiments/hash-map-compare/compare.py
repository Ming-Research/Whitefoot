#!/usr/bin/env python3
"""Before/after comparison of the ordinary HashMap (README.md beside this file).

    compare.py build --base-wfc WFC --candidate-wfc WFC --runtime-root TREE --out DIR
    compare.py measure --out DIR --cpu N [--sizes 1000,10000,1000000]
                       [--min-rounds 10] [--max-rounds 40] [--samples 5] [--slow]
    compare.py reduce --out DIR [--expect-regression] [--floor FLOOR]
    compare.py self-test

`build` writes DIR/images: the base, twin and candidate images of bench.wf,
the candidate image of hold.wf and the memory cell's observed images. The
twin is a byte copy of the base image. `measure` runs the interleaved rounds
and appends DIR/raw.tsv, DIR/memory.tsv and DIR/hold.tsv. `reduce` writes
DIR/summary.txt and prints it; FLOOR is the recorded noise floor of each
cell, as "operation:size=spread" items separated by commas, and a cell whose
twin spread is above its floor is not counted. Exit status: 0 when the data are complete and
consistent, whatever the verdicts; 1 for missing, malformed or inconsistent
data or floor, or, with --expect-regression, when a timed cell is not judged
a regression; 2 for usage.
"""
import argparse
import hashlib
import itertools
import math
import os
import random
import shutil
import statistics
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

OPERATIONS = {
    1: "insert-presized",
    2: "insert-growing",
    3: "lookup-hit",
    4: "lookup-miss",
    5: "remove",
    6: "remove-insert",
    7: "iterate",
}
MEMORY_OPERATION = 8
ARMS = ["base", "candidate", "twin"]
ORDERS = list(itertools.permutations(ARMS))
TARGET_OPS = 262144
MASK = (1 << 64) - 1
# The smallest difference each cell must resolve, set before measurement with
# the hash-scan investigation (board item coord-wfbl-02-01): 3% at 1,000
# pairs, 2% at 10,000 and 1,000,000. It decides when a cell has been measured
# precisely enough, never whether a change is accepted.
RESOLUTION = {1000: 0.03}
DEFAULT_RESOLUTION = 0.02
# The hold criterion, set before measurement with the same investigation.
HOLD_SMALL, HOLD_LARGE = 1000, 1000000
HOLD_GROWTH_FACTOR = 4.0
HOLD_LIMIT_NS = 20000
CHAINS = [64, 1024, 65536]
HOLD_CALLS = 20000
CHAIN_CALLS = 2000
RAW_COLUMNS = ["round", "position", "arm", "operation", "size", "sample", "ops", "ns", "check", "extra"]
MEMORY_COLUMNS = ["arm", "run", "size", "before", "retained", "length", "peak", "maxrss_kib"]
HOLD_COLUMNS = ["mode", "size", "call", "ns", "pairs"]


class Invalid(Exception):
    pass


def resolution(size):
    return RESOLUTION.get(size, DEFAULT_RESOLUTION)


def reps_for(operation, size):
    reps = -(-TARGET_OPS // size)
    return 8 * reps if operation == 7 else reps


def expected_row(operation, size):
    """The ops and check every row of a cell must carry, from the workload's
    definition alone: values are the indexes 0..size, and every lookup of a
    present key, removal of a present key and full visit sees its pair."""
    reps = reps_for(operation, size)
    ops = reps * size
    triangle = size * (size - 1) // 2
    check = {
        1: ops,
        2: ops,
        3: ops + reps * triangle,
        4: 0,
        5: ops,
        6: ops + size,
        7: ops + reps * triangle,
    }[operation]
    return ops, check & MASK


# ---------------------------------------------------------------- statistics


def median_interval(values):
    """A distribution-free interval of at least 95% coverage for the median
    of values: the order statistics x(k) and x(n-k+1), k the largest index
    with P(Binomial(n, 1/2) <= k - 1) <= 0.025. None below six values, where
    no such interval exists."""
    n = len(values)
    ordered = sorted(values)
    k, tail = 0, 0.0
    while True:
        nxt = tail + math.comb(n, k) / 2 ** n
        if nxt > 0.025:
            break
        tail, k = nxt, k + 1
    if k == 0:
        return None
    return ordered[k - 1], ordered[n - k]


def judge(candidate, twin, size):
    """One timed cell: candidate and twin are the per-round ratios to the
    base. The twin is usable once its interval contains 1 and its half-width
    is below the cell's resolution. Then the candidate's interval wholly
    above 1 is a regression and wholly below an improvement; across 1 it is
    no detected change only when it also lies within 1 plus or minus the
    resolution, since a wider interval has not resolved a change of that
    size. Otherwise the cell is unresolved and takes more rounds."""
    ci = median_interval(candidate)
    wi = median_interval(twin)
    out = {
        "rounds": len(candidate),
        "ratio": statistics.median(candidate),
        "twin": statistics.median(twin),
        "ci": ci,
        "wi": wi,
        "noise": None if wi is None else (wi[1] - wi[0]) / 2,
        "resolution": resolution(size),
    }
    if ci is None or wi is None:
        out["verdict"] = "unresolved"
    elif not (wi[0] <= 1 <= wi[1]) or out["noise"] >= out["resolution"]:
        out["verdict"] = "unresolved"
    elif ci[0] > 1:
        out["verdict"] = "regression"
    elif ci[1] < 1:
        out["verdict"] = "improvement"
    elif 1 - out["resolution"] <= ci[0] and ci[1] <= 1 + out["resolution"]:
        out["verdict"] = "no change"
    else:
        out["verdict"] = "unresolved"
    return out


def twin_spread(twin):
    """The twin's per-round noise: the median over rounds of |twin/base - 1|
    and the distribution-free 95% interval of that median. Unlike the twin
    interval's half-width it does not shrink as rounds are added, so a run
    can be compared with the floor another run on the same machine recorded.
    The floor to record is the interval's upper end, the noisiest the
    machine's usual spread could be; a later run whose spread lies above it
    is noisier than usual."""
    deviations = [abs(r - 1) for r in twin]
    return statistics.median(deviations), median_interval(deviations)


def parse_floor(text):
    """FLOOR as {(operation, size): spread}; Invalid when malformed or naming
    an unknown operation."""
    names = {name: number for number, name in OPERATIONS.items()}
    floor = {}
    for item in text.split(","):
        item = item.strip()
        if not item:
            continue
        try:
            cell, value = item.split("=")
            name, size = cell.split(":")
            key, spread = (names[name], int(size)), float(value)
        except (ValueError, KeyError):
            raise Invalid(f"floor item {item!r} is not operation:size=spread")
        if not spread >= 0 or key in floor:
            raise Invalid(f"floor item {item!r} is negative or repeated")
        floor[key] = spread
    if not floor:
        raise Invalid("the floor names no cell")
    return floor


def quantile(values, q):
    """Nearest-rank quantile."""
    ordered = sorted(values)
    rank = max(1, math.ceil(q * len(ordered)))
    return ordered[rank - 1]


# ---------------------------------------------------------------- reading


def read_tsv(path, columns):
    with open(path) as f:
        header = f.readline().rstrip("\n").split("\t")
        if header != columns:
            raise Invalid(f"{path}: columns {header} are not {columns}")
        rows = []
        for number, line in enumerate(f, 2):
            fields = line.rstrip("\n").split("\t")
            if len(fields) != len(columns):
                raise Invalid(f"{path}:{number}: {len(fields)} fields")
            row = dict(zip(columns, fields))
            for name in columns:
                if name != "arm":
                    try:
                        row[name] = int(row[name])
                    except ValueError:
                        raise Invalid(f"{path}:{number}: {name} {row[name]!r} is not an integer")
            rows.append(row)
    return rows


def timed_cells(rows, slow):
    """Per cell, per round, per arm: the median nanoseconds per operation of
    the process's measured samples. Checks every row against the workload's
    expected ops and check, the extra field against the slow control, and
    that every round of a cell holds all three arms with the same samples."""
    processes = {}
    for row in rows:
        operation, size = row["operation"], row["size"]
        if operation not in OPERATIONS:
            raise Invalid(f"unknown operation {operation}")
        if row["arm"] not in ARMS:
            raise Invalid(f"unknown arm {row['arm']!r}")
        ops, check = expected_row(operation, size)
        where = f"round {row['round']} {row['arm']} {OPERATIONS[operation]} {size} sample {row['sample']}"
        if row["ops"] != ops or row["check"] != check:
            raise Invalid(f"{where}: ops {row['ops']} check {row['check']}, expected {ops} {check}")
        repeated = slow and row["arm"] == "candidate"
        if (row["extra"] != 0) == (not repeated):
            raise Invalid(f"{where}: extra {row['extra']} {'without' if repeated else 'with'} slow repetition")
        if row["ns"] <= 0:
            raise Invalid(f"{where}: no time")
        key = (operation, size, row["round"], row["arm"])
        processes.setdefault(key, {})
        if row["sample"] in processes[key]:
            raise Invalid(f"{where}: repeated")
        processes[key][row["sample"]] = row["ns"] / row["ops"]
    if not processes:
        raise Invalid("no timed rows")
    cells = {}
    for (operation, size, round_, arm), samples in processes.items():
        count = len(samples)
        if sorted(samples) != list(range(count)) or count < 2:
            raise Invalid(f"round {round_} {arm} {OPERATIONS[operation]} {size}: samples {sorted(samples)}")
        measured = [samples[s] for s in range(1, count)]
        cells.setdefault((operation, size), {}).setdefault(round_, {})[arm] = (count, statistics.median(measured))
    ratios = {}
    for cell, rounds in cells.items():
        candidate, twin = [], []
        counts = set()
        for round_, arms in sorted(rounds.items()):
            if sorted(arms) != sorted(ARMS):
                raise Invalid(f"{OPERATIONS[cell[0]]} {cell[1]} round {round_}: arms {sorted(arms)}")
            counts.update(c for c, _ in arms.values())
            base = arms["base"][1]
            candidate.append(arms["candidate"][1] / base)
            twin.append(arms["twin"][1] / base)
        if len(counts) != 1:
            raise Invalid(f"{OPERATIONS[cell[0]]} {cell[1]}: sample counts {sorted(counts)}")
        ratios[cell] = (candidate, twin, [rounds[r]["base"][1] for r in sorted(rounds)])
    return ratios


# ---------------------------------------------------------------- reduction


def fmt_interval(interval):
    return "-" if interval is None else f"[{interval[0]:.4f}, {interval[1]:.4f}]"


def reduce_timed(rows, slow, out, floor=None):
    """Prints and returns every timed cell's verdict. With floor, a cell
    whose twin spread is above its floor is not counted, whatever its
    verdict would be; without it, the run only measures the floor."""
    ratios = timed_cells(rows, slow)
    if floor is not None:
        missing = [c for c in sorted(ratios) if c not in floor]
        if missing:
            raise Invalid("the floor gives no spread for " + ", ".join(f"{OPERATIONS[o]}:{s}" for o, s in missing))
    verdicts = {}
    bounds = {}
    print("Timed cells: ratio = median over rounds of candidate/base nanoseconds per operation;", file=out)
    print("twin = the same for the twin; intervals are distribution-free 95% intervals of the median;", file=out)
    print("noise = the twin interval's half-width; a cell is judged once the twin interval holds 1", file=out)
    print("and noise is below the resolution. spread = median over rounds of |twin/base - 1|, with", file=out)
    print("the upper end of its interval; a cell whose spread is above its floor is not counted.", file=out)
    print(f"{'operation':<16} {'size':>8} {'rounds':>6} {'base ns/op':>10} {'ratio':>7} {'interval':>18} "
          f"{'twin':>7} {'twin interval':>18} {'noise':>7} {'res':>5} {'spread':>7} {'upper':>7} {'floor':>7}"
          "  verdict", file=out)
    for (operation, size) in sorted(ratios):
        candidate, twin, base = ratios[(operation, size)]
        reading = judge(candidate, twin, size)
        spread, interval = twin_spread(twin)
        upper = None if interval is None else interval[1]
        bounds[(operation, size)] = upper
        limit = None if floor is None else floor[(operation, size)]
        verdict = reading["verdict"]
        if limit is not None and spread > limit:
            verdict = "not counted"
        verdicts[(operation, size)] = verdict
        noise = "-" if reading["noise"] is None else f"{reading['noise']:.4f}"
        shown_upper = "-" if upper is None else f"{upper:.4f}"
        shown_limit = "-" if limit is None else f"{limit:.4f}"
        print(f"{OPERATIONS[operation]:<16} {size:>8} {reading['rounds']:>6} {statistics.median(base):>10.2f} "
              f"{reading['ratio']:>7.4f} {fmt_interval(reading['ci']):>18} {reading['twin']:>7.4f} "
              f"{fmt_interval(reading['wi']):>18} {noise:>7} {reading['resolution']:>5.2f} {spread:>7.4f} "
              f"{shown_upper:>7} {shown_limit:>7}  {verdict}", file=out)
    if floor is None:
        print(file=out)
        print("No floor given: no cell was compared with a floor, so these timed verdicts decide", file=out)
        print("nothing; the run measures the floor. Each cell's spread upper end, as a later run's floor:", file=out)
        if any(b is None for b in bounds.values()):
            print("  none: a cell has fewer than six rounds", file=out)
        else:
            print("floor " + ",".join(f"{OPERATIONS[o]}:{s}={math.ceil(b * 10000) / 10000:.4f}"
                                      for (o, s), b in sorted(bounds.items())), file=out)
    return verdicts


def reduce_memory(rows, out):
    """Memory is deterministic: each arm runs twice and both runs must agree.
    Peak counts heap bytes in use above the reading before building."""
    runs = {}
    for row in rows:
        if row["arm"] not in ("base", "candidate"):
            raise Invalid(f"memory arm {row['arm']!r}")
        if row["length"] != row["size"]:
            raise Invalid(f"memory {row['arm']} {row['size']}: length {row['length']}")
        if row["peak"] < row["before"] + row["retained"]:
            raise Invalid(f"memory {row['arm']} {row['size']}: peak {row['peak']} below the built map")
        value = (row["retained"], row["peak"] - row["before"])
        runs.setdefault((row["size"], row["arm"]), []).append((value, row["maxrss_kib"]))
    if not runs:
        raise Invalid("no memory rows")
    print("Memory cells: bytes counted by heap_in_use; peak is the maximum while building", file=out)
    print("from capacity zero, above the reading before building; any difference is a change.", file=out)
    print(f"{'size':>8} {'base retained':>14} {'cand retained':>14} {'ratio':>7} {'base peak':>12} "
          f"{'cand peak':>12} {'ratio':>7} {'base rss KiB':>12} {'cand rss KiB':>12}", file=out)
    verdicts = {}
    for size in sorted({s for s, _ in runs}):
        values = {}
        for arm in ("base", "candidate"):
            got = runs.get((size, arm), [])
            if len(got) != 2 or got[0][0] != got[1][0]:
                raise Invalid(f"memory {arm} {size}: runs {got}")
            values[arm] = (got[0][0], max(r for _, r in got))
        (br, bp), brss = values["base"]
        (cr, cp), crss = values["candidate"]
        print(f"{size:>8} {br:>14} {cr:>14} {cr / br:>7.4f} {bp:>12} {cp:>12} {cp / bp:>7.4f} "
              f"{brss:>12} {crss:>12}", file=out)
        verdicts[size] = ("higher" if cr > br else "lower" if cr < br else "equal",
                          "higher" if cp > bp else "lower" if cp < bp else "equal")
    return verdicts


def reduce_hold(rows, out):
    calls = {}
    for row in rows:
        calls.setdefault((row["mode"], row["size"]), []).append(row)
    needed = [(1, HOLD_SMALL), (1, HOLD_LARGE)] + [(2, m) for m in CHAINS]
    for key in needed:
        if key not in calls:
            raise Invalid(f"hold mode {key[0]} size {key[1]}: no calls")
    for (mode, size), got in calls.items():
        if sorted(r["call"] for r in got) != list(range(len(got))):
            raise Invalid(f"hold mode {mode} size {size}: call indexes")
        if mode == 2 and any(r["pairs"] < size for r in got):
            raise Invalid(f"hold chain {size}: a call lent fewer than {size} pairs")
    print("Scan hold, candidate only: nanoseconds of one hash_map_scan call with limit 64.", file=out)
    print(f"{'mode':<8} {'size':>8} {'calls':>6} {'p50':>9} {'p99':>9} {'max':>9} {'pairs p50':>9}", file=out)
    stats = {}
    for (mode, size) in sorted(calls):
        ns = [r["ns"] for r in calls[(mode, size)]]
        pairs = [r["pairs"] for r in calls[(mode, size)]]
        stats[(mode, size)] = (quantile(ns, 0.5), quantile(ns, 0.99))
        print(f"{'uniform' if mode == 1 else 'chain':<8} {size:>8} {len(ns):>6} {quantile(ns, 0.5):>9} "
              f"{quantile(ns, 0.99):>9} {max(ns):>9} {quantile(pairs, 0.5):>9}", file=out)
    small, large = stats[(1, HOLD_SMALL)][1], stats[(1, HOLD_LARGE)][1]
    readings = {
        "H1": large <= HOLD_GROWTH_FACTOR * small,
        "H2": large <= HOLD_LIMIT_NS,
    }
    print(f"H1 p99 at {HOLD_LARGE} ({large} ns) <= {HOLD_GROWTH_FACTOR:g} x p99 at {HOLD_SMALL} "
          f"({small} ns): {'pass' if readings['H1'] else 'fail'}", file=out)
    print(f"H2 p99 at {HOLD_LARGE} ({large} ns) <= {HOLD_LIMIT_NS} ns: {'pass' if readings['H2'] else 'fail'}",
          file=out)
    per_pair = statistics.median(r["ns"] / r["pairs"] for r in calls[(1, HOLD_LARGE)] if r["pairs"] > 0)
    for m in CHAINS:
        bound = 2 * m * per_pair
        median = stats[(2, m)][0]
        readings[f"H3-{m}"] = median <= bound
        print(f"H3 chain {m}: median {median} ns <= 2 x {m} x {per_pair:.2f} ns per pair "
              f"({bound:.0f} ns): {'pass' if median <= bound else 'fail'}", file=out)
    return readings


def reduce_all(directory, expect_regression, out, floor=None):
    manifest = os.path.join(directory, "manifest.txt")
    slow = False
    if os.path.exists(manifest):
        with open(manifest) as f:
            print(f.read().rstrip(), file=out)
        print(file=out)
        slow = "control slow" in open(manifest).read().splitlines()
    timed = reduce_timed(read_tsv(os.path.join(directory, "raw.tsv"), RAW_COLUMNS), slow, out, floor)
    print(file=out)
    reduce_memory(read_tsv(os.path.join(directory, "memory.tsv"), MEMORY_COLUMNS), out)
    print(file=out)
    reduce_hold(read_tsv(os.path.join(directory, "hold.tsv"), HOLD_COLUMNS), out)
    print(file=out)
    counts = {v: sum(1 for x in timed.values() if x == v) for v in sorted(set(timed.values()))}
    print("Timed verdicts: " + ", ".join(f"{v} {n}" for v, n in counts.items()), file=out)
    if expect_regression:
        missed = [c for c, v in timed.items() if v != "regression"]
        if missed:
            print("Slow control FAILED: not judged a regression: "
                  + ", ".join(f"{OPERATIONS[o]} {s}" for o, s in sorted(missed)), file=out)
            return 1
        print(f"Slow control passed: all {len(timed)} timed cells judged a regression.", file=out)
    return 0


# ---------------------------------------------------------------- build


def sha256(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


def run(command, **kwargs):
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, check=True, **kwargs)


def build(args):
    images = os.path.join(args.out, "images")
    os.makedirs(images, exist_ok=True)
    bench = os.path.join(HERE, "bench.wf")
    for arm, wfc in (("base", args.base_wfc), ("candidate", args.candidate_wfc)):
        run([wfc, bench, "-o", os.path.join(images, f"bench-{arm}")])
        run(["make", "-s", "-C", HERE, "peak", f"ROOT={args.runtime_root}", f"WFC={wfc}",
             f"BUILD={os.path.join(args.out, 'peak-' + arm)}"])
        shutil.copyfile(os.path.join(args.out, f"peak-{arm}", "bench-peak"), os.path.join(images, f"peak-{arm}"))
    shutil.copyfile(os.path.join(images, "bench-base"), os.path.join(images, "bench-twin"))
    run([args.candidate_wfc, os.path.join(HERE, "hold.wf"), "-o", os.path.join(images, "hold-candidate")])
    for name in os.listdir(images):
        os.chmod(os.path.join(images, name), 0o755)
    digests = {name: sha256(os.path.join(images, name)) for name in sorted(os.listdir(images))}
    if digests["bench-twin"] != digests["bench-base"]:
        raise Invalid("the twin image differs from the base image")
    if digests["bench-candidate"] == digests["bench-base"]:
        raise Invalid("the candidate image equals the base image: the library change does not reach the program")
    with open(os.path.join(args.out, "manifest.txt"), "a") as f:
        f.write(f"base compiler {sha256(args.base_wfc)}\n")
        f.write(f"candidate compiler {sha256(args.candidate_wfc)}\n")
        for name, digest in digests.items():
            f.write(f"image {name} {digest}\n")
    return 0


# ---------------------------------------------------------------- measure


def launch(image, arguments, cpu):
    environment = dict(os.environ, WF_DRIVERS="1")
    command = ["taskset", "-c", str(cpu), image] + [str(a) for a in arguments]
    done = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=900)
    if done.returncode != 0:
        raise Invalid(f"{' '.join(command)} exited {done.returncode}: {done.stderr.strip()}")
    return [line.split("\t") for line in done.stdout.splitlines() if line]


def append(path, columns, rows):
    fresh = not os.path.exists(path)
    with open(path, "a") as f:
        if fresh:
            f.write("\t".join(columns) + "\n")
        for row in rows:
            f.write("\t".join(str(v) for v in row) + "\n")


def measure(args):
    images = os.path.join(args.out, "images")
    sizes = [int(s) for s in args.sizes.split(",")]
    raw = os.path.join(args.out, "raw.tsv")
    if os.path.exists(raw):
        raise Invalid(f"{raw} exists; measure writes a fresh directory")
    with open(os.path.join(args.out, "manifest.txt"), "a") as f:
        f.write(f"cpu {args.cpu}\nsizes {args.sizes}\nsamples {args.samples}\n")
        f.write(f"rounds {args.min_rounds}..{args.max_rounds}\n")
        f.write("control slow\n" if args.slow else "control none\n")
    active = [(o, s) for o in sorted(OPERATIONS) for s in sizes]
    collected = []
    for round_ in range(args.max_rounds):
        order = list(active)
        random.Random(round_).shuffle(order)
        for index, (operation, size) in enumerate(order):
            arms = ORDERS[(round_ + index) % len(ORDERS)]
            for position, arm in enumerate(arms):
                # The slow control's candidate is the base program with the
                # repetition, so the repetition is the only difference.
                slow = 1 if args.slow and arm == "candidate" else 0
                image = "base" if slow else arm
                lines = launch(os.path.join(images, f"bench-{image}"), [operation, size, args.samples, slow], args.cpu)
                rows = []
                for fields in lines:
                    if len(fields) != 7 or int(fields[0]) != operation or int(fields[1]) != size:
                        raise Invalid(f"{arm} {OPERATIONS[operation]} {size}: row {fields}")
                    rows.append([round_, position, arm, operation, size] + fields[2:])
                append(raw, RAW_COLUMNS, rows)
                collected.extend(dict(zip(RAW_COLUMNS, [int(v) if c != "arm" else v for c, v in zip(RAW_COLUMNS, r)]))
                                 for r in rows)
        done = round_ + 1
        if done >= args.min_rounds:
            ratios = timed_cells(collected, args.slow)
            active = [c for c in active if judge(ratios[c][0], ratios[c][1], c[1])["verdict"] == "unresolved"]
        print(f"round {done}: {len(active)} cells unresolved", flush=True)
        if not active:
            break
    memory = os.path.join(args.out, "memory.tsv")
    for size in sizes:
        for run_ in range(2):
            for arm in ("base", "candidate"):
                lines = launch(os.path.join(images, f"peak-{arm}"), [MEMORY_OPERATION, size, 1, 0], args.cpu)
                if len(lines) != 2 or lines[1][0] != "peak" or len(lines[0]) != 7 or len(lines[1]) != 3:
                    raise Invalid(f"memory {arm} {size}: output {lines}")
                row = lines[0]
                append(memory, MEMORY_COLUMNS, [[arm, run_, size, row[4], row[5], row[6], lines[1][1], lines[1][2]]])
    hold = os.path.join(args.out, "hold.tsv")
    image = os.path.join(images, "hold-candidate")
    cells = [(1, HOLD_SMALL, HOLD_CALLS), (1, HOLD_LARGE, HOLD_CALLS)] + [(2, m, CHAIN_CALLS) for m in CHAINS]
    for mode, size, count in cells:
        lines = launch(image, [mode, size, count], args.cpu)
        if len(lines) != count or any(len(f) != 5 for f in lines):
            raise Invalid(f"hold mode {mode} size {size}: {len(lines)} rows for {count} calls")
        append(hold, HOLD_COLUMNS, [f for f in lines])
    return 0


# ---------------------------------------------------------------- self-test


def synthetic(cells, rounds, factor, twin_factor=1.0, noise=0.0, slow=False, seed=1, candidate_noise=None):
    """Raw rows for the given cells: every process's samples equal its
    per-operation time, base 10 ns, candidate factor times that and twin
    twin_factor times, each scaled per process by a uniform draw within
    plus or minus noise (candidate_noise for the candidate, when given)."""
    rng = random.Random(seed)
    rows = []
    for round_ in range(rounds):
        for operation, size in cells:
            ops, check = expected_row(operation, size)
            for position, arm in enumerate(ARMS):
                scale = {"base": 1.0, "candidate": factor, "twin": twin_factor}[arm]
                spread = candidate_noise if arm == "candidate" and candidate_noise is not None else noise
                scale *= 1 + rng.uniform(-spread, spread)
                extra = 1 if slow and arm == "candidate" else 0
                for sample in range(4):
                    rows.append({"round": round_, "position": position, "arm": arm, "operation": operation,
                                 "size": size, "sample": sample, "ops": ops, "ns": round(10 * scale * ops),
                                 "check": check, "extra": extra})
    return rows


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

    # The interval: below six values none exists; at six it is the extremes
    # (coverage 1 - 2/64); at ten the 2nd and 9th order statistics (coverage
    # 1 - 2 * 11/1024), since the 3rd and 8th would cover only 1 - 2 * 56/1024.
    expect("interval 5", median_interval([1, 2, 3, 4, 5]), None)
    expect("interval 6", median_interval([6, 1, 5, 2, 4, 3]), (1, 6))
    expect("interval 10", median_interval(list(range(10, 0, -1))), (2, 9))
    expect("interval 17", median_interval(list(range(17))), (4, 12))

    cells = [(3, 1000), (7, 1000000)]
    sink = open(os.devnull, "w")
    expect("equal arms", set(reduce_timed(synthetic(cells, 10, 1.0), False, sink).values()), {"no change"})
    expect("slower", set(reduce_timed(synthetic(cells, 10, 1.05, noise=0.005), False, sink).values()),
           {"regression"})
    expect("faster", set(reduce_timed(synthetic(cells, 10, 0.95, noise=0.005), False, sink).values()),
           {"improvement"})
    expect("too few rounds", set(reduce_timed(synthetic(cells, 5, 1.2), False, sink).values()), {"unresolved"})
    expect("noisy twin", set(reduce_timed(synthetic(cells, 10, 1.2, noise=0.2), False, sink).values()),
           {"unresolved"})
    expect("offset twin", set(reduce_timed(synthetic(cells, 10, 1.2, twin_factor=1.05), False, sink).values()),
           {"unresolved"})
    # A 1.5% slowdown at 1,000 pairs (resolution 3%) with 0.5% noise: decided,
    # and the interval excludes 1, so it counts as a regression of any size.
    expect("small regression", reduce_timed(synthetic([(3, 1000)], 12, 1.015, noise=0.005), False, sink),
           {(3, 1000): "regression"})

    # The floor: the spread of a twin within 0.5% of the base is at most
    # 0.005, so a floor of 0.0001 leaves every cell not counted, one of 0.01
    # none, and a floor missing a measured cell or malformed is refused.
    noisy = synthetic(cells, 12, 1.05, noise=0.005)
    expect("below the floor", set(reduce_timed(noisy, False, sink, {c: 0.01 for c in cells}).values()),
           {"regression"})
    expect("above the floor", set(reduce_timed(noisy, False, sink, {c: 0.0001 for c in cells}).values()),
           {"not counted"})
    expect_invalid("floor missing a cell", lambda: reduce_timed(noisy, False, sink, {cells[0]: 0.01}))
    expect("floor parsed", parse_floor("lookup-hit:1000=0.012, iterate:1000000=0.003"),
           {(3, 1000): 0.012, (7, 1000000): 0.003})
    for text in ("lookup-hit:1000", "lookup:1000=0.01", "lookup-hit:x=0.01", "lookup-hit:1000=-1",
                 "lookup-hit:1000=0.01,lookup-hit:1000=0.02", " , "):
        expect_invalid(f"floor {text!r}", lambda text=text: parse_floor(text))
    expect("spread of equal arms", twin_spread([1.0] * 6), (0.0, (0.0, 0.0)))

    # A quiet twin does not decide a cell whose candidate interval is wide:
    # with 30% candidate noise over 8 rounds the interval is the extremes,
    # across 1 and far wider than the 3% resolution, so the cell stays
    # unresolved instead of no change; the hosted slow control once judged a
    # 13% slowdown no change this way.
    wide = reduce_timed(synthetic([(1, 1000)], 8, 1.0, noise=0.002, candidate_noise=0.3), False, sink)
    expect("wide candidate interval", wide, {(1, 1000): "unresolved"})

    good = synthetic(cells, 6, 1.0)
    expect_invalid("missing arm", lambda: timed_cells([r for r in good if not (r["round"] == 2 and r["arm"] == "twin")],
                                                      False))
    expect_invalid("missing sample", lambda: timed_cells([r for r in good if not (r["round"] == 1 and r["sample"] == 2
                                                                                  and r["arm"] == "base")], False))
    expect_invalid("wrong check", lambda: timed_cells([dict(r, check=r["check"] + 1) if r["round"] == 0 else r
                                                       for r in good], False))
    expect_invalid("wrong ops", lambda: timed_cells([dict(r, ops=r["ops"] - 1) for r in good], False))
    expect_invalid("duplicate", lambda: timed_cells(good + good[:1], False))
    expect_invalid("extra without slow", lambda: timed_cells(synthetic(cells, 6, 1.0, slow=True), False))
    expect_invalid("slow without extra", lambda: timed_cells(good, True))
    expect("slow rows accepted", sorted(timed_cells(synthetic(cells, 6, 1.0, slow=True), True)), sorted(cells))
    # 262,144 operations need 263 batches of 1,000; each batch finds every
    # key once and sums the values 0..999, which is 499,500.
    expect("expected hit check", expected_row(3, 1000), (263000, 263000 + 263 * 499500))
    expect("expected iteration batches", expected_row(7, 1000000), (8000000, (8000000 + 8 * 499999500000) & MASK))

    memory = [{"arm": arm, "run": run_, "size": 1000, "before": 100, "retained": retained,
               "length": 1000, "peak": 100 + peak, "maxrss_kib": 2000}
              for arm, retained, peak in (("base", 40000, 60000), ("candidate", 48000, 50000)) for run_ in (0, 1)]
    expect("memory", reduce_memory(memory, sink), {1000: ("higher", "lower")})
    expect_invalid("memory runs differ", lambda: reduce_memory(memory[:-1] + [dict(memory[-1], retained=1)], sink))
    expect_invalid("memory one run", lambda: reduce_memory(memory[:-1], sink))

    def hold_rows(small, large, chain_ns):
        rows = []
        for size, ns in ((HOLD_SMALL, small), (HOLD_LARGE, large)):
            rows += [{"mode": 1, "size": size, "call": c, "ns": ns, "pairs": 64} for c in range(100)]
        for m in CHAINS:
            rows += [{"mode": 2, "size": m, "call": c, "ns": chain_ns(m), "pairs": m} for c in range(10)]
        return rows
    expect("hold pass", set(reduce_hold(hold_rows(1000, 3000, lambda m: m * 40), sink).values()), {True})
    readings = reduce_hold(hold_rows(1000, 5000, lambda m: m * 400), sink)
    expect("hold H1 fail", readings["H1"], False)
    expect("hold H3 fail", readings["H3-64"], False)
    expect("hold H2 fail", reduce_hold(hold_rows(10000, 30000, lambda m: m), sink)["H2"], False)
    expect_invalid("hold short chain", lambda: reduce_hold(
        [dict(r, pairs=1) if r["mode"] == 2 else r for r in hold_rows(1, 1, lambda m: 1)], sink))
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
    p.add_argument("--runtime-root", required=True)
    p.add_argument("--out", required=True)
    p = sub.add_parser("measure")
    p.add_argument("--out", required=True)
    p.add_argument("--cpu", required=True, type=int)
    p.add_argument("--sizes", default="1000,10000,1000000")
    p.add_argument("--samples", type=int, default=5)
    p.add_argument("--min-rounds", type=int, default=10)
    p.add_argument("--max-rounds", type=int, default=40)
    p.add_argument("--slow", action="store_true")
    p = sub.add_parser("reduce")
    p.add_argument("--out", required=True)
    p.add_argument("--expect-regression", action="store_true")
    p.add_argument("--floor", default="")
    sub.add_parser("self-test")
    args = parser.parse_args(argv)
    try:
        if args.command == "build":
            return build(args)
        if args.command == "measure":
            if args.min_rounds < 6 or args.max_rounds < args.min_rounds or args.samples < 1:
                parser.error("rounds need 6 <= min-rounds <= max-rounds, and samples at least 1")
            return measure(args)
        if args.command == "reduce":
            floor = parse_floor(args.floor) if args.floor.strip() else None
            with open(os.path.join(args.out, "summary.txt"), "w") as out:
                status = reduce_all(args.out, args.expect_regression, out, floor)
            with open(os.path.join(args.out, "summary.txt")) as f:
                print(f.read(), end="")
            return status
        return self_test()
    except (Invalid, subprocess.CalledProcessError, subprocess.TimeoutExpired, OSError) as error:
        print(f"compare.py {args.command}: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
