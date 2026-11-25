#!/usr/bin/env python3
"""Reduce one host's results of probe.sh to the tables the criterion reads.

usage: reduce.py RESULTS

For every campaign it prints each kernel and width's paired ratio with the
regression rule applied in both directions, and stencil's process medians at
one worker per arm. For every traced series it prints, per distance between
the two grids of a timed call, how many launches got it and their times. Then
it pools the launches made with address randomization by distance class,
applies the position rule to the launches made without it, compares two arms'
positions one by one and in order of time, and compares the revisions' mapping
calls in those launches.
"""

import os
import re
import statistics
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

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


def distance_class(distance):
    if distance == 0x2001000:
        return "adjacent, +0x2001000"
    if abs(distance) == 0x7FFF000:
        return "0x7fff000 either way"
    if 0x6000000 <= distance < 0x7FFF000:
        return "+0x61ff000 to +0x7dff000"
    if 0x46000000 <= distance <= 0x49000000:
        return "+0x46000000 to +0x49000000"
    return "another distance"


def pooled(results):
    """Every launch made with address randomization, by distance class."""
    directory = os.path.join(results, "trace")
    classes = {}
    for entry in sorted(os.listdir(directory)):
        m = re.match(r"([a-z]+-(?:none|thread|arena1))-(\d+)\.tsv$", entry)
        row = launch(results, entry[:-4]) if m else None
        if row:
            classes.setdefault(distance_class(row[0]), []).append(row[1])
    print("launches with address randomization, all series, by distance class")
    for name, times in sorted(classes.items(), key=lambda item: -len(item[1])):
        times.sort()
        print(f"  {name:28s} n={len(times):3d} min={times[0]:6.1f} "
              f"median={statistics.median(times):6.1f} max={times[-1]:6.1f} ms")


def positions(results):
    """The position rule: a position is off when its launch is 10 percent or
    more from the median position; b and h, which map alike, act as two rounds."""
    off = {}
    print("positions without address randomization, 10 percent or more from the median position")
    for revision in "bhs":
        times = {}
        for k in range(32):
            row = launch(results, f"{revision}-fixed-{k}-0") \
                if os.path.exists(os.path.join(results, "trace", f"{revision}-fixed-{k}-0.tsv")) else None
            if row:
                times[k] = row[1]
        if not times:
            continue
        middle = statistics.median(times.values())
        off[revision] = {k: t / middle for k, t in times.items() if t >= 1.10 * middle or t <= 0.90 * middle}
        rest = [t / middle for k, t in times.items() if k not in off[revision]]
        print(f"  {revision}: {len(times)} positions read, off: "
              + (" ".join(f"{k}:{ratio:.2f}" for k, ratio in sorted(off[revision].items())) or "none")
              + f"; the others {min(rest):.3f} to {max(rest):.3f}")
    if "b" in off and "h" in off:
        both = sorted(k for k in off["b"] if k in off["h"] and (off["b"][k] > 1) == (off["h"][k] > 1))
        print(f"  off the same way in b and in h: {both or 'none'}")


def fixed_times(results):
    """revision -> position -> ms, for the launches without address randomization."""
    times = {}
    for revision in "bhs":
        times[revision] = {}
        for k in range(32):
            if os.path.exists(os.path.join(results, "trace", f"{revision}-fixed-{k}-0.tsv")):
                row = launch(results, f"{revision}-fixed-{k}-0")
                if row:
                    times[revision][k] = row[1]
    return times


def unlike(ratio):
    return ratio >= 1.10 or ratio <= 1 / 1.10


def profiles(results):
    """Two arms' positions compared: each arm's time at a position over the
    arm's median position, and the positions where the two differ by 10 percent
    or more. A slowdown of one arm at every position leaves the ratios at one."""
    times = fixed_times(results)
    print("positions where two arms' profiles differ by 10 percent or more")
    for first, second in ("bh", "bs", "hs"):
        if len(times[first]) < 32 or len(times[second]) < 32:
            continue
        middle = {arm: statistics.median(times[arm].values()) for arm in (first, second)}
        ratios = {k: (times[second][k] / middle[second]) / (times[first][k] / middle[first])
                  for k in range(32)}
        differ = {k: r for k, r in ratios.items() if unlike(r)}
        print(f"  {second} over {first}: {len(differ)} of 32"
              + "".join(f" {k}:{r:.2f}" for k, r in sorted(differ.items()))
              + f"; all ratios {min(ratios.values()):.3f} to {max(ratios.values()):.3f}")


def ranks(results):
    """Two arms' positions compared in order of time: each arm's times over its
    median position, sorted, and the ranks where the two differ by 10 percent or
    more, which does not read the position a time came from. Then b against h
    moved by r positions, one position against one: h maps as b does, so h read
    from position k + r is a reading of the image b would be with r times 2 MiB
    more mapped before the entry."""
    times = fixed_times(results)
    print("positions in order of time, ranks where two arms differ by 10 percent or more")
    for first, second in ("bh", "bs", "hs"):
        if len(times[first]) < 32 or len(times[second]) < 32:
            continue
        ordered = {}
        for arm in (first, second):
            middle = statistics.median(times[arm].values())
            ordered[arm] = sorted(t / middle for t in times[arm].values())
        ratios = [ordered[second][i] / ordered[first][i] for i in range(32)]
        differ = [str(i + 1) for i, r in enumerate(ratios) if unlike(r)]
        print(f"  {second} over {first}: {len(differ)} of 32"
              + (" at " + " ".join(differ) if differ else "")
              + f"; all ratios {min(ratios):.3f} to {max(ratios):.3f}")
    if len(times["b"]) == 32 and len(times["h"]) == 32:
        middle = {arm: statistics.median(times[arm].values()) for arm in "bh"}
        counts = []
        for r in range(1, 32):
            counts.append(sum(unlike((times["h"][(k + r) % 32] / middle["h"]) / (times["b"][k] / middle["b"]))
                              for k in range(32)))
        print("positions unlike one against one, h moved by r positions over b, r = 1 to 31")
        print("  " + " ".join(str(c) for c in counts) + f"; least {min(counts)}, most {max(counts)}")


def own_calls(path):
    """A launch's own mapping calls without address randomization: the mmap and
    munmap calls whose addresses lie in 0x7fff00000000 and above, joined across
    interrupted lines, up to the last grid mapped; the launcher tools' own calls
    before it are randomized and lie elsewhere."""
    calls, pending = [], {}
    with open(path) as lines:
        for line in lines:
            m = re.match(r"(\d+)\s+(.*)", line.rstrip("\n"))
            if not m:
                continue
            pid, rest = m.groups()
            if rest.endswith("<unfinished ...>"):
                pending[pid] = rest[:-len("<unfinished ...>")]
                continue
            resumed = re.match(r"<\.\.\. \w+ resumed>(.*)", rest)
            if resumed and pid in pending:
                rest = pending.pop(pid) + resumed.group(1)
            if re.match(r"(mmap|munmap)\(", rest) and re.search(r"0x7fff[0-9a-f]{8}", rest):
                calls.append(re.sub(r"\s+", " ", rest))
    last = max((i for i, call in enumerate(calls) if call.startswith("mmap(NULL, 3355")), default=-1)
    return calls[:last + 1]


def layouts(results):
    counts = {"b and h": [0, 0], "b and s": [0, 0]}
    for k in range(32):
        paths = {r: os.path.join(results, "trace", f"{r}-fixed-{k}-0.strace") for r in "bhs"}
        if not all(os.path.exists(path) for path in paths.values()):
            continue
        calls = {r: own_calls(path) for r, path in paths.items()}
        for pair, other in (("b and h", "h"), ("b and s", "s")):
            counts[pair][calls["b"] == calls[other] and bool(calls["b"])] += 1
    print("mapping calls without address randomization, position by position")
    for pair, (differ, same) in counts.items():
        print(f"  {pair}: identical at {same} positions, different at {differ}")


MODELLED = {"b-none": "entry", "h-none": "entry", "s-none": "thread-then-entry",
            "b-thread": "thread-then-entry", "h-thread": "thread-then-entry",
            "b-arena1": "entry-one-heap", "h-arena1": "entry-one-heap",
            "s-arena1": "thread-then-entry-one-heap"}


def model(results):
    """layout.py against each launch made with address randomization: given the
    launch's C library base and thread stack size, does it give the traced
    distance between the grids to within 256 KiB? Per startup order, with the
    traced distance classes."""
    import layout
    agree, classes = {}, {}
    directory = os.path.join(results, "trace")
    for entry in sorted(os.listdir(directory)):
        m = re.match(r"([a-z]+-(?:none|thread|arena1))-(\d+)\.strace$", entry)
        if not m or m.group(1) not in MODELLED:
            continue
        order = MODELLED[m.group(1)]
        library, stacks = None, []
        with open(os.path.join(directory, entry)) as lines:
            for line in lines:
                file_map = re.search(r"mmap\(NULL, (\d+), PROT_READ, MAP_PRIVATE\|MAP_DENYWRITE, \d+, 0\) = (0x[0-9a-f]+)", line)
                # The last such mapping is the program's own C library: a
                # launch through env maps env's first, elsewhere.
                if file_map and int(file_map.group(1)) >= 2 << 20:
                    library = int(file_map.group(2), 16)
                stack = re.search(r"mmap\(NULL, (\d+), PROT_NONE, MAP_PRIVATE\|MAP_ANONYMOUS\|MAP_STACK", line)
                if stack and int(stack.group(1)) < 1 << 30:
                    stacks.append(int(stack.group(1)))
        found = grids(os.path.join(directory, entry))
        if library is None or len(found) < 5:
            continue
        traced = found[3] - found[4]
        a, b = layout.launch(library, order, **({"thread_stack": stacks[0]} if stacks else {}))
        agree.setdefault(order, [0, 0])[abs(a - b - traced) <= 0x40000] += 1
        classes.setdefault(order, {}).setdefault(distance_class(traced), 0)
        classes[order][distance_class(traced)] += 1
    print("layout.py against the launches with address randomization, per startup order")
    for order, (wrong, right) in agree.items():
        shares = ", ".join(f"{name} {count}" for name, count in
                           sorted(classes[order].items(), key=lambda item: -item[1]))
        print(f"  {order}: distance given in {right} of {right + wrong} launches; traced: {shares}")


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
    if os.path.isdir(os.path.join(results, "trace")):
        pooled(results)
        model(results)
        positions(results)
        profiles(results)
        ranks(results)
        layouts(results)


if __name__ == "__main__":
    main()
