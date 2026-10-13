#!/usr/bin/env python3
"""Temporary manual Firn check acquisition; workflow is its only caller.

No native/LLVM products or verdict cache are constructed. Remove this helper
with int-match-firn-time.yml when the experiment ends. Compilation and its
sample/rounds run only inside the workflow's parent run-check process group.
"""

import csv
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import signal
import statistics
import subprocess
import sys
import time


VARIANTS = ("A", "B", "C", "T")
FIELDS = ("stage", "round", "order", "variant", "seconds",
          "actual_returncode", "launcher_returncode", "timed_out", "verdict")
ATTRIBUTION = ("NOT COMPARABLE: fixed B and current C lack the redundant-condition "
               "rule. A uses B's Firn application sources but A's own embedded "
               "standard library; A/B is not a complete same-source comparison. "
               "Numeric thresholds are diagnostics, not redundant-rule attribution.")


def out():
    return Path(os.environ["FIRN_TIME_OUT"])


def work():
    return Path(os.environ["GITHUB_WORKSPACE"])


def manifest():
    return json.loads((out() / "manifest.json").read_text())


def save_manifest(value):
    (out() / "manifest.json").write_text(json.dumps(value, indent=2) + "\n")


def git(*args, cwd=None):
    return subprocess.check_output(["git", *map(str, args)], cwd=cwd or work(), text=True).strip()


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def command(argv, name, cwd=None, env=None):
    print(f"{name}: {list(map(str, argv))}", flush=True)
    with (out() / "logs" / f"{name}.stdout").open("wb") as stdout, \
            (out() / "logs" / f"{name}.stderr").open("wb") as stderr:
        result = subprocess.run(list(map(str, argv)), cwd=cwd or work(), env=env,
                                stdout=stdout, stderr=stderr)
    if result.returncode:
        raise RuntimeError(f"{name}: actual subprocess exit {result.returncode}; see logs")


def clean_environment():
    env = os.environ.copy()
    # Remove inherited proof/work counters, test timing, redundant-condition
    # switches, all other WHITEFOOT instrumentation, and compiler wrappers.
    for key in list(env):
        if key.startswith(("WHITEFOOT_", "CARGO_PROFILE_")) or key in {
            "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER", "CARGO_TARGET_DIR", "CCC_OVERRIDE_OPTIONS",
        }:
            del env[key]
    env["CARGO_INCREMENTAL"] = "0"
    env["LC_ALL"] = "C"
    return env


def require_wrapper():
    owner = os.environ.get("WHITEFOOT_CHECK_OWNER", "")
    group = os.environ.get("WHITEFOOT_CHECK_PGID", "")
    if not owner.isdigit() or not group.isdigit() or os.getpgrp() != int(group):
        raise RuntimeError("heavy construction/acquisition requires the parent run-check process group")
    os.kill(int(owner), 0)


def snapshot(name):
    text = subprocess.check_output(
        ["ps", "-eo", "pid,ppid,pgid,pcpu,comm,args", "--sort=-pcpu"], text=True)
    (out() / "logs" / f"{name}-processes.txt").write_text(text)
    (out() / "logs" / f"{name}-load.txt").write_text(Path("/proc/loadavg").read_text())
    return text


def init():
    save_manifest({
        "acquisition": "NOT MEASURED", "attribution": ATTRIBUTION,
        "run_url": f'{os.environ["GITHUB_SERVER_URL"]}/{os.environ["GITHUB_REPOSITORY"]}/actions/runs/{os.environ["GITHUB_RUN_ID"]}',
        "run_attempt": os.environ["GITHUB_RUN_ATTEMPT"],
        "cpu_affinity": "taskset -c 2", "per_check_timeout_seconds": 120,
        "profile": "gate", "cargo_incremental": "0",
        "cargo_flags": ["build", "--profile", "gate", "--bin", "whitefootc", "--locked", "--offline"],
        "criterion": "delta = median(B)-median(C); numeric pass iff delta >= 0.5 seconds and delta > max(T)-min(T)",
        "measured_rounds": 7, "warmups_per_variant": 1, "samples_per_variant": 1,
        "historical_1_6_seconds_baseline": "UNAVAILABLE: no source or comparable acquisition supplied",
        "timing_scope": "monotonic time immediately before launching whitefootc until wait returns; excludes parent run-check, timeout/taskset/Python launcher startup, verdict validation and file processing",
        "verdict_validation": "exact complete stdout firn: accepted, direct exit zero; historical command unchanged",
        "environment_removed": sorted(key for key in os.environ if key not in clean_environment()),
        "source_library_policy": "Exact unmodified compiler checkouts embed their own lib/std via include_str!; no --std-root exists and no library overlay is made",
    })


def host():
    processes = snapshot("initial")
    ancestors = {os.getpid()}
    parent = os.getppid()
    rows = [line.split(None, 5) for line in processes.splitlines()[1:]]
    while parent and parent not in ancestors:
        ancestors.add(parent)
        matching = next((row for row in rows if row[0] == str(parent)), None)
        parent = int(matching[1]) if matching else 0
    # Conservatively refuse orphaned experiments and unrelated heavy commands.
    # Never signal processes found by command-name matching.
    busy = [row for row in rows if int(row[0]) not in ancestors and
            (row[4] in {"whitefootc", "cargo", "rustc", "make", "clang", "clang++", "ld.lld", "cc1", "valgrind"}
             or "int-match-firn-time.py" in row[5]
             or float(row[3]) >= 20)]
    lock = Path(f"/tmp/whitefoot-check-{os.getuid()}.lock")
    if lock.exists():
        pid = (lock / "pid").read_text().strip() if (lock / "pid").is_file() else ""
        text = (lock / "command").read_text() if (lock / "command").is_file() else ""
        (out() / "logs/initial-lock.txt").write_text(f"lock={lock}\npid={pid}\ncommand={text}\n")
        print(f"Inspecting lock {lock}: PID {pid}; command {text}", flush=True)
        if not pid.isdigit():
            raise RuntimeError("host lock owner is unknown; refusing to remove it")
        try:
            os.kill(int(pid), 0)
        except ProcessLookupError:
            if busy:
                raise RuntimeError("dead lock owner but live heavy/experiment processes; refusing stale cleanup")
            # Inspect recorded PID and command first; unlink only known lock
            # files. Unknown contents, symlinks or a concurrent owner fail closed.
            if lock.is_symlink() or {p.name for p in lock.iterdir()} - {"pid", "command"}:
                raise RuntimeError("unexpected stale-lock contents; refusing cleanup")
            if (lock / "pid").read_text().strip() != pid:
                raise RuntimeError("lock owner changed during inspection")
            (lock / "pid").unlink()
            (lock / "command").unlink(missing_ok=True)
            lock.rmdir()
        else:
            raise RuntimeError(f"live host lock owner {pid}; refusing busy host")
    if busy:
        raise RuntimeError("host is busy; see initial-processes.txt: " + repr(busy))
    tools = ["/usr/bin/time", "git", "perl", "python3", "taskset", "timeout",
             "cargo", "rustc", "make", "clang", "ld.lld", "sha256sum", "lscpu"]
    missing = [tool for tool in tools if shutil.which(tool) is None]
    if missing:
        raise RuntimeError("missing tools: " + ", ".join(missing))
    versions = {}
    for tool in tools:
        flag = "-vV" if tool == "rustc" else "--version"
        result = subprocess.run([tool, flag], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        if result.returncode != 0:
            raise RuntimeError(f"tool version unavailable: {tool}, exit {result.returncode}")
        versions[tool] = {"path": shutil.which(tool), "version": result.stdout.strip()}
    if "GNU" not in versions["/usr/bin/time"]["version"]:
        raise RuntimeError("/usr/bin/time must be GNU time")
    command(["taskset", "-c", "2", "true"], "affinity-probe")
    data = manifest()
    data["tools"] = versions
    data["cpu_topology"] = subprocess.check_output(["lscpu", "-e"], text=True)
    data["runner_allowed_cpus"] = sorted(os.sched_getaffinity(0))
    data["uname"] = subprocess.check_output(["uname", "-a"], text=True).strip()
    data["cpu_governors"] = {str(p): p.read_text().strip() for p in
                             Path("/sys/devices/system/cpu").glob("cpu*/cpufreq/scaling_governor")}
    data["initial_load"] = Path("/proc/loadavg").read_text().strip()
    save_manifest(data)


def source_inventory(tree, prefix):
    files = []
    for base in ("apps/firn", "lib/std"):
        for relative in git("ls-files", base, cwd=tree).splitlines():
            path = tree / relative
            if path.is_symlink() or not path.is_file():
                raise RuntimeError("source inventory requires real tracked files: " + str(path))
            files.append({"path": str(path), "relative": relative, "sha256": sha(path),
                          "git_blob": git("hash-object", path, cwd=tree)})
    (out() / "inputs" / f"{prefix}-sources.json").write_text(json.dumps(files, indent=2) + "\n")
    return files


def prepare():
    data = manifest()
    scratch = out().parent / (out().name + "-scratch")
    scratch.mkdir()  # unique run directory; an existing one is an error
    data["scratch"] = str(scratch)
    save_manifest(data)
    git("worktree", "prune")
    revisions = {"A": os.environ["FIRN_TIME_A"], "B": os.environ["FIRN_TIME_B"], "C": git("rev-parse", "HEAD")}
    data["variants"] = {}
    for variant, revision in revisions.items():
        command(["git", "fetch", "--no-tags", "origin", revision], f"fetch-{variant}")
        if git("rev-parse", revision + "^{commit}") != revision:
            raise RuntimeError("revision did not resolve exactly: " + revision)
        tree = scratch / variant
        git("worktree", "add", "--detach", tree, revision)
        info = {"compiler_revision": revision, "compiler_tree": git("rev-parse", revision + "^{tree}"),
                "compiler_path": str(tree), "application_source_variant": "B" if variant == "A" else variant,
                "application_tree": git("rev-parse", revision + ":apps/firn"),
                "embedded_std_tree": git("rev-parse", revision + ":lib/std"),
                "spec_title": (tree / "spec/kernel-spec.md").read_text().splitlines()[0],
                "spec_sha256": sha(tree / "spec/kernel-spec.md"),
                "cargo_toml_sha256": sha(tree / "compiler/Cargo.toml"),
                "cargo_lock_sha256": sha(tree / "compiler/Cargo.lock")}
        spec = (tree / "spec/kernel-spec.md").read_text()
        op5 = spec[spec.index("[OP-5] Every"):spec.index("[OP-6] Exact")]
        conditions = tree / "compiler/src/semantic/check/control/matches.rs"
        text = conditions.read_text()
        start = text.index("fn check_condition(")
        body = text[start:text.index("\n    ///", start)]
        # Inspection evidence is preserved, rather than pretending a switch can
        # recreate the historical rule. Fixed B/C are known to lack it.
        info["redundant_condition_rule_present"] = bool(re.search(
            r"redundan|already.prov|always.true|always.false|known.truth", op5 + body, re.I))
        info["rule_presence_inspection"] = "scoped OP-5 text and check_condition body; snippets preserved; not a semantic proof of arbitrary future revisions"
        (out() / "inputs" / f"{variant}-condition-inspection.txt").write_text(op5 + "\n" + body + "\n")
        source_inventory(tree, variant)
        data["variants"][variant] = info
        save_manifest(data)
    data["variants"]["A"]["application_tree"] = data["variants"]["B"]["application_tree"]
    for variant in ("A", "B", "C"):
        info = data["variants"][variant]
        source = data["variants"][info["application_source_variant"]]["compiler_path"]
        info["application_source_path"] = str(Path(source) / "apps/firn")
        info["graph_path"] = str(Path(source) / "apps/firn/modules.wfg")
        info["graph_sha256"] = sha(info["graph_path"])
        source_files = json.loads((out() / "inputs" / f'{info["application_source_variant"]}-sources.json').read_text())
        library_files = json.loads((out() / "inputs" / f"{variant}-sources.json").read_text())
        info["actual_source_inventory"] = [record for record in source_files if record["relative"].startswith("apps/firn/")]
        info["embedded_std_inventory"] = [record for record in library_files if record["relative"].startswith("lib/std/")]
    data["variants"]["T"] = dict(data["variants"]["B"], byte_copy_of="B")
    data["a_b_same_application_sources"] = True
    data["a_b_same_embedded_std"] = data["variants"]["A"]["embedded_std_tree"] == data["variants"]["B"]["embedded_std_tree"]
    data["complete_same_source_a_b"] = data["a_b_same_embedded_std"]
    save_manifest(data)


def build():
    require_wrapper()
    data = manifest()
    for variant in ("A", "B", "C"):
        info = data["variants"][variant]
        tree = Path(info["compiler_path"])
        if git("status", "--porcelain", cwd=tree):
            raise RuntimeError("compiler checkout is not exact/clean: " + variant)
        target = Path(data["scratch"]) / (variant + "-target")
        if target.exists():
            raise RuntimeError("Cargo target must be fresh: " + str(target))
        env = clean_environment()
        env["CARGO_TARGET_DIR"] = str(target)
        command(["cargo", "fetch", "--locked", "--manifest-path", tree / "compiler/Cargo.toml"],
                f"{variant}-cargo-fetch", cwd=tree, env=env)
        command(["cargo", "build", "--manifest-path", tree / "compiler/Cargo.toml", *data["cargo_flags"][1:]],
                f"{variant}-cargo-build", cwd=tree, env=env)
        image = out() / "images" / ("whitefootc-" + variant)
        shutil.copy2(target / "gate/whitefootc", image)
        info["binary_path"] = str(image)
        info["binary_sha256"] = sha(image)
        info["cargo_target_path"] = str(target)
        # The CLI has no compiler --version. Exact revision, spec identity,
        # Cargo package version and binary hash identify the constructed image.
        info["compiler_package_version"] = re.search(r'^version\s*=\s*"([^"]+)"',
            (tree / "compiler/Cargo.toml").read_text(), re.M).group(1)
        save_manifest(data)
    twin = out() / "images/whitefootc-T"
    shutil.copy2(data["variants"]["B"]["binary_path"], twin)
    data["variants"]["T"].update(binary_path=str(twin), binary_sha256=sha(twin))
    if sha(twin) != data["variants"]["B"]["binary_sha256"]:
        raise RuntimeError("twin is not a byte copy of B")
    for variant, info in data["variants"].items():
        info["timed_command"] = [info["binary_path"], "--check", "--graph", "apps/firn/modules.wfg",
                                 "--entry", "firn"]
        info["timed_cwd"] = str(Path(info["application_source_path"]).parent.parent)
    data["acquisition"] = "BUILT; NOT MEASURED"
    save_manifest(data)
    (out() / "images.sha256").write_text("".join(
        f'{data["variants"][v]["binary_sha256"]}  images/whitefootc-{v}\n' for v in VARIANTS))


def accepted(text):
    # publish_verdicts reports pending acceptance separately. No extra driver
    # flags change the historical workload to obtain a structured verdict.
    return text.strip() == "firn: accepted"


def one(stage, number, position, variant):
    info = manifest()["variants"][variant]
    prefix = out() / "raw" / f"{stage}-{number}-{position}-{variant}"
    row = {"stage": stage, "round": int(number), "order": int(position), "variant": variant,
           "seconds": None, "actual_returncode": None, "launcher_returncode": None,
           "timed_out": False, "verdict": "UNAVAILABLE"}
    child = None

    def terminate(signum, frame):
        row["timed_out"] = True
        if child is not None:
            child.terminate()

    signal.signal(signal.SIGTERM, terminate)
    signal.signal(signal.SIGINT, terminate)
    try:
        with Path(str(prefix) + ".stdout").open("wb") as stdout, \
                Path(str(prefix) + ".stderr").open("wb") as stderr:
            started = time.monotonic_ns()
            child = subprocess.Popen(info["timed_command"], cwd=info["timed_cwd"],
                                     stdout=stdout, stderr=stderr, env=clean_environment())
            row["actual_returncode"] = child.wait()
            row["seconds"] = (time.monotonic_ns() - started) / 1e9
        if row["actual_returncode"] == 0 and not row["timed_out"]:
            row["verdict"] = "accepted" if accepted(Path(str(prefix) + ".stdout").read_text()) else "INVALID VERDICT"
    finally:
        # Timeout owns the deadline; this record preserves the direct compiler
        # status whenever wait completed. GNU timeout's status is separate.
        Path(str(prefix) + ".json").write_text(json.dumps(row) + "\n")
    return 0 if row["verdict"] == "accepted" else 1


def read_rows():
    with (out() / "raw/times.csv").open(newline="") as file:
        return list(csv.DictReader(file))


def acquire(stage, number, order):
    for position, variant in enumerate(order):
        prefix = out() / "raw" / f"{stage}-{number}-{position}-{variant}"
        argv = ["timeout", "--foreground", "--signal=TERM", "--kill-after=2s", "120s",
                "taskset", "-c", "2", sys.executable, str(Path(__file__).resolve()),
                "one", stage, str(number), str(position), variant]
        launcher = subprocess.run(argv)
        record = Path(str(prefix) + ".json")
        if record.exists():
            row = json.loads(record.read_text())
        else:
            row = dict(stage=stage, round=number, order=position, variant=variant,
                       seconds=None, actual_returncode=None, timed_out=launcher.returncode in (124, 137),
                       verdict="MISSING DIRECT PROCESS RECORD")
        row["launcher_returncode"] = launcher.returncode
        row["timed_out"] = row["timed_out"] or launcher.returncode in (124, 137)
        record.write_text(json.dumps(row, indent=2) + "\n")
        with (out() / "raw/times.csv").open("a", newline="") as file:
            csv.DictWriter(file, fieldnames=FIELDS).writerow(row)
        print(f'{stage} round {number} order {position} {variant}: {row}', flush=True)
        if launcher.returncode != 0 or row["actual_returncode"] != 0 or row["verdict"] != "accepted" or row["timed_out"]:
            raise RuntimeError(f"{stage}/{number}/{variant}: acquisition invalid; see raw process logs")


def validate_matrix(rows, stages):
    expected = {}
    for stage, count in stages.items():
        for number in range(count):
            order = VARIANTS[number % 4:] + VARIANTS[:number % 4] if stage == "measured" else VARIANTS
            for position, variant in enumerate(order):
                expected[(stage, number, position, variant)] = True
    found = set()
    for row in rows:
        key = (row["stage"], int(row["round"]), int(row["order"]), row["variant"])
        if key not in expected or key in found:
            raise ValueError("unexpected/duplicate raw row: " + str(key))
        found.add(key)
        if (str(row["actual_returncode"]) != "0" or str(row["launcher_returncode"]) != "0"
                or str(row["timed_out"]).lower() != "false" or row["verdict"] != "accepted"
                or not math.isfinite(float(row["seconds"])) or float(row["seconds"]) <= 0):
            raise ValueError("invalid raw row: " + str(key))
    if found != set(expected):
        raise ValueError("missing raw rows: " + str(set(expected) - found))


def numeric(medians, twin_range):
    delta = medians["B"] - medians["C"]
    return delta, delta >= 0.5 and delta > twin_range


def sample():
    require_wrapper()
    if read_rows():
        raise RuntimeError("sample matrix must start empty")
    snapshot("before-sample")
    acquire("sample", 0, VARIANTS)
    rows = read_rows()
    validate_matrix(rows, {"sample": 1})
    lines = ["### Smallest full-entry sample", "", "| Variant | Seconds | Direct exit | Verdict |",
             "|---|---:|---:|---|"]
    lines += [f'| {r["variant"]} | {float(r["seconds"]):.9f} | {r["actual_returncode"]} | {r["verdict"]} |' for r in rows]
    lines += ["", "One sample each; these observations are not the seven-round criterion.", ""]
    (out() / "sample.md").write_text("\n".join(lines))
    print("\n".join(lines), flush=True)
    print(f'Sample range {min(float(r["seconds"]) for r in rows):.9f}..'
          f'{max(float(r["seconds"]) for r in rows):.9f} seconds; '
          'next stage preserves the prescribed one warmup and seven rounds; '
          'stage/per-process deadlines invalidate incomplete panels.', flush=True)


def rounds():
    require_wrapper()
    validate_matrix(read_rows(), {"sample": 1})
    snapshot("before-rounds")
    acquire("warmup", 0, VARIANTS)
    for number in range(7):
        order = VARIANTS[number % 4:] + VARIANTS[:number % 4]
        acquire("measured", number, order)
    snapshot("after-rounds")
    rows = read_rows()
    validate_matrix(rows, {"sample": 1, "warmup": 1, "measured": 7})
    times = {v: [float(r["seconds"]) for r in rows if r["stage"] == "measured" and r["variant"] == v] for v in VARIANTS}
    medians = {v: statistics.median(values) for v, values in times.items()}
    twin_range = max(times["T"]) - min(times["T"])
    delta, passed = numeric(medians, twin_range)
    lines = ["### Seven-round timing diagnostic", "", "| Variant | Median seconds | Min seconds | Max seconds |",
             "|---|---:|---:|---:|"]
    lines += [f"| {v} | {medians[v]:.9f} | {min(times[v]):.9f} | {max(times[v]):.9f} |" for v in VARIANTS]
    lines += ["", "| Diagnostic | Result |", "|---|---:|",
              f'| B−A seconds | {medians["B"] - medians["A"]:.9f} |',
              f'| C−A seconds | {medians["C"] - medians["A"]:.9f} |',
              f"| delta = (B−A)−(C−A) = B−C seconds | {delta:.9f} |",
              f"| Twin range = max(T)−min(T) seconds | {twin_range:.9f} |",
              f'| Numeric threshold: delta ≥ 0.5 s AND delta > twin range | {"PASS" if passed else "FAIL"} |',
              "| Redundant-rule attribution | NOT COMPARABLE |", "", ATTRIBUTION,
              "", "The numeric decision uses unrounded values and exactly seven measured rounds; sample and warmup are excluded.", ""]
    (out() / "timing.md").write_text("\n".join(lines))
    print("\n".join(lines), flush=True)
    data = manifest()
    data.update(acquisition="COMPLETE", numeric_threshold="PASS" if passed else "FAIL",
                medians_seconds=medians, delta_seconds=delta, twin_range_seconds=twin_range,
                redundant_rule_attribution="NOT COMPARABLE")
    save_manifest(data)


def finalize():
    try:
        data = manifest()
    except (ValueError, OSError):
        data = {"acquisition": "UNAVAILABLE", "manifest_preparation": "UNAVAILABLE", "attribution": ATTRIBUTION}
    if data.get("acquisition") == "COMPLETE" and os.environ.get("FIRN_TIME_JOB_STATUS") == "success":
        validate_matrix(read_rows(), {"sample": 1, "warmup": 1, "measured": 7})
        status = "COMPLETE: valid acquisition; numeric diagnostic " + data["numeric_threshold"] + ". " + ATTRIBUTION
    else:
        previous = (out() / "acquisition-status.txt").read_text().strip()
        reason = previous.split("\n", 1)[0] if previous.startswith("UNAVAILABLE:") else "Acquisition did not complete successfully."
        status = "UNAVAILABLE: " + reason + " No numeric pass/fail verdict. " + ATTRIBUTION
        data["acquisition"] = "UNAVAILABLE"
        (out() / "timing.md").write_text(status + "\n")
    data["job_status_at_finalization"] = os.environ.get("FIRN_TIME_JOB_STATUS", "unknown")
    save_manifest(data)
    (out() / "acquisition-status.txt").write_text(status + "\n")
    print(status, flush=True)


def cleanup():
    data = manifest()
    if "scratch" not in data:
        return
    scratch = Path(data["scratch"])
    if scratch != out().parent / (out().name + "-scratch") or scratch.is_symlink():
        raise RuntimeError("cleanup path is not this run's owned scratch")
    processes = snapshot("cleanup")
    if any(str(scratch) in line for line in processes.splitlines()[1:]):
        raise RuntimeError("live scratch process; conservative cleanup refusal")
    for variant in ("A", "B", "C"):
        tree = scratch / variant
        if tree.exists():
            git("worktree", "remove", "--force", tree)
    if scratch.exists():
        shutil.rmtree(scratch)
    git("worktree", "prune")


def selfcheck():
    rows = []
    for number in range(7):
        order = VARIANTS[number % 4:] + VARIANTS[:number % 4]
        for position, variant in enumerate(order):
            rows.append(dict(stage="measured", round=number, order=position, variant=variant,
                             seconds=1.0, actual_returncode=0, launcher_returncode=0,
                             timed_out=False, verdict="accepted"))
    validate_matrix(rows, {"measured": 7})
    invalid = [rows[:-1], rows + [rows[0]]]
    for field, value in [("actual_returncode", 1), ("launcher_returncode", 124),
                         ("timed_out", True), ("verdict", "rejected"),
                         ("seconds", float("nan")), ("order", 8)]:
        bad = [dict(r) for r in rows]
        bad[0][field] = value
        invalid.append(bad)
    for bad in invalid:
        try:
            validate_matrix(bad, {"measured": 7})
        except ValueError:
            pass
        else:
            raise RuntimeError("reducer accepted a representative invalid acquisition")
    if numeric({"A": 10, "B": 12, "C": 11.5}, 0.49) != (0.5, True):
        raise RuntimeError("inclusive 0.5-second threshold failed")
    if numeric({"A": 10, "B": 12, "C": 11.5}, 0.5) != (0.5, False):
        raise RuntimeError("strict twin-range boundary failed")
    if numeric({"A": 10, "B": 12, "C": 11.6}, 0.1)[1]:
        raise RuntimeError("sub-0.5-second delta incorrectly passed")
    if not accepted("firn: accepted\n"):
        raise RuntimeError("ordinary complete verdict was refused")
    for text in ["other: accepted", "firn: accepted, pending work", "firn: rejected", ""]:
        if accepted(text):
            raise RuntimeError("invalid full-entry verdict accepted")
    print("reducer self-check: valid matrix accepted; missing/duplicate/failing/timed-out/nonfinite/misordered rows refused; threshold and verdict boundaries checked")


if __name__ == "__main__":
    action = sys.argv[1]
    try:
        if action == "one":
            sys.exit(one(*sys.argv[2:]))
        {"init": init, "host": host, "prepare": prepare, "build": build,
         "sample": sample, "rounds": rounds, "finalize": finalize,
         "cleanup": cleanup, "selfcheck": selfcheck}[action]()
    except Exception as error:
        print(f"UNAVAILABLE: {action}: {error}", file=sys.stderr, flush=True)
        if action != "selfcheck" and "FIRN_TIME_OUT" in os.environ:
            (out() / "acquisition-status.txt").write_text(f"UNAVAILABLE: {action}: {error}\n{ATTRIBUTION}\n")
            (out() / "logs" / f"{action}-failure.txt").write_text(repr(error) + "\n")
        sys.exit(1)
