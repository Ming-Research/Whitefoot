#!/usr/bin/env python3
"""Summarize raw.tsv: medians per arm and CPU list, and the requested ratios."""
import csv
import statistics
import sys
from collections import defaultdict

LISTS = ["0,2", "0,2,4,6", "0,2,4,6,8,10,12,14", "0-15"]
LIST_NAMES = {LISTS[0]: "2 P", LISTS[1]: "4 P", LISTS[2]: "8 P", LISTS[3]: "8 P+HT"}
NAMES = {
    "A1": "A1 perthread-faa", "A1t": "A1t perthread-faa (twin)", "A2": "A2 shared-faa",
    "A3": "A3 shard-ticket, no reader", "A3r": "A3r shard-ticket, reader",
    "A4": "A4 shard-asym, no reader", "A4r": "A4r shard-asym, reader",
    "A5a": "A5a 16-byte mallocs", "A5b": "A5b aligned 64", "A5c": "A5c one alloc, 16 apart",
    "A5d": "A5d one alloc, 64 apart", "A5e": "A5e one alloc, 128 apart",
    "A6sl": "A6 shared load", "A6sf": "A6 shared fetch_add", "A6sx": "A6 shared exchange",
    "A6sc": "A6 shared CAS loop", "A6pl": "A6 per-thread load", "A6pf": "A6 per-thread fetch_add",
    "A6px": "A6 per-thread exchange", "A6pc": "A6 per-thread CAS loop",
    "A7n": "A7 noseq", "A7s": "A7 seq", "A8": "A8 timedwait",
}


def load(path):
    cells = defaultdict(list)
    with open(path, newline="") as stream:
        for row in csv.reader(stream, delimiter="\t"):
            if not row or row[0] == "arm":
                continue
            fields = {}
            for token in row[3:]:
                if "=" in token:
                    key, value = token.split("=", 1)
                    fields[key] = value
            fields["block"] = int(row[2])
            cells[row[0], row[1]].append(fields)
    return cells


def num(rows, key):
    values = []
    for row in rows:
        try:
            values.append(float(row[key]))
        except (KeyError, ValueError):
            pass
    return values


def med(cells, arm, cpus, key="ops_per_s"):
    values = num(cells.get((arm, cpus), []), key)
    return statistics.median(values) if values else None


def mps(value):
    return "-" if value is None else f"{value / 1e6:.1f}"


def ratio(a, b):
    return "-" if a is None or not b else f"{a / b:.3f}"


def table(cells, title, arms, lists=LISTS):
    print(title)
    print("  " + "arm".ljust(30) + "".join(LIST_NAMES[c].rjust(10) for c in lists) + "   (M ops/s, median of blocks)")
    for arm in arms:
        print("  " + NAMES[arm].ljust(30) + "".join(mps(med(cells, arm, c)).rjust(10) for c in lists))
    print()


def reader_table(cells, title, arms):
    print(title)
    print("  " + "arm / cpus".ljust(34) + "reads".rjust(8) + "median us".rjust(11) + "p99 us".rjust(9)
          + "max us (med)".rjust(14) + "max us (worst)".rjust(16))
    for arm in arms:
        for cpus in LISTS:
            rows = cells.get((arm, cpus), [])
            if not rows:
                continue
            r = statistics.median(num(rows, "reads"))
            m = statistics.median(num(rows, "read_med_ns")) / 1000
            p = statistics.median(num(rows, "read_p99_ns")) / 1000
            x = statistics.median(num(rows, "read_max_ns")) / 1000
            w = max(num(rows, "read_max_ns")) / 1000
            print("  " + f"{arm} {LIST_NAMES[cpus]}".ljust(34) + f"{r:.0f}".rjust(8) + f"{m:.1f}".rjust(11)
                  + f"{p:.1f}".rjust(9) + f"{x:.1f}".rjust(14) + f"{w:.1f}".rjust(16))
    print()


def ratio_table(cells, title, pairs):
    print(title)
    print("  " + "ratio".ljust(30) + "".join(LIST_NAMES[c].rjust(10) for c in LISTS))
    for label, num_arm, den_arm in pairs:
        print("  " + label.ljust(30) + "".join(
            ratio(med(cells, num_arm, c), med(cells, den_arm, c)).rjust(10) for c in LISTS))
    print()


def main(path):
    cells = load(path)
    blocks = sorted({r["block"] for rows in cells.values() for r in rows})
    print(f"Blocks: {len(blocks)}; cells: {sum(len(v) for v in cells.values())}")
    print()
    table(cells, "A1/A2: atomic fetch_add", ["A1", "A1t", "A2"])
    table(cells, "A3/A4: sharded counters", ["A3", "A3r", "A4", "A4r"])
    reader_table(cells, "Reader runs (one whole read per millisecond)", ["A3r", "A4r"])
    table(cells, "A5: layout of parts", ["A5a", "A5b", "A5c", "A5d", "A5e"])
    table(cells, "A6: atomic operations",
          ["A6sl", "A6sf", "A6sx", "A6sc", "A6pl", "A6pf", "A6px", "A6pc"])
    table(cells, "A7: sequence structure", ["A7n", "A7s"])
    print("A8: deadline wait, lateness in microseconds (median of blocks)")
    print("  " + "cpus".ljust(24) + "median".rjust(9) + "p99".rjust(9) + "max (med)".rjust(11) + "max (worst)".rjust(13))
    for cpus in ["0", LISTS[2]]:
        rows = cells.get(("A8", cpus), [])
        if not rows:
            continue
        label = "1 thread, CPU 0" if cpus == "0" else "8 threads, 8 P-cores"
        f = lambda k: statistics.median(num(rows, k)) / 1000
        print("  " + label.ljust(24) + f"{f('lat_med_ns'):.1f}".rjust(9) + f"{f('lat_p99_ns'):.1f}".rjust(9)
              + f"{f('lat_max_ns'):.1f}".rjust(11) + f"{max(num(rows, 'lat_max_ns')) / 1000:.1f}".rjust(13))
    print()
    ratio_table(cells, "Ratios", [
        ("A1t/A1 (noise)", "A1t", "A1"),
        ("A3/A1", "A3", "A1"), ("A3r/A1", "A3r", "A1"),
        ("A4/A1", "A4", "A1"), ("A4r/A1", "A4r", "A1"),
        ("A4r/A4", "A4r", "A4"), ("A3r/A3", "A3r", "A3"),
        ("A5a/A5b", "A5a", "A5b"), ("A5c/A5b", "A5c", "A5b"), ("A5d/A5b", "A5d", "A5b"),
        ("A5e/A5b", "A5e", "A5b"),
        ("A7 seq/noseq", "A7s", "A7n"),
        ("A6 shared load/A1", "A6sl", "A1"), ("A6 shared faa/A1", "A6sf", "A1"),
        ("A6 shared xchg/A1", "A6sx", "A1"), ("A6 shared cas/A1", "A6sc", "A1"),
        ("A6 per-thread load/A1", "A6pl", "A1"), ("A6 per-thread faa/A1", "A6pf", "A1"),
        ("A6 per-thread xchg/A1", "A6px", "A1"), ("A6 per-thread cas/A1", "A6pc", "A1"),
        ("A2/A1", "A2", "A1"),
    ])
    stride = {}
    for arm in ["A5a", "A5b", "A5c", "A5d", "A5e"]:
        rows = cells.get((arm, LISTS[1]), [])
        if rows and "stride" in rows[0]:
            stride[arm] = rows[0]["stride"]
    print("A5 byte distance between part 0 and part 1: " + ", ".join(f"{k}={v}" for k, v in stride.items()))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
