"""Paired-ratio statistics and verdict rules of the call-grain-args investigation.

Every rule here is the one the investigation's README states; nothing reads a
result to choose a threshold. Pure functions, covered by selftest.py.
"""
import math
import random
import statistics
from collections import defaultdict

SEED = 20261010
RESAMPLES = 10000
VOID_SPREAD = 0.03
MIN_ROUNDS = 4


def balanced_order(items, round_id, offset=0):
    """Rotate once per round pair, then visit the rotation in both directions."""
    items = list(items)
    if not items or len(set(items)) != len(items) or round_id < 0:
        raise ValueError("balanced order needs distinct items and a nonnegative round")
    shift = (round_id // 2 + offset) % len(items)
    ordered = items[shift:] + items[:shift]
    return ordered[::-1] if round_id % 2 else ordered


def median_ci(values, seed=SEED, resamples=RESAMPLES):
    """(median, low, high): 95% percentile interval of bootstrap medians."""
    values = list(values)
    if len(values) < MIN_ROUNDS:
        raise ValueError(f"need at least {MIN_ROUNDS} paired rounds, have {len(values)}")
    rng = random.Random(seed)
    n = len(values)
    medians = sorted(statistics.median(rng.choices(values, k=n)) for _ in range(resamples))
    return (statistics.median(values), medians[int(0.025 * resamples)], medians[int(0.975 * resamples) - 1])


def read_rows(path):
    """raw.tsv columns: attempt round cell arm width sample wall cpu count grants."""
    rows = []
    with open(path) as handle:
        for line in handle:
            f = line.rstrip("\n").split("\t")
            if len(f) != 10:
                raise ValueError(f"malformed raw row: {line!r}")
            rows.append(dict(attempt=int(f[0]), round=int(f[1]), cell=f[2], arm=f[3], width=int(f[4]),
                             sample=int(f[5]), wall=int(f[6]), cpu=int(f[7]), count=int(f[8]), grants=int(f[9])))
    return rows


class Table:
    """Per (attempt, cell, arm, width, round): median wall of the timed samples."""

    def __init__(self, rows):
        samples = defaultdict(list)
        self.grants = defaultdict(int)
        for r in rows:
            key = (r["attempt"], r["cell"], r["arm"], r["width"], r["round"])
            if r["sample"] >= 1:
                samples[key].append(r["wall"])
                self.grants[key] += r["grants"]
        self.wall = {k: statistics.median(v) for k, v in samples.items()}

    def cells(self):
        return sorted({k[1] for k in self.wall})

    def attempts(self, cell):
        return sorted({k[0] for k in self.wall if k[1] == cell})

    def ratios(self, attempt, cell, num, den):
        rounds = sorted({k[4] for k in self.wall if k[0] == attempt and k[1] == cell})
        out = []
        for rnd in rounds:
            a = self.wall.get((attempt, cell, num[0], num[1], rnd))
            b = self.wall.get((attempt, cell, den[0], den[1], rnd))
            if a is not None and b is not None and b > 0:
                out.append(a / b)
        return out

    def ci(self, attempt, cell, num, den):
        return median_ci(self.ratios(attempt, cell, num, den))

    def mean_grants(self, attempt, cell, arm, width):
        values = [v for k, v in self.grants.items() if k[0] == attempt and k[1] == cell and k[2] == arm and k[3] == width]
        return sum(values) / len(values) if values else None

    def seconds(self, attempt, cell, arm, width):
        values = [v for k, v in self.wall.items() if k[0] == attempt and k[1] == cell and k[2] == arm and k[3] == width]
        return statistics.median(values) / 1e9 if values else None


def twin_spread(twin_ci):
    """Largest deviation from 1 of the twin ratio's 95% interval."""
    _, low, high = twin_ci
    return max(high - 1.0, 1.0 - low)


def judge_upper(ci, twin_ci, bound, not_slower=False):
    """State of one attempt for a criterion on the ratio's 95% interval (median, low, high).

    void when the twin's spread exceeds VOID_SPREAD (the cell cannot decide).
    not_slower=True is the rule "not slower than the reference": the cell is
    slower (fail) exactly when the lower end lies above bound + twin spread,
    and passes otherwise. not_slower=False is "the upper end is at most
    bound": pass then, fail when the lower end is already above the bound,
    unresolved in between.
    """
    spread = twin_spread(twin_ci)
    if spread > VOID_SPREAD:
        return "void", spread
    _, low, high = ci
    if not_slower:
        return ("fail" if low > bound + spread else "pass"), spread
    if high <= bound:
        return "pass", spread
    if low > bound:
        return "fail", spread
    return "unresolved", spread


def combine(first, second):
    """Final state of a cell from its attempt-1 and (optional) attempt-2 states.

    pass on attempt 1 stands. A cell that did not pass is measured once more:
    a pass the second time disagrees with the first, so the cell is
    inconclusive; fail the second time is fail; anything else stays
    unresolved, or inconclusive after a first fail.
    """
    if "void" in (first, second):
        return "void"
    if first == "pass" or second is None:
        return first
    if second == "pass":
        return "inconclusive"
    if second == "fail":
        return "fail"
    return "inconclusive" if first == "fail" else "unresolved"


def project_rounds(spread, rounds, target=0.01, low=10, high=30):
    """Even round count at which the twin spread is projected to reach target."""
    needed = math.ceil(rounds * (spread / target) ** 2)
    needed = max(low, min(high, needed))
    return needed + needed % 2
