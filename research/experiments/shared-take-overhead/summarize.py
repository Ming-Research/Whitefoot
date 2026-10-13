#!/usr/bin/env python3
"""Reduces run.sh's raw.tsv to the uncontended-cost tables and the registered
verdict (research/investigations/shared-take-fairness/DESIGN.md,
"Registered criterion", part 3, and "Uncontended-cost procedure").

    python3 summarize.py RAW.tsv
    python3 summarize.py --self-test

Per program (take, own) and CPU list it prints the median rate of the base,
twin and candidate images over blocks, their ranges, and two time ratios of
medians, where a statement's time is the inverse of the rate:

    t = median rate(base) / median rate(candidate)   candidate time / base time
    w = median rate(base) / median rate(twin)        twin time / base time

A cell is inconclusive when w lies outside [0.99, 1.01]; otherwise it passes
when t <= 1.02 and rejects when t > 1.02. The registered cell is the take
program at one driver. The extension, registered with the procedure before
any measurement, applies the same rule to every cell: it rejects when any
cell rejects, passes when every cell passes, and is otherwise inconclusive.
Beside each cell it prints whether t's distance from 1.02 exceeds the noise
e = |w - 1|; that reading is descriptive and decides nothing.

Exit status: 0 when every cell has the same positive number of rows for each
of the three images, whatever the verdict; 1 for a missing, extra or
malformed row; 2 for usage.
"""
import csv
import io
import statistics
import sys
from collections import defaultdict

PROGRAMS = ["take", "own"]
IMAGES = ["base", "twin", "candidate"]
MEANING = {
    "take": "holds an entry of its own map, then takes its own object (wf__shared_take)",
    "own": "its own object alone, through the ordinary acquisition",
}
TIME_LIMIT = 1.02
TWIN_LOW = 0.99
TWIN_HIGH = 1.01
REGISTERED = ("take", 1)
COLUMNS = ["block", "pos", "arm", "program", "image", "cpus", "drivers", "contexts",
           "ops", "ns", "ops_per_s", "check", "wrong", "user_frac", "sys_frac", "host_busy"]


class Malformed(Exception):
    pass


def read(stream):
    reader = csv.DictReader(stream, delimiter="\t")
    if reader.fieldnames != COLUMNS:
        raise Malformed(f"columns {reader.fieldnames} are not {COLUMNS}")
    cells = defaultdict(lambda: defaultdict(list))
    drivers_of = {}
    seen = set()
    for number, row in enumerate(reader, 2):
        try:
            program, image, cpus = row["program"], row["image"], row["cpus"]
            block = int(row["block"])
            drivers = int(row["drivers"])
            rate = float(row["ops_per_s"])
            wrong = int(row["wrong"])
            contexts = int(row["contexts"])
        except (TypeError, ValueError) as error:
            raise Malformed(f"line {number}: {error}")
        if program not in PROGRAMS or image not in IMAGES:
            raise Malformed(f"line {number}: unknown program {program!r} or image {image!r}")
        if rate <= 0 or wrong != 0 or contexts != drivers or drivers <= 0:
            raise Malformed(f"line {number}: rate {rate}, wrong {wrong}, contexts {contexts}, drivers {drivers}")
        if drivers_of.setdefault(cpus, drivers) != drivers:
            raise Malformed(f"line {number}: CPU list {cpus} has two driver counts")
        key = (block, program, image, cpus)
        if key in seen:
            raise Malformed(f"line {number}: a second row for block {block}, {program} {image}, CPUs {cpus}")
        seen.add(key)
        cells[(program, cpus)][image].append(rate)
    if not cells:
        raise Malformed("no rows")
    lists = sorted(drivers_of, key=lambda c: drivers_of[c])
    blocks = {key[0] for key in seen}
    for program in PROGRAMS:
        for cpus in lists:
            for image in IMAGES:
                n = len(cells[(program, cpus)][image])
                if n != len(blocks):
                    raise Malformed(f"{program} {image} at CPUs {cpus} has {n} rows for {len(blocks)} blocks")
    return cells, lists, drivers_of, len(blocks)


def cell_reading(t, w):
    if w < TWIN_LOW or w > TWIN_HIGH:
        return "inconclusive"
    return "pass" if t <= TIME_LIMIT else "reject"


def summarize(stream, out):
    cells, lists, drivers_of, blocks = read(stream)
    print(f"blocks {blocks}; CPU lists " + ", ".join(f"{c} ({drivers_of[c]} drivers)" for c in lists), file=out)
    print(file=out)
    print("Table 1. Rate, millions of atomic statements per second summed over all contexts (one per driver):", file=out)
    print("median over blocks [min-max]. Source: raw.tsv column ops_per_s. Higher is faster.", file=out)
    readings = {}
    for program in PROGRAMS:
        print(f"{program}: {MEANING[program]}", file=out)
        for cpus in lists:
            parts = []
            for image in IMAGES:
                rates = [r / 1e6 for r in cells[(program, cpus)][image]]
                parts.append(f"{image} {statistics.median(rates):.3f} [{min(rates):.3f}-{max(rates):.3f}]")
            print(f"  {drivers_of[cpus]:>2} drivers  " + "  ".join(parts), file=out)
    print(file=out)
    print("Table 2. Time ratios of medians at each cell, and each cell's reading.", file=out)
    print(f"t = candidate time / base time (registered limit {TIME_LIMIT}); "
          f"w = twin time / base time (valid within [{TWIN_LOW}, {TWIN_HIGH}]);", file=out)
    print("ns/statement = drivers x 1e9 / median base rate, one context's time per statement;", file=out)
    print("beyond noise = |1.02 - t| > e = |w - 1|, descriptive only.", file=out)
    for program in PROGRAMS:
        for cpus in lists:
            images = cells[(program, cpus)]
            base = statistics.median(images["base"])
            t = base / statistics.median(images["candidate"])
            w = base / statistics.median(images["twin"])
            reading = cell_reading(t, w)
            readings[(program, drivers_of[cpus])] = reading
            e = abs(w - 1)
            beyond = "yes" if abs(TIME_LIMIT - t) > e else "no"
            per = drivers_of[cpus] * 1e9 / base
            print(f"  {program:<4} {drivers_of[cpus]:>2} drivers  t {t:.4f}  overhead {t - 1:+.2%}  w {w:.4f}  "
                  f"ns/statement {per:.2f}  beyond noise {beyond}  {reading}", file=out)
    print(file=out)
    registered = readings.get(REGISTERED)
    if registered is None:
        print("Registered criterion (take, one driver): not decided; that cell was not run (a pilot).", file=out)
    else:
        print(f"Registered criterion (take, one driver): {registered}", file=out)
    values = set(readings.values())
    if "reject" in values:
        extension = "reject"
    elif values == {"pass"}:
        extension = "pass"
    else:
        extension = "inconclusive"
    print(f"Every-cell extension ({len(readings)} cells): {extension}", file=out)
    return registered, extension, readings


def main(path):
    try:
        with open(path, newline="") as f:
            summarize(f, sys.stdout)
    except Malformed as error:
        print(f"malformed or incomplete raw rows: {error}", file=sys.stderr)
        return 1
    return 0


def table(rows):
    lines = ["\t".join(COLUMNS)]
    for block, program, image, cpus, drivers, rate in rows:
        lines.append("\t".join(str(v) for v in [block, 1, "x", program, image, cpus, drivers, drivers,
                                                 1000, 1000, rate, 0, 0, 0.5, 0.0, 0.1]))
    return "\n".join(lines) + "\n"


def grid(rates, blocks=1, lists=(("0", 1), ("0,2", 2))):
    """Rows for every program, image and list; rates maps (program, image,
    drivers) to the rate, default 100."""
    rows = []
    for block in range(blocks):
        for program in PROGRAMS:
            for cpus, drivers in lists:
                for image in IMAGES:
                    rows.append((block, program, image, cpus, drivers, rates.get((program, image, drivers), 100)))
    return rows


def self_test():
    failures = []

    def expect(label, rows, registered, extension, cell=None):
        sink = io.StringIO()
        got = summarize(io.StringIO(table(rows)), sink)
        if got[0] != registered or got[1] != extension or (cell and got[2].get(cell[0]) != cell[1]):
            failures.append(f"{label}: got {got[0]}, {got[1]}, {got[2]}")

    def expect_malformed(label, text):
        try:
            summarize(io.StringIO(text), io.StringIO())
        except Malformed:
            return
        failures.append(f"{label}: accepted")

    expect("equal", grid({}), "pass", "pass")
    # Candidate time exactly at the limit (102 / 100 is the double nearest
    # 1.02, as the literal is) passes; just above it rejects.
    expect("limit", grid({("take", "base", 1): 102, ("take", "twin", 1): 102,
                          ("take", "candidate", 1): 100}), "pass", "pass")
    expect("just above", grid({("take", "base", 1): 102, ("take", "twin", 1): 102,
                               ("take", "candidate", 1): 99.99}), "reject", "reject")
    expect("above", grid({("take", "candidate", 1): 97}), "reject", "reject")
    expect("other cell", grid({("own", "candidate", 2): 97}), "pass", "reject", (("own", 2), "reject"))
    # A twin at the edge of its range keeps the cell valid; outside it, the
    # cell is inconclusive even when the candidate is far slower.
    expect("twin edge", grid({("take", "base", 1): 99, ("take", "candidate", 1): 99}), "pass", "pass")
    expect("twin out", grid({("take", "twin", 1): 98, ("take", "candidate", 1): 50}),
           "inconclusive", "inconclusive")
    expect("twin out elsewhere", grid({("own", "twin", 2): 102}), "pass", "inconclusive")
    expect("reject beats inconclusive", grid({("own", "twin", 2): 102, ("take", "candidate", 2): 90}),
           "pass", "reject")
    # Medians over blocks, not means: one slow block of three does not move t.
    rows = grid({}, blocks=3)
    rows = [(b, p, i, c, d, 10 if (b, p, i, d) == (0, "take", "candidate", 1) else r) for b, p, i, c, d, r in rows]
    expect("median", rows, "pass", "pass")
    expect("pilot", grid({}, lists=(("0,2,4,6,8,10,12,14", 8),)), None, "pass")
    full = grid({})
    expect_malformed("missing image", table([r for r in full if not (r[1] == "own" and r[2] == "twin")]))
    expect_malformed("missing program", table([r for r in full if r[1] != "own"]))
    expect_malformed("duplicate", table(full + full[:1]))
    expect_malformed("uneven blocks", table(grid({}, blocks=2)[:-1]))
    expect_malformed("empty", table([]))
    expect_malformed("zero rate", table(grid({("take", "base", 1): 0})))
    bad = table(full).splitlines()
    fields = bad[1].split("\t")
    fields[COLUMNS.index("wrong")] = "1"
    expect_malformed("wrong count", "\n".join([bad[0], "\t".join(fields)] + bad[2:]) + "\n")
    expect_malformed("columns", table(full).replace("host_busy", "busy", 1))
    for failure in failures:
        print("self-test failed:", failure, file=sys.stderr)
    print(f"self-test: {'failed' if failures else 'passed'}")
    return 1 if failures else 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        sys.exit(self_test())
    if len(sys.argv) != 2:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
