#!/usr/bin/env python3
"""Reduce placement.sh's raw.tsv and apply the criteria fixed in DESIGN.md.

usage: summarize.py RAW_TSV [--loop WIDTH=FEWEST:MOST ...]
       summarize.py fastest RAW_TSV KERNEL WIDTH ARM
       summarize.py self-test

A process's time is the median of its five recorded calls; a cell (arm,
placement, kernel, width) is the median of its processes over the rounds. The
arms, kernels, widths and placements are read from the rows, and every
(arm, placement) a kernel was given must have a row at each of its widths in
each round. A placement whose name starts with `d` moves data; every other
placement moves code, and an arm's series is, per round, the median over its
code placements.

Two series are compared by round. The ratio is the median of the per-round
ratios reference/candidate, below 1 when the candidate is slower, and the
gate's rule calls the candidate slower (faster) when that median is below
0.97 (above 1/0.97) with at least 80 percent of the rounds leaning that way.
The 95 percent interval of the median ratio comes from order statistics:
with n rounds, the k-th smallest and k-th largest ratios, k the largest rank
whose binomial(n, 1/2) tail stays at or under 2.5 percent on each side (the
3rd and 10th of 12 rounds, the 6th and 15th of 20). It assumes only that the
rounds are independent, and fewer than six rounds give none.

C0, the host control: a kernel and width where the byte-identical copy
`null` differs from `U` at `p0` by the gate's rule is inconclusive. C2: an
arm is invariant at a conclusive kernel and width when no code placement
moves it against its `p0` by the gate's rule, in either direction.

The readings of "The next runs" follow, each for this one host: the cost,
its attribution, the loop reading (the arms named by --loop for each width),
the data reading, the baseline reading and each rule's three conditions.
`fastest` prints the code placement of ARM with the lowest cell.
"""

import collections
import math
import statistics
import sys

ORDER = ("p0", "m16", "m32", "m48", "r16", "r48", "m1024", "m2048")
RULES = ("FR", "L32", "L64", "A32", "P")


def load(lines):
    calls = collections.defaultdict(list)
    for number, line in enumerate(lines, 1):
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
    given = collections.defaultdict(set)
    widths = collections.defaultdict(set)
    for arm, placement, kernel, width, _ in process:
        given[kernel].add((arm, placement))
        widths[kernel].add(width)
    for kernel in given:
        for arm, placement in given[kernel]:
            for width in widths[kernel]:
                for round_ in rounds:
                    if (arm, placement, kernel, width, round_) not in process:
                        sys.exit(f"REFUSED: missing {arm} {placement} {kernel} W={width} round {round_}")
        if ("null", "p0") not in given[kernel] or ("U", "p0") not in given[kernel]:
            sys.exit(f"REFUSED: {kernel} has no null or no U p0 image")
    return Data(process, rounds, given, {k: sorted(v) for k, v in widths.items()})


class Data:
    def __init__(self, process, rounds, given, widths):
        self.process, self.rounds, self.given, self.widths = process, rounds, given, widths
        self.kernels = sorted(given, key=lambda k: (k != "records", k))

    def arms(self, kernel):
        arms = sorted({arm for arm, _ in self.given[kernel]} - {"null"})
        return sorted(arms, key=lambda a: (a not in ("U", "UR", "F", "FR"), a))

    def has(self, kernel, arm, placement="p0"):
        return (arm, placement) in self.given[kernel]

    def code(self, kernel, arm):
        placements = {p for a, p in self.given[kernel] if a == arm and not p.startswith("d")}
        return sorted(placements, key=lambda p: (ORDER.index(p) if p in ORDER else len(ORDER), p))

    def data(self, kernel, arm):
        return sorted(p for a, p in self.given[kernel] if a == arm and p.startswith("d"))

    def at(self, arm, placement, kernel, width):
        return [self.process[(arm, placement, kernel, width, r)] for r in self.rounds]

    def series(self, arm, kernel, width):
        """Per round, the median over the arm's code placements."""
        placements = self.code(kernel, arm)
        return [statistics.median(self.process[(arm, p, kernel, width, r)] for p in placements)
                for r in self.rounds]

    def cell(self, arm, placement, kernel, width):
        return statistics.median(self.at(arm, placement, kernel, width))


def ratios(reference, candidate):
    return [a / b for a, b in zip(reference, candidate)]


def gate(reference, candidate):
    """The gate's rule: 'slower' or 'faster' for the candidate, else 'same';
    and the median ratio."""
    values = ratios(reference, candidate)
    ratio = statistics.median(values)
    if ratio < 0.97 and sum(v < 1 for v in values) / len(values) >= 0.8:
        return "slower", ratio
    if ratio > 1 / 0.97 and sum(v > 1 for v in values) / len(values) >= 0.8:
        return "faster", ratio
    return "same", ratio


def interval(reference, candidate):
    """The 95 percent order-statistic interval of the median of
    reference/candidate per round, or None when the rounds are too few."""
    values = sorted(ratios(reference, candidate))
    count = len(values)
    rank, tail = 0, 0.0
    while True:
        tail += math.comb(count, rank) / 2 ** count
        if tail > 0.025:
            break
        rank += 1
    if rank == 0:
        return None
    return values[rank - 1], values[count - rank]


def side(bounds):
    """'wholly slower' when the interval lies below 1, 'wholly faster' above
    it, 'spans 1' otherwise, and 'no interval' without one."""
    if bounds is None:
        return "no interval"
    if bounds[1] < 1:
        return "wholly slower"
    if bounds[0] > 1:
        return "wholly faster"
    return "spans 1"


def describe(reference, candidate):
    verdict, ratio = gate(reference, candidate)
    bounds = interval(reference, candidate)
    span = "" if bounds is None else f" ({bounds[0]:.3f} to {bounds[1]:.3f})"
    return f"{ratio:.3f}{span} {side(bounds)}, gate {verdict}"


class Readings:
    def __init__(self, data):
        self.d = data

    def inconclusive(self, kernel, width):
        """C0: the identical-image null moved this cell."""
        return gate(self.d.at("U", "p0", kernel, width), self.d.at("null", "p0", kernel, width))[0] != "same"

    def moved(self, arm, kernel, width):
        """C2's failures: each code placement that moves the arm against its p0."""
        reference = self.d.at(arm, "p0", kernel, width)
        problems = []
        for placement in self.d.code(kernel, arm)[1:]:
            verdict, ratio = gate(reference, self.d.at(arm, placement, kernel, width))
            if verdict != "same":
                problems.append(f"{placement} against p0 {ratio:.3f}")
        return problems

    def against_u(self, arm, kernel, width):
        return side(interval(self.d.series("U", kernel, width), self.d.series(arm, kernel, width)))

    def cost(self, width, kernel="records"):
        """'established' on this host when FR's interval against U lies wholly
        below 1."""
        found = self.against_u("FR", kernel, width)
        return "established" if found == "wholly slower" else f"not established ({found})"

    def loop(self, width, fewest, most, kernel="records"):
        """The loop reading at one width: 'holds' when the fewest-crossing arm
        is faster than the most-crossing arm by the gate's rule, 'rejected'
        when their interval spans 1 or lies below it, 'undetermined'
        otherwise; and whether it accounts for the cost."""
        reference, candidate = self.d.series(most, kernel, width), self.d.series(fewest, kernel, width)
        verdict, _ = gate(reference, candidate)
        found = side(interval(reference, candidate))
        if verdict == "faster":
            result = "holds"
        elif found in ("spans 1", "wholly slower"):
            result = "rejected"
        else:
            result = "undetermined"
        accounts = result == "holds" and fewest != "FR" and \
            self.against_u(fewest, kernel, width) != "wholly slower"
        return result, accounts

    def data_reading(self, kernel="records"):
        """'holds' when a data placement moves FR against its p0, or dFR moves
        U against its p0, by the gate's rule in a conclusive cell; 'rejected'
        when none does; 'not measured' without data placements."""
        moved, measured = [], False
        for width in self.d.widths[kernel]:
            if self.inconclusive(kernel, width):
                continue
            for arm in ("FR", "U"):
                for placement in self.d.data(kernel, arm):
                    measured = True
                    verdict, ratio = gate(self.d.at(arm, "p0", kernel, width),
                                          self.d.at(arm, placement, kernel, width))
                    if verdict != "same":
                        moved.append(f"{arm} {placement} W={width} {ratio:.3f}")
        if not measured:
            return "not measured", moved
        return ("holds" if moved else "rejected"), moved

    def baseline(self, width, kernel="records"):
        """At p0: 'holds' when FR is slower than UR by the gate's rule and not
        slower than U; 'rejected' when FR is not slower than UR; 'own cost'
        when FR is slower than both."""
        fr = self.d.at("FR", "p0", kernel, width)
        under_ur = gate(self.d.at("UR", "p0", kernel, width), fr)[0] == "slower"
        under_u = gate(self.d.at("U", "p0", kernel, width), fr)[0] == "slower"
        if not under_ur:
            return "rejected"
        return "own cost" if under_u else "holds"

    def rule(self, arm):
        """A rule's three conditions on this host: records' interval against U
        not wholly below 1 at any width; C2 in every conclusive cell of every
        kernel; and, for a rule other than FR, no kernel slower than under FR
        by the gate's rule."""
        cost = [w for w in self.d.widths["records"] if self.against_u(arm, "records", w) == "wholly slower"]
        moved, slower = [], []
        for kernel in self.d.kernels:
            if not self.d.has(kernel, arm):
                continue
            for width in self.d.widths[kernel]:
                if self.inconclusive(kernel, width):
                    continue
                moved += [f"{kernel} W={width}: {p}" for p in self.moved(arm, kernel, width)]
                if arm != "FR" and self.d.has(kernel, "FR"):
                    verdict, ratio = gate(self.d.series("FR", kernel, width), self.d.series(arm, kernel, width))
                    if verdict == "slower":
                        slower.append(f"{kernel} W={width} {ratio:.3f}")
        return cost, moved, slower


def report(data, loops):
    read = Readings(data)
    print(f"rounds={len(data.rounds)}; cell = median over rounds of each process's median call (ms)")
    for kernel in data.kernels:
        for width in data.widths[kernel]:
            print(f"\n{kernel} W={width}  null U-p0/null-p0 "
                  f"{data.cell('U', 'p0', kernel, width) / 1e6:.3f}/{data.cell('null', 'p0', kernel, width) / 1e6:.3f}")
            for arm in data.arms(kernel):
                placements = data.code(kernel, arm) + data.data(kernel, arm)
                values = " ".join(f"{p}={data.cell(arm, p, kernel, width) / 1e6:.3f}" for p in placements)
                print(f"  {arm:4s} {values}")

    cells = [(k, w) for k in data.kernels for w in data.widths[k]]
    noisy = [f"{k} W={w}" for k, w in cells if read.inconclusive(k, w)]
    print(f"\nC0 host control: the null moved {len(noisy)} of {len(cells)} cells"
          + (f" ({', '.join(noisy)})" if noisy else ""))
    print("\nC1 discrimination: some code placement of U differs from U p0 in a conclusive cell")
    for kernel, width in cells:
        if not read.inconclusive(kernel, width) and (problems := read.moved("U", kernel, width)):
            print(f"  {kernel} W={width}: {'; '.join(problems)}")

    print("\nEach arm against U: U's median over its code placements / the arm's, per round;"
          " below 1 the arm is slower")
    for kernel, width in cells:
        for arm in data.arms(kernel):
            if arm != "U":
                print(f"  {kernel} W={width} {arm}: "
                      f"{describe(data.series('U', kernel, width), data.series(arm, kernel, width))}")
    print("\nEach arm against FR, the same way")
    for kernel, width in cells:
        for arm in data.arms(kernel):
            if arm != "FR" and data.has(kernel, "FR"):
                print(f"  {kernel} W={width} {arm}: "
                      f"{describe(data.series('FR', kernel, width), data.series(arm, kernel, width))}")
    print("\nC2 invariance: code placements that move an arm against its p0 in a conclusive cell")
    for kernel, width in cells:
        if read.inconclusive(kernel, width):
            continue
        for arm in data.arms(kernel):
            if problems := read.moved(arm, kernel, width):
                print(f"  {kernel} W={width} {arm}: {'; '.join(problems)}")

    if "records" not in data.given:
        return
    widths = data.widths["records"]
    print("\nREADINGS on this host (records)")
    for width in widths:
        print(f"  cost W={width}: {read.cost(width)}")
    for width in widths:
        found = [f"{arm} {read.against_u(arm, 'records', width)}" for arm in ("F", "UR") if data.has("records", arm)]
        print(f"  attribution W={width}: against U, {', '.join(found) or 'F and UR not measured'}")
    for width in widths:
        if width in loops:
            fewest, most = loops[width]
            if data.has("records", fewest) and data.has("records", most):
                result, accounts = read.loop(width, fewest, most)
                print(f"  loop W={width}: {fewest} against {most}: {result}; "
                      f"{'accounts for the cost' if accounts else 'does not account for the cost'}")
                continue
        print(f"  loop W={width}: not given")
    result, moved = read.data_reading()
    print(f"  data: {result}" + (f" ({'; '.join(moved)})" if moved else ""))
    for width in widths:
        if width in (2, 4):
            if data.has("records", "UR"):
                print(f"  baseline W={width}: {read.baseline(width)}")
            else:
                print(f"  baseline W={width}: UR not measured")
    for arm in RULES:
        if not data.has("records", arm):
            continue
        cost, moved, slower = read.rule(arm)
        met = not cost and not moved and (arm == "FR" or not slower)
        print(f"  rule {arm}: {'meets' if met else 'fails'} its conditions on this host; "
              f"records wholly slower than U at W={cost or 'none'}; "
              f"moved: {'; '.join(moved) or 'none'}"
              + ("" if arm == "FR" else f"; slower than FR: {'; '.join(slower) or 'none'}"))


def parse_loops(arguments):
    loops = {}
    rest = list(arguments)
    while rest:
        flag = rest.pop(0)
        if flag != "--loop" or not rest:
            sys.exit(__doc__)
        for item in rest.pop(0).split():
            width, arms = item.split("=")
            fewest, most = arms.split(":")
            loops[int(width)] = (fewest, most)
    return loops


def fastest(data, kernel, width, arm):
    return min(data.code(kernel, arm), key=lambda p: data.cell(arm, p, kernel, width))


# --- self-test --------------------------------------------------------------

def synthetic(times, rounds, noise):
    """raw.tsv lines for {(arm, placement, kernel, width): base ms}, each
    round's time scaled by noise(arm, placement, kernel, width, round)."""
    lines = []
    for (arm, placement, kernel, width), base in times.items():
        for round_ in range(rounds):
            value = base * 1e6 * noise(arm, placement, kernel, width, round_)
            for call in range(6):
                lines.append(f"{arm}\t{placement}\t{round_}\t{kernel}\t{width}\t{call}\t{int(value)}\t0")
    return lines


def design(overrides=(), arms=("U", "UR", "F", "FR", "O48", "L32"), data_placements=True):
    times = {}
    for width in (1, 2, 4):
        for arm in arms:
            placements = ("p0", "m16", "m32", "m48", "r16", "r48")
            if arm not in ("U", "UR"):
                placements += ("m1024", "m2048")
            for placement in placements:
                times[(arm, placement, "records", width)] = 10.0
        times[("null", "p0", "records", width)] = 10.0
        if data_placements:
            for placement in ("d64", "d1024", "d2048"):
                times[("FR", placement, "records", width)] = 10.0
            times[("U", "dFR", "records", width)] = 10.0
    times.update(dict(overrides))
    return times


def wobble(arm, placement, kernel, width, round_):
    """A spread of 0.2 percent around 1 that every image shares in a round."""
    return 1 + (round_ % 5 - 2) * 0.001


def self_test():
    failures = []

    def expect(label, actual, wanted):
        if actual != wanted:
            failures.append(f"{label}: got {actual!r}, wanted {wanted!r}")

    slow_fr = {(("FR", p, "records", 2)): 11.0 for p in ("p0", "m16", "m32", "m48", "r16", "r48", "m1024", "m2048")}
    read = Readings(load(synthetic(design(slow_fr.items()), 12, wobble)))
    expect("an interval wholly below 1", read.cost(2), "not established")
    expect("no cost at another width", read.cost(1), "not established (spans 1)")
    expect("the baseline: FR slower than UR and U", read.baseline(2), "own cost")
    expect("the baseline rejected", read.baseline(4), "rejected")
    expect("FR fails its first condition", read.rule("FR")[0], [2])
    expect("a rule slower than FR nowhere", read.rule("L32")[2], [])
    expect("the loop reading holds", read.loop(2, "O48", "FR"), ("holds", True))
    expect("the loop reading rejected", read.loop(1, "O48", "FR"), ("rejected", False))

    def flip(arm, placement, kernel, width, round_):
        scale = (0.95 if round_ % 2 else 1.05) if arm == "FR" else 1
        return wobble(arm, placement, kernel, width, round_) * scale

    read = Readings(load(synthetic(design(), 12, flip)))
    expect("an interval spanning 1", read.against_u("FR", "records", 1), "spans 1")
    read = Readings(load(synthetic(design(slow_fr.items()), 5, wobble)))
    expect("fewer than six rounds", read.cost(2), "not established (no interval)")

    moving = {("L32", "m16", "records", 1): 11.0, ("FR", "d1024", "records", 1): 11.0,
              ("UR", "p0", "records", 2): 9.0, ("U", "p0", "records", 4): 9.0,
              ("null", "p0", "records", 4): 9.0, ("UR", "p0", "records", 4): 9.0}
    read = Readings(load(synthetic(design(moving.items()), 12, wobble)))
    expect("a C2 move", read.moved("L32", "records", 1)[0].startswith("m16 against p0 0.909"), True)
    expect("the rule that moved fails", read.rule("L32")[1] != [], True)
    expect("a data placement that moves", read.data_reading()[0], "holds")
    expect("the baseline holds", read.baseline(2), "holds")
    expect("the baseline: FR slower than U as well", read.baseline(4), "own cost")
    read = Readings(load(synthetic(design(), 12, wobble)))
    expect("no data placement moves", read.data_reading(), ("rejected", []))
    read = Readings(load(synthetic(design(data_placements=False), 12, wobble)))
    expect("no data placement measured", read.data_reading()[0], "not measured")
    noisy_null = {("null", "p0", "records", 1): 12.0, ("L32", "m16", "records", 1): 11.0}
    read = Readings(load(synthetic(design(noisy_null.items()), 12, wobble)))
    expect("an inconclusive cell is not read", read.rule("L32")[1], [])
    data = load(synthetic(design({("U", "m48", "records", 1): 9.0}.items()), 12, wobble))
    expect("fastest", fastest(data, "records", 1, "U"), "m48")

    lines = synthetic(design(), 12, wobble)
    missing = [line for line in lines if not line.startswith("L32\tm2048\t3\trecords\t2\t")]
    try:
        load(missing)
        failures.append("a missing row was not refused")
    except SystemExit as refusal:
        expect("a missing row", str(refusal), "REFUSED: missing L32 m2048 records W=2 round 3")

    for failure in failures:
        print(f"FAIL {failure}")
    if failures:
        sys.exit(f"summarize.py self-test: {len(failures)} failure(s)")
    print("summarize.py self-test: all cases pass")


def main(arguments):
    if arguments == ["self-test"]:
        self_test()
    elif len(arguments) == 5 and arguments[0] == "fastest":
        with open(arguments[1], encoding="utf-8") as handle:
            data = load(handle)
        print(fastest(data, arguments[2], int(arguments[3]), arguments[4]))
    elif arguments and not arguments[0].startswith("-"):
        loops = parse_loops(arguments[1:])
        with open(arguments[0], encoding="utf-8") as handle:
            data = load(handle)
        report(data, loops)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
