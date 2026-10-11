#!/usr/bin/env python3
"""CI-only count sweep for shared_take_witness.c; removed with its workflow.

Python supplies subprocess deadlines, CSV validation and raw child exit codes
in one caller; shell pipelines would obscure that distinction. It does not
implement runtime semantics or serve as a compiler correctness oracle.
"""

import argparse
import csv
import io
import json
from pathlib import Path
import subprocess
import sys
import time
import unittest


FIELDS = ["mode", "n", "cap", "trial", "bound", "y_at_first_fail",
          "y_at_acquire", "overtakes", "failed_probes", "y_total",
          "drained_before_acquire", "result"]


def validate(output, status, n, cap, trial):
    """Reject missing evidence; only a measured bound violation is XFAIL."""
    reader = csv.DictReader(io.StringIO(output))
    if reader.fieldnames != FIELDS:
        raise ValueError("missing or incorrect CSV header")
    rows = list(reader)
    if len(rows) != 1 or set(rows[0]) != set(FIELDS) or None in rows[0].values():
        raise ValueError("expected exactly one complete CSV row")
    row = rows[0]
    values = {key: int(row[key]) for key in FIELDS if key not in {"mode", "result"}}
    if any(value < 0 for value in values.values()):
        raise ValueError("negative count")
    if (values["n"], values["cap"], values["trial"], values["bound"]) != (n, cap, trial, 64 * n):
        raise ValueError("row does not match invocation and registered bound")
    if row["mode"] != ("contended" if n else "control"):
        raise ValueError("incorrect trial mode")
    if values["y_total"] != n * cap or values["y_at_first_fail"] != 0:
        raise ValueError("incomplete Y work or broken initial-failure handshake")
    if not 0 <= values["y_at_acquire"] <= values["y_total"]:
        raise ValueError("acquisition count outside completed work")
    if values["overtakes"] != values["y_at_acquire"] - values["y_at_first_fail"]:
        raise ValueError("overtaking subtraction does not match raw counts")
    if n and (values["failed_probes"] == 0 or values["overtakes"] == 0):
        raise ValueError("contested trial did not observe X miss the seeded hold")
    if not n and (values["failed_probes"] or values["overtakes"]):
        raise ValueError("uncontended control failed")
    expected_drained = int(n > 0 and values["y_at_acquire"] == n * cap)
    if values["drained_before_acquire"] != expected_drained:
        raise ValueError("drain marker disagrees with counts")
    exceeded = values["overtakes"] > 64 * n
    expected = "bound_exceeded" if exceeded else "bound_pass"
    if row["result"] != expected or status != (2 if exceeded else 0):
        raise ValueError("child exit or verdict disagrees with count criterion")
    return row


class EvidenceControls(unittest.TestCase):
    def output(self, overtakes=65):
        row = ["contended", 1, 256, 1, 64, 0, overtakes, overtakes, 7, 256, 0,
               "bound_exceeded" if overtakes > 64 else "bound_pass"]
        stream = io.StringIO()
        writer = csv.writer(stream)
        writer.writerow(FIELDS)
        writer.writerow(row)
        return stream.getvalue()

    def test_boundary_and_exceeded(self):
        validate(self.output(64), 0, 1, 256, 1)
        validate(self.output(65), 2, 1, 256, 1)

    def test_wrong_child_verdict_is_detected(self):
        with self.assertRaises(ValueError):
            validate(self.output(65), 0, 1, 256, 1)
        with self.assertRaises(ValueError):
            validate(self.output(64), 2, 1, 256, 1)

    def test_runtime_error_is_not_xfail(self):
        for status in (1, 9, 124, -11):
            with self.assertRaises(ValueError):
                validate(self.output(), status, 1, 256, 1)

    def test_missing_rows_or_wrong_counts_are_detected(self):
        for output in ("", ",".join(FIELDS) + "\n", self.output() + self.output(),
                       self.output().replace(",65,65,", ",65,64,"),
                       self.output().replace(",7,256,", ",0,256,"),
                       self.output().replace(",7,256,", ",7,255,")):
            with self.assertRaises(ValueError):
                validate(output, 2, 1, 256, 1)


def sweep(binary, destination, require_bound):
    destination.mkdir(parents=True, exist_ok=True)
    observations = []

    def run(label, n, cap, trial):
        command = [str(binary), str(n), str(cap), str(trial)]
        start = time.monotonic()
        with (destination / f"{label}.stdout").open("w") as stdout, \
                (destination / f"{label}.stderr").open("w") as stderr:
            try:
                child = subprocess.run(command, stdout=stdout, stderr=stderr,
                                       timeout=30, check=False)
            except subprocess.TimeoutExpired:
                (destination / f"{label}.json").write_text(json.dumps(
                    {"command": command, "timeout": True}, indent=2) + "\n")
                raise ValueError(f"{label}: watchdog expired; evidence invalid") from None
        elapsed = time.monotonic() - start
        (destination / f"{label}.json").write_text(json.dumps(
            {"command": command, "exit": child.returncode, "seconds": elapsed}, indent=2) + "\n")
        row = validate((destination / f"{label}.stdout").read_text(), child.returncode,
                       n, cap, trial)
        observations.append({"invocation": label, **row})
        with (destination / "counts.csv").open("w", newline="") as file:
            writer = csv.DictWriter(file, fieldnames=["invocation", *FIELDS])
            writer.writeheader()
            writer.writerows(observations)
        print(f"{label}: overtakes={row['overtakes']} bound={row['bound']} "
              f"failed_probes={row['failed_probes']} {row['result']}", flush=True)
        return elapsed

    run("control", 0, 0, 1)
    pilots = [run(f"pilot-{trial}", 1, 256, trial) for trial in (1, 2)]
    (destination / "pilots.json").write_text(json.dumps(
        {"seconds": pilots, "spread_seconds": max(pilots) - min(pilots)}, indent=2) + "\n")
    if max(pilots) > 10:
        raise ValueError("pilot exceeded ten seconds; full sweep not started")
    for n in (1, 2, 8):
        for cap in (256, 4096, 65536):
            for trial in range(1, 6):
                run(f"n{n}-k{cap}-trial{trial}", n, cap, trial)
    failures = sum(row["result"] == "bound_exceeded" for row in observations)
    verdict = (f"XFAIL: {failures} valid trials exceeded B(N)=64*N; sampled bound rejected."
               if failures else
               "SAMPLED PASS: every valid trial met B(N)=64*N; unbounded schedules remain open.")
    (destination / "verdict.txt").write_text(verdict + "\n")
    print(verdict, flush=True)
    return int(require_bound and failures > 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--require-bound", action="store_true",
                        help="phase 2: fail the sweep when any valid trial exceeds B(N)")
    args = parser.parse_args()
    try:
        return sweep(args.binary.resolve(), args.output, args.require_bound)
    except (ValueError, OSError) as error:
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / "verdict.txt").write_text(f"INVALID: {error}\n")
        print(f"INVALID: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
    else:
        sys.exit(main())
