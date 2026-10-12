#!/usr/bin/env python3
"""CI-only count sweep for shared_take_witness.c; removed with its workflow.

Python supplies subprocess deadlines, CSV validation and raw child exit codes
in one caller; shell pipelines would obscure that distinction. It does not
implement runtime semantics or serve as a compiler correctness oracle.
"""

import argparse
import csv
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import time

# Research consumes the maintained oracle; runtime checks import no research.
CHECKER = Path(__file__).resolve().parents[3] / "compiler/src/backend/completion/shared_take_check.py"
SPEC = importlib.util.spec_from_file_location("shared_take_check", CHECKER)
CHECKS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKS)
FIELDS, validate = CHECKS.FIELDS, CHECKS.validate


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
    verdict = (f"{'FAIL' if require_bound else 'XFAIL'}: {failures} valid trials exceeded B(N)=64*N; sampled bound rejected."
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
        CHECKS.unittest.main(module=CHECKS, argv=[sys.argv[0]])
    else:
        sys.exit(main())
