#!/usr/bin/env python3
"""Research-only CSV validation and the pre-registered fairness decision.

The shell cannot reliably validate named CSV fields or compare decimal ratios.
This helper is wired only to this experiment and its temporary CI workflow.
"""

import csv
import io
import sys
import tempfile
from pathlib import Path

FIELDS = "contexts rounds count p50_ns p99_ns max_ns hold_ns_mean runtime_ns checksum".split()
# The judged candidate is wake tests (mode 3); A1 and oldest-only handoff
# (mode 2) are non-voting references, and twin copies the baseline.
ARMS = ("baseline", "candidate", "a1", "oldest", "twin")
CANDIDATE = "candidate"
VOTING = ("baseline", CANDIDATE, "twin")
COUNTER_FIELDS = "woken spurious refused_locks clock_reads".split()
EVENT_FIELDS = "belowT_batch_woken initial_age_selections false_handoffs rescans pass_links granted_batch_woken clock_fallback_batch_woken".split()
RETURN_FIELDS = "marked samples ns_sum ns_max clock_failures".split()
TEST_FIELDS = "tests_evaluated wakes_avoided".split()
HANDOFF_FIELDS = "handoffs_installed handoff_samples handoff_ns_sum handoff_ns_max handoff_clock_failures".split()
SCHEMAS = {"guard_counters": COUNTER_FIELDS, "guard_events": EVENT_FIELDS,
           "guard_return_wait": RETURN_FIELDS, "guard_wake_tests": TEST_FIELDS,
           "guard_handoff": HANDOFF_FIELDS}
TOTAL_FIELDS = COUNTER_FIELDS + EVENT_FIELDS + RETURN_FIELDS + TEST_FIELDS + HANDOFF_FIELDS
NORMAL_FIELDS = (COUNTER_FIELDS + EVENT_FIELDS + ["marked", "samples", "clock_failures"]
                 + TEST_FIELDS + ["handoffs_installed"])
DIAGNOSTIC_FIELDS = (TOTAL_FIELDS + [field + "_per_acquisition" for field in NORMAL_FIELDS]
                     + ["sampled_return_ns_mean", "sampled_handoff_ns_mean"])
# Registered prediction at N = 50 (DESIGN.md, "Registered experiment"):
# notifications are woken watches, each handed reservation included.
PREDICTED_NOTIFICATIONS = 3
PREDICTED_FALSE_HANDOFFS = 0.1
PREDICTED_A1_WAKES = 15
REFUTING_RATIO = 1.25


def require(condition, message):
    if not condition:
        raise ValueError(message)


def probe_row(text, n, rounds):
    rows = list(csv.reader(io.StringIO(text)))
    require(len(rows) == 2 and rows[0] == FIELDS, "missing or malformed probe CSV")
    require(len(rows[1]) == len(FIELDS), "wrong probe field count")
    row = dict(zip(FIELDS, map(int, rows[1])))
    require(all(value >= 0 for value in row.values()), "negative probe field")
    require(row["contexts"] == n and row["rounds"] == rounds, "probe arguments differ")
    require(row["count"] == n * rounds, "successful acquisition count differs")
    require(0 <= row["p50_ns"] <= row["p99_ns"] <= row["max_ns"], "unordered percentiles")
    require(row["runtime_ns"] > 0, "zero runtime")
    return row


def record(prefix, pass_id, arm, n, rounds, diagnostic, out):
    row = probe_row(Path(prefix + ".stdout").read_text(), n, rounds)
    require(arm in ARMS, "unknown arm")
    stderr = Path(prefix + ".stderr").read_text().splitlines()
    counters = [line for line in stderr if line.startswith("guard_")]
    if diagnostic:
        schemas = SCHEMAS
        require(len(counters) == len(schemas), "missing or duplicate runtime diagnostic record")
        totals = {}
        seen = set()
        for line in counters:
            name, *values = line.split(",")
            require(name in schemas and name not in seen, "unknown or duplicate runtime diagnostic record")
            seen.add(name)
            require(len(values) == len(schemas[name]), "wrong runtime diagnostic field count")
            values = list(map(int, values))
            require(all(value >= 0 for value in values), "negative runtime diagnostic counter")
            totals.update(zip(schemas[name], values))
        require(totals["marked"] == row["count"], "return observer did not mark exactly once per checkout")
        require(totals["samples"] + totals["clock_failures"] == n * ((rounds + 63) // 64), "sampled return count differs")
        require(totals["ns_max"] <= totals["ns_sum"], "sampled return maximum exceeds sum")
        require(totals["samples"] > 0 or totals["ns_sum"] == totals["ns_max"] == 0, "failed samples contain return timing")
        require(totals["samples"] > 0 or totals["clock_failures"] > 0, "missing sampled return observations")
        # Each sampled hand-off ends at its owner's acquisition; only modes 2
        # and 3 hand any, and only mode 3 evaluates tests.
        require(totals["handoff_samples"] + totals["handoff_clock_failures"] <= totals["handoffs_installed"],
                "more sampled hand-offs than hand-offs")
        require(totals["handoff_ns_max"] <= totals["handoff_ns_sum"], "sampled hand-off maximum exceeds sum")
        require(totals["handoff_samples"] > 0 or totals["handoff_ns_sum"] == 0, "failed hand-off samples contain timing")
        require(arm in ("candidate", "a1", "oldest") or totals["handoffs_installed"] == 0,
                "a mode without hand-offs handed one")
        require(arm in ("candidate", "a1") or totals["tests_evaluated"] == totals["wakes_avoided"] == 0,
                "a mode without wake tests evaluated one")
        normalized = [totals[field] / row["count"] for field in NORMAL_FIELDS]
        mean = totals["ns_sum"] / totals["samples"] if totals["samples"] else ""
        handoff_mean = totals["handoff_ns_sum"] / totals["handoff_samples"] if totals["handoff_samples"] else ""
        diagnostic_values = [totals[field] for field in TOTAL_FIELDS] + normalized + [mean, handoff_mean]
        with (out / "counters.csv").open("a") as stream:
            csv.writer(stream).writerow([pass_id, arm, n, rounds, row["count"]] + diagnostic_values)
        print("diagnostic,pass,arm," + ",".join(FIELDS + DIAGNOSTIC_FIELDS))
        print(",".join(map(str, ["diagnostic", pass_id, arm] + [row[field] for field in FIELDS] + diagnostic_values)))
    else:
        require(not counters, "instrumented arm entered the timing comparison")
    # Invalid instrumentation must not contaminate either measurement file.
    with (out / "measurements.csv").open("a") as stream:
        csv.writer(stream).writerow([pass_id, arm] + [row[field] for field in FIELDS])


def initialize(out, diagnostic):
    out.mkdir(parents=True, exist_ok=True)
    with (out / "measurements.csv").open("w") as stream:
        csv.writer(stream).writerow(["pass", "arm"] + FIELDS)
    if diagnostic:
        with (out / "counters.csv").open("w") as stream:
            csv.writer(stream).writerow(["pass", "arm", "contexts", "rounds", "count"] + DIAGNOSTIC_FIELDS)


def judge(rows, rounds):
    panel = {}
    for row in rows:
        if row["pass"].startswith("pilot"):
            continue
        pass_id, arm, n = row["pass"], row["arm"], int(row["contexts"])
        require(pass_id in ("1", "2") and arm in ARMS and n in (2, 8, 50), "unexpected comparison cell")
        key = (pass_id, n, arm)
        require(key not in panel, "duplicate comparison cell")
        parsed = {field: int(row[field]) for field in FIELDS}
        # Use the same independent count/shape constraints as raw recording.
        text = ",".join(FIELDS) + "\n" + ",".join(str(parsed[field]) for field in FIELDS) + "\n"
        panel[key] = probe_row(text, n, rounds)
    require(len(panel) == 2 * 3 * len(ARMS), "comparison needs every arm at each N in both passes")
    noisy = False
    failed = False
    details = []
    for pass_id in ("1", "2"):
        for n in (2, 8, 50):
            cell = {arm: panel[(pass_id, n, arm)] for arm in ARMS}
            require(len({row["checksum"] for row in cell.values()}) == 1, "arms completed different work")
            base = cell["baseline"]["runtime_ns"]
            twin = cell["twin"]["runtime_ns"] / base
            cost = cell[CANDIDATE]["runtime_ns"] / base
            control_ok = base * 95 <= cell["twin"]["runtime_ns"] * 100 <= base * 105
            cost_ok = cell[CANDIDATE]["runtime_ns"] * 100 <= base * 110
            fairness_ok = n != 50 or cell[CANDIDATE]["max_ns"] <= 3_300_000
            noisy |= not control_ok
            failed |= not cost_ok or not fairness_ok
            # Non-voting readouts: the references' ratios and the predicted
            # hold, which no criterion judges.
            references = ",".join(f"{arm}/base={cell[arm]['runtime_ns'] / base:.6f}"
                                  for arm in ARMS if arm not in VOTING)
            hold = cell[CANDIDATE]["hold_ns_mean"] / max(cell["baseline"]["hold_ns_mean"], 1)
            details.append(f"pass={pass_id},N={n},twin/base={twin:.6f},candidate/base={cost:.6f},candidate_max_ns={cell[CANDIDATE]['max_ns']},twin_ok={control_ok},cost_ok={cost_ok},fairness_ok={fairness_ok},{references},hold/base={hold:.6f}")
    verdict = "inconclusive" if noisy else "reject" if failed else "pass"
    return verdict, details


def diagnostic_integrity(timing_rows, diagnostic_rows, rounds):
    """Check completed work, without deciding any diagnostic performance."""
    timing = {}
    for row in timing_rows:
        if row["pass"] == "1" and row["arm"] != "twin":
            key = (int(row["contexts"]), row["arm"])
            require(key not in timing, "duplicate matching timing cell")
            timing[key] = int(row["checksum"])
    panel = {}
    for row in diagnostic_rows:
        if row["pass"].startswith("pilot"):
            continue
        n, arm = int(row["contexts"]), row["arm"]
        require(row["pass"] == "diagnostic" and n in (2, 8, 50) and arm in ARMS and arm != "twin",
                "unexpected diagnostic cell")
        key = (n, arm)
        require(key not in panel, "duplicate diagnostic cell")
        text = ",".join(FIELDS) + "\n" + ",".join(str(row[field]) for field in FIELDS) + "\n"
        parsed = probe_row(text, n, rounds)
        require(key in timing and parsed["checksum"] == timing[key], "diagnostic and timing completed different work")
        panel[key] = parsed
    require(len(panel) == 3 * (len(ARMS) - 1), "diagnostics need every arm at each N")
    return panel


def predictions(timing_rows, diagnostic_rows, rounds):
    """The registered prediction and its refutations; no acceptance verdict."""
    diagnostic_integrity(timing_rows, diagnostic_rows, rounds)
    at = {row["arm"]: row for row in diagnostic_rows if row["pass"] == "diagnostic" and int(row["contexts"]) == 50}
    timing = {(row["pass"], row["arm"]): row for row in timing_rows
              if row["pass"] in ("1", "2") and int(row["contexts"]) == 50}
    notifications = float(at[CANDIDATE]["woken_per_acquisition"])
    false_handoffs = float(at[CANDIDATE]["false_handoffs_per_acquisition"])
    a1_wakes = float(at["a1"]["woken_per_acquisition"])
    ratios = [int(timing[(pass_id, CANDIDATE)]["runtime_ns"]) / int(timing[(pass_id, "baseline")]["runtime_ns"])
              for pass_id in ("1", "2")]
    holds = [int(timing[(pass_id, CANDIDATE)]["hold_ns_mean"]) / max(int(timing[(pass_id, "baseline")]["hold_ns_mean"]), 1)
             for pass_id in ("1", "2")]
    lines = [
        f"prediction,candidate_notifications_per_take,{notifications:.6f},at_most,{PREDICTED_NOTIFICATIONS},{notifications <= PREDICTED_NOTIFICATIONS}",
        f"prediction,candidate_false_handoffs_per_take,{false_handoffs:.6f},at_most,{PREDICTED_FALSE_HANDOFFS},{false_handoffs <= PREDICTED_FALSE_HANDOFFS}",
        f"prediction,candidate_hold_over_baseline,{holds[0]:.6f},{holds[1]:.6f},within,0.10,{all(abs(hold - 1) <= 0.10 for hold in holds)}",
        f"prediction,a1_wakes_per_take,{a1_wakes:.6f},at_least,{PREDICTED_A1_WAKES},{a1_wakes >= PREDICTED_A1_WAKES}",
        f"refutation,mechanism,more_than_{PREDICTED_NOTIFICATIONS}_notifications_per_take,{notifications > PREDICTED_NOTIFICATIONS}",
        f"refutation,wakeups_main_cost,{ratios[0]:.6f},{ratios[1]:.6f},{notifications <= PREDICTED_NOTIFICATIONS and all(ratio >= REFUTING_RATIO for ratio in ratios)}",
    ]
    return lines


def self_test():
    # CI-only sensitivity fixtures: these must distinguish each independent
    # criterion, and missing evidence must not become a passing panel.
    def fixture():
        return [dict(pass_id=pass_id, arm=arm, contexts=n, rounds=1000,
                     count=n * 1000, p50_ns=20, p99_ns=100,
                     max_ns=3_000_000, hold_ns_mean=6000,
                     runtime_ns=100_000_000, checksum=n)
                for pass_id in ("1", "2") for n in (2, 8, 50) for arm in ARMS]

    def evaluate(rows):
        return judge([{("pass" if key == "pass_id" else key): str(value)
                       for key, value in row.items()} for row in rows], 1000)[0]

    require(evaluate(fixture()) == "pass", "good fixture failed")
    for field, value, verdict in (("max_ns", 3_300_001, "reject"),
                                  ("runtime_ns", 110_000_001, "reject")):
        rows = fixture()
        next(row for row in rows if row["arm"] == CANDIDATE and row["contexts"] == 50)[field] = value
        require(evaluate(rows) == verdict, field + " fault escaped")
    # A non-voting reference's failure decides nothing.
    for arm in ARMS:
        if arm in VOTING:
            continue
        rows = fixture()
        for row in rows:
            if row["arm"] == arm:
                row["max_ns"], row["runtime_ns"] = 9_000_000, 900_000_000
        require(evaluate(rows) == "pass", arm + " reference voted")
    rows = fixture()
    next(row for row in rows if row["arm"] == "twin")["runtime_ns"] = 105_000_001
    require(evaluate(rows) == "inconclusive", "noisy twin escaped")
    unequal = fixture()
    unequal[0]["checksum"] += 1
    for rows in ([], fixture()[:-1], fixture() + [fixture()[0]], unequal):
        try:
            evaluate(rows)
        except ValueError:
            continue
        raise ValueError("missing, duplicate or unequal input escaped")
    # Equality belongs to the allowed boundary, independently at each cell.
    rows = fixture()
    for row in rows:
        if row["arm"] == CANDIDATE:
            row["max_ns"], row["runtime_ns"] = 3_300_000, 110_000_000
        if row["arm"] == "twin":
            row["runtime_ns"] = 95_000_000 if row["pass_id"] == "1" else 105_000_000
    require(evaluate(rows) == "pass", "allowed equality failed")
    timing = [{("pass" if key == "pass_id" else key): str(value)
               for key, value in row.items()} for row in fixture()]
    diagnostic = [dict(row, **{"pass": "diagnostic"}) for row in timing
                  if row["pass"] == "1" and row["arm"] != "twin"]
    diagnostic_integrity(timing, diagnostic, 1000)
    # Predictions read the N = 50 diagnostic cells; each readout moves with
    # its own counter and ratio.
    def with_counters(rows, **values):
        out = [row.copy() for row in rows]
        for row in out:
            row.update({"woken_per_acquisition": "1.0", "false_handoffs_per_acquisition": "0.0"})
            if row["arm"] in values:
                row.update(values[row["arm"]])
        return out
    good = with_counters(diagnostic, a1={"woken_per_acquisition": "20.0"})
    lines = predictions(timing, good, 1000)
    require(all(line.endswith("True") for line in lines[:4]) and all(line.endswith("False") for line in lines[4:]),
            "good prediction fixture failed: " + ";".join(lines))
    lines = predictions(timing, with_counters(diagnostic, candidate={"woken_per_acquisition": "3.5"},
                                              a1={"woken_per_acquisition": "20.0"}), 1000)
    require(lines[0].endswith("False") and lines[4].endswith("True") and lines[5].endswith("False"),
            "excess notifications escaped")
    slow = [dict(row) for row in timing]
    for row in slow:
        if row["arm"] == CANDIDATE and row["contexts"] == "50":
            row["runtime_ns"] = "125000000"
    lines = predictions(slow, good, 1000)
    require(lines[5].endswith("True"), "a slow candidate with few notifications escaped")
    lines = predictions(timing, with_counters(diagnostic, candidate={"false_handoffs_per_acquisition": "0.2"},
                                              a1={"woken_per_acquisition": "20.0"}), 1000)
    require(lines[1].endswith("False"), "false hand-offs escaped")
    lines = predictions(timing, with_counters(diagnostic, a1={"woken_per_acquisition": "14.0"}), 1000)
    require(lines[3].endswith("False"), "a quiet A1 escaped")
    mismatch = [row.copy() for row in diagnostic]
    mismatch[0]["checksum"] = "999"
    for faulty in (diagnostic[:-1], diagnostic + [diagnostic[0]], mismatch):
        try:
            diagnostic_integrity(timing, faulty, 1000)
        except ValueError:
            continue
        raise ValueError("missing, duplicate or unequal diagnostic work escaped")
    # Every extended record is mandatory only in diagnostics. Shape and
    # sample-count faults fail independently; timing forbids all three kinds.
    with tempfile.TemporaryDirectory() as directory:
        out = Path(directory)
        prefix = str(out / "record")
        (out / "record.stdout").write_text(",".join(FIELDS) + "\n2,100,200,20,100,3000000,6000,100000000,2\n")
        valid = ["guard_counters,1,2,3,4", "guard_events,1,2,3,4,5,6,7",
                 "guard_return_wait,200,4,100,50,0", "guard_wake_tests,0,0",
                 "guard_handoff,0,0,0,0,0"]
        initialize(out, True)
        (out / "record.stderr").write_text("\n".join(valid) + "\n")
        record(prefix, "diagnostic", "baseline", 2, 100, True, out)
        # A candidate's hand-offs and test evaluations are its own.
        (out / "record.stderr").write_text("\n".join(valid[:3] + ["guard_wake_tests,7,3",
                                                                   "guard_handoff,9,2,40,30,1"]) + "\n")
        record(prefix, "diagnostic", "candidate", 2, 100, True, out)
        failed_clocks = valid.copy()
        failed_clocks[2] = "guard_return_wait,200,0,0,0,4"
        (out / "record.stderr").write_text("\n".join(failed_clocks) + "\n")
        record(prefix, "diagnostic", "baseline", 2, 100, True, out)
        with (out / "counters.csv").open() as stream:
            require(list(csv.DictReader(stream))[-1]["sampled_return_ns_mean"] == "", "failed sample mean must be absent")
        faults = [(True, "")]
        for index in range(len(valid)):
            faults.append((True, "\n".join(valid[:index] + valid[index + 1:])))
            malformed = valid.copy()
            malformed[index] += ",9"
            faults.append((True, "\n".join(malformed)))
            faults.append((False, valid[index]))
        faults += [(True, "\n".join(valid + [valid[1]])),
                   (True, "\n".join(valid).replace(",200,4,", ",201,4,")),
                   (True, "\n".join(valid).replace(",200,4,", ",200,3,")),
                   (True, "\n".join(valid).replace(",100,50,0", ",40,50,0")),
                   (True, "\n".join(valid).replace("guard_events,1", "guard_events,-1")),
                   (True, "\n".join(valid).replace("guard_events,1", "guard_events,bad")),
                   # The baseline neither hands notifications nor evaluates tests.
                   (True, "\n".join(valid).replace("guard_wake_tests,0,0", "guard_wake_tests,1,0")),
                   (True, "\n".join(valid).replace("guard_handoff,0,0,0,0,0", "guard_handoff,1,0,0,0,0")),
                   # More samples than hand-offs, and a maximum above the sum.
                   (True, "\n".join(valid).replace("guard_handoff,0,0,0,0,0", "guard_handoff,0,1,5,5,0")),
                   (True, "\n".join(valid).replace("guard_handoff,0,0,0,0,0", "guard_handoff,0,0,0,5,0"))]
        for diagnostic, stderr in faults:
            (out / "record.stderr").write_text(stderr)
            try:
                record(prefix, "pilot1", "baseline", 2, 100, diagnostic, out)
            except ValueError:
                continue
            raise ValueError("missing or forbidden instrumentation escaped")
    print("fairness summary sensitivity fixtures passed")


def main():
    if sys.argv[1:] == ["self-test"]:
        self_test()
    elif sys.argv[1] == "record":
        _, _, prefix, pass_id, arm, n, rounds, diagnostic, out = sys.argv
        record(prefix, pass_id, arm, int(n), int(rounds), diagnostic == "1", Path(out))
    elif sys.argv[1] == "init":
        initialize(Path(sys.argv[2]), sys.argv[3] == "1")
    elif sys.argv[1] == "judge":
        with Path(sys.argv[2]).open() as stream:
            rows = list(csv.DictReader(stream))
        verdict, details = judge(rows, int(sys.argv[3]))
        print("\n".join(details))
        print("verdict," + verdict)
    elif sys.argv[1] == "diagnostics":
        with Path(sys.argv[2]).open() as stream:
            timing = list(csv.DictReader(stream))
        with Path(sys.argv[3]).open() as stream:
            diagnostic = list(csv.DictReader(stream))
        lines = predictions(timing, diagnostic, int(sys.argv[4]))
        print(f"diagnostic_integrity,{3 * (len(ARMS) - 1)}_cells,no_acceptance_judgment")
        print("\n".join(lines))
    else:
        raise ValueError("expected record, judge, or self-test")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, IndexError) as error:
        sys.exit(f"invalid fairness evidence: {error}")
