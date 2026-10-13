#!/usr/bin/env python3
"""Reduces run.sh's raw.tsv to the tables and the registered decisions the
arms of this experiment can make (README.md; research/investigations/
shared-counters/DESIGN.md, section 8).

    python3 summarize.py [--smoke] RAW.tsv

It prints, per arm and CPU list, the median and the range of the rate over
blocks; the noise e from the twin pair W2t and W2; the descriptive ratios; and
the three registered conditions these arms can decide, R2 (approximated by a
stand-in), R3 and P3. Every threshold is provisional, as section 8.4 says.
A condition is decided only when its ratio clears the threshold by more than e;
with e above 0.03 (provisional) it is inconclusive.

When the C model's arms are present (W7, its twin W7t, its reader variant
W7rd and the comparison W5pN), it also prints the C model's reading under the
rule registered in DESIGN.md section 8.6: per CPU list, the ratios of two arms
paired within each block, their median and a 95% percentile bootstrap
interval, the twin's validity, the gate W7 / W5pN against 0.5 and the reader
criteria. --smoke marks a wiring run on a host that is not the 14900K: it
reads every CPU list as a decision list and labels every reading as no verdict.
"""
import csv
import random
import statistics
import sys
from collections import defaultdict

ARMS = ["W0", "W1", "W2", "W2t", "W2r", "W6", "W3", "W3rd", "W5", "W5p",
        "W5pN", "W7", "W7t", "W7rd"]
MEANING = {
    "W0": "no atomic statement (runtime floor)",
    "W1": "own Shared<u64> per context (ceiling for an uncontended statement)",
    "W2": "one Shared<u64>, value handle (the contended base)",
    "W2t": "second build of W2's source (noise control)",
    "W2r": "W2 through a read-only reference parameter (attributes the handle count)",
    "W6": "read-only statement on one Shared<u64>",
    "W3": "+wrap 1 on element index mod 16 of an array of 16 Shared<u64>",
    "W3rd": "W3 plus a reader of all 16 every millisecond",
    "W5": "C: one 64-byte-aligned counter, atomic_fetch_add, seq_cst",
    "W5p": "C: each thread on its own padded line",
    "W5pN": "C: W5p's binary with one thread per CPU (the C model's comparison)",
    "W7": "C model: one writer thread per CPU, each on its own shard line, ticket lock beside the count",
    "W7t": "second build of W7's source (the C model's noise control)",
    "W7rd": "W7 plus a reader of every shard each millisecond on a CPU outside the list",
}
NOISE_LIMIT = 0.03
R2_THRESHOLD = 0.8
R3_THRESHOLD = 0.5
P3_THRESHOLD = 0.25
P_CORE_DRIVERS = 8
HYPERTHREAD_DRIVERS = 16

# The C model's registered rule (DESIGN.md section 8.6).
MODEL_LISTS = {
    "0,2": "2 P-cores",
    "0,2,4,6": "4 P-cores",
    "0,2,4,6,8,10,12,14": "8 P-cores",
}
GATE_THRESHOLD = 0.5
RETENTION_THRESHOLD = 0.95
READ_LIMIT_US = 50.0
MIN_BLOCKS = 6
DRAWS = 10000
SEED = 20261012


def number(row, name):
    """A raw.tsv field; a column an older raw.tsv lacks reads as 0."""
    value = row.get(name)
    return int(value) if value not in (None, "") else 0


def read(path):
    cells = defaultdict(list)
    with open(path, newline="") as f:
        for row in csv.DictReader(f, delimiter="\t"):
            key = (row["arm"], row["cpus"])
            cells[key].append({
                "block": int(row["block"]),
                "drivers": int(row["drivers"]),
                "rate": float(row["ops_per_s"]),
                "busy": float(row["user_frac"]) + float(row["sys_frac"]),
                "reads": int(row["reads"]),
                "read_ns": int(row["read_ns"]),
                "read_max_ns": int(row["read_max_ns"]),
                "read_med_ns": number(row, "read_med_ns"),
                "read_p99_ns": number(row, "read_p99_ns"),
            })
    return cells


def median_rate(cells, arm, cpus):
    rows = cells.get((arm, cpus))
    return statistics.median(r["rate"] for r in rows) if rows else None


def ratio(cells, top, bottom, cpus):
    a = median_rate(cells, top, cpus)
    b = median_rate(cells, bottom, cpus)
    if a is None or b is None or b == 0:
        return None
    return a / b


def lists_in_order(cells):
    drivers = {}
    for (arm, cpus), rows in cells.items():
        drivers[cpus] = rows[0]["drivers"]
    return sorted(drivers, key=lambda c: drivers[c]), drivers


def fmt(value, places=3):
    return "n/a" if value is None else f"{value:.{places}f}"


def verdict(value, threshold, at_least, noise):
    """A ratio's reading against a threshold, with the noise margin."""
    if value is None:
        return "no data"
    if noise is None:
        return "inconclusive: no noise estimate (W2t and W2 absent)"
    if noise > NOISE_LIMIT:
        return f"inconclusive: noise e = {noise:.3f} exceeds {NOISE_LIMIT} (provisional)"
    margin = value - threshold if at_least else threshold - value
    if margin > noise:
        return "met"
    if margin < -noise:
        return "not met"
    return f"inconclusive: within e = {noise:.3f} of the threshold"


def by_block(cells, arm, cpus, field):
    return {r["block"]: r[field] for r in cells.get((arm, cpus), [])}


def paired_ratios(cells, top, bottom, cpus):
    """top / bottom within each block where both arms have a row."""
    a = by_block(cells, top, cpus, "rate")
    b = by_block(cells, bottom, cpus, "rate")
    return [a[k] / b[k] for k in sorted(a) if k in b and b[k] > 0]


def bootstrap(values):
    """The median of values with its 95% percentile bootstrap interval:
    DRAWS resamples of len(values) values drawn with replacement by a
    generator seeded with SEED, the interval running from the 251st to the
    9750th of the sorted resample medians. None without values."""
    if not values:
        return None
    rng = random.Random(SEED)
    n = len(values)
    medians = sorted(
        statistics.median([values[int(rng.random() * n)] for _ in range(n)])
        for _ in range(DRAWS))
    return (statistics.median(values), len(values),
            medians[DRAWS * 25 // 1000], medians[DRAWS * 975 // 1000 - 1])


def show(interval, places=3):
    if interval is None:
        return "no data"
    middle, n, low, high = interval
    return f"{middle:.{places}f} [{low:.{places}f}-{high:.{places}f}] n={n}"


def interval_reading(interval, threshold, at_least):
    """met, not met or inconclusive under section 8.6: an interval wholly on
    the passing side of the threshold meets it, one wholly on the other side
    does not, and too few paired blocks decide nothing."""
    if interval is None:
        return "inconclusive: no data"
    _, n, low, high = interval
    if n < MIN_BLOCKS:
        return f"inconclusive: {n} paired blocks, fewer than {MIN_BLOCKS}"
    if at_least:
        if low >= threshold:
            return "met"
        if high < threshold:
            return "not met"
    else:
        if high <= threshold:
            return "met"
        if low > threshold:
            return "not met"
    return "inconclusive: the interval contains the threshold"


def model_section(cells, lists, drivers, smoke):
    """The C model's reading under the rule of DESIGN.md section 8.6."""
    if not any(("W7", c) in cells for c in lists):
        return
    decides = [c for c in lists if smoke or c in MODEL_LISTS]
    mark = "smoke run, no verdict: " if smoke else ""
    print("C model (DESIGN.md section 8.6, registered before measurement)")
    print("Each ratio is formed within a block, top arm over bottom arm on the same CPU list; shown is the median")
    print(f"over blocks with its 95% percentile bootstrap interval ({DRAWS} resamples, seed {SEED}) and n, the paired blocks.")
    print("A list's readings count only when the twin's interval W7t / W7 contains 1; otherwise the list is void.")
    print(f"Fewer than {MIN_BLOCKS} paired blocks decide nothing. Lists outside {', '.join(MODEL_LISTS)} are reported only.")
    if smoke:
        print("SMOKE RUN: a wiring check on a host that is not the 14900K. Nothing below is a verdict.")
    print()
    gate_readings = {}
    reader_readings = {}
    for c in lists:
        name = MODEL_LISTS.get(c, f"{drivers[c]} CPUs")
        twin = bootstrap(paired_ratios(cells, "W7t", "W7", c))
        gate = bootstrap(paired_ratios(cells, "W7", "W5pN", c))
        retention = bootstrap(paired_ratios(cells, "W7rd", "W7", c))
        medians_us = [r["read_med_ns"] / 1000 for r in cells.get(("W7rd", c), []) if r["reads"] > 0]
        latency = bootstrap(medians_us)
        print(f"{name} ({c}, {drivers[c]} writer threads, {2 * drivers[c]} shards)")
        twin_valid = twin is not None and twin[2] <= 1 <= twin[3]
        print(f"  twin W7t / W7:            {show(twin)}  -> {'valid' if twin_valid else 'VOID: the interval excludes 1' if twin else 'VOID: no twin'}")
        print(f"  W7 / W5pN (gate, >= {GATE_THRESHOLD}):  {show(gate)}")
        print(f"  W7 / W5p, 64 threads:     {show(bootstrap(paired_ratios(cells, 'W7', 'W5p', c)))}  (reported only)")
        print(f"  W5pN / W5p:               {show(bootstrap(paired_ratios(cells, 'W5pN', 'W5p', c)))}  (reported only)")
        print(f"  W7rd / W7 (>= {RETENTION_THRESHOLD}):       {show(retention)}")
        print(f"  whole read, median us (<= {READ_LIMIT_US:.0f}): {show(latency, 2)}  (median over blocks of each cell's median read)")
        rows = cells.get(("W7rd", c), [])
        if rows:
            p99 = statistics.median(r["read_p99_ns"] / 1000 for r in rows)
            longest = max(r["read_max_ns"] / 1000 for r in rows)
            reads = statistics.median(r["reads"] for r in rows)
            print(f"  whole read, 99th percentile us (median over blocks): {p99:.2f}; longest of any block: {longest:.2f}; reads per cell (median): {reads:.0f}  (reported only)")
        if c in decides:
            if not twin_valid:
                gate_readings[c] = reader_readings[c] = "void"
            else:
                gate_readings[c] = interval_reading(gate, GATE_THRESHOLD, True)
                write_kept = interval_reading(retention, RETENTION_THRESHOLD, True)
                read_time = interval_reading(latency, READ_LIMIT_US, False)
                if write_kept == "met" and read_time == "met":
                    reader_readings[c] = "met"
                elif "not met" in (write_kept, read_time):
                    reader_readings[c] = "not met"
                else:
                    reader_readings[c] = "inconclusive"
                print(f"  {mark}reader retention: {write_kept}; whole read time: {read_time}")
            print(f"  {mark}gate at {name}: {gate_readings[c]}")
        print()

    expected = list(lists) if smoke else list(MODEL_LISTS)
    gates = [gate_readings.get(c, "inconclusive: no data") for c in expected]
    if any(g == "not met" for g in gates):
        overall = "not met at a list: the locked approach is insufficient; switch to the writer side without an atomic instruction (section 8.5)"
    elif all(g == "met" for g in gates):
        overall = "met at every list: implement the compiler and the runtime (section 8.5)"
    else:
        overall = "inconclusive or void at a list, none not met: one rerun of the whole run is allowed (section 8.6)"
    print(f"{mark}Gate, W7 >= {GATE_THRESHOLD} x W5pN at {', '.join(MODEL_LISTS[c] if c in MODEL_LISTS else c for c in expected)}: {overall}")
    readers = [reader_readings.get(c, "inconclusive") for c in expected]
    if any(r == "not met" for r in readers):
        reader_overall = "not met at a list: brought to the owner before the implementation starts (section 8.6)"
    elif all(r == "met" for r in readers):
        reader_overall = "met at every list"
    else:
        reader_overall = "inconclusive or void at a list"
    print(f"{mark}Reader criteria (writes with the reader >= {RETENTION_THRESHOLD} x without; median whole read <= {READ_LIMIT_US:.0f} us): {reader_overall}")
    print()


def main(path, smoke=False):
    cells = read(path)
    lists, drivers = lists_in_order(cells)
    if not lists:
        print("no rows")
        return 1
    blocks = sorted({r["block"] for rows in cells.values() for r in rows})
    print(f"source: {path}; blocks {len(blocks)}; CPU lists {', '.join(f'{c} ({drivers[c]} drivers)' for c in lists)}")
    print("Every threshold below is provisional (DESIGN.md, section 8.4).")
    print()

    print("Table 1. Rate, millions of operations per second summed over all contexts: median over blocks [min-max] (n = blocks)")
    print("Source: raw.tsv column ops_per_s. Higher is better; W0 is the runtime ceiling and W1 the uncontended-statement ceiling.")
    header = f"{'arm':<5}" + "".join(f"{drivers[c]:>3} CPUs{'':<18}" for c in lists)
    print(header)
    for arm in ARMS:
        parts = []
        any_row = False
        for c in lists:
            rows = cells.get((arm, c))
            if not rows:
                parts.append(f"{'-':<26}")
                continue
            any_row = True
            rates = [r["rate"] / 1e6 for r in rows]
            parts.append(f"{statistics.median(rates):8.2f} [{min(rates):.2f}-{max(rates):.2f}] n={len(rates):<2}".ljust(26))
        if any_row:
            print(f"{arm:<5}" + "".join(parts) + f"  {MEANING[arm]}")
    print()

    print("Table 2. Share of the listed CPUs' time spent in user and system mode, median over blocks (1.00 = all busy)")
    print("Source: raw.tsv columns user_frac + sys_frac. A low value means contexts or threads were parked.")
    print(f"{'arm':<5}" + "".join(f"{drivers[c]:>3} CPUs  " for c in lists))
    for arm in ARMS:
        if not any((arm, c) in cells for c in lists):
            continue
        parts = []
        for c in lists:
            rows = cells.get((arm, c))
            parts.append(f"{statistics.median(r['busy'] for r in rows):8.2f} " if rows else f"{'-':>9}")
        print(f"{arm:<5}" + "".join(parts))
    print()

    noise_terms = []
    for c in lists:
        ratio_twin = ratio(cells, "W2t", "W2", c)
        if ratio_twin is not None:
            noise_terms.append(abs(ratio_twin - 1))
    noise = max(noise_terms) if noise_terms else None
    if noise is not None:
        print("Noise e = max over CPU lists of |median(W2t) / median(W2) - 1|  (DESIGN.md, section 8.3)")
        print("W2t is the same source built a second time; the compared binaries' hashes are in host.txt.")
        print(f"  per list: " + ", ".join(
            f"{drivers[c]} CPUs {fmt(ratio(cells, 'W2t', 'W2', c))}" for c in lists))
        print(f"  e = {fmt(noise)}  ({'inconclusive threshold exceeded' if noise > NOISE_LIMIT else 'within ' + str(NOISE_LIMIT) + ' (provisional)'})")
        print()

    print("Table 3. Descriptive ratios of median rates (no rejection condition attached)")
    pairs = [("W2/W1", "W2", "W1"), ("W3/W2", "W3", "W2"), ("W3/W1", "W3", "W1"),
             ("W2r/W2", "W2r", "W2"), ("W6/W2", "W6", "W2"), ("W6/W1", "W6", "W1"),
             ("W3rd/W3", "W3rd", "W3"), ("W5/W5p", "W5", "W5p"),
             ("W5pN/W5p", "W5pN", "W5p"), ("W7/W5pN", "W7", "W5pN"), ("W7/W5p", "W7", "W5p"),
             ("W7t/W7", "W7t", "W7"), ("W7rd/W7", "W7rd", "W7")]
    print(f"{'ratio':<9}" + "".join(f"{drivers[c]:>3} CPUs  " for c in lists))
    for label, top, bottom in pairs:
        values = [ratio(cells, top, bottom, c) for c in lists]
        if all(v is None for v in values):
            continue
        print(f"{label:<9}" + "".join(f"{fmt(v):>9} " for v in values))
    print()

    if any(("W3rd", c) in cells for c in lists):
        print("Table 4. The reader context of W3rd: its all-sixteen atomic statement, microseconds")
        print("Source: raw.tsv columns reads, read_ns, read_max_ns. Mean = read_ns / reads per cell, median over blocks; longest = median over blocks of the longest read.")
        print(f"{'':<8}" + "".join(f"{drivers[c]:>3} CPUs  " for c in lists))
        means, longest, counts = [], [], []
        for c in lists:
            rows = cells.get(("W3rd", c))
            if not rows:
                means.append(None); longest.append(None); counts.append(None)
                continue
            means.append(statistics.median(r["read_ns"] / r["reads"] / 1000 for r in rows if r["reads"]))
            longest.append(statistics.median(r["read_max_ns"] / 1000 for r in rows))
            counts.append(statistics.median(r["reads"] for r in rows))
        print(f"{'mean':<8}" + "".join(f"{fmt(v, 1):>9} " for v in means))
        print(f"{'longest':<8}" + "".join(f"{fmt(v, 1):>9} " for v in longest))
        print(f"{'reads':<8}" + "".join(f"{fmt(v, 0):>9} " for v in counts))
        print("About 5000 reads are expected in a 5 s cell; fewer means the reader fell behind its 1 ms period.")
        print()

    model_section(cells, lists, drivers, smoke)

    if not any(("W3", c) in cells or ("W5", c) in cells for c in lists):
        return 0

    print("Registered conditions these arms can decide (all thresholds provisional; e as above)")
    print()
    print("R2, approximated. Registered: W3 >= 0.8 x W4 at every CPU list rejects the recommendation.")
    print("  No sharded state exists yet, so there is no W4. Stand-in: W1 (private objects), taken as an upper bound")
    print("  on what any sharded implementation could reach. This is an inference: W4 could in principle exceed W1,")
    print("  and the stand-in decides only one direction. W3 >= 0.8 x W1 at every list implies W3 >= 0.8 x W4 if")
    print("  W4 <= W1; a lower W3/W1 leaves R2 undecided, because W4 may sit below W1.")
    r2_all = True
    for c in lists:
        value = ratio(cells, "W3", "W1", c)
        reading = verdict(value, R2_THRESHOLD, True, noise)
        print(f"  W3/W1 at {drivers[c]} CPUs ({c}): {fmt(value)}  vs {R2_THRESHOLD}: {reading}")
        r2_all = r2_all and reading == "met"
    print(f"  Stand-in reading: {'W3 >= 0.8 x W1 at every list (R2 would hold if W4 <= W1)' if r2_all else 'not met at some list: R2 stays undecided until W4 exists'}")
    print()

    key8 = next((c for c in lists if drivers[c] == P_CORE_DRIVERS), None)
    if key8 is None:
        print("R3 and P3: no 8-P-core list in the data.")
        return 0
    value = ratio(cells, "W5", "W5p", key8)
    print(f"R3. Registered: W5 / W5p >= {R3_THRESHOLD} at 8 P-cores rejects the recommendation (a contended hardware add then serves counts).")
    print(f"  W5/W5p at 8 P-cores ({key8}): {fmt(value)}  vs {R3_THRESHOLD}: {verdict(value, R3_THRESHOLD, True, noise)}")
    print(f"P3. Registered: W5 / W5p <= {P3_THRESHOLD} at 8 P-cores (a contended hardware add stays far below private increments).")
    print(f"  W5/W5p at 8 P-cores ({key8}): {fmt(value)}  vs {P3_THRESHOLD}: {verdict(value, P3_THRESHOLD, False, noise)}")
    print("  e comes from the Whitefoot twin pair; the C arms have no twin, so it is applied to them as an approximation.")
    return 0


if __name__ == "__main__":
    arguments = sys.argv[1:]
    smoke = arguments[:1] == ["--smoke"]
    if smoke:
        arguments = arguments[1:]
    if len(arguments) != 1:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(arguments[0], smoke))
