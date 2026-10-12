#!/usr/bin/env python3
"""Reduces run.sh's raw.tsv to the tables and the registered decisions the
arms of this experiment can make (README.md; research/investigations/
shared-counters/DESIGN.md, section 8).

    python3 summarize.py RAW.tsv

It prints, per arm and CPU list, the median and the range of the rate over
blocks; the noise e from the twin pair W2t and W2; the descriptive ratios; and
the three registered conditions these arms can decide, R2 (approximated by a
stand-in), R3 and P3. Every threshold is provisional, as section 8.4 says.
A condition is decided only when its ratio clears the threshold by more than e;
with e above 0.03 (provisional) it is inconclusive.
"""
import csv
import statistics
import sys
from collections import defaultdict

ARMS = ["W0", "W1", "W2", "W2t", "W2r", "W6", "W3", "W3rd", "W5", "W5p"]
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
}
NOISE_LIMIT = 0.03
R2_THRESHOLD = 0.8
R3_THRESHOLD = 0.5
P3_THRESHOLD = 0.25
P_CORE_DRIVERS = 8
HYPERTHREAD_DRIVERS = 16


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


def main(path):
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
    print("Noise e = max over CPU lists of |median(W2t) / median(W2) - 1|  (DESIGN.md, section 8.3)")
    print("W2t is the same source built a second time; the compared binaries' hashes are in host.txt.")
    print(f"  per list: " + ", ".join(
        f"{drivers[c]} CPUs {fmt(ratio(cells, 'W2t', 'W2', c))}" for c in lists))
    print(f"  e = {fmt(noise)}  ({'inconclusive threshold exceeded' if noise is not None and noise > NOISE_LIMIT else 'within ' + str(NOISE_LIMIT) + ' (provisional)'})")
    print()

    print("Table 3. Descriptive ratios of median rates (no rejection condition attached)")
    pairs = [("W2/W1", "W2", "W1"), ("W3/W2", "W3", "W2"), ("W3/W1", "W3", "W1"),
             ("W2r/W2", "W2r", "W2"), ("W6/W2", "W6", "W2"), ("W6/W1", "W6", "W1"),
             ("W3rd/W3", "W3rd", "W3"), ("W5/W5p", "W5", "W5p")]
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
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
