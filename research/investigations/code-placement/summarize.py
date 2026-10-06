#!/usr/bin/env python3
"""Reduce placement.sh's raw.tsv and apply the criterion fixed in DESIGN.md.

usage: summarize.py RAW_TSV

A process's time is the median of its five recorded calls; a cell (arm,
placement, kernel, width) is the median of its processes over the rounds. A
comparison of two cells is paired by round: its ratio is the median of the
per-round ratios, and it is adverse when that median is below 0.97 and at
least 80 percent of the rounds lean the same way, the regression gate's own
threshold. Prints the cells, then one verdict line per criterion.
"""

import collections
import statistics
import sys

ARMS = ("U", "F", "FR", "FRL")
PLACEMENTS = ("p0", "m16", "m32", "m48", "r16", "r48")
MODULE_PLACEMENTS = ("p0", "m16", "m32", "m48")
KERNELS = ("mandelbrot", "records", "fir", "quadrature", "stencil")
WIDTHS = (1, 2, 4)


def load(path):
    calls = collections.defaultdict(list)
    for number, line in enumerate(open(path, encoding="utf-8"), 1):
        fields = line.split()
        if len(fields) != 8:
            sys.exit(f"REFUSED: row {number} has {len(fields)} fields")
        arm, placement, round_, kernel, width, call, wall, _ = fields
        if int(call) == 0:
            continue
        calls[(arm, placement, kernel, int(width), int(round_))].append(int(wall))
    process = {}
    for key, values in calls.items():
        if len(values) != 5:
            sys.exit(f"REFUSED: {key} has {len(values)} recorded calls")
        process[key] = statistics.median(values)
    rounds = sorted({key[4] for key in process})
    for arm in ARMS + ("null",):
        for placement in PLACEMENTS if arm != "null" else ("p0",):
            for kernel in KERNELS:
                for width in WIDTHS:
                    for round_ in rounds:
                        if (arm, placement, kernel, width, round_) not in process:
                            sys.exit(f"REFUSED: missing {arm} {placement} {kernel} W={width} round {round_}")
    return process, rounds


def series(process, rounds, arm, placements, kernel, width):
    """Per round, the median over the given placements of one arm."""
    return [statistics.median(process[(arm, p, kernel, width, r)] for p in placements)
            for r in rounds]


def paired(reference, candidate):
    """Median of reference/candidate per round and the share of rounds below 1."""
    ratios = [a / b for a, b in zip(reference, candidate)]
    return statistics.median(ratios), sum(r < 1 for r in ratios) / len(ratios), \
        sum(r > 1 for r in ratios) / len(ratios)


def adverse(reference, candidate):
    ratio, slower, _ = paired(reference, candidate)
    return ratio < 0.97 and slower >= 0.8, ratio, slower


def main():
    process, rounds = load(sys.argv[1])
    print(f"rounds={len(rounds)}; cell = median over rounds of each process's median call (ms)")
    cell = {}
    for kernel in KERNELS:
        for width in WIDTHS:
            for arm in ARMS:
                for placement in PLACEMENTS:
                    cell[(arm, placement, kernel, width)] = statistics.median(
                        process[(arm, placement, kernel, width, r)] for r in rounds)
            cell[("null", "p0", kernel, width)] = statistics.median(
                process[("null", "p0", kernel, width, r)] for r in rounds)

    def spread(arm, placements, kernel, width):
        values = [cell[(arm, p, kernel, width)] for p in placements]
        return max(values) / min(values)

    def shifted(reference, candidate):
        """The gate's rule in either direction: a paired ratio beyond 3 percent
        with at least 80 percent of the rounds leaning that way."""
        ratio, slower, faster = paired(reference, candidate)
        return (ratio < 0.97 and slower >= 0.8) or (ratio > 1 / 0.97 and faster >= 0.8), ratio

    def inconclusive(kernel, width):
        """C0: the identical-image null moved this cell, so the host cannot
        resolve it in this run."""
        return shifted([process[("U", "p0", kernel, width, r)] for r in rounds],
                       [process[("null", "p0", kernel, width, r)] for r in rounds])[0]

    def invariant(arm, placements, kernel, width):
        """C2: no placement moves the arm's time against its p0 by the gate's rule."""
        reference = [process[(arm, "p0", kernel, width, r)] for r in rounds]
        problems = []
        for placement in placements[1:]:
            moved, ratio = shifted(
                reference, [process[(arm, placement, kernel, width, r)] for r in rounds])
            if moved:
                problems.append(f"{placement} against p0 {ratio:.3f}")
        return "; ".join(problems) or None

    for kernel in KERNELS:
        for width in WIDTHS:
            print(f"\n{kernel} W={width}  null U-p0/null-p0 "
                  f"{cell[('U', 'p0', kernel, width)] / 1e6:.3f}/{cell[('null', 'p0', kernel, width)] / 1e6:.3f}")
            for arm in ARMS:
                values = " ".join(f"{p}={cell[(arm, p, kernel, width)] / 1e6:7.3f}" for p in PLACEMENTS)
                print(f"  {arm:4s} {values}  spread {spread(arm, PLACEMENTS, kernel, width):.3f}")

    noisy = [f"{k} W={w}" for k in KERNELS for w in WIDTHS if inconclusive(k, w)]
    print(f"\nC0 host control: the null moved {len(noisy)} of {len(KERNELS) * len(WIDTHS)} cells"
          + (f" ({', '.join(noisy)})" if noisy else ""))
    print("\nC1 discrimination: U spread over all placements >= 1.10 somewhere")
    worst_u = max(((spread("U", PLACEMENTS, k, w), k, w) for k in KERNELS for w in WIDTHS))
    c1 = worst_u[0] >= 1.10
    print(f"  {'MET' if c1 else 'NOT MET'}: largest U spread {worst_u[0]:.3f} ({worst_u[1]} W={worst_u[2]})")

    def verdict(title, arm, placements, cost_reference, cost_placements):
        print(f"\n{title}")
        failures = skipped = 0
        for kernel in KERNELS:
            for width in WIDTHS:
                if inconclusive(kernel, width):
                    skipped += 1
                    print(f"  {kernel} W={width}: inconclusive, the null control moved")
                    continue
                problem = invariant(arm, placements, kernel, width)
                reference = series(process, rounds, cost_reference, cost_placements, kernel, width)
                candidate = series(process, rounds, arm, cost_placements, kernel, width)
                bad, ratio, slower = adverse(reference, candidate)
                if bad:
                    problem = (problem + "; " if problem else "") + \
                        f"cost {ratio:.3f} against {cost_reference} ({slower:.0%} of rounds slower)"
                if problem:
                    failures += 1
                    print(f"  {kernel} W={width}: {problem}")
                else:
                    print(f"  {kernel} W={width}: ok (cost ratio {ratio:.3f} against {cost_reference})")
        print(f"  {'MET' if failures == 0 else 'NOT MET'}: {failures} kernel/width cell(s) fail, "
              f"{skipped} inconclusive")
        return failures == 0

    verdict("Q1 F: invariant over module placements, no cost against U",
            "F", MODULE_PLACEMENTS, "U", MODULE_PLACEMENTS)
    verdict("Q2 FR: invariant over every placement, no cost against F",
            "FR", PLACEMENTS, "F", PLACEMENTS)
    print("\nQ3 FRL against FR: loop alignment stays refused unless FRL is invariant and,")
    print("  in some kernel at two or more widths, faster than FR by the gate's rule with no cell slower")
    faster = collections.Counter()
    slower_cells = 0
    for kernel in KERNELS:
        for width in WIDTHS:
            reference = series(process, rounds, "FR", PLACEMENTS, kernel, width)
            candidate = series(process, rounds, "FRL", PLACEMENTS, kernel, width)
            bad, ratio, _ = adverse(reference, candidate)
            gain, _, _ = adverse(candidate, reference)
            slower_cells += bad
            faster[kernel] += gain
            print(f"  {kernel} W={width}: FR/FRL ratio {ratio:.3f}{' slower' if bad else ''}{' faster' if gain else ''}")
    frl_invariant = all(invariant("FRL", PLACEMENTS, k, w) is None
                        for k in KERNELS for w in WIDTHS if not inconclusive(k, w))
    selects = frl_invariant and slower_cells == 0 and any(count >= 2 for count in faster.values())
    print(f"  {'SELECTS loop alignment' if selects else 'does not select loop alignment'} "
          f"(invariant: {frl_invariant}, slower cells: {slower_cells})")


if __name__ == "__main__":
    main()
