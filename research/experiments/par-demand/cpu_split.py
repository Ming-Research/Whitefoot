#!/usr/bin/env python3
"""Temporary: where ordinary --par spends its extra CPU (DESIGN.md, "Where
today's --par spends its extra CPU"). Arms seq, par and nospin (par's object
linked against a core.c whose idle window is zero) at four and eight workers.
Removed with the commit that records the result.
"""
import argparse
import csv
import json
import os
import platform
import statistics
import subprocess
from pathlib import Path
from measure import one_cpu_per_core, run
from summarize import MANIFEST

ARMS = ("seq", "par", "nospin")
WIDTHS = (4, 8)
EVENTS = ("task-clock", "context-switches", "cpu-migrations", "page-faults",
          "cycles:u", "instructions:u", "cycles:k", "instructions:k")


def settings(env, name):
    meta = MANIFEST[name]
    values = dict(env)
    if "repetitions" in meta:
        values["WFD_REPETITIONS"] = str(meta["repetitions"])
        values["WFD_EXTENT"] = str(meta["extent"])
    return values


def schedule(rounds):
    """Every (round, workload, width, arm), rotated and alternately reversed
    per round as measure.py interleaves its arms."""
    for round_id in range(rounds):
        names = list(MANIFEST)
        names = names[round_id % len(names):] + names[:round_id % len(names)]
        if round_id % 2:
            names.reverse()
        for name in names:
            widths = list(WIDTHS) if round_id % 2 == 0 else list(reversed(WIDTHS))
            for width in widths:
                arms = list(ARMS)
                shift = (round_id + WIDTHS.index(width)) % len(arms)
                arms = arms[shift:] + arms[:shift]
                if round_id % 2:
                    arms.reverse()
                for arm in arms:
                    yield round_id, name, width, arm


def perf_available(env):
    """The perf events this host grants as it stands; nothing is changed."""
    paranoid = Path("/proc/sys/kernel/perf_event_paranoid")
    found = dict(paranoid=paranoid.read_text().strip() if paranoid.exists() else None, events={})
    probe = subprocess.run(["perf", "stat", "-x", ",", "-e", ",".join(EVENTS), "--", "true"],
                           capture_output=True, text=True, env=env) if subprocess.run(
        ["sh", "-c", "command -v perf"], capture_output=True).returncode == 0 else None
    if probe is None:
        found["error"] = "perf not installed"
        return found
    found["status"] = probe.returncode
    for line in probe.stderr.splitlines():
        fields = line.split(",")
        if len(fields) > 2 and fields[2] in EVENTS:
            found["events"][fields[2]] = fields[0]
    if probe.returncode:
        found["error"] = probe.stderr[-2000:]
    return found


def median(values):
    return statistics.median(values) if values else None


def ratio(top, bottom):
    return None if top is None or not bottom else top / bottom


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--build", required=True, type=Path)
    parser.add_argument("--rounds", type=int, default=10)
    parser.add_argument("--counter-rounds", type=int, default=3)
    args = parser.parse_args()
    build = args.build.resolve()
    env = dict(os.environ)
    for variable in ("WF_SPLIT_WORK", "WF_SCHED_REPORT", "WF_PAR_DEMAND"):
        env.pop(variable, None)
    cores = one_cpu_per_core()
    if len(cores) < max(WIDTHS):
        raise SystemExit("cpu_split needs one CPU per core for eight workers")
    pinned = {width: cores[:width] for width in WIDTHS}

    def command(arm, name, width, round_id, attempt):
        return ["taskset", "-c", ",".join(map(str, pinned[width])),
                str(build / arm / name), "measure", arm, str(width), str(round_id), str(attempt)]

    perf = perf_available(env)
    identity = dict(host=platform.uname()._asdict(), rounds=args.rounds, counter_rounds=args.counter_rounds,
                    pinned=pinned, perf=perf, manifest=MANIFEST,
                    revision=run(["git", "rev-parse", "HEAD"], capture_output=True).stdout.strip())
    (build / "cpu-split-identity.json").write_text(json.dumps(identity, indent=2) + "\n")

    timed = build / "cpu-split.tsv"
    with timed.open("w") as output:
        for round_id, name, width, arm in schedule(args.rounds):
            print(f"timed round={round_id} {name} W={width} {arm}", flush=True)
            run(command(arm, name, width, round_id, 1),
                env=dict(settings(env, name), WF_WORKERS=str(width)), stdout=output)
            output.flush()

    counters = build / "cpu-split-counters.tsv"
    granted = [event for event, value in perf["events"].items()
               if value not in ("<not supported>", "<not counted>")]
    with counters.open("w") as output:
        if granted:
            for round_id, name, width, arm in schedule(args.counter_rounds):
                print(f"counted round={round_id} {name} W={width} {arm}", flush=True)
                record = build / "perf.csv"
                run(["perf", "stat", "-x", ",", "-o", str(record), "-e", ",".join(granted), "--"]
                    + command(arm, name, width, round_id, 2),
                    env=dict(settings(env, name), WF_WORKERS=str(width)), stdout=subprocess.DEVNULL)
                for line in record.read_text().splitlines():
                    fields = line.split(",")
                    if len(fields) > 2 and fields[2] in granted:
                        output.write(f"{name}\t{arm}\t{width}\t{round_id}\t{fields[2]}\t{fields[0]}\n")

    reports = build / "cpu-split-reports.tsv"
    with reports.open("w") as output:
        for name in MANIFEST:
            for width in WIDTHS:
                for arm in ("par", "nospin"):
                    done = run(command(arm, name, width, 0, 3),
                               env=dict(settings(env, name), WF_WORKERS=str(width), WF_SCHED_REPORT="2"),
                               capture_output=True)
                    line = [text for text in done.stderr.splitlines() if text.startswith("compute:")]
                    output.write(f"{name}\t{arm}\t{width}\t{line[-1] if line else 'no report'}\n")

    samples = {}
    for row in csv.reader(timed.open(), delimiter="\t"):
        name, arm, width, _round, _attempt, sample, wall, cpu, _count = row
        samples.setdefault((name, arm, int(width), int(sample)), []).append((int(wall) / 1e6, int(cpu) / 1e6))
    counted = {}
    for name, arm, width, _round, event, value in csv.reader(counters.open(), delimiter="\t"):
        try:
            counted.setdefault((name, arm, int(width), event), []).append(float(value))
        except ValueError:
            pass
    table = []
    for name in MANIFEST:
        for width in WIDTHS:
            cell = dict(workload=name, width=width)
            steady = {arm: samples.get((name, arm, width, 1), []) for arm in ARMS}
            first = {arm: samples.get((name, arm, width, 0), []) for arm in ARMS}
            wall = {arm: median([s[0] for s in steady[arm]]) for arm in ARMS}
            cpu = {arm: median([s[1] for s in steady[arm]]) for arm in ARMS}
            cell.update(seq_wall_ms=wall["seq"], seq_cpu_ms=cpu["seq"])
            for arm in ("par", "nospin"):
                cell[f"{arm}_wall_ratio"] = ratio(wall[arm], wall["seq"])
                cell[f"{arm}_cpu_ratio"] = ratio(cpu[arm], cpu["seq"])
                cell[f"{arm}_first_call_cpu_above_wall_ms"] = median([s[1] - s[0] for s in first[arm]])
            cell["spin_share"] = ratio(None if cpu["par"] is None or cpu["nospin"] is None
                                       else cpu["par"] - cpu["nospin"], cpu["seq"])
            events = {arm: {event: median(counted.get((name, arm, width, event), [])) for event in granted}
                      for arm in ARMS}
            cell["counters"] = events
            inst = {arm: events[arm].get("instructions:u") for arm in ARMS}
            cyc = {arm: events[arm].get("cycles:u") for arm in ARMS}
            if inst["seq"] and inst["nospin"]:
                cell["extra_user_instructions"] = (inst["nospin"] - inst["seq"]) / inst["seq"]
            if all((inst["seq"], inst["nospin"], cyc["seq"], cyc["nospin"])):
                cell["cycles_per_instruction_ratio"] = (cyc["nospin"] / inst["nospin"]) / (cyc["seq"] / inst["seq"])
            table.append(cell)
    (build / "cpu-split-summary.json").write_text(json.dumps(table, indent=2) + "\n")
    for cell in table:
        print(json.dumps({key: value for key, value in cell.items() if key != "counters"}))


if __name__ == "__main__":
    main()
