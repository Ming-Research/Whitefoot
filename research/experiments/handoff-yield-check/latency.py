"""The latency runs of the 14900K measurements in
research/investigations/driver-handoff/DESIGN.md ("Plan for the 14900K
measurements", items 1 and 2, under the rule for a loaded host of "Second
14900K run"), base against candidate, interleaved run by run.

    python3 latency.py OUT

OUT/bin holds, for each side (base, candidate), the images ready_SIDE
(tests/programs/ready_beside_computations.wf), poll_SIDE
(tests/programs/lone_timer_writer.wf) and turn_SIDE
(tests/programs/poll_contexts.wf). Each image runs on the CPUs LATENCY_CPUS
lists (default 0,2,4,6, four physical P-cores of the 14900K) and this script
on HARNESS_CPU (default 16, an E-core; empty for none). Writes
OUT/latency.tsv, one row per attempt:

    shape side route drivers attempt status qualified clean background
    reassignments yield_requests stolen wall_us

`background` is the busy share of every other CPU over the attempt, and an
attempt is clean when it is at most QUIET (0.05); one that is not is kept but
counts toward nothing, and the script waits for a quiet host (quiet.py)
before the next attempt, as it does before each shape. `ready`: the exit
status is the time in milliseconds from letting the guard go to the waiting
context's first step, rounded down, so a status of 3 or less is a start
within 4 ms. The status means the same on any placement, so every clean
attempt counts against that bound. At two drivers an attempt qualifies when
the scheduler report shows a reassignment, which only decides how many
attempts are made: each side makes attempts until 64 clean ones qualify, at
most 128 clean ones. `poll`: the exit status is the largest lateness in
milliseconds of sixteen 1 ms sleeps and of a read whose byte is written 20 ms
after the program's first output byte, beside a context polling an object; 64
clean runs per side and configuration. `turn`: the wall time of a whole run of
a context polling an object until a context it started on its one driver
writes it, 64 clean runs per side. Each side stops after eight times its
wanted count of attempts, clean or not. The script judges nothing; it prints
the counts the plan's criteria read.
"""

import os
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import quiet  # noqa: E402

OUT = Path(sys.argv[1])
BIN = OUT / "bin"
SIDES = ("base", "candidate")
WANTED = 64
QUIET = 0.05
LONGEST_WAIT = 600
LATENCY_CPUS = os.environ.get("LATENCY_CPUS", "0,2,4,6")
HARNESS_CPU = os.environ.get("HARNESS_CPU", "16")
OWN = quiet.cpus(LATENCY_CPUS) + quiet.cpus(HARNESS_CPU)
if HARNESS_CPU:
    os.sched_setaffinity(0, {int(HARNESS_CPU)})
table = open(OUT / "latency.tsv", "w", encoding="utf-8", buffering=1)
print("shape\tside\troute\tdrivers\tattempt\tstatus\tqualified\tclean\tbackground\treassignments"
      "\tyield_requests\tstolen\twall_us", file=table)


def counter(stderr, name):
    return sum(int(value) for value in re.findall(rf"\b{name}=(\d+)", stderr))


def environment(route, drivers):
    env = dict(os.environ, WF_DRIVERS=str(drivers), WF_SCHED_REPORT="2")
    env.pop("WF_IO_NO_NATIVE_RING", None)
    if route == "no-ring":
        env["WF_IO_NO_NATIVE_RING"] = "1"
    return env


def settle():
    waited, share = quiet.wait(QUIET, LONGEST_WAIT)
    print(f"quiet: waited {waited:.0f} s, last share {share:.4f}", flush=True)


def run(image, route, drivers, feed=None):
    env = environment(route, drivers)
    before = quiet.times(OWN)
    began = time.monotonic_ns()
    process = subprocess.Popen(["taskset", "-c", LATENCY_CPUS, str(image)], env=env,
                               stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if feed is not None:
        first = process.stdout.read(1)
        if first:
            time.sleep(feed)
            process.stdin.write(b"z")
            process.stdin.flush()
    try:
        out, err = process.communicate(timeout=60)
    except subprocess.TimeoutExpired:
        process.kill()
        out, err = process.communicate()
    wall = (time.monotonic_ns() - began) // 1000
    background = quiet.share(before, quiet.times(OWN))
    return process.returncode, err.decode("utf-8", "replace"), wall, background


def record(shape, side, route, drivers, index, status, qualified, background, stderr, wall):
    clean = background <= QUIET
    row = (shape, side, route, drivers, index, status, int(qualified), int(clean), f"{background:.4f}",
           counter(stderr, "reassignments"), counter(stderr, "yield_requests"),
           counter(stderr, "stolen_contexts"), wall)
    print("\t".join(str(field) for field in row), file=table)
    print(" ".join(str(field) for field in row), flush=True)
    if not clean:
        settle()
    return clean


def ready(drivers):
    clean = {side: 0 for side in SIDES}
    qualified = {side: 0 for side in SIDES}
    attempts = {side: 0 for side in SIDES}

    def going(side):
        return qualified[side] < WANTED and clean[side] < 2 * WANTED and attempts[side] < 8 * WANTED

    settle()
    while any(going(side) for side in SIDES):
        for side in SIDES:
            if not going(side):
                continue
            attempts[side] += 1
            status, stderr, wall, background = run(BIN / f"ready_{side}", "io_uring", drivers)
            placed = drivers == 1 or counter(stderr, "reassignments") > 0
            if record("ready", side, "io_uring", drivers, attempts[side], status, placed, background,
                      stderr, wall):
                clean[side] += 1
                qualified[side] += placed


def repeated(shape, image, route, drivers, feed=None):
    clean = {side: 0 for side in SIDES}
    attempts = {side: 0 for side in SIDES}

    def going(side):
        return clean[side] < WANTED and attempts[side] < 8 * WANTED

    settle()
    while any(going(side) for side in SIDES):
        for side in SIDES:
            if not going(side):
                continue
            attempts[side] += 1
            status, stderr, wall, background = run(BIN / f"{image}_{side}", route, drivers, feed)
            if record(shape, side, route, drivers, attempts[side], status, True, background, stderr, wall):
                clean[side] += 1


for drivers in (1, 2):
    ready(drivers)
for route, drivers in (("io_uring", 1), ("no-ring", 1), ("io_uring", 2)):
    repeated("poll", "poll", route, drivers, feed=0.020)
repeated("turn", "turn", "io_uring", 1)
table.close()

rows = [line.rstrip("\n").split("\t") for line in open(OUT / "latency.tsv", encoding="utf-8")][1:]
print("summary: shape side route drivers attempts clean clean-qualified clean-over-3ms clean-max-status "
      "clean-median-wall-us voided voided-over-3ms")
groups = {}
for shape, side, route, drivers, _, status, qualified, clean, _, _, _, _, wall in rows:
    groups.setdefault((shape, side, route, drivers), []).append(
        (int(status), qualified == "1", clean == "1", int(wall)))
for key, runs in sorted(groups.items()):
    kept = [run for run in runs if run[2]]
    voided = [run for run in runs if not run[2]]
    print("summary:", *key, len(runs), len(kept), sum(run[1] for run in kept),
          sum(run[0] > 3 for run in kept), max((run[0] for run in kept), default="-"),
          statistics.median(run[3] for run in kept) if kept else "-",
          len(voided), sum(run[0] > 3 for run in voided))
