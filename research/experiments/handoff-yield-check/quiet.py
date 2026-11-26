"""The host's load for the measurements of this directory, read from
/proc/stat: waiting for a quiet host before a unit of measurement, and the
busy share of the CPUs a unit does not use while it runs, which the plan's
rule for a loaded host reads (research/investigations/driver-handoff/
DESIGN.md, "Second 14900K run").

    python3 quiet.py wait SHARE LONGEST
        waits until every CPU together was at most SHARE busy over two
        seconds, or LONGEST seconds have passed; prints
        `waited=SECONDS quiet=SHARE`
    python3 quiet.py read CPUS
        prints a reading of the CPUs not in the comma list CPUS
    python3 quiet.py since CPUS READING
        prints their busy share since READING

latency.py imports `times`, `share` and `wait`.
"""

import sys
import time


def times(exclude=()):
    """Busy and total jiffies, summed over the CPUs not named in exclude;
    idle and I/O wait count as not busy."""
    busy = total = 0
    with open("/proc/stat", encoding="ascii") as stat:
        for line in stat:
            fields = line.split()
            if not fields or not fields[0].startswith("cpu") or fields[0] == "cpu":
                continue
            if fields[0][3:] in exclude:
                continue
            values = [int(value) for value in fields[1:9]]
            total += sum(values)
            busy += sum(values) - values[3] - values[4]
    return busy, total


def share(before, after):
    """The busy share between two readings of `times`."""
    elapsed = after[1] - before[1]
    return (after[0] - before[0]) / elapsed if elapsed > 0 else 0.0


def wait(limit, longest, sample=2.0):
    """Waits until every CPU was at most `limit` busy over `sample` seconds, or
    `longest` seconds have passed; answers the seconds waited and the last
    share read."""
    waited = 0.0
    while True:
        before = times()
        time.sleep(sample)
        waited += sample
        current = share(before, times())
        if current <= limit or waited >= longest:
            return waited, current


def cpus(text):
    return tuple(cpu for cpu in text.split(",") if cpu)


if __name__ == "__main__":
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command == "wait" and len(sys.argv) == 4:
        waited, current = wait(float(sys.argv[2]), float(sys.argv[3]))
        print(f"waited={waited:.0f} quiet={current:.4f}")
    elif command == "read" and len(sys.argv) == 3:
        print(*times(cpus(sys.argv[2])))
    elif command == "since" and len(sys.argv) == 4:
        before = tuple(int(value) for value in sys.argv[3].split())
        print(f"{share(before, times(cpus(sys.argv[2]))):.4f}")
    else:
        sys.exit(__doc__)
