#!/usr/bin/env python3
"""Reduces run.sh's raw.tsv to the reading registered in
research/investigations/handle-count/README.md.

    python3 summarize.py [--smoke | --pilot] RAW.tsv

It prints the per-arm rates of every block, the per-block ratios, and under
the registered rule: the twin deviation that voids a block, the median and the
95% percentile bootstrap interval of V/R (PRV / PRR) over the blocks that stay,
the falsifier BV / PRR, and the verdict. --smoke and --pilot print the
readings and no verdict (a smoke runs on a hosted runner; a pilot has too few
blocks).
"""
import csv
import random
import statistics
import sys

TWIN_LIMIT = 0.03
RETENTION = 0.95
FALSIFIER_LIMIT = 0.95
MIN_BLOCKS = 6
DRAWS = 10000
SEED = 20261013
ARMS = ["PRV", "PRR", "BV", "PRR2"]
MEANING = {
    "PRV": "PR compiler, value handle (V)",
    "PRR": "PR compiler, read-only reference parameter (R)",
    "BV": "base compiler, value handle (the falsifier: counts the handle)",
    "PRR2": "second build of PRR's source (the twin, noise control)",
}


def read(path):
    blocks = {}
    with open(path, newline="") as f:
        for row in csv.DictReader(f, delimiter="\t"):
            blocks.setdefault(int(row["block"]), {})[row["arm"]] = float(row["ops_per_s"])
    return blocks


def bootstrap(values):
    """Median and 95% percentile bootstrap interval: DRAWS resamples with
    replacement by a generator seeded with SEED, the interval from the 251st
    to the 9750th sorted resample median."""
    rng = random.Random(SEED)
    n = len(values)
    medians = sorted(
        statistics.median([values[int(rng.random() * n)] for _ in range(n)])
        for _ in range(DRAWS))
    return (statistics.median(values), medians[DRAWS * 25 // 1000],
            medians[DRAWS * 975 // 1000 - 1])


def main(path, verdict_wanted):
    blocks = read(path)
    print("Arms:")
    for arm in ARMS:
        print(f"  {arm:5s} {MEANING[arm]}")
    print()
    print("block  " + "  ".join(f"{a:>12s}" for a in ARMS) + "   V/R     BV/PRR  PRR2/PRR-1  kept")
    kept = []
    falsifier = []
    twin = []
    incomplete = []
    for block in sorted(blocks):
        row = blocks[block]
        if any(a not in row for a in ARMS):
            incomplete.append(block)
            print(f"{block:5d}  incomplete: has {sorted(row)}")
            continue
        ratio = row["PRV"] / row["PRR"]
        fal = row["BV"] / row["PRR"]
        dev = row["PRR2"] / row["PRR"] - 1
        twin.append(abs(dev))
        keep = abs(dev) <= TWIN_LIMIT
        if keep:
            kept.append(ratio)
            falsifier.append(fal)
        print(f"{block:5d}  " + "  ".join(f"{row[a]:12.0f}" for a in ARMS)
              + f"   {ratio:.3f}   {fal:.3f}   {dev:+.3f}      {'yes' if keep else 'no (twin)'}")
    print()
    if not twin:
        print("no complete block")
        return 1
    print(f"twin |PRR2/PRR - 1|: median {statistics.median(twin):.3f}, largest {max(twin):.3f}; "
          f"a block above {TWIN_LIMIT} is void")
    print(f"blocks kept: {len(kept)} of {len(blocks)}"
          + (f" ({len(incomplete)} incomplete)" if incomplete else ""))
    reasons = []
    if incomplete:
        reasons.append(f"{len(incomplete)} block(s) lack an arm")
    if len(kept) < MIN_BLOCKS:
        reasons.append(f"{len(kept)} blocks kept, fewer than {MIN_BLOCKS}")
    if kept:
        median, low, high = bootstrap(kept)
        fal_median = statistics.median(falsifier)
        print(f"V/R = PRV/PRR: median {median:.3f}, 95% bootstrap interval [{low:.3f}, {high:.3f}], n={len(kept)}")
        print(f"falsifier BV/PRR: median {fal_median:.3f} (must be at most {FALSIFIER_LIMIT})")
        if fal_median > FALSIFIER_LIMIT:
            reasons.append(f"falsifier invisible: BV/PRR median {fal_median:.3f} above {FALSIFIER_LIMIT}")
    else:
        median = None
    print()
    if not verdict_wanted:
        print("VERDICT: none (smoke or pilot run)")
        return 0
    if reasons:
        print("VERDICT: VOID -- " + "; ".join(reasons))
        return 1
    if median >= RETENTION:
        print(f"VERDICT: PASS -- median V/R {median:.3f} >= {RETENTION}")
        return 0
    print(f"VERDICT: FAIL -- median V/R {median:.3f} < {RETENTION}")
    return 1


if __name__ == "__main__":
    args = sys.argv[1:]
    wanted = True
    while args and args[0] in ("--smoke", "--pilot"):
        wanted = False
        args = args[1:]
    if len(args) != 1:
        sys.exit("usage: summarize.py [--smoke | --pilot] RAW.tsv")
    sys.exit(main(args[0], wanted))
