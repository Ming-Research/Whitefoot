#!/usr/bin/env python3
"""Manual, interleaved same-image comparisons; timing lives in the native runner.

The native runner cannot interleave independent executables or apply the
prospective re-run rule, so this script owns those two experiment-only jobs.
"""
import argparse
import hashlib
import json
import os
import platform
import subprocess
from pathlib import Path
from summarize import ARMS, WIDTHS, MANIFEST, attempt_result, load, summarize


def run(command, **kwargs):
    return subprocess.run(command, check=True, text=True, **kwargs)


def one_cpu_per_core():
    """The first logical CPU of each physical core, from Linux's sibling lists.

    A timed process of width W runs on the first max(W, 1) of these, the same
    set for every arm of that width, so no two of its threads share a core's
    hardware threads and the host cannot migrate it between rounds. Empty
    where the lists or taskset are unavailable; the run then records that it
    was not pinned."""
    root = Path("/sys/devices/system/cpu")
    if not root.exists() or subprocess.run(["sh", "-c", "command -v taskset"],
                                           capture_output=True).returncode:
        return []
    firsts = set()
    for siblings in root.glob("cpu[0-9]*/topology/thread_siblings_list"):
        first = siblings.read_text().strip().replace("-", ",").split(",")[0]
        firsts.add(int(first))
    return sorted(firsts)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--build", required=True, type=Path)
    parser.add_argument("--rounds", type=int, default=10)
    parser.add_argument("--sizing", action="store_true")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    if args.rounds < 1:
        parser.error("rounds must be positive")
    build = args.build.resolve()
    env = dict(os.environ)
    for variable in ("WF_SPLIT_WORK", "WF_SCHED_REPORT", "WF_PAR_DEMAND"):
        env.pop(variable, None)
    env["WF_PAR_DEMAND"] = "off-never-request"
    hashes = {}
    for name in MANIFEST:
        digest = lambda arm: hashlib.sha256((build / arm / name).read_bytes()).hexdigest()
        hashes[name] = {arm: digest(arm) for arm in ARMS}
        if hashes[name]["demand"] != hashes[name]["twin"]:
            raise ValueError(f"{name}: candidate/twin images differ")
    cores = one_cpu_per_core()
    pinned = {width: cores[:max(width, 1)] for width in WIDTHS} if len(cores) >= max(WIDTHS) else {}
    def pin(width, command):
        cpus = pinned.get(width)
        return ["taskset", "-c", ",".join(map(str, cpus))] + command if cpus else command
    identity = dict(host=platform.uname()._asdict(), sizing=args.sizing, rounds=args.rounds, pinned=pinned,
                    revision=run(["git", "rev-parse", "HEAD"], capture_output=True).stdout.strip(),
                    dirty=run(["git", "status", "--porcelain"], capture_output=True).stdout,
                    hashes=hashes, setting="off-never-request", manifest=MANIFEST)
    (build / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    if args.verify_only:
        for name in MANIFEST:
            for arm in ARMS:
                for width in (1, 4):
                    for setting in ("on", "off-never-request"):
                        run([str(build / arm / name), "verify"],
                            env=dict(env, WF_WORKERS=str(width), WF_PAR_DEMAND=setting))
        return
    path = build / "measurements.tsv"
    def batch(attempt, selected):
        with path.open("w" if attempt == 1 else "a") as output:
            for round_id in range(args.rounds):
                names = list(MANIFEST)
                names = names[round_id % len(names):] + names[:round_id % len(names)]
                if round_id % 2:
                    names.reverse()
                for name in names:
                    meta = MANIFEST[name]
                    settings = dict(env)
                    if "repetitions" in meta:
                        settings["WFD_REPETITIONS"] = str(meta.get("sizing_repetitions", meta["repetitions"]) if args.sizing else meta["repetitions"])
                        settings["WFD_EXTENT"] = str(meta.get("sizing_extent", meta["extent"]) if args.sizing else meta["extent"])
                    widths = list(WIDTHS)
                    widths = widths[round_id % 3:] + widths[:round_id % 3]
                    for width in widths:
                        if (name, width) not in selected:
                            continue
                        arms = list(ARMS)
                        shift = (round_id + WIDTHS.index(width)) % len(arms)
                        arms = arms[shift:] + arms[:shift]
                        if round_id % 2:
                            arms.reverse()
                        for arm in arms:
                            print(f"attempt={attempt} round={round_id} {name} W={width} {arm}", flush=True)
                            run(pin(width, [str(build / arm / name), "measure", arm, str(width), str(round_id), str(attempt)]),
                                env=dict(settings, WF_WORKERS=str(width)), stdout=output)
                            output.flush()
    batch(1, {(name, width) for name in MANIFEST for width in WIDTHS})
    groups = load(path)
    def decisions(name):
        meta = MANIFEST[name]
        repetitions = meta.get("sizing_repetitions", meta.get("repetitions", 0)) if args.sizing else meta.get("repetitions", 0)
        return meta.get("decisions_per_repetition", 0) * repetitions
    rerun = {(name, width) for (name, width, attempt), arms in groups.items()
             if attempt_result(arms, width, decisions(name))["status"] == "exceeds"}
    if rerun:
        batch(2, rerun)
    inspection_path = build / "inspection.json"
    inspection = json.loads(inspection_path.read_text()) if inspection_path.exists() else {}
    (build / "summary.json").write_text(json.dumps(summarize(path, inspection, args.sizing), indent=2) + "\n")


if __name__ == "__main__":
    main()
