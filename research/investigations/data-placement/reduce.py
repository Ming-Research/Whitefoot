#!/usr/bin/env python3
"""Reduce one host's results of probe.sh to the tables the criterion reads.

usage: reduce.py RESULTS

For every campaign it prints each kernel and width's paired ratio with the
regression rule applied in both directions, and stencil's process medians at
one worker per arm. For every traced series it prints, per distance between
the two grids of a timed call, how many launches got it and their times.
"""

import os
import re
import statistics
import sys

KERNELS = ["mandelbrot", "records", "fir", "quadrature", "stencil"]
GRID_LOW, GRID_HIGH = 32 << 20, (32 << 20) + (1 << 16)


def process_medians(path):
    """(kernel, arm, width) -> the median wall time of each process, in ms."""
    samples = {}
    with open(path) as rows:
        for row in rows:
            f = row.split()
            if len(f) != 8 or f[4] == "0":
                continue
            samples.setdefault((f[0], f[1], f[2], f[3]), []).append(int(f[5]))
    cells = {}
    for (kernel, arm, width, _), walls in sorted(samples.items()):
        cells.setdefault((kernel, arm, width), []).append(statistics.median(walls) / 1e6)
    return cells


def campaign(results, name):
    paired = os.path.join(results, name, "paired.tsv")
    if not os.path.exists(paired):
        print(f"campaign {name}: no paired table")
        return
    print(f"campaign {name}")
    moved = []
    with open(paired) as rows:
        for row in rows:
            kernel, width, ratio, lower = row.split()[:4]
            ratio, lower = float(ratio), int(lower)
            # The regression rule, and its mirror for a faster candidate.
            mark = ""
            if ratio < 0.97 and lower >= 4:
                mark = "slower"
            elif ratio > 1 / 0.97 and lower <= 1:
                mark = "faster"
            if mark:
                moved.append(f"{kernel} W={width} {ratio:.3f} {mark}")
            print(f"  {kernel:11s} W={width} {ratio:.3f} lower={lower}/5 {mark}")
    print(f"  moved: {'; '.join(moved) if moved else 'none'}")
    cells = process_medians(os.path.join(results, name, "raw.tsv"))
    for arm in ("baseline", "candidate"):
        times = sorted(cells.get(("stencil", arm, "1"), []))
        print(f"  stencil W=1 {arm:9s} ms: " + " ".join(f"{t:.1f}" for t in times))


def grids(path):
    """The addresses of the grid-sized mappings a traced launch made, in order."""
    found, pending = [], {}
    with open(path) as lines:
        for line in lines:
            m = re.match(r"(\d+)\s+(.*)", line)
            if not m:
                continue
            pid, rest = m.groups()
            size = re.match(r"mmap\(NULL, (\d+),", rest)
            if size and GRID_LOW <= int(size.group(1)) <= GRID_HIGH:
                result = re.search(r"\) = (0x[0-9a-f]+)", rest)
                if result:
                    found.append(int(result.group(1), 16))
                else:
                    pending[pid] = True
            elif rest.startswith("<... mmap resumed>") and pending.pop(pid, False):
                result = re.search(r"= (0x[0-9a-f]+)", rest)
                if result:
                    found.append(int(result.group(1), 16))
    return found


def launch(results, name):
    """One traced launch: (distance between a timed call's two grids, ms)."""
    base = os.path.join(results, "trace", name)
    walls = []
    with open(base + ".tsv") as rows:
        for row in rows:
            f = row.split()
            if len(f) == 8 and f[4] != "0":
                walls.append(int(f[5]))
    found = grids(base + ".strace")
    if len(walls) != 5 or len(found) < 5:
        return None
    # The oracle's reference maps three grids first; each timed call maps two.
    return found[3] - found[4], statistics.median(walls) / 1e6, found[3], found[4]


def traces(results):
    directory = os.path.join(results, "trace")
    if not os.path.isdir(directory):
        return
    series = {}
    for entry in os.listdir(directory):
        m = re.match(r"(.+)-(\d+)\.tsv$", entry)
        if m:
            series.setdefault(m.group(1), []).append(int(m.group(2)))
    fixed = {}
    for name in sorted(series):
        rows = [launch(results, f"{name}-{i}") for i in sorted(series[name])]
        good = [r for r in rows if r]
        m = re.match(r"(.+)-fixed-(\d+)$", name)
        if m:
            fixed.setdefault(m.group(1), {})[int(m.group(2))] = good[0] if good else None
            continue
        print(f"traced {name}: {len(good)} of {len(rows)} launches read")
        by_distance = {}
        for distance, ms, _, _ in good:
            by_distance.setdefault(distance, []).append(ms)
        for distance, times in sorted(by_distance.items(), key=lambda item: -len(item[1])):
            times.sort()
            print(f"  A-B={distance:+#x} n={len(times)} median={statistics.median(times):.1f} ms: "
                  + " ".join(f"{t:.1f}" for t in times))
    for revision in sorted(fixed):
        print(f"fixed addresses, {revision}: pad of k * 2 MiB -> A-B, ms")
        for k in sorted(fixed[revision]):
            row = fixed[revision][k]
            if row:
                print(f"  k={k:2d} A-B={row[0]:+#x} {row[1]:.1f} A={row[2]:#x} B={row[3]:#x}")
            else:
                print(f"  k={k:2d} not read")


def main():
    results = sys.argv[1]
    for name in ("host.txt", "statuses.txt", "alias.txt", "datapad.txt"):
        path = os.path.join(results, name)
        if os.path.exists(path):
            print(f"-- {name}")
            print(open(path).read().rstrip())
    names = sorted(entry for entry in os.listdir(results)
                   if os.path.isdir(os.path.join(results, entry)) and entry not in ("arms", "trace"))
    for name in names:
        campaign(results, name)
    traces(results)


if __name__ == "__main__":
    main()
