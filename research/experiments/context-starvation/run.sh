#!/bin/sh
# Compile once, then observe the two output pipes without line buffering.
set -eu

ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
OUT=${OUT:-${RUNNER_TEMP:-/tmp}/context-starvation}
WFC=${WFC:-$ROOT/compiler/target/gate/whitefootc}
mkdir -p "$OUT"
"$WFC" -o "$OUT/witness" "$ROOT/research/experiments/context-starvation/witness.wf"

# Python supplies a monotonic clock and multiplexed pipe reads; shell time
# measures only process exit and cannot timestamp each independent output.
python3 - "$OUT/witness" "$OUT/results.tsv" <<'PY'
import os
import selectors
import statistics
import subprocess
import sys
import time

binary, table_path = sys.argv[1:]
if not {0, 1}.issubset(os.sched_getaffinity(0)):
    raise SystemExit("CPUs 0 and 1 must be available for the two-CPU arms")

table = open(table_path, "w", encoding="utf-8", buffering=1)


def report(line):
    print(line, flush=True)
    print(line, file=table)


report("phase\tpass\trequested_drivers\tcpus\titerations\ttimer_s\tcompute_s\texit_s\tchecksum\tthreads_50ms")


def sample(phase, repetition, drivers, count, cpus="0"):
    env = os.environ.copy()
    env.update(WF_DRIVERS=str(drivers), WF_WORKERS="1")
    command = ["taskset", "-c", cpus, binary, str(count)]
    data = {"timer": bytearray(), "compute": bytearray()}
    observed = {}
    threads = -1
    started = time.monotonic()
    process = subprocess.Popen(command, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, env=env, bufsize=0)
    try:
        with selectors.DefaultSelector() as selector:
            selector.register(process.stdout, selectors.EVENT_READ, "timer")
            selector.register(process.stderr, selectors.EVENT_READ, "compute")
            while selector.get_map():
                elapsed = time.monotonic() - started
                if threads < 0 and elapsed >= 0.05:
                    try:
                        threads = len(os.listdir(f"/proc/{process.pid}/task"))
                    except OSError:
                        threads = 0
                remaining = 30.0 - elapsed
                if threads < 0:
                    remaining = min(remaining, max(0.0, 0.05 - elapsed))
                if remaining <= 0 and threads >= 0:
                    raise RuntimeError("witness exceeded 30 s; no timing result")
                for key, _ in selector.select(remaining):
                    chunk = os.read(key.fd, 4096)
                    if chunk:
                        observed.setdefault(key.data, time.monotonic() - started)
                        data[key.data].extend(chunk)
                    else:
                        selector.unregister(key.fileobj)
            remaining = max(0.001, 30.0 - (time.monotonic() - started))
            code = process.wait(timeout=remaining)
        exited = time.monotonic() - started
    finally:
        if process.poll() is None:
            process.kill()
        process.wait()
        process.stdout.close()
        process.stderr.close()
    if code != 0 or data["timer"] != b"T" or len(data["compute"]) != 9 or data["compute"][:1] != b"C":
        raise RuntimeError(f"invalid run: exit={code}, stdout={bytes(data['timer'])!r}, "
                           f"stderr={bytes(data['compute'])!r}")
    checksum = int.from_bytes(data["compute"][1:], "little")
    report(f"{phase}\t{repetition}\t{drivers}\t{cpus}\t{count}\t{observed['timer']:.6f}\t"
           f"{observed['compute']:.6f}\t{exited:.6f}\t{checksum}\t{threads}")
    return observed["compute"], checksum


# Start small and print the spread before scaling. All calibration is on CI.
sample("probe-zero", 1, 1, 0)
count = 100_000
for step in range(8):
    probes = [sample("probe", index + 1, 1, count)[0] for index in range(3)]
    middle = statistics.median(probes)
    report(f"# probe iterations={count} compute_s min={min(probes):.6f} "
           f"median={middle:.6f} max={max(probes):.6f}")
    if middle >= 0.2:
        break
    count *= 4
else:
    raise SystemExit("could not obtain a useful calibration sample")

# Use one count for all driver/affinity arms; never tune them separately.
count = max(1, min(999_999_999_999, round(count * 2.0 / middle)))
report(f"# selected iterations={count}; target computation observation=2 s")
checksums = {}
for repetition in range(1, 4):
    for phase, drivers, iterations, cpus in [
        ("a", 1, count, "0"), ("b", 2, count, "0"), ("c", 1, 0, "0"),
        ("d", 2, count, "0,1"), ("e", 2, 0, "0,1"),
    ]:
        duration, checksum = sample(phase, repetition, drivers, iterations, cpus)
        prior = checksums.setdefault(iterations, checksum)
        if checksum != prior or (iterations == 0 and checksum != 1):
            raise SystemExit("checksum differs between identical inputs or from the zero-work oracle")
        if iterations and not 1.0 <= duration <= 4.0:
            report("# calibration missed the approximate 2 s target; interpret this row at its measured duration")
table.close()
PY
