#!/usr/bin/env python3
"""Temporary hosted library experiment; no measurement selects gate acceptance."""
import argparse
import csv
import hashlib
import json
import math
import os
import platform
import random
import statistics
import subprocess
import time
from pathlib import Path

from generate import generate

HERE = Path(__file__).resolve().parent
SHAPES = ("scalar", "bytes", "wide")
IMAGES = SHAPES + tuple(shape + "-par" for shape in SHAPES)
CELLS = tuple((name, mode, 1, 1) for mode, name in enumerate(("grow", "presized", "hit", "sparse-pop", "full-drain"))) + tuple(
    (f"disjoint-{name}-w{workers}", mode, 2, workers)
    for mode, name in enumerate(("grow", "presized", "hit")) for workers in (1, 2))
WORKLOADS = tuple(cell[0] for cell in CELLS)
ARMS = ("base", "twin", "head")
FIELDS = ("phase", "cohort", "block", "shape", "workload", "arm", "members",
          "remaining", "requests", "seed", "tables", "workers", "elapsed_ns", "first_table_ns", "second_table_ns", "checksum", "capacity",
          "wall_seconds", "exit_status", "source_sha256", "binary_sha256", "log")


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def command(arguments, log, cwd=None):
    with log.open("w") as stream:
        stream.write(json.dumps([str(a) for a in arguments]) + "\n")
        stream.flush()
        subprocess.run([str(a) for a in arguments], cwd=cwd, stdout=stream,
                       stderr=subprocess.STDOUT, check=True)


def revision(root):
    return subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()


def identity(args):
    result = {"host": platform.platform(), "machine": platform.machine(),
              "cpu": Path("/proc/cpuinfo").read_text(), "settings": "single-table default sequential; disjoint --par --par-call-grain off with WF_WORKERS=1,2; clang -O2 -falign-functions=64; WF_DRIVERS=1",
              "timer": "std::time monotonic nanoseconds inside WF; fresh process per cell",
              "control": "twin is separately emitted/linked using an identical copied baseline compiler; same build/source noise control, not an independent compiler build",
              "scope": "hosted exploratory; no precise 14900K criterion, no proof of zero grow slowdown; actual disjoint-loop concurrency needs emitted/scheduler evidence",
              "baseline": revision(args.base_root), "head": revision(args.head_root),
              "assets": {p.name: digest(p) for p in sorted(HERE.iterdir()) if p.is_file()}}
    result["arms"] = {}
    for arm in ARMS:
        compiler = getattr(args, arm + "_compiler")
        root = args.head_root if arm == "head" else args.base_root
        result["arms"][arm] = {
            "compiler": str(compiler), "compiler_sha256": digest(compiler),
            "library": {str(p.relative_to(root)): digest(p)
                        for p in sorted((root / "lib/std/collections/hash_map").iterdir()) if p.is_file()}}
    return result


def build(args):
    out = args.output
    (out / "logs").mkdir(parents=True, exist_ok=True)
    generated = out / "generated"
    generated.mkdir(exist_ok=True)
    for shape in SHAPES:
        generate(shape, generated / (shape + ".wf"))
        (generated / (shape + "-par.wf")).write_bytes((generated / (shape + ".wf")).read_bytes())
    for arm in ARMS:
        root = args.head_root if arm == "head" else args.base_root
        command(["make", "-f", HERE / "Makefile", "images", f"ROOT={root}",
                 f"BUILD={out / 'images' / arm}", f"SOURCES={generated}",
                 f"WFC={getattr(args, arm + '_compiler')}"], out / "logs" / (arm + "-build.log"), HERE)
    info = identity(args)
    info["generated"] = {p.name: digest(p) for p in sorted(generated.glob("*.wf"))}
    info["images"] = {arm: {shape: digest(out / "images" / arm / shape) for shape in IMAGES} for arm in ARMS}
    (out / "identities.json").write_text(json.dumps(info, indent=2) + "\n")


def interval(logs):
    # Fixed seed, paired nonparametric bootstrap on independent rotated blocks.
    rng = random.Random(20261013)
    samples = sorted(sum(rng.choices(logs, k=len(logs))) / len(logs) for _ in range(10000))
    return math.exp(samples[249]), math.exp(samples[9749])


def summarize(rows, path):
    summary = {"scope": "Hosted exploratory paired log ratios; bootstrap 95% interval. No acceptance verdict.",
               "cells": [], "maximum_process_wall_seconds": max(float(r["wall_seconds"]) for r in rows)}
    for cohort in sorted({r["cohort"] for r in rows}):
        for shape in SHAPES:
            for workload in WORKLOADS:
                cell = [r for r in rows if r["cohort"] == cohort and r["shape"] == shape and r["workload"] == workload]
                grouped = {arm: sorted((r for r in cell if r["arm"] == arm), key=lambda r: r["block"]) for arm in ARMS}
                base = grouped["base"]
                entry = {"cohort": cohort, "shape": shape, "workload": workload,
                         "blocks": len(base), "arms": {}}
                for arm in ARMS:
                    durations = [int(r["elapsed_ns"]) for r in grouped[arm]]
                    entry["arms"][arm] = {"median_ns": statistics.median(durations), "min_ns": min(durations), "max_ns": max(durations)}
                for arm in ("twin", "head"):
                    logs = [math.log(int(r["elapsed_ns"]) / int(b["elapsed_ns"])) for r, b in zip(grouped[arm], base)]
                    lower, upper = interval(logs)
                    median = math.exp(statistics.median(logs))
                    entry[arm + "_over_base"] = {"median_ratio": median, "paired_log_mean_ratio": math.exp(statistics.mean(logs)),
                                                 "bootstrap_95_low": lower, "bootstrap_95_high": upper}
                twin = entry["twin_over_base"]
                entry["registered_twin_resolution_unresolved"] = (not 0.99 <= twin["median_ratio"] <= 1.01
                                                                    or twin["bootstrap_95_high"] - twin["bootstrap_95_low"] > 0.02)
                summary["cells"].append(entry)
    path.write_text(json.dumps(summary, indent=2) + "\n")
    return summary


def measure(args):
    out = args.output
    manifest = json.loads((out / "identities.json").read_text())
    current = identity(args)
    for key in ("baseline", "head", "assets", "arms"):
        if manifest[key] != current[key]:
            raise RuntimeError(f"construction identity changed: {key}")
    if args.phase == "panel":
        if not args.pilot_evidence:
            raise RuntimeError("panel needs a prior pilot artifact selected after inspection")
        prior = json.loads((args.pilot_evidence / "library/identities.json").read_text())
        for key in ("baseline", "head", "assets", "generated"):
            if prior[key] != manifest[key]:
                raise RuntimeError(f"prior pilot differs: {key}")
        pilot = json.loads((args.pilot_evidence / "library/pilot-summary.json").read_text())
        local_pilot = json.loads((out / "pilot-summary.json").read_text())
        with (out / "pilot-raw.tsv").open() as stream:
            pilot_rows = list(csv.DictReader(stream, delimiter="\t"))
        pilot_blocks = len({r["block"] for r in pilot_rows})
        pilot_members = int(pilot_rows[0]["members"])
        # Process wall includes untimed construction/verification/cleanup; use
        # that conservative total for sizing, not the much smaller timed kernel.
        predicted = (sum(float(r["wall_seconds"]) for r in pilot_rows) / pilot_blocks
                     * args.members / pilot_members * 2 * args.rounds * 2)
        (out / "scale-selection.json").write_text(json.dumps({
            "prior_pilot": str(args.pilot_evidence), "prior_maximum_process_wall_seconds": pilot["maximum_process_wall_seconds"],
            "current_host_maximum_process_wall_seconds": local_pilot["maximum_process_wall_seconds"],
            "conservative_linear_wall_prediction_seconds": predicted,
            "sizing_limit_seconds": 1800,
            "selected_panel_rounds": args.rounds, "members": args.members, "remaining": args.remaining,
            "requests": args.requests, "basis": "explicit panel dispatch after prior pilot inspection; hosted resolution remains exploratory"
        }, indent=2) + "\n")
        if predicted > 1800:
            raise RuntimeError("current-host pilot predicts over 30 minutes; inspect scale-selection.json before another panel dispatch (not a performance regression verdict)")
    rows = []
    raw = out / (args.phase + "-raw.tsv")
    with raw.open("w", newline="") as stream:
        writer = csv.DictWriter(stream, FIELDS, delimiter="\t")
        writer.writeheader()
        # Panel cohorts are separate independent blocks, not two reductions of one sample.
        cohorts = (0,) if args.phase == "pilot" else (0, 1)
        for cohort in cohorts:
            for block in range(args.rounds):
                shift = (block + cohort) % 3
                order = ARMS[shift:] + ARMS[:shift]
                if cohort == 1:
                    order = tuple(reversed(order))
                for shape in SHAPES:
                    for workload, mode, tables, workers in CELLS:
                        for arm in order:
                            image_name = shape + ("-par" if tables == 2 else "")
                            image = out / "images" / arm / image_name
                            if digest(image) != manifest["images"][arm][image_name]:
                                raise RuntimeError("binary differs from the construction record")
                            stem = f"{args.phase}-c{cohort}-b{block}-{shape}-{workload}-{arm}"
                            log = out / "logs" / (stem + ".json")
                            env = dict(os.environ)
                            for name in ("WF_WORKERS", "WF_SPLIT_WORK", "WF_SCHED_REPORT", "WF_STACKS", "WF_IO_HELPERS"):
                                env.pop(name, None)
                            env.update(WF_DRIVERS="1", WF_WORKERS=str(workers))
                            cmd = [str(image), str(args.members), str(mode), str(args.remaining), str(args.requests), str(tables)]
                            started = time.monotonic()
                            log.write_text(json.dumps({"command": cmd, "status": "started", "environment": {"WF_DRIVERS": "1", "WF_WORKERS": str(workers)}}, indent=2) + "\n")
                            try:
                                process = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=300)
                            except subprocess.TimeoutExpired as error:
                                def decoded(value):
                                    return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
                                process = subprocess.CompletedProcess(cmd, 124, decoded(error.stdout), decoded(error.stderr))
                            wall = time.monotonic() - started
                            log.write_text(json.dumps({"command": cmd, "environment": {"WF_DRIVERS": "1", "WF_WORKERS": str(workers)}, "wall_seconds": wall,
                                                       "status": process.returncode, "stdout": process.stdout, "stderr": process.stderr}, indent=2) + "\n")
                            values = process.stdout.splitlines()
                            row = dict(phase=args.phase, cohort=cohort, block=block, shape=shape, workload=workload, arm=arm,
                                       members=args.members, remaining=args.remaining, requests=args.requests,
                                       seed=88172645463325252, tables=tables, workers=workers, elapsed_ns="", first_table_ns="", second_table_ns="", checksum="", capacity="", wall_seconds=wall,
                                       exit_status=process.returncode, source_sha256=manifest["generated"][image_name + ".wf"],
                                       binary_sha256=manifest["images"][arm][image_name], log=str(log.relative_to(out)))
                            valid = process.returncode == 0 and len(values) == 5 and all(v.isdecimal() for v in values)
                            if valid:
                                row.update(elapsed_ns=int(values[0]), checksum=int(values[1]), capacity=int(values[2]),
                                           first_table_ns=int(values[3]), second_table_ns=int(values[4]))
                                valid = row["elapsed_ns"] > 0
                            writer.writerow(row)
                            stream.flush()
                            if not valid:
                                raise RuntimeError(f"incomplete benchmark row: {stem}; see {log}")
                            rows.append(row)
    summary = summarize(rows, out / (args.phase + "-summary.json"))
    print(json.dumps(summary, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase", required=True, choices=("build", "pilot", "panel"))
    parser.add_argument("--base-root", type=Path, required=True)
    parser.add_argument("--head-root", type=Path, required=True)
    for arm in ARMS:
        parser.add_argument("--" + arm + "-compiler", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--pilot-evidence", type=Path)
    parser.add_argument("--rounds", type=int, default=3, help="independent blocks per cohort; panel has two cohorts")
    parser.add_argument("--members", type=int, default=1024)
    parser.add_argument("--remaining", type=int, default=128)
    parser.add_argument("--requests", type=int, default=32)
    args = parser.parse_args()
    if os.environ.get("GITHUB_ACTIONS") != "true":
        parser.error("builds and measurements run in GitHub Actions only")
    if args.rounds < 3 or not 0 < args.remaining <= args.members <= 1000000 or not 0 < args.requests <= args.remaining:
        parser.error("need rounds>=3 and 0<requests<=remaining<=members<=1000000")
    if args.phase == "panel" and (args.members != 1000000 or args.remaining != 1000):
        parser.error("registered panel uses 1000000 members and 1000 sparse survivors")
    if args.phase == "build":
        build(args)
    else:
        measure(args)


if __name__ == "__main__":
    main()
