#!/usr/bin/env python3
"""Serves concurrent-map-bench: turns run.sh rows into tables.

    python3 summarize.py ROWS.csv [ROWS.csv ...] [--ours NAME]

For every size, key choice and mix it prints one table: an implementation
per row, a thread count per column, the median rate over repetitions in
millions of operations per second, the fastest native comparator per column,
and, with --ours, that implementation's ratio to it. Failed checks are listed
first; a failed implementation's rates are marked.
"""
import csv
import statistics
import sys
from collections import defaultdict

CONTROLS = {"empty", "mutex-flat", "wf-current"}
MANAGED = {"java-chm", "go-syncmap", "go-xsync", "dotnet-cd"}
MIX_ORDER = ["read", "mostly-read", "balanced", "update", "churn", "grow", "prefill"]
DIST_ORDER = ["uniform", "zipf", "one"]


def main(argv):
    ours = None
    paths = []
    i = 0
    while i < len(argv):
        if argv[i] == "--ours":
            ours = argv[i + 1]
            i += 2
        else:
            paths.append(argv[i])
            i += 1
    rates = defaultdict(list)
    flags = {}
    failed = set()
    failures = []
    for path in paths:
        with open(path, newline="") as f:
            for r in csv.DictReader(f):
                impl, mix = r["impl"], r["mix"]
                flags[impl] = r["flags"]
                if r["check"].startswith("fail"):
                    failed.add((impl, r["size"], r["dist"]))
                    failures.append(f"{impl} size {r['size']} {r['dist']} {mix} threads {r['threads']}: {r['check']}")
                if mix.startswith("check-"):
                    continue
                rates[(r["size"], r["dist"], mix, impl, int(r["threads"]))].append(float(r["mops"]))
    if failures:
        print("Failed checks:")
        for line in failures:
            print(f"- {line}")
        print()
    cells = defaultdict(lambda: defaultdict(dict))
    for (size, dist, mix, impl, threads), values in rates.items():
        cells[(size, dist, mix)][impl][threads] = statistics.median(values)

    def order(key):
        size, dist, mix = key
        return (int(size), DIST_ORDER.index(dist) if dist in DIST_ORDER else 9,
                MIX_ORDER.index(mix) if mix in MIX_ORDER else 9)

    for key in sorted(cells, key=order):
        size, dist, mix = key
        table = cells[key]
        counts = sorted({t for per in table.values() for t in per})
        native = [i for i in table if i not in CONTROLS and i not in MANAGED and i != ours
                  and "one-thread" not in flags.get(i, "")]
        print(f"### N = {size}, {dist}, {mix} (Mop/s, median)")
        print()
        print("| implementation | flags | " + " | ".join(f"{t} thr" for t in counts) + " |")
        print("|---|---|" + "---|" * len(counts))
        best = {}
        for t in counts:
            candidates = [(table[i][t], i) for i in native if t in table[i]]
            if candidates:
                best[t] = max(candidates)

        def group(i):
            return (0 if i == ours else 1 if i in CONTROLS else 3 if i in MANAGED else 2, i)

        for impl in sorted(table, key=group):
            cols = []
            for t in counts:
                v = table[impl].get(t)
                if v is None:
                    cols.append("")
                    continue
                text = f"{v:.2f}"
                if best.get(t) and best[t][1] == impl:
                    text = f"**{text}**"
                if (impl, size, dist) in failed:
                    text += " (failed)"
                cols.append(text)
            print(f"| {impl} | {flags.get(impl, '-')} | " + " | ".join(cols) + " |")
        if ours and ours in table:
            ratios = []
            for t in counts:
                if t in best and t in table[ours]:
                    ratios.append(f"{t} thr {table[ours][t] / best[t][0]:.2f} of {best[t][1]}")
            print()
            print(f"{ours} against the fastest native comparator: " + "; ".join(ratios))
        print()


if __name__ == "__main__":
    main(sys.argv[1:])
