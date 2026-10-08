"""Reduces paired process wall times to per-histogram medians and ratios.

Why a script and not shell: medians and paired per-round ratios are awkward in
awk, and python3 is already a prerequisite of the other experiment harnesses.
It reads only the TSV, prints text, and decides nothing: the thresholds it
prints beside the ratios are the criterion's, copied from README.md.
"""

import csv
import statistics
import sys
from collections import defaultdict

BUILDS = ["seq", "par8", "twin", "par1"]
THRESHOLD = 2.0
FIELDS = ["cells", "round", "build", "workers", "k", "wall_ns", "checksum"]


def read_pairs(path):
    runs = defaultdict(dict)
    repetitions = set()
    with open(path, encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        if reader.fieldnames != FIELDS:
            raise ValueError(f"expected TSV columns: {' '.join(FIELDS)}")
        for row in reader:
            cells, rnd = int(row["cells"]), int(row["round"])
            build, k = row["build"], int(row["k"])
            wall = int(row["wall_ns"])
            if cells not in (256, 4096) or rnd < 0 or build not in BUILDS or k < 0 or wall <= 0:
                raise ValueError(f"invalid run at TSV line {reader.line_num}")
            key = (cells, rnd, build)
            if k in runs[key]:
                raise ValueError(f"duplicate run: {key}, K={k}")
            runs[key][k] = wall
            if k:
                repetitions.add(k)
    if len(repetitions) != 1:
        raise ValueError("expected one positive REPS value shared by every pair")
    reps = repetitions.pop()
    cell_counts = sorted({cells for cells, _rnd, _build in runs})
    if cell_counts != [256, 4096]:
        raise ValueError("expected paired runs for both 256 and 4096 cells")
    rounds = sorted({rnd for _cells, rnd, _build in runs})
    if rounds != list(range(len(rounds))):
        raise ValueError("expected consecutive rounds starting at zero")
    samples = defaultdict(dict)
    for cells in cell_counts:
        for rnd in rounds:
            for build in BUILDS:
                pair = runs.get((cells, rnd, build), {})
                if set(pair) != {0, reps}:
                    raise ValueError(f"missing K=0/K={reps} pair: {cells} cells, round {rnd}, {build}")
                samples[(cells, build)][rnd] = (pair[reps] - pair[0]) / reps
    return cell_counts, rounds, reps, samples


def describe(values):
    median = statistics.median(values)
    low, high = min(values), max(values)
    spread = f"{100.0 * (high - low) / median:.1f}" if median > 0 else "n/a"
    return median, low, high, spread


def main(path):
    cell_counts, rounds, reps, samples = read_pairs(path)
    print(f"per-histogram ns = (wall_ns(K={reps}) - wall_ns(K=0)) / {reps}")
    for cells in cell_counts:
        print(f"== {cells} cells ==")
        print(f"{'build':6} {'n':>3} {'hist med ms':>12} {'min':>9} {'max':>9} {'spread%':>8}")
        for build in BUILDS:
            values = [samples[(cells, build)][rnd] / 1e6 for rnd in rounds]
            median, low, high, spread = describe(values)
            print(f"{build:6} {len(values):>3} {median:>12.3f} {low:>9.3f} {high:>9.3f} {spread:>8}")
            unresolved = [rnd for rnd in rounds if samples[(cells, build)][rnd] <= 0]
            if unresolved:
                print(f"  {build}: nonpositive paired wall difference in rounds {unresolved};"
                      " the repetition cost is unresolved.")
        print()
        print("paired per-round ratios (seq divided by the build; above 1 means the build is faster)")
        print(f"{'ratio':10} {'hist median':>12} {'min':>8} {'max':>8} {'spread%':>8}")
        for build in BUILDS[1:]:
            base, other = samples[(cells, "seq")], samples[(cells, build)]
            unresolved = [rnd for rnd in rounds if base[rnd] <= 0 or other[rnd] <= 0]
            if unresolved:
                print(f"seq/{build:6} unavailable: nonpositive paired wall difference in rounds {unresolved}")
                continue
            ratios = [base[rnd] / other[rnd] for rnd in rounds]
            median, low, high, spread = describe(ratios)
            print(f"seq/{build:6} {median:>12.3f} {low:>8.3f} {high:>8.3f} {spread:>8}")
        print()
    print(f"criterion (README.md): seq/par8 on baseline-subtracted process wall time per histogram"
          f" at least {THRESHOLD} at 256 cells; a seq/par8 below 1 at 4096 cells rejects lowering A.")
    print("The seq/twin row is the noise control: a seq/par8 within the seq/twin range is not a measured effect.")


if __name__ == "__main__":
    try:
        main(sys.argv[1])
    except ValueError as error:
        sys.exit(f"invalid timing data: {error}")
