"""The latency runs of the 14900K measurements in
research/investigations/driver-handoff/DESIGN.md ("Plan for the 14900K
measurements", items 1 and 2), base against candidate, interleaved run by run.

    python3 latency.py OUT

OUT/bin holds, for each side (base, candidate), the images ready_SIDE
(tests/programs/ready_beside_computations.wf), poll_SIDE
(tests/programs/lone_timer_writer.wf) and turn_SIDE
(tests/programs/poll_contexts.wf). Writes OUT/latency.tsv, one row per run:

    shape side route drivers run status qualified reassignments
    yield_requests stolen wall_us

`ready`: the exit status is the time in milliseconds from letting the guard go
to the waiting context's first step, rounded down, so a status of 3 or less is
a start within 4 ms. At two drivers a run qualifies when the scheduler report
shows a reassignment, so that the waiting context was served beside
computations on both drivers; each side needs 64 qualifying runs and makes at
most 128 attempts. `poll`: the exit status is the largest lateness in
milliseconds of sixteen 1 ms sleeps and of a read whose byte is written 20 ms
after the program's first output byte, beside a context polling an object;
64 runs per side and configuration. `turn`: the wall time of a whole run of a
context polling an object until a context it started on its one driver
writes it, 64 runs per side. The script judges nothing; it prints the counts
the plan's criteria read.
"""

import os
import re
import subprocess
import sys
import time
from pathlib import Path

OUT = Path(sys.argv[1])
BIN = OUT / "bin"
SIDES = ("base", "candidate")
WANTED = 64
table = open(OUT / "latency.tsv", "w", encoding="utf-8", buffering=1)
print("shape\tside\troute\tdrivers\trun\tstatus\tqualified\treassignments\tyield_requests\tstolen\twall_us",
      file=table)


def counter(stderr, name):
    return sum(int(value) for value in re.findall(rf"\b{name}=(\d+)", stderr))


def environment(route, drivers):
    env = dict(os.environ, WF_DRIVERS=str(drivers), WF_SCHED_REPORT="2")
    env.pop("WF_IO_NO_NATIVE_RING", None)
    if route == "no-ring":
        env["WF_IO_NO_NATIVE_RING"] = "1"
    return env


def run(image, route, drivers, feed=None):
    env = environment(route, drivers)
    began = time.monotonic_ns()
    process = subprocess.Popen([str(image)], env=env, stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE)
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
    return process.returncode, err.decode("utf-8", "replace"), wall


def record(shape, side, route, drivers, index, status, qualified, stderr, wall):
    row = (shape, side, route, drivers, index, status, int(qualified), counter(stderr, "reassignments"),
           counter(stderr, "yield_requests"), counter(stderr, "stolen_contexts"), wall)
    print("\t".join(str(field) for field in row), file=table)
    print(" ".join(str(field) for field in row), flush=True)


def ready(drivers):
    qualified = {side: 0 for side in SIDES}
    attempts = {side: 0 for side in SIDES}
    while any(qualified[side] < WANTED and attempts[side] < 2 * WANTED for side in SIDES):
        for side in SIDES:
            if qualified[side] >= WANTED or attempts[side] >= 2 * WANTED:
                continue
            attempts[side] += 1
            status, stderr, wall = run(BIN / f"ready_{side}", "io_uring", drivers)
            placed = drivers == 1 or counter(stderr, "reassignments") > 0
            qualified[side] += placed
            record("ready", side, "io_uring", drivers, attempts[side], status, placed, stderr, wall)


def repeated(shape, image, route, drivers, feed=None):
    for index in range(1, WANTED + 1):
        for side in SIDES:
            status, stderr, wall = run(BIN / f"{image}_{side}", route, drivers, feed)
            record(shape, side, route, drivers, index, status, True, stderr, wall)


for drivers in (1, 2):
    ready(drivers)
for route, drivers in (("io_uring", 1), ("no-ring", 1), ("io_uring", 2)):
    repeated("poll", "poll", route, drivers, feed=0.020)
repeated("turn", "turn", "io_uring", 1)
table.close()

rows = [line.rstrip("\n").split("\t") for line in open(OUT / "latency.tsv", encoding="utf-8")][1:]
print("summary: shape side route drivers qualified over-3ms max-status")
groups = {}
for shape, side, route, drivers, _, status, qualified, *_ in rows:
    if qualified == "1":
        groups.setdefault((shape, side, route, drivers), []).append(int(status))
for key, statuses in sorted(groups.items()):
    print("summary:", *key, len(statuses), sum(status > 3 for status in statuses), max(statuses))
