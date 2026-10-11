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
ARMS = ("baseline", "age", "oldest", "twin")


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
    with (out / "measurements.csv").open("a") as stream:
        csv.writer(stream).writerow([pass_id, arm] + [row[field] for field in FIELDS])
    stderr = Path(prefix + ".stderr").read_text().splitlines()
    counters = [line for line in stderr if line.startswith("guard_counters,")]
    if diagnostic:
        require(len(counters) == 1, "missing or duplicate runtime counter record")
        values = counters[0].split(",")[1:]
        require(len(values) == 4, "wrong runtime counter field count")
        values = list(map(int, values))
        require(all(value >= 0 for value in values), "negative runtime counter")
        normalized = [value / row["count"] for value in values]
        with (out / "counters.csv").open("a") as stream:
            csv.writer(stream).writerow([pass_id, arm, n, rounds, row["count"]] + normalized)
        print("diagnostic,pass,arm," + ",".join(FIELDS) + ",woken_per_acquisition,spurious_per_acquisition,refused_locks_per_acquisition,clock_reads_per_acquisition")
        print(",".join(map(str, ["diagnostic", pass_id, arm] + [row[field] for field in FIELDS] + normalized)))
    else:
        require(not counters, "instrumented arm entered the timing comparison")


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
    require(len(panel) == 24, "comparison needs every arm at each N in both passes")
    noisy = False
    failed = False
    details = []
    for pass_id in ("1", "2"):
        for n in (2, 8, 50):
            cell = {arm: panel[(pass_id, n, arm)] for arm in ARMS}
            require(len({row["checksum"] for row in cell.values()}) == 1, "arms completed different work")
            base = cell["baseline"]["runtime_ns"]
            twin = cell["twin"]["runtime_ns"] / base
            cost = cell["oldest"]["runtime_ns"] / base
            control_ok = base * 95 <= cell["twin"]["runtime_ns"] * 100 <= base * 105
            cost_ok = cell["oldest"]["runtime_ns"] * 100 <= base * 110
            fairness_ok = n != 50 or cell["oldest"]["max_ns"] <= 3_300_000
            noisy |= not control_ok
            failed |= not cost_ok or not fairness_ok
            details.append(f"pass={pass_id},N={n},twin/base={twin:.6f},oldest/base={cost:.6f},oldest_max_ns={cell['oldest']['max_ns']},twin_ok={control_ok},cost_ok={cost_ok},fairness_ok={fairness_ok}")
    verdict = "inconclusive" if noisy else "reject" if failed else "pass"
    return verdict, details


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
        next(row for row in rows if row["arm"] == "oldest" and row["contexts"] == 50)[field] = value
        require(evaluate(rows) == verdict, field + " fault escaped")
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
        if row["arm"] == "oldest":
            row["max_ns"], row["runtime_ns"] = 3_300_000, 110_000_000
        if row["arm"] == "twin":
            row["runtime_ns"] = 95_000_000 if row["pass_id"] == "1" else 105_000_000
    require(evaluate(rows) == "pass", "allowed equality failed")
    # Counter output is mandatory only in diagnostics; both mistakes must
    # fail so missing instrumentation cannot silently become zero counters.
    with tempfile.TemporaryDirectory() as directory:
        out = Path(directory)
        prefix = str(out / "record")
        (out / "record.stdout").write_text(",".join(FIELDS) + "\n2,100,200,20,100,3000000,6000,100000000,2\n")
        for diagnostic, stderr in ((True, ""), (False, "guard_counters,1,2,3,4\n")):
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
    elif sys.argv[1] == "judge":
        with Path(sys.argv[2]).open() as stream:
            rows = list(csv.DictReader(stream))
        verdict, details = judge(rows, int(sys.argv[3]))
        print("\n".join(details))
        print("verdict," + verdict)
    else:
        raise ValueError("expected record, judge, or self-test")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, OSError, IndexError) as error:
        sys.exit(f"invalid fairness evidence: {error}")
