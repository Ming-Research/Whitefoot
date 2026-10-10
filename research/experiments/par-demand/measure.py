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
import shutil
import subprocess
from pathlib import Path
from summarize import (E5A_PADDING, experiment_matrix, parse_experiment, attempt_result,
                       attempt_result_e2, attempt_result_e4, e2_excluded, load, summarize)


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


def cpu_list(text):
    """Linux cpulist syntax, including multiple ranges."""
    result = set()
    for part in text.strip().split(","):
        if not part:
            continue
        bounds = part.split("-")
        first, last = (int(bounds[0]), int(bounds[-1]))
        if len(bounds) > 2 or first < 0 or last < first:
            raise ValueError(f"invalid CPU list: {text}")
        result.update(range(first, last + 1))
    return result


def performance_cores(root=Path("/sys/devices/system/cpu"),
                      performance_file=Path("/sys/devices/cpu_core/cpus"), available=None, minimum_cores=8):
    """One first sibling per P-core; homogeneous hosts fall back to all cores.

    Keep source contents (not only a derived CPU count) in identity.json. A
    restricted affinity must still include the requested number of P-cores;
    never silently substitute SMT siblings or efficiency cores.
    """
    files = {}
    def record(path):
        value = path.read_text()
        files[str(path)] = value
        return value
    if available is None:
        available = os.sched_getaffinity(0)
    performance = cpu_list(record(performance_file)) if performance_file.exists() else None
    firsts = set()
    for siblings in sorted(root.glob("cpu[0-9]*/topology/thread_siblings_list")):
        members = cpu_list(record(siblings))
        if not members:
            raise ValueError(f"empty sibling list: {siblings}")
        first = min(members)
        # Select a core only when its first sibling is known to be a P CPU;
        # the task's pin sets never use an arbitrary CPU from the PMU mask.
        if (performance is None or first in performance) and first in available:
            firsts.add(first)
        for field in ("core_id", "physical_package_id", "core_type", "cpu_capacity"):
            path = siblings.parent / field
            if path.exists():
                record(path)
    for name in ("online", "present", "possible"):
        path = root / name
        if path.exists():
            record(path)
    cpuinfo = Path("/proc/cpuinfo")
    if cpuinfo.exists():
        record(cpuinfo)
    cores = sorted(firsts)
    if len(cores) < minimum_cores:
        raise ValueError(f"need {minimum_cores} performance cores with their first siblings allowed; found {cores}")
    return cores, dict(files=files, available_cpus=sorted(available),
                       performance_cpus=sorted(performance) if performance is not None else None,
                       performance_source=str(performance_file) if performance is not None else "all cores (P-core file absent)")


def demand_setting(experiment, arm):
    return "on" if (experiment == 2 and arm in ("demand", "idle1", "twin")
                    or experiment in (3, 4) and arm not in ("seq", "par")
                    or experiment == "5a" and arm in ("demand", "twin")) else "off-never-request"


def layout_identity(build, manifest):
    """Require real text shifts of the reused seq object; retain linked offsets.

    These ELF images are built on Linux by the hosted job. A linker that
    discards or reorders padding fails construction rather than measuring a
    control that changed no placement.
    """
    def symbols(path):
        result = {}
        for line in run(["nm", "-n", "--defined-only", str(path)], capture_output=True).stdout.splitlines():
            fields = line.split()
            if len(fields) == 3 and fields[1] in ("t", "T"):
                result[fields[2]] = int(fields[0], 16)
        return result
    result = {}
    for name in manifest:
        wf_object = build / "seq" / f"{name}.o"
        owned = symbols(wf_object)
        if not owned:
            raise ValueError(f"{name}: seq object has no text symbols")
        base = symbols(build / "seq" / name)
        offsets = {"seq": {symbol: base[symbol] for symbol in owned}}
        for arm, size in E5A_PADDING.items():
            shifted = symbols(build / arm / name)
            offsets[arm] = {symbol: shifted[symbol] for symbol in owned}
            if any(shifted[symbol] - base[symbol] != size for symbol in owned):
                raise ValueError(f"{name}/{arm}: linked WF text did not shift by {size} bytes")
        result[name] = dict(wf_object_sha256=hashlib.sha256(wf_object.read_bytes()).hexdigest(),
                            padding_bytes=E5A_PADDING, linked_text_offsets=offsets)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--build", required=True, type=Path)
    parser.add_argument("--rounds", type=int)
    parser.add_argument("--experiment", type=parse_experiment, choices=(1, 2, 3, 4, "5a"), default=1)
    parser.add_argument("--instrumented", action="store_true")
    parser.add_argument("--sizing", action="store_true")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    if args.experiment in (3, 4, "5a") and args.rounds is not None:
        parser.error("experiments 3, 4 and 5a select their rounds from six sizing rounds; omit --rounds")
    if args.instrumented and (args.experiment != 3 or args.sizing or args.verify_only):
        parser.error("--instrumented requires experiment 3 without sizing or verify-only")
    if args.rounds is None:
        args.rounds = 30 if args.experiment == 2 else 10
    arms_for_run, widths_for_run, manifest = experiment_matrix(args.experiment)
    if args.rounds < 1:
        parser.error("rounds must be positive")
    build = args.build.resolve()
    if args.instrumented != (build / "counter-build").exists():
        parser.error("counter builds require --instrumented and cannot enter timing summaries")
    env = dict(os.environ)
    for variable in ("WF_SPLIT_WORK", "WF_SCHED_REPORT", "WF_PAR_DEMAND"):
        env.pop(variable, None)
    env["WF_PAR_DEMAND"] = "off-never-request"
    hashes = {}
    for name in manifest:
        digest = lambda arm: hashlib.sha256((build / arm / name).read_bytes()).hexdigest()
        hashes[name] = {arm: digest(arm) for arm in arms_for_run}
        if hashes[name]["demand"] != hashes[name]["twin"]:
            raise ValueError(f"{name}: candidate/twin images differ")
    topology = None
    if args.experiment in (2, 3, 4, "5a") and not args.verify_only:
        if platform.system() != "Linux" or shutil.which("taskset") is None:
            raise ValueError("experiments 2, 3, 4 and 5a require Linux topology and taskset pinning")
        cores, topology = performance_cores(minimum_cores=1) if args.experiment == "5a" else performance_cores()
    else:
        # Verification has no timing verdict and may run on the hosted sizing
        # machine. Every experiment-2 timing batch, sizing included, is strict.
        cores = one_cpu_per_core()
    pinned = {width: cores[:max(width, 1)] for width in widths_for_run} if len(cores) >= max(widths_for_run) else {}
    if args.experiment == "5a" and not args.verify_only:
        if 2 not in cores or topology["performance_cpus"] is None or 2 not in topology["performance_cpus"]:
            raise ValueError("experiment 5a requires CPU 2 to be an allowed first P-core sibling")
        pinned = {1: [2]}
    layout = layout_identity(build, manifest) if args.experiment == "5a" else None
    def pin(width, command):
        cpus = pinned.get(width)
        return ["taskset", "-c", ",".join(map(str, cpus))] + command if cpus else command
    identity = dict(host=platform.uname()._asdict(), sizing=args.sizing, rounds=args.rounds, pinned=pinned,
                    revision=run(["git", "rev-parse", "HEAD"], capture_output=True).stdout.strip(),
                    dirty=run(["git", "status", "--porcelain"], capture_output=True).stdout,
                    hashes=hashes, experiment=args.experiment, topology=topology,
                    settings={arm: demand_setting(args.experiment, arm) for arm in arms_for_run},
                    setting="on" if args.experiment in (2, 3, 4, "5a") else "off-never-request", manifest=manifest,
                    instrumented=args.instrumented, layout=layout)
    (build / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    if args.verify_only:
        for name in manifest:
            for arm in arms_for_run:
                for width in widths_for_run if args.experiment in (2, 3, 4, "5a") else (1, 4):
                    for setting in ("on", "off-never-request"):
                        run([str(build / arm / name), "verify"],
                            env=dict(env, WF_WORKERS=str(width), WF_PAR_DEMAND=setting))
        return
    if args.instrumented:
        output_dir = build / "counters"
        output_dir.mkdir(exist_ok=True)
        for name, meta in manifest.items():
            settings = dict(env)
            if "repetitions" in meta:
                settings.update(WFD_REPETITIONS=str(meta["repetitions"]), WFD_EXTENT=str(meta["extent"]))
            for width in widths_for_run:
                for arm in arms_for_run:
                    with (output_dir / f"{name}-{width}-{arm}.txt").open("w") as output:
                        run(pin(width, [str(build / arm / name), "measure", arm, str(width), "0", "1"]),
                            env=dict(settings, WF_WORKERS=str(width), WF_SCHED_REPORT="2",
                                     WF_PAR_DEMAND=demand_setting(3, arm)), stdout=output, stderr=output)
        return
    path = build / "measurements.tsv"
    def batch(attempt, selected):
        with path.open("w" if attempt == 1 else "a") as output:
            for round_id in range(args.rounds):
                names = list(manifest)
                names = names[round_id % len(names):] + names[:round_id % len(names)]
                if round_id % 2:
                    names.reverse()
                for name in names:
                    meta = manifest[name]
                    settings = dict(env)
                    if "repetitions" in meta:
                        settings["WFD_REPETITIONS"] = str(meta.get("sizing_repetitions", meta["repetitions"]) if args.sizing and args.experiment not in (3, 4, "5a") else meta["repetitions"])
                        settings["WFD_EXTENT"] = str(meta.get("sizing_extent", meta["extent"]) if args.sizing and args.experiment not in (3, 4, "5a") else meta["extent"])
                    widths = list(widths_for_run)
                    widths = widths[round_id % len(widths):] + widths[:round_id % len(widths)]
                    for width in widths:
                        if (name, width) not in selected:
                            continue
                        arms = list(arms_for_run)
                        shift = (round_id + widths_for_run.index(width)) % len(arms)
                        arms = arms[shift:] + arms[:shift]
                        if round_id % 2:
                            arms.reverse()
                        for arm in arms:
                            print(f"attempt={attempt} round={round_id} {name} W={width} {arm}", flush=True)
                            run(pin(width, [str(build / arm / name), "measure", arm, str(width), str(round_id), str(attempt)]),
                                env=dict(settings, WF_WORKERS=str(width),
                                         WF_PAR_DEMAND=demand_setting(args.experiment, arm)), stdout=output)
                            output.flush()
    selected = {(name, width) for name in manifest for width in widths_for_run}
    if args.experiment in (3, 4, "5a"):
        args.rounds = 6
        batch(1, selected)
        sample = summarize(path, sizing=True, experiment=args.experiment)
        sample_dir = build / f"sizing-e{args.experiment}"
        sample_dir.mkdir(exist_ok=True)
        path.replace(sample_dir / "measurements.tsv")
        (sample_dir / "summary.json").write_text(json.dumps(sample, indent=2) + "\n")
        identity.update(rounds=6, sizing=True)
        (sample_dir / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
        args.rounds = sample["decisive_rounds"]
        identity.update(rounds=args.rounds, sizing=args.sizing, sizing_rounds=6)
        (build / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
        if args.sizing:
            return
        if args.experiment in (3, "5a"):
            batch(1, selected)
            (build / "summary.json").write_text(json.dumps(summarize(path, experiment=args.experiment), indent=2) + "\n")
            return
    batch(1, selected)
    groups = load(path, args.experiment)
    def decisions(name):
        meta = manifest[name]
        repetitions = meta.get("sizing_repetitions", meta.get("repetitions", 0)) if args.sizing else meta.get("repetitions", 0)
        return meta.get("decisions_per_repetition", 0) * repetitions
    inspection_path = build / "inspection.json"
    inspection = json.loads(inspection_path.read_text()) if inspection_path.exists() else {}
    def exceeded(name, width, arms):
        if args.experiment == 4:
            return attempt_result_e4(arms, width,
                                     excluded=e2_excluded(name, inspection.get(name, {})))["status"] == "exceeds"
        if args.experiment == 2:
            return attempt_result_e2(arms, width, decisions(name),
                                     excluded=e2_excluded(name, inspection.get(name, {})))["status"] == "exceeds"
        return attempt_result(arms, width, decisions(name))["status"] == "exceeds"
    rerun = {(name, width) for (name, width, attempt), arms in groups.items()
             if exceeded(name, width, arms)}
    if rerun:
        batch(2, rerun)
    (build / "summary.json").write_text(json.dumps(summarize(path, inspection, args.sizing, args.experiment), indent=2) + "\n")


if __name__ == "__main__":
    main()
