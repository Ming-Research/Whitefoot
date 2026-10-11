#!/usr/bin/env python3
"""Maintained shared-take count checks, called by the runtime gate.

Python retains child exit status and validates the count evidence without a
shell pipeline. It supplies no compiler or runtime semantic implementation.
The dispatch-only research sweep imports this validator, never the reverse.
"""

import argparse
import csv
import io
from pathlib import Path
import subprocess
import sys
import unittest


FIELDS = ["mode", "n", "cap", "trial", "bound", "y_at_first_fail",
          "y_at_acquire", "overtakes", "failed_probes", "y_total",
          "drained_before_acquire", "result"]


def validate(output, status, n, cap, trial):
    """Reject incomplete evidence and distinguish bound failure from crashes."""
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


def check_scheduled(row, negative):
    """The fixed schedule must show handoff, or its deliberately missing claim."""
    expected = 65 if negative else 1
    if int(row["overtakes"]) != expected:
        raise ValueError(f"scheduled control expected {expected} overtakes")


class EvidenceControls(unittest.TestCase):
    """Mutated evidence must fail before it can disguise a runtime regression."""

    def output(self, overtakes=65, cap=256, probes=7):
        row = ["contended", 1, cap, 1, 64, 0, overtakes, overtakes, probes, cap,
               int(overtakes == cap),
               "bound_exceeded" if overtakes > 64 else "bound_pass"]
        stream = io.StringIO()
        writer = csv.writer(stream)
        writer.writerow(FIELDS)
        writer.writerow(row)
        return stream.getvalue()

    def test_boundary_and_exceeded(self):
        """Changing >64 to >=64 or admitting 65 fails these two boundary cases."""
        validate(self.output(64), 0, 1, 256, 1)
        validate(self.output(65), 2, 1, 256, 1)

    def test_wrong_child_verdict_is_detected(self):
        """Exit status cannot turn a measured bound violation into a pass."""
        with self.assertRaises(ValueError):
            validate(self.output(65), 0, 1, 256, 1)
        with self.assertRaises(ValueError):
            validate(self.output(64), 2, 1, 256, 1)

    def test_runtime_error_is_not_bound_failure(self):
        """Crash, invalid-run and timeout statuses never count as negative success."""
        for status in (1, 9, 124, -11):
            with self.assertRaises(ValueError):
                validate(self.output(), status, 1, 256, 1)

    def test_missing_rows_or_wrong_counts_are_detected(self):
        """Missing/duplicated rows, false subtraction and incomplete work fail."""
        for output in ("", ",".join(FIELDS) + "\n", self.output() + self.output(),
                       self.output().replace(",65,65,", ",65,64,"),
                       self.output().replace(",7,256,", ",0,256,"),
                       self.output().replace(",7,256,", ",7,255,")):
            with self.assertRaises(ValueError):
                validate(output, 2, 1, 256, 1)

    def test_scheduled_control_requires_its_intended_failure(self):
        """A passing mutant or a different failure count cannot satisfy sensitivity."""
        positive = validate(self.output(1, 65, 1), 0, 1, 65, 1)
        negative = validate(self.output(65, 65, 1), 2, 1, 65, 1)
        check_scheduled(positive, False)
        check_scheduled(negative, True)
        with self.assertRaises(ValueError):
            check_scheduled(positive, True)
        with self.assertRaises(ValueError):
            check_scheduled(negative, False)
        negative["overtakes"] = "64"
        with self.assertRaises(ValueError):
            check_scheduled(negative, True)


def run(binary, n, cap, trial, scheduled=False, negative=False, require_bound=True):
    command = [str(binary), str(n), str(cap), str(trial)]
    if scheduled:
        command.append("--scheduled")
    try:
        child = subprocess.run(command, capture_output=True, text=True,
                               timeout=70, check=False)
    except subprocess.TimeoutExpired:
        raise ValueError("runtime watchdog did not finish; evidence invalid") from None
    if child.stderr:
        print(child.stderr, end="", file=sys.stderr)
    row = validate(child.stdout, child.returncode, n, cap, trial)
    if scheduled:
        check_scheduled(row, negative)
    if require_bound and not negative and child.returncode != 0:
        raise ValueError("required shared-take bound exceeded")
    print(f"{'negative' if negative else 'required'} "
          f"{'scheduled' if scheduled else 'natural'} n={n} cap={cap} trial={trial}: "
          f"overtakes={row['overtakes']} bound={row['bound']} {row['result']}", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--negative-binary", type=Path, required=True)
    parser.add_argument("--require-bound", action="store_true")
    args = parser.parse_args()
    try:
        child = subprocess.run([str(args.binary.resolve()), "--arbitration"],
                               timeout=70, check=False)
        if child.returncode != 0:
            raise ValueError("shared-take arbitration fixture failed")
        run(args.binary.resolve(), 0, 0, 1, require_bound=args.require_bound)
        run(args.binary.resolve(), 1, 65, 1, scheduled=True,
            require_bound=args.require_bound)
        run(args.negative_binary.resolve(), 1, 65, 1, scheduled=True, negative=True)
        # Fixed correctness inventory: uncontended setup, escalation sensitivity,
        # and natural contention at each maintained driver count; no timing sweep.
        for n in (1, 2, 8):
            for trial in (1, 2):
                run(args.binary.resolve(), n, 256, trial, require_bound=args.require_bound)
    except (ValueError, OSError, subprocess.TimeoutExpired) as error:
        print(f"shared-take-check: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
    else:
        sys.exit(main())
