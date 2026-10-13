#!/usr/bin/env python3
"""Reduces the output directory of run.sh to the tables and the decisions
registered in research/investigations/sequence-take/DESIGN.md section 7 for Q1
and Q2.

    python3 summarize.py [--smoke | --pilot] DIR

DIR holds raw.tsv, correct.tsv, calib.tsv, idle.tsv and host.txt. It prints,
per arm, w and CPU list, the median and the range of the rate over blocks; the
noise e; the descriptive numbers; the Q1 and Q2 decisions under the registered
thresholds; and the correctness runs. A ratio is decided only when it clears
its threshold by more than e. --smoke marks a wiring run on a host that is not
the 14900K: it prints no verdict and exits 1 when an arm, a check value or a
correctness run is wrong. --pilot marks a one-block run: e cannot be formed
from medians over blocks, so decisions are printed as no verdict.
"""
import csv
import os
import statistics
import sys
from collections import defaultdict

Q1_ARMS = ["S0", "S0t", "S1", "S1t", "S2", "S3", "S4"]
Q2_ARMS = ["T0", "T0t", "T2"]
Q1_MEANING = {
    "S0": "work only",
    "S0t": "S0 built a second time (noise control)",
    "S1": "work, then one seq_cst FAA on the one shared line",
    "S1t": "S1 built a second time (noise control)",
    "S2": "work, then one FAA on the thread's own line",
    "S3": "own test-and-set lock, work, FAA on the shared line, release",
    "S4": "work, then a load of a shared line nobody writes, and a compare",
}
Q2_MEANING = {
    "T0": "map entry and own part; record (key, 0)",
    "T0t": "T0 built a second time (noise control)",
    "T2": "T0 plus a whole target on one Shared<u64>, +checked 1",
}
DECISION_W = 400
Q1_THRESHOLD = 0.75
Q1_PREDICTION = 0.80
Q1_HYPERTHREAD_PREDICTION = 0.75
S3_PREDICTION = 0.95
S4_THRESHOLD = 0.95
Q2_THRESHOLD = 0.75
Q2_PREDICTION = 0.25
W2_RATE = 2.81e6
NOISE_RERUN = 0.05
P_CORE_DRIVERS = 8
HYPERTHREAD_DRIVERS = 16


def read_tsv(path):
    if not os.path.exists(path):
        return []
    with open(path, newline="") as f:
        return list(csv.DictReader(f, delimiter="\t"))


def load(directory):
    cells = defaultdict(list)
    drivers = {}
    for row in read_tsv(os.path.join(directory, "raw.tsv")):
        key = (row["arm"], int(row["w"]), row["cpus"])
        cells[key].append({
            "block": int(row["block"]),
            "rate": float(row["ops_per_s"]),
            "check": int(row["check"]),
            "contexts": int(row["contexts"]),
        })
        drivers[row["cpus"]] = int(row["drivers"])
    return cells, drivers


def median(cells, arm, w, cpus):
    rows = cells.get((arm, w, cpus))
    return statistics.median(r["rate"] for r in rows) if rows else None


def ratio(cells, top, bottom, w, cpus):
    a = median(cells, top, w, cpus)
    b = median(cells, bottom, w, cpus)
    if a is None or b is None or b == 0:
        return None
    return a / b


def fmt(value, places=3):
    return "n/a" if value is None else f"{value:.{places}f}"


def reading(value, threshold, at_least, e, final):
    """met, not met, or inconclusive: a ratio decides only when it clears the
    threshold by more than e."""
    if value is None:
        return "no data"
    if not final:
        return "no verdict"
    if e is None:
        return "inconclusive: no noise estimate"
    margin = value - threshold if at_least else threshold - value
    if margin > e:
        return "met"
    if margin < -e:
        return "not met"
    return f"inconclusive: within e = {e:.3f} of {threshold}"


def noise(cells, twin, arm, w, lists):
    terms = []
    per_list = {}
    for c in lists:
        r = ratio(cells, twin, arm, w, c)
        if r is not None:
            per_list[c] = abs(r - 1)
            terms.append(abs(r - 1))
    return (max(terms) if terms else None), per_list


def main(directory, smoke=False, pilot=False):
    cells, drivers = load(directory)
    if not cells:
        print("no rows")
        return 1
    lists = sorted(drivers, key=lambda c: drivers[c])
    blocks = sorted({r["block"] for rows in cells.values() for r in rows})
    final = not smoke and not pilot and len(blocks) >= 2
    mark = ""
    calib = {int(r["w"]): (int(r["steps"]), float(r["ns_per_op"])) for r in read_tsv(os.path.join(directory, "calib.tsv"))}
    ws = sorted({k[1] for k in cells if k[0] in Q1_ARMS})
    key8 = next((c for c in lists if drivers[c] == P_CORE_DRIVERS), None)
    if key8 is None and lists:
        key8 = lists[-1]
    key16 = next((c for c in lists if drivers[c] == HYPERTHREAD_DRIVERS), None)
    problems = []

    print(f"source: {directory}; blocks {len(blocks)}; CPU lists {', '.join(f'{c} ({drivers[c]} drivers)' for c in lists)}")
    if smoke:
        print("SMOKE RUN: a wiring check on a host that is not the 14900K. Nothing below is a verdict.")
    if pilot:
        print("PILOT: one block. Nothing below is a verdict; see pilot.txt for the timing.")
    print()

    if calib:
        print("Calibration (the pilot's: one thread of S0 on one CPU; the median of three calibrations)")
        print(f"{'w ns':>6} {'steps':>8} {'measured ns/op':>15}")
        for w in sorted(calib):
            print(f"{w:>6} {calib[w][0]:>8} {calib[w][1]:>15.2f}")
        print()

    def table(title, arms, meaning, ws_for):
        print(title)
        print("Source: raw.tsv column ops_per_s, summed over all threads or contexts. Median over blocks [min-max] (n = blocks).")
        print(f"{'arm':<5}{'w ns':>6}  " + "".join(f"{drivers[c]:>3} CPUs{'':<19}" for c in lists))
        for w in ws_for:
            for arm in arms:
                parts = []
                present = False
                for c in lists:
                    rows = cells.get((arm, w, c))
                    if not rows:
                        parts.append(f"{'-':<26}")
                        continue
                    present = True
                    rates = [r["rate"] / 1e6 for r in rows]
                    parts.append(f"{statistics.median(rates):8.3f} [{min(rates):.3f}-{max(rates):.3f}] n={len(rates):<2}".ljust(26))
                if present:
                    wlabel = str(w) if w else "-"
                    print(f"{arm:<5}{wlabel:>6}  " + "".join(parts) + f" {meaning[arm]}")
        print()

    if ws:
        table("Table 1 (Q1). Rate, millions of operations per second", Q1_ARMS, Q1_MEANING, ws)
    if any(k[0] in Q2_ARMS for k in cells):
        table("Table 2 (Q2). Rate, millions of operations per second", Q2_ARMS, Q2_MEANING, [0])

    idle = read_tsv(os.path.join(directory, "idle.tsv"))
    if idle:
        worst = max(float(r["busy"]) for r in idle)
        print(f"Idle checks before each block: worst busy share {worst:.4f} (limit 0.05)")
        if worst > 0.05 and not smoke:
            problems.append("a block started on a host that was not idle")
        print()

    e1, e1_lists = noise(cells, "S1t", "S1", DECISION_W, lists)
    e0, _ = noise(cells, "S0t", "S0", DECISION_W, lists)
    e2, e2_lists = noise(cells, "T0t", "T0", 0, lists)
    print("Noise e = largest |median(twin) / median(arm) - 1| over the CPU lists (DESIGN.md section 7)")
    if e1 is not None:
        print(f"  Q1, S1t / S1 at w = {DECISION_W}: " + ", ".join(f"{drivers[c]} CPUs {fmt(v)}" for c, v in e1_lists.items()) + f"  e = {fmt(e1)}")
        others = []
        for w in ws:
            ew, _ = noise(cells, "S1t", "S1", w, lists)
            others.append(f"w={w}: {fmt(ew)}")
        print("  S1t / S1 at every w: " + ", ".join(others))
    if e0 is not None:
        print(f"  S0t / S0 at w = {DECISION_W}: e = {fmt(e0)} (uncontended twin, reported)")
    if e2 is not None:
        print(f"  Q2, T0t / T0: " + ", ".join(f"{drivers[c]} CPUs {fmt(v)}" for c, v in e2_lists.items()) + f"  e = {fmt(e2)}")
    rerun = []
    if key8 is not None:
        for label, per in (("S1t / S1", e1_lists), ("T0t / T0", e2_lists)):
            if per.get(key8, 0) > NOISE_RERUN:
                rerun.append(label)
    if rerun and final:
        print(f"  RERUN REQUIRED: {', '.join(rerun)} differ by more than {NOISE_RERUN} at the 8-P-core list; rerun once with MILLIS=10000, never a wider threshold.")
    print()

    if ws:
        print("Q1 descriptive: S1 / S0, S2 / S0, S3 / S1, S4 / S0 (ratios of medians)")
        print(f"{'ratio':<9}{'w ns':>6}  " + "".join(f"{drivers[c]:>3} CPUs  " for c in lists))
        for label, top, bottom in (("S1/S0", "S1", "S0"), ("S2/S0", "S2", "S0"), ("S3/S1", "S3", "S1"), ("S4/S0", "S4", "S0")):
            for w in ws:
                print(f"{label:<9}{w:>6}  " + "".join(f"{fmt(ratio(cells, top, bottom, w, c)):>9} " for c in lists))
        print()
        print("Smallest w at which S1 / S0 reaches 0.75, per CPU list (among the measured w; none if no w reaches it)")
        for c in lists:
            reached = [w for w in ws if (ratio(cells, "S1", "S0", w, c) or 0) >= Q1_THRESHOLD]
            print(f"  {drivers[c]} CPUs: {min(reached) if reached else 'none'}")
        print()

    if ws and key8 is not None and DECISION_W in ws:
        s1 = ratio(cells, "S1", "S0", DECISION_W, key8)
        s4 = ratio(cells, "S4", "S0", DECISION_W, key8)
        s3 = ratio(cells, "S3", "S1", DECISION_W, key8)
        e = e1
        print(f"Q1 decisions (w = {DECISION_W} ns, 8-P-core list {key8}, e = {fmt(e)})")
        print(f"  Question: S1 / S0 >= {Q1_THRESHOLD}: {fmt(s1)} -> {mark}{reading(s1, Q1_THRESHOLD, True, e, final)}")
        r = reading(s1, Q1_THRESHOLD, True, e, final)
        if r == "not met":
            print("  Rejection: S1 / S0 < 0.75 beyond e: no design with one shared line per take, (V), (A) or (B), can meet Firn's bar at Firn's size; the card returns to the owner (the remaining choices are (D)'s costs and its loss of real-time order, or E1).")
        elif r == "met":
            print("  Rejection: not triggered.")
        print(f"  Second, descriptive: S4 / S0 >= {S4_THRESHOLD}: {fmt(s4)} -> {mark}{reading(s4, S4_THRESHOLD, True, e, final)}")
        print(f"  Prediction S1 / S0 >= {Q1_PREDICTION} at 8 P-cores: {fmt(s1)} -> {mark}{reading(s1, Q1_PREDICTION, True, e, final)}")
        if key16 is not None:
            s16 = ratio(cells, "S1", "S0", DECISION_W, key16)
            print(f"  Prediction S1 / S0 >= {Q1_HYPERTHREAD_PREDICTION} at 16 hyperthreads: {fmt(s16)} -> {mark}{reading(s16, Q1_HYPERTHREAD_PREDICTION, True, e, final)}")
        print(f"  Prediction S3 / S1 >= {S3_PREDICTION} at 8 P-cores: {fmt(s3)} -> {mark}{reading(s3, S3_PREDICTION, True, e, final)}")
        print()

    t0 = median(cells, "T0", 0, key8) if key8 else None
    if t0 is not None:
        t2 = median(cells, "T2", 0, key8)
        q2 = ratio(cells, "T2", "T0", 0, key8)
        e = e2
        print(f"Q2 decisions (8-P-core list {key8}, e = {fmt(e)})")
        print(f"  Question: T2 / T0 >= {Q2_THRESHOLD}: {fmt(q2)} -> {mark}{reading(q2, Q2_THRESHOLD, True, e, final)}")
        r = reading(q2, Q2_THRESHOLD, True, e, final)
        if r == "met":
            print("  Rejection of the request: T2 / T0 >= 0.75 beyond e: today's form serves this shape and the language change is not needed; the card says so.")
        print(f"  Prediction T2 / T0 <= {Q2_PREDICTION}: {fmt(q2)} -> {mark}{reading(q2, Q2_PREDICTION, False, e, final)}")
        if t2 is not None:
            print(f"  Prediction T2 bounded near W2's 2.81 million a second: T2 = {t2 / 1e6:.3f} million a second ({t2 / W2_RATE:.2f} x 2.81)")
        print()
        print("Q2 descriptive: T0's nanoseconds per operation per driver (drivers / rate), and the Q1 w nearest to it")
        for c in lists:
            rate = median(cells, "T0", 0, c)
            if rate:
                ns = drivers[c] / rate * 1e9
                nearest = min(ws, key=lambda w: abs(w - ns)) if ws else None
                print(f"  {drivers[c]} CPUs: {ns:.1f} ns per operation; nearest Q1 w: {nearest}")
        print()

    correct = read_tsv(os.path.join(directory, "correct.tsv"))
    if correct:
        print("Correctness runs (1 s each at the 8-P-core list; verdict bit 1 = N1, 2 = N2, 4 = N3 failed)")
        print(f"{'block':>5} {'arm':<7} {'attempts':>8} {'status':>6} {'verdict':>7}  expectation")
        for r in correct:
            wanted = "T2 passes N1-N3 (verdict 0)" if r["arm"] == "c_t2" else "Tbad fails N1 and N3 (bits 1 and 4)"
            print(f"{r['block']:>5} {r['arm']:<7} {r['attempts']:>8} {r['status']:>6} {r['verdict']:>7}  {wanted}: {r['expect']}")
        bad = [r for r in correct if r["expect"] != "ok"]
        if bad:
            print("  The checker is wrong or the implementation violates N1-N3: Q3 waits for it.")
            problems.append("a correctness run did not give the required verdict")
        print()
    elif any(k[0] in Q2_ARMS for k in cells):
        problems.append("no correctness runs")

    if smoke:
        expected_q1 = [(a, w) for a in Q1_ARMS for w in ws] if ws else []
        expected_q2 = [(a, 0) for a in Q2_ARMS] if any(k[0] in Q2_ARMS for k in cells) else []
        for c in lists:
            for arm, w in expected_q1 + expected_q2:
                rows = cells.get((arm, w, c))
                if not rows:
                    problems.append(f"no row for {arm} w={w} on {c}")
                    continue
                if any(r["rate"] <= 0 for r in rows):
                    problems.append(f"{arm} w={w} on {c} ran at rate 0")
                if arm in Q1_ARMS and any(r["check"] == 0 for r in rows):
                    problems.append(f"{arm} w={w} on {c} printed check 0")
        print("Smoke wiring: " + ("every arm ran on every list, check values nonzero, correctness runs as required" if not problems else "PROBLEMS"))

    for p in problems:
        print(f"PROBLEM: {p}")
    return 1 if problems else 0


if __name__ == "__main__":
    arguments = sys.argv[1:]
    smoke = "--smoke" in arguments
    pilot = "--pilot" in arguments
    arguments = [a for a in arguments if a not in ("--smoke", "--pilot")]
    if len(arguments) != 1:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(arguments[0], smoke, pilot))
