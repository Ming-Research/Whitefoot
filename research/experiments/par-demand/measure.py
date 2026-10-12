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
import time
from pathlib import Path
from summarize import (E5A_PADDING, E6B, E6_FAMILY, experiment_matrix, parse_experiment, attempt_result,
                       attempt_result_e2, attempt_result_e4, e6_attempt, cell_widths, cell_arms, construction_identity,
                       e6b_construction, e2_excluded, load, summarize)
from entry_dispatch import (MIN_CALLS, MAX_CALLS, MIN_BATCH_NS, WIDTHS as ENTRY_WIDTHS,
                            validate_proof as entry_dispatch_proof, validate_sample)
from inspection import validate_inspection


def balanced_order(items, round_id, offset=0):
    """Rotate once per round pair, then visit both directions of that rotation."""
    items = list(items)
    if not items or len(set(items)) != len(items) or type(round_id) is not int or round_id < 0:
        raise ValueError('balanced order requires distinct items and a nonnegative round')
    shift = (round_id // 2 + offset) % len(items)
    ordered = items[shift:] + items[:shift]
    return ordered[::-1] if round_id % 2 else ordered


def require_paired_rounds(rounds, *, sizing=False):
    if type(rounds) is not int or rounds < 1:
        raise ValueError('paired ordering requires a positive round count')
    if not sizing and rounds % 2:
        raise ValueError(f'paired decisive ordering requires an even frozen round count; protocol selected {rounds}; no rounding applied')


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


E5C_PLACEMENT = {1: [2], 4: [2, 4, 6, 8], 8: [2, 4, 6, 8, 10, 12, 14, 1]}


def e5c_placement(root=Path("/sys/devices/system/cpu"),
                   performance_file=Path("/sys/devices/cpu_core/cpus"), available=None):
    """Validate fixed logical IDs, without substituting CPUs on another host."""
    _, topology = performance_cores(root, performance_file, available, minimum_cores=1)
    performance = topology["performance_cpus"]
    if performance is None:
        raise ValueError("experiment 5c requires the P-core CPU mask")
    siblings = {int(path.parent.parent.name[3:]): cpu_list(value)
                for filename, value in topology["files"].items()
                if (path := Path(filename)).name == "thread_siblings_list"}
    selected = set(E5C_PLACEMENT[8])
    if not selected <= set(performance) or not selected <= set(topology["available_cpus"]):
        raise ValueError("experiment 5c fixed CPUs must all be allowed P-core CPUs")
    online = topology["files"].get(str(root / "online"))
    if online is None or not selected <= cpu_list(online):
        raise ValueError("experiment 5c fixed CPUs must all be online")
    if 0 not in siblings or 1 not in siblings or siblings[0] != siblings[1] or 0 not in siblings[1] or 1 not in siblings[0]:
        raise ValueError("experiment 5c requires CPU 1 to be CPU 0's sibling")
    physical = set()
    for cpu in E5C_PLACEMENT[8]:
        members = siblings.get(cpu, set())
        if cpu not in members or any(siblings.get(member) != members for member in members):
            raise ValueError(f"experiment 5c inconsistent sibling topology for CPU {cpu}")
        core = tuple(sorted(members))
        if core in physical:
            raise ValueError("experiment 5c fixed CPUs must use distinct physical cores")
        physical.add(core)
        if cpu != 1 and cpu != min(members):
            raise ValueError(f"experiment 5c CPU {cpu} must be its core's first sibling")
    topology.update(placement_policy="fixed E5c P-core sets; never CPU 0",
                    selected_siblings={str(cpu): sorted(siblings[cpu]) for cpu in E5C_PLACEMENT[8]},
                    w8_shares_cpu0_physical_core=True,
                    cpu0_siblings=sorted(siblings[0]))
    return {width: list(cpus) for width, cpus in E5C_PLACEMENT.items()}, topology


def demand_setting(experiment, arm):
    return "on" if (experiment == 2 and arm in ("demand", "idle1", "twin")
                    or experiment in (3, 4) and arm not in ("seq", "par")
                    or experiment in ("5a", "5c", *E6_FAMILY) and arm in ("demand", "unversioned", "previous", "twin")) else "off-never-request"


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
    parser.add_argument("--experiment", type=parse_experiment, choices=(1, 2, 3, 4, "5a", "5c", 6, E6B), default=1)
    parser.add_argument("--instrumented", action="store_true")
    parser.add_argument("--sizing", action="store_true")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    if args.experiment in (3, 4, "5a", "5c", *E6_FAMILY) and args.rounds is not None:
        parser.error("experiments 3, 4, 5a, 5c, 6 and 6b select their rounds from six sizing rounds; omit --rounds")
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
    if args.experiment in (2, 3, 4, "5a", "5c", *E6_FAMILY) and not args.verify_only:
        if platform.system() != "Linux" or shutil.which("taskset") is None:
            raise ValueError("experiments 2, 3, 4, 5a, 5c, 6 and 6b require Linux topology and taskset pinning")
        if args.experiment in ("5c", *E6_FAMILY):
            # CPU 0 itself need not be in this service's allowed affinity.
            pinned, topology = e5c_placement()
            if args.experiment in E6_FAMILY:
                pinned[2] = [2, 4]
                topology["placement_policy"] = "fixed E6 P-core sets; never CPU 0"
            cores = []
        else:
            cores, topology = performance_cores(minimum_cores=1) if args.experiment == "5a" else performance_cores()
    else:
        # Verification has no timing verdict and may run on the hosted sizing
        # machine. Every experiment-2 timing batch, sizing included, is strict.
        cores = one_cpu_per_core()
    if args.experiment not in ("5c", *E6_FAMILY) or args.verify_only:
        pinned = {width: cores[:max(width, 1)] for width in widths_for_run} if len(cores) >= max(widths_for_run) else {}
    if args.experiment == "5a" and not args.verify_only:
        if 2 not in cores or topology["performance_cpus"] is None or 2 not in topology["performance_cpus"]:
            raise ValueError("experiment 5a requires CPU 2 to be an allowed first P-core sibling")
        pinned = {1: [2]}
    layout = layout_identity(build, manifest) if args.experiment in ("5a", *E6_FAMILY) else None
    def pin(width, command):
        cpus = pinned.get(width)
        return ["taskset", "-c", ",".join(map(str, cpus))] + command if cpus else command
    identity = dict(host=platform.uname()._asdict(), sizing=args.sizing, rounds=args.rounds, pinned=pinned,
                    revision=run(["git", "rev-parse", "HEAD"], capture_output=True).stdout.strip(),
                    dirty=run(["git", "status", "--porcelain"], capture_output=True).stdout,
                    hashes=hashes, experiment=args.experiment, topology=topology,
                    settings={arm: demand_setting(args.experiment, arm) for arm in arms_for_run},
                    setting="on" if args.experiment in (2, 3, 4, "5a", "5c", *E6_FAMILY) else "off-never-request", manifest=manifest,
                    instrumented=args.instrumented, layout=layout)
    (build / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    if args.verify_only:
        for name in manifest:
            for arm in arms_for_run:
                for width in cell_widths(args.experiment, name) if args.experiment in (2, 3, 4, "5a", "5c", *E6_FAMILY) else (1, 4):
                    if arm not in cell_arms(args.experiment, width, name):
                        continue
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
    if args.experiment in E6_FAMILY:
        # E6b also requires spine's sequentialized hand-off before timing.
        (e6b_construction if args.experiment == E6B else construction_identity)(build)
        entry_dispatch_proof(build)
    entry_evidence = dict(calibration={}, sizing=[], decisive=[])
    inspection = {}
    def checked_inspection():
        return validate_inspection(build, build / 'inspection.json', identity['revision'], manifest, arms_for_run)
    def entry_sample(width, calls, order):
        completed = run(pin(width, [str(build / "entry-dispatch"), str(width), str(calls), str(order)]),
                        env=dict(env, WF_WORKERS=str(width), WF_PAR_DEMAND="on"), capture_output=True)
        return validate_sample(json.loads(completed.stdout), width, calls, order)
    if args.experiment in E6_FAMILY:
        for width in ENTRY_WIDTHS:
            calls, pilots = MIN_CALLS, []
            while True:
                sample = entry_sample(width, calls, len(pilots) % 2)
                pilots.append(sample)
                threshold = max(MIN_BATCH_NS, 1000 * sample["resolution_ns"])
                if min(sample[arm]["wall_ns"] for arm in ("direct", "dispatch")) >= threshold or calls == MAX_CALLS:
                    break
                calls *= 2
            entry_evidence["calibration"][str(width)] = dict(calls=calls, pilots=pilots)
        (build / "entry-dispatch-measurements.json").write_text(json.dumps(entry_evidence, indent=2) + "\n")
    path = build / "measurements.tsv"
    def batch(attempt, selected, *, sizing=False):
        require_paired_rounds(args.rounds, sizing=sizing)
        with path.open("w" if attempt == 1 else "a") as output:
            for round_id in range(args.rounds):
                if args.experiment in E6_FAMILY and attempt == 1:
                    phase = "sizing" if len(entry_evidence["sizing"]) < 6 * len(ENTRY_WIDTHS) else "decisive"
                    for width in ENTRY_WIDTHS:
                        calls = entry_evidence["calibration"][str(width)]["calls"]
                        entry_evidence[phase].append(dict(entry_sample(width, calls, round_id % 2), round=round_id))
                    (build / "entry-dispatch-measurements.json").write_text(json.dumps(entry_evidence, indent=2) + "\n")
                names = balanced_order(manifest, round_id)
                for name in names:
                    meta = manifest[name]
                    settings = dict(env)
                    if "repetitions" in meta:
                        settings["WFD_REPETITIONS"] = str(meta.get("sizing_repetitions", meta["repetitions"]) if args.sizing and args.experiment not in (3, 4, "5a", "5c", *E6_FAMILY) else meta["repetitions"])
                        settings["WFD_EXTENT"] = str(meta.get("sizing_extent", meta["extent"]) if args.sizing and args.experiment not in (3, 4, "5a", "5c", *E6_FAMILY) else meta["extent"])
                    widths = balanced_order(cell_widths(args.experiment, name), round_id)
                    for width in widths:
                        if (name, width) not in selected:
                            continue
                        arms = balanced_order(cell_arms(args.experiment, width, name), round_id, widths_for_run.index(width))
                        for arm in arms:
                            print(f"attempt={attempt} round={round_id} {name} W={width} {arm}", flush=True)
                            run(pin(width, [str(build / arm / name), "measure", arm, str(width), str(round_id), str(attempt)]),
                                env=dict(settings, WF_WORKERS=str(width),
                                         WF_PAR_DEMAND=demand_setting(args.experiment, arm)), stdout=output)
                            output.flush()
    selected = {(name, width) for name in manifest for width in cell_widths(args.experiment, name)}
    if args.experiment in (3, 4, "5a", "5c", *E6_FAMILY):
        args.rounds = 6
        sizing_start = time.monotonic()
        batch(1, selected, sizing=True)
        sizing_seconds = time.monotonic() - sizing_start
        sample = summarize(path, sizing=True, experiment=args.experiment)
        sample_dir = build / f"sizing-e{args.experiment}"
        sample_dir.mkdir(exist_ok=True)
        path.replace(sample_dir / "measurements.tsv")
        (sample_dir / "summary.json").write_text(json.dumps(sample, indent=2) + "\n")
        identity.update(rounds=6, sizing=True)
        (sample_dir / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
        args.rounds = sample["decisive_rounds"]
        if args.experiment in E6_FAMILY:
            invocations = sum(len(cell_arms(args.experiment, width, name)) for name in manifest
                              for width in cell_widths(args.experiment, name))
            identity["duration_projection"] = dict(sizing_elapsed_seconds=sizing_seconds,
                decisive_rounds=args.rounds, image_invocations_per_round=invocations,
                timed_workload_calls_per_round=2 * invocations,
                initial_total_seconds=sizing_seconds * (1 + args.rounds / 6),
                all_cells_rerun_total_seconds=sizing_seconds * (1 + 2 * args.rounds / 6),
                scope="six measured rounds including paired entry diagnostics; excludes calibration, reduction, verification, hosted construction and queue time")
        identity.update(rounds=args.rounds, sizing=args.sizing, sizing_rounds=6)
        (build / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
        if args.sizing:
            return
        require_paired_rounds(args.rounds)
        inspection = checked_inspection()
        if args.experiment in (3, "5a"):
            batch(1, selected)
            (build / "summary.json").write_text(json.dumps(summarize(path, inspection, experiment=args.experiment), indent=2) + "\n")
            return
    if args.experiment not in (3, 4, "5a", "5c", *E6_FAMILY) and not args.sizing:
        require_paired_rounds(args.rounds)
        inspection = checked_inspection()
    batch(1, selected, sizing=args.sizing)
    groups = load(path, args.experiment)
    def decisions(name):
        meta = manifest[name]
        repetitions = meta.get("sizing_repetitions", meta.get("repetitions", 0)) if args.sizing else meta.get("repetitions", 0)
        return meta.get("decisions_per_repetition", 0) * repetitions
    def exceeded(name, width, arms):
        if args.experiment in E6_FAMILY:
            return e6_attempt(name, arms, width, experiment=args.experiment)["status"] == "exceeds"
        if args.experiment in (4, "5c"):
            return attempt_result_e4(arms, width,
                                     excluded=e2_excluded(name, inspection.get(name, {})),
                                     experiment=args.experiment)["status"] == "exceeds"
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
