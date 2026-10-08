"""Reduces raw.tsv of run.sh to medians, spreads and paired ratios.

Why a script and not shell: medians and paired per-round ratios are awkward in
awk, and python3 is already a prerequisite of the other experiment harnesses.
It reads only the TSV, prints text, and decides nothing: the thresholds it
prints beside the ratios are the criterion's, copied from README.md.
"""

import statistics
import sys
from collections import defaultdict

BUILDS = ["seq", "par8", "twin", "par1"]
THRESHOLD = 2.0


def main(path):
    rows = defaultdict(dict)
    with open(path, encoding="utf-8") as handle:
        next(handle)
        for line in handle:
            cells, rnd, build, _workers, wall, hist, _sum = line.rstrip("\n").split("\t")
            rows[(int(cells), build)][int(rnd)] = (int(wall), int(hist))
    cell_counts = sorted({cells for cells, _ in rows})
    for cells in cell_counts:
        print(f"== {cells} cells ==")
        print(f"{'build':6} {'n':>3} {'hist med ms':>12} {'min':>9} {'max':>9} {'spread%':>8}"
              f" {'wall med ms':>12} {'min':>9} {'max':>9} {'spread%':>8}")
        for build in BUILDS:
            samples = rows.get((cells, build))
            if not samples:
                continue
            parts = []
            for index in (1, 0):
                values = [value[index] / 1e6 for value in samples.values()]
                median = statistics.median(values)
                spread = 100.0 * (max(values) - min(values)) / median
                parts.append((median, min(values), max(values), spread))
            hist, wall = parts
            print(f"{build:6} {len(samples):>3} {hist[0]:>12.3f} {hist[1]:>9.3f} {hist[2]:>9.3f} {hist[3]:>8.1f}"
                  f" {wall[0]:>12.3f} {wall[1]:>9.3f} {wall[2]:>9.3f} {wall[3]:>8.1f}")
        print()
        print(f"paired per-round ratios (seq divided by the build; above 1 means the build is faster)")
        print(f"{'ratio':10} {'hist median':>12} {'min':>8} {'max':>8} {'wall median':>12} {'min':>8} {'max':>8}")
        for build in BUILDS[1:]:
            base = rows.get((cells, "seq"), {})
            other = rows.get((cells, build), {})
            common = sorted(set(base) & set(other))
            if not common:
                continue
            cols = []
            for index in (1, 0):
                ratios = [base[r][index] / other[r][index] for r in common]
                cols.append((statistics.median(ratios), min(ratios), max(ratios)))
            hist, wall = cols
            print(f"seq/{build:6} {hist[0]:>12.3f} {hist[1]:>8.3f} {hist[2]:>8.3f}"
                  f" {wall[0]:>12.3f} {wall[1]:>8.3f} {wall[2]:>8.3f}")
        print()
    print(f"criterion (README.md): seq/par8 on the histogram interval at least {THRESHOLD}"
          " at 256 cells; a seq/par8 below 1 at 4096 cells rejects lowering A.")
    print("The seq/twin row is the noise control: a seq/par8 within the seq/twin range is not a measured effect.")


if __name__ == "__main__":
    main(sys.argv[1])
