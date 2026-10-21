#!/usr/bin/env python3
"""Manual Firn check-time acquisition; firn-check-time.yml is its caller.

Two pairs of compilers check the same Firn source. In a pair, `base` lacks the
redundant-condition rule, `rule` has it, and `twin` is a byte copy of `base`
whose spread is the run's noise. Pair `now` is the primary question; pair
`then` is a reference that reproduces the historical measurement on the same
machine and is voided, with its reason recorded, if its compilers do not
produce `firn: accepted` on the source. Every revision comes from the
workflow inputs. The calculation and its prior criterion are in the check-time
investigation's DESIGN.md, section "Firn total checking comparison".

No native or LLVM product and no verdict cache is constructed. Compilation and
the sample and rounds run only inside the workflow's run-check process group.
The exit status reports whether the acquisition is valid; whether the
criterion is met is written in the report and never decides the status.
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


ROLES = ("base", "rule", "twin")
PAIRS = ("now", "then")
# The pair the criterion judges; the other is a reference that may be voided.
PRIMARY = "now"
FIELDS = ("stage", "round", "order", "variant", "seconds",
          "actual_returncode", "launcher_returncode", "timed_out", "verdict")
FULL_SHA = re.compile(r"[0-9a-f]{40}")
CRITERION_ROUNDS = 7
PER_CHECK_SECONDS = 120
GRAPH = "apps/firn/modules.wfg"


def out():
    return Path(os.environ["FIRN_TIME_OUT"])


def work():
    return Path(os.environ["GITHUB_WORKSPACE"])


def rounds_wanted():
    return int(os.environ.get("FIRN_TIME_ROUNDS", str(CRITERION_ROUNDS)))


def cpu():
    return os.environ.get("FIRN_TIME_CPU", "2")


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
    # Remove inherited work counters, test timing, other WHITEFOOT
    # instrumentation and compiler wrappers; the checks run without any.
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


# ---- variants, inputs and matrices ---------------------------------------

def variant_id(pair, role):
    return f"{pair}-{role}"


def variant_pair(variant):
    return variant.split("-", 1)[0]


def all_variants(data):
    return [variant_id(pair, role) for pair in PAIRS if pair in data["pairs"] for role in ROLES]


def active_variants(data):
    return [v for v in all_variants(data) if data["pairs"][variant_pair(v)]["status"] == "valid"]


def rotated(order, number):
    shift = number % len(order)
    return order[shift:] + order[:shift]


def read_pairs(environ):
    """The pairs the workflow inputs name; the primary pair is required."""
    pairs = {}
    for pair in PAIRS:
        values = {key: environ.get(f"FIRN_TIME_{pair.upper()}_{key.upper()}", "").strip()
                  for key in ("base", "rule", "source")}
        if not any(values.values()):
            if pair == PRIMARY:
                raise ValueError(f"pair {pair} is required")
            continue
        for key, value in values.items():
            if not FULL_SHA.fullmatch(value):
                raise ValueError(f"pair {pair} {key}: need a full 40-digit lowercase commit id, got {value!r}")
        if values["base"] == values["rule"]:
            raise ValueError(f"pair {pair}: base and rule are the same revision; the twin is the noise control")
        pairs[pair] = dict(values, status="valid")
    return pairs


def expected_cells(stages, order, active, rounds):
    """(stage, round, position) -> variant for every cell the stages define."""
    cells = {}
    if "sample" in stages:
        for position, variant in enumerate(order):
            cells[("sample", 0, position)] = variant
    if "warmup" in stages:
        for position, variant in enumerate(active):
            cells[("warmup", 0, position)] = variant
    if "measured" in stages:
        for number in range(rounds):
            for position, variant in enumerate(rotated(active, number)):
                cells[("measured", number, position)] = variant
    return cells


def row_valid(row):
    return (str(row["actual_returncode"]) == "0" and str(row["launcher_returncode"]) == "0"
            and str(row["timed_out"]).lower() == "false" and row["verdict"] == "accepted"
            and math.isfinite(float(row["seconds"])) and float(row["seconds"]) > 0)


def validate_matrix(rows, stages, order, active, rounds):
    """Raise ValueError unless rows are exactly the cells of the active variants.

    A voided pair's variants are in `order` but not in `active`: its rows may
    be absent or failing, and are not required.
    """
    expected = expected_cells(stages, order, active, rounds)
    found = set()
    for row in rows:
        key = (row["stage"], int(row["round"]), int(row["order"]))
        if expected.get(key) != row["variant"] or key in found:
            raise ValueError("unexpected/duplicate raw row: " + str(key))
        found.add(key)
        if row["variant"] in active and not row_valid(row):
            raise ValueError("invalid raw row: " + str(key))
    required = {key for key, variant in expected.items() if variant in active}
    if required - found:
        raise ValueError("missing raw rows: " + str(sorted(required - found)))


# ---- criterion and report ------------------------------------------------

def criterion(overhead, twin_range, claimed, required):
    """The owner's criterion: the claimed overhead shrinks by `required` seconds
    and by more than the twin's full range. Returns (margin, met)."""
    margin = claimed - overhead
    return margin, margin >= required and margin > twin_range


def accepted(text):
    # No extra driver flag changes the historical workload to obtain a
    # structured verdict; the complete stdout must be exactly this line.
    return text.strip() == "firn: accepted"


def summarize(rows, active):
    times = {v: [float(r["seconds"]) for r in rows
                 if r["stage"] == "measured" and r["variant"] == v] for v in active}
    return {v: dict(median=statistics.median(t), min=min(t), max=max(t)) for v, t in times.items()}


def pair_results(stats, pairs):
    result = {}
    for pair in pairs:
        base, rule, twin = (stats[variant_id(pair, role)] for role in ROLES)
        result[pair] = dict(overhead=rule["median"] - base["median"], twin_range=twin["max"] - twin["min"])
    return result


def render(rows, pairs, rounds, claimed, required):
    """Return (markdown, numbers) for a complete, valid set of measured rows."""
    active = [variant_id(p, r) for p in PAIRS if p in pairs and pairs[p]["status"] == "valid" for r in ROLES]
    stats = summarize(rows, active)
    results = pair_results(stats, [p for p in PAIRS if p in pairs and pairs[p]["status"] == "valid"])
    lines = [f"### Firn check time, {rounds} measured rounds", "",
             "| Variant | Median seconds | Min seconds | Max seconds |", "|---|---:|---:|---:|"]
    lines += [f"| {v} | {stats[v]['median']:.9f} | {stats[v]['min']:.9f} | {stats[v]['max']:.9f} |"
              for v in active]
    lines += ["", "| Pair | Overhead O = median(rule) − median(base), seconds | Twin range = max(twin) − min(twin), seconds |",
              "|---|---:|---:|"]
    lines += [f"| {p} | {r['overhead']:.9f} | {r['twin_range']:.9f} |" for p, r in results.items()]
    numbers = dict(medians={v: s["median"] for v, s in stats.items()}, pairs=results)
    now = results[PRIMARY]
    lines += [""]
    if claimed is None:
        lines += ["No claimed overhead was given: no criterion is evaluated.", ""]
    else:
        margin, met = criterion(now["overhead"], now["twin_range"], claimed, required)
        evaluated = rounds == CRITERION_ROUNDS
        verdict = ("MEETS" if met else "DOES NOT MEET") if evaluated else \
            f"NOT EVALUATED: the criterion is defined for {CRITERION_ROUNDS} rounds, this run had {rounds}"
        lines += ["| Criterion on pair now | Value |", "|---|---:|",
                  f"| claimed − O = {claimed} − O, seconds | {margin:.9f} |",
                  f"| required reduction, seconds | {required} |",
                  f"| twin range, seconds | {now['twin_range']:.9f} |",
                  f"| margin ≥ required AND margin > twin range | {verdict} |", ""]
        numbers.update(margin=margin, criterion=verdict)
    reference = [p for p in PAIRS if p != PRIMARY and p in pairs]
    for pair in reference:
        if pairs[pair]["status"] != "valid":
            lines += [f"Reference pair `{pair}`: VOID. {pairs[pair]['void_reason']}", ""]
            numbers.setdefault("void", {})[pair] = pairs[pair]["void_reason"]
            continue
        then = results[pair]
        difference = then["overhead"] - now["overhead"]
        larger = max(now["twin_range"], then["twin_range"])
        lines += [f"Reference on this machine: O({pair}) − O({PRIMARY}) = {difference:.9f} s; twin ranges "
                  f"{PRIMARY} {now['twin_range']:.9f} s, {pair} {then['twin_range']:.9f} s; "
                  f"the difference exceeds the larger twin range: {'yes' if difference > larger else 'no'}. "
                  "A claimed overhead from another machine is not comparable with these numbers.", ""]
        numbers["reference_difference"] = difference
    lines += ["The decision uses unrounded values; sample and warmup rounds are excluded.", ""]
    return "\n".join(lines), numbers


# ---- host inspection -----------------------------------------------------

def busy_processes(rows, ancestors):
    """Rows of heavy or experiment processes outside this run's ancestry."""
    heavy = {"whitefootc", "cargo", "rustc", "make", "clang", "clang++", "ld.lld", "cc1", "valgrind"}
    return [row for row in rows if int(row[0]) not in ancestors and
            (row[4] in heavy or "firn-check-time.py" in row[5] or float(row[3]) >= 20)]


def host():
    """Record the tools, the CPU topology and the pinning probe."""
    tools = ["/usr/bin/time", "git", "perl", "python3", "taskset", "timeout",
             "cargo", "rustc", "sha256sum", "lscpu"]
    missing = [tool for tool in tools if shutil.which(tool) is None]
    if missing:
        raise RuntimeError("missing tools: " + ", ".join(missing))
    versions = {}
    for tool in tools + [t for t in ("clang", "ld.lld", "make") if shutil.which(t)]:
        flag = "-vV" if tool == "rustc" else "--version"
        result = subprocess.run([tool, flag], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        if result.returncode != 0:
            raise RuntimeError(f"tool version unavailable: {tool}, exit {result.returncode}")
        versions[tool] = {"path": shutil.which(tool), "version": result.stdout.strip()}
    if "GNU" not in versions["/usr/bin/time"]["version"]:
        raise RuntimeError("/usr/bin/time must be GNU time")
    command(["taskset", "-c", cpu(), "true"], "affinity-probe")
    data = manifest()
    data["tools"] = versions
    data["cpu_topology"] = subprocess.check_output(["lscpu", "-e"], text=True)
    data["runner_allowed_cpus"] = sorted(os.sched_getaffinity(0))
    data["uname"] = subprocess.check_output(["uname", "-a"], text=True).strip()
    data["cpu_governors"] = {str(p): p.read_text().strip() for p in
                             Path("/sys/devices/system/cpu").glob("cpu*/cpufreq/scaling_governor")}
    data["initial_load"] = Path("/proc/loadavg").read_text().strip()
    save_manifest(data)


def idle():
    """Refuse a host that is busy or holds a live lock: only the dedicated
    timing machine can promise one process at a time."""
    processes = snapshot("initial")
    ancestors = {os.getpid()}
    parent = os.getppid()
    rows = [line.split(None, 5) for line in processes.splitlines()[1:]]
    while parent and parent not in ancestors:
        ancestors.add(parent)
        matching = next((row for row in rows if row[0] == str(parent)), None)
        parent = int(matching[1]) if matching else 0
    # Never signal a process found by command-name matching.
    busy = busy_processes(rows, ancestors)
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
            # The recorded PID and command were inspected above. Unlink only
            # known lock files; unknown contents or a concurrent owner fail closed.
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


# ---- preparation and build -----------------------------------------------

def init():
    try:
        pairs = read_pairs(os.environ)
    except ValueError as error:
        raise RuntimeError(f"inputs: {error}")
    claimed = os.environ.get("FIRN_TIME_CLAIMED_OVERHEAD", "").strip()
    save_manifest({
        "acquisition": "NOT MEASURED",
        "run_url": f'{os.environ["GITHUB_SERVER_URL"]}/{os.environ["GITHUB_REPOSITORY"]}/actions/runs/{os.environ["GITHUB_RUN_ID"]}',
        "run_attempt": os.environ["GITHUB_RUN_ATTEMPT"],
        "pairs": pairs,
        "cpu_affinity": f"taskset -c {cpu()}", "per_check_timeout_seconds": PER_CHECK_SECONDS,
        "profile": "gate", "cargo_incremental": "0",
        "cargo_flags": ["build", "--profile", "gate", "--bin", "whitefootc", "--locked", "--offline"],
        "claimed_overhead_seconds": claimed or None,
        "required_reduction_seconds": os.environ.get("FIRN_TIME_REQUIRED_REDUCTION", "0.5"),
        "criterion": "margin = claimed - O(now); met iff margin >= required reduction and margin > max(twin now) - min(twin now)",
        "measured_rounds": rounds_wanted(), "warmups_per_variant": 1, "samples_per_variant": 1,
        "timing_scope": "monotonic time immediately before launching whitefootc until wait returns; excludes parent run-check, timeout/taskset/Python launcher startup, verdict validation and file processing",
        "verdict_validation": "exact complete stdout firn: accepted, direct exit zero",
        "environment_removed": sorted(key for key in os.environ if key not in clean_environment()),
        "source_library_policy": "Each compiler embeds its own lib/std via include_str!; no --std-root exists and no library overlay is made",
    })


def identity(tree, revision):
    spec = tree / "spec/kernel-spec.md"
    return {"revision": revision, "tree": git("rev-parse", revision + "^{tree}"),
            "embedded_std_tree": git("rev-parse", revision + ":lib/std"),
            "firn_tree": git("rev-parse", revision + ":apps/firn"),
            "spec_title": spec.read_text().splitlines()[0], "spec_sha256": sha(spec),
            "cargo_toml_sha256": sha(tree / "compiler/Cargo.toml"),
            "cargo_lock_sha256": sha(tree / "compiler/Cargo.lock")}


def prepare():
    data = manifest()
    scratch = out().parent / (out().name + "-scratch")
    scratch.mkdir()  # unique run directory; an existing one is an error
    data["scratch"] = str(scratch)
    save_manifest(data)
    git("worktree", "prune")
    data["revisions"] = {}
    for pair, spec in data["pairs"].items():
        for revision in sorted({spec["base"], spec["rule"], spec["source"]}):
            if revision in data["revisions"]:
                continue
            command(["git", "fetch", "--no-tags", "origin", revision], f"fetch-{revision[:12]}")
            if git("rev-parse", revision + "^{commit}") != revision:
                raise RuntimeError("revision did not resolve exactly: " + revision)
            tree = scratch / revision[:12]
            git("worktree", "add", "--detach", tree, revision)
            data["revisions"][revision] = dict(identity(tree, revision), path=str(tree))
            save_manifest(data)
        base, rule, source = (data["revisions"][spec[key]] for key in ("base", "rule", "source"))
        if not (Path(source["path"]) / GRAPH).is_file():
            raise RuntimeError(f"pair {pair}: source revision has no {GRAPH}")
        spec["embedded_std_identical"] = base["embedded_std_tree"] == rule["embedded_std_tree"]
        spec["firn_source_tree"] = source["firn_tree"]
        # What differs between the two compilers, kept for the reader who must
        # judge whether the pair isolates the rule.
        (out() / "inputs" / f"{pair}-base-to-rule.diffstat").write_text(
            git("diff", "--stat", spec["base"], spec["rule"], "--", "compiler", "lib", "spec") + "\n")
        save_manifest(data)


def build():
    require_wrapper()
    data = manifest()
    built = {}
    for pair, spec in data["pairs"].items():
        for revision in (spec["base"], spec["rule"]):
            if revision in built:
                continue
            tree = Path(data["revisions"][revision]["path"])
            if git("status", "--porcelain", cwd=tree):
                raise RuntimeError("compiler checkout is not exact/clean: " + revision)
            target = Path(data["scratch"]) / (revision[:12] + "-target")
            if target.exists():
                raise RuntimeError("Cargo target must be fresh: " + str(target))
            env = clean_environment()
            env["CARGO_TARGET_DIR"] = str(target)
            command(["cargo", "fetch", "--locked", "--manifest-path", tree / "compiler/Cargo.toml"],
                    f"{revision[:12]}-cargo-fetch", cwd=tree, env=env)
            command(["cargo", "build", "--manifest-path", tree / "compiler/Cargo.toml", *data["cargo_flags"][1:]],
                    f"{revision[:12]}-cargo-build", cwd=tree, env=env)
            image = out() / "images" / ("whitefootc-" + revision[:12])
            shutil.copy2(target / "gate/whitefootc", image)
            built[revision] = image
            data["revisions"][revision].update(
                binary_path=str(image), binary_sha256=sha(image),
                compiler_package_version=re.search(r'^version\s*=\s*"([^"]+)"',
                                                   (tree / "compiler/Cargo.toml").read_text(), re.M).group(1))
            save_manifest(data)
    data["variants"] = {}
    for pair, spec in data["pairs"].items():
        twin = out() / "images" / f"whitefootc-{pair}-twin"
        shutil.copy2(built[spec["base"]], twin)
        if sha(twin) != sha(built[spec["base"]]):
            raise RuntimeError(f"pair {pair}: twin is not a byte copy of base")
        binaries = {"base": built[spec["base"]], "rule": built[spec["rule"]], "twin": twin}
        source = Path(data["revisions"][spec["source"]]["path"])
        for role in ROLES:
            data["variants"][variant_id(pair, role)] = {
                "compiler_revision": spec["rule" if role == "rule" else "base"],
                "source_revision": spec["source"], "binary_path": str(binaries[role]),
                "binary_sha256": sha(binaries[role]), "timed_cwd": str(source),
                "timed_command": [str(binaries[role]), "--check", "--graph", GRAPH, "--entry", "firn"]}
    data["acquisition"] = "BUILT; NOT MEASURED"
    save_manifest(data)
    (out() / "images.sha256").write_text("".join(
        f'{data["variants"][v]["binary_sha256"]}  {data["variants"][v]["binary_path"]}\n' for v in all_variants(data)))


# ---- acquisition ---------------------------------------------------------

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
        # Timeout owns the deadline; this record keeps the direct compiler
        # status whenever wait completed. GNU timeout's status is separate.
        Path(str(prefix) + ".json").write_text(json.dumps(row) + "\n")
    return 0 if row["verdict"] == "accepted" else 1


def read_rows():
    with (out() / "raw/times.csv").open(newline="") as file:
        return list(csv.DictReader(file))


def run_cell(stage, number, position, variant):
    """Time one compiler process under taskset and timeout; append and return its row."""
    prefix = out() / "raw" / f"{stage}-{number}-{position}-{variant}"
    argv = ["timeout", "--foreground", "--signal=TERM", "--kill-after=2s", f"{PER_CHECK_SECONDS}s",
            "taskset", "-c", cpu(), sys.executable, str(Path(__file__).resolve()),
            "one", stage, str(number), str(position), variant]
    launcher = subprocess.run(argv)
    record = Path(str(prefix) + ".json")
    if record.exists():
        row = json.loads(record.read_text())
    else:
        row = dict(stage=stage, round=number, order=position, variant=variant, seconds=None,
                   actual_returncode=None, timed_out=launcher.returncode in (124, 137),
                   verdict="MISSING DIRECT PROCESS RECORD")
    row["launcher_returncode"] = launcher.returncode
    row["timed_out"] = row["timed_out"] or launcher.returncode in (124, 137)
    record.write_text(json.dumps(row, indent=2) + "\n")
    with (out() / "raw/times.csv").open("a", newline="") as file:
        csv.DictWriter(file, fieldnames=FIELDS).writerow(row)
    print(f'{stage} round {number} order {position} {variant}: {row}', flush=True)
    return row


def cell_failure(row):
    """None for a valid cell, 'completed' when the compiler ran to the end
    without printing the accepted verdict, else 'invalid'."""
    if row_valid(row):
        return None
    if not row["timed_out"] and row["actual_returncode"] is not None and row["launcher_returncode"] in (0, 1):
        return "completed"
    return "invalid"


def void_reason(row):
    prefix = out() / "raw" / f'{row["stage"]}-{row["round"]}-{row["order"]}-{row["variant"]}'
    text = ""
    for suffix in (".stderr", ".stdout"):
        text = " ".join(Path(str(prefix) + suffix).read_text(errors="replace").split())
        if text:
            break
    return (f'{row["variant"]} completed with exit {row["actual_returncode"]} and verdict {row["verdict"]} '
            f'instead of an accepted Firn check; first output: {text[:300] or "(none)"}. '
            "This voids the reference pair as a measurement and is not a source-language verdict.")


def acquire(stage, number, order):
    for position, variant in enumerate(order):
        row = run_cell(stage, number, position, variant)
        if cell_failure(row):
            raise RuntimeError(f"{stage}/{number}/{variant}: acquisition invalid; see raw process logs")


def sample_pass(data, order, run, reason):
    """Run each variant once with `run`. A reference pair whose compiler completes
    without the accepted verdict is voided with `reason`'s text and its remaining
    variants are skipped; every other failure raises, the primary pair's included."""
    for position, variant in enumerate(order):
        pair = variant_pair(variant)
        if data["pairs"][pair]["status"] != "valid":
            continue
        row = run("sample", 0, position, variant)
        failure = cell_failure(row)
        if failure is None:
            continue
        if pair != PRIMARY and failure == "completed":
            data["pairs"][pair].update(status="void", void_reason=reason(row))
            print(f"Reference pair {pair} voided: {data['pairs'][pair]['void_reason']}", flush=True)
            continue
        raise RuntimeError(f"sample/{variant}: acquisition invalid; see raw process logs")


def sample():
    require_wrapper()
    if read_rows():
        raise RuntimeError("sample matrix must start empty")
    snapshot("before-sample")
    data = manifest()
    order = all_variants(data)
    sample_pass(data, order, run_cell, void_reason)
    save_manifest(data)
    active = active_variants(data)
    rows = read_rows()
    validate_matrix(rows, {"sample"}, order, active, 0)
    lines = ["### Smallest full-entry sample", "", "| Variant | Seconds | Direct exit | Verdict |",
             "|---|---:|---:|---|"]
    lines += [f'| {r["variant"]} | {r["seconds"] or "n/a"} | {r["actual_returncode"]} | {r["verdict"]} |' for r in rows]
    lines += [""] + [f"Pair {p} VOID: {s['void_reason']}" for p, s in data["pairs"].items() if s["status"] != "valid"]
    lines += ["", "One sample each; these observations are not the measured-round criterion.", ""]
    (out() / "sample.md").write_text("\n".join(lines))
    print("\n".join(lines), flush=True)
    seconds = [float(r["seconds"]) for r in rows if r["variant"] in active]
    print(f'Sample range {min(seconds):.9f}..{max(seconds):.9f} seconds', flush=True)


def rounds():
    require_wrapper()
    data = manifest()
    order, active, wanted = all_variants(data), active_variants(data), rounds_wanted()
    validate_matrix(read_rows(), {"sample"}, order, active, 0)
    snapshot("before-rounds")
    acquire("warmup", 0, active)
    for number in range(wanted):
        acquire("measured", number, rotated(active, number))
    snapshot("after-rounds")
    rows = read_rows()
    validate_matrix(rows, {"sample", "warmup", "measured"}, order, active, wanted)
    claimed = data["claimed_overhead_seconds"]
    text, numbers = render(rows, data["pairs"], wanted,
                           float(claimed) if claimed else None, float(data["required_reduction_seconds"]))
    (out() / "timing.md").write_text(text)
    print(text, flush=True)
    data.update(acquisition="COMPLETE", report=numbers)
    save_manifest(data)


def finalize():
    try:
        data = manifest()
    except (ValueError, OSError):
        data = {"acquisition": "UNAVAILABLE", "manifest_preparation": "UNAVAILABLE"}
    if data.get("acquisition") == "COMPLETE" and os.environ.get("FIRN_TIME_JOB_STATUS") == "success":
        validate_matrix(read_rows(), {"sample", "warmup", "measured"}, all_variants(data),
                        active_variants(data), rounds_wanted())
        voided = [p for p, s in data["pairs"].items() if s["status"] != "valid"]
        status = "COMPLETE: valid acquisition." + (f" Reference pair voided: {', '.join(voided)}." if voided else "")
    else:
        previous = (out() / "acquisition-status.txt").read_text().strip()
        reason = previous.split("\n", 1)[0] if previous.startswith("UNAVAILABLE:") else "Acquisition did not complete successfully."
        status = "UNAVAILABLE: " + reason + " No timing report and no criterion result."
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
    for revision in data.get("revisions", {}).values():
        tree = Path(revision["path"])
        if tree.exists():
            git("worktree", "remove", "--force", tree)
    if scratch.exists():
        shutil.rmtree(scratch)
    git("worktree", "prune")


# ---- self-check ----------------------------------------------------------

def selfcheck():
    """Show that the reducers refuse invalid acquisitions and keep the criterion's
    boundaries; runs before any build, without a compiler."""
    sha_a, sha_b, sha_c = "a" * 40, "b" * 40, "c" * 40
    both = {"now": dict(base=sha_a, rule=sha_b, source=sha_b, status="valid"),
            "then": dict(base=sha_c, rule=sha_b, source=sha_b, status="valid")}
    order = [variant_id(p, r) for p in PAIRS for r in ROLES]

    def good(seconds):
        return dict(actual_returncode=0, launcher_returncode=0, timed_out=False,
                    verdict="accepted", seconds=seconds)

    def matrix(active, stages, rounds):
        cells = expected_cells(stages, order, active, rounds)
        return [dict(stage=s, round=n, order=p, variant=v, **good(1.0))
                for (s, n, p), v in cells.items() if v in active]

    def refused(rows, stages, active, rounds, why):
        try:
            validate_matrix(rows, stages, order, active, rounds)
        except ValueError:
            return
        raise RuntimeError("reducer accepted " + why)

    stages = {"sample", "warmup", "measured"}
    validate_matrix(matrix(order, stages, 7), stages, order, order, 7)
    # A voided reference pair: its variants need no rows, and a failing row of
    # it is ignored; the same failing row in the primary pair is refused.
    primary = order[:3]
    rows = matrix(primary, stages, 7)
    validate_matrix(rows, stages, order, primary, 7)
    failing = dict(stage="sample", round=0, order=3, variant="then-base", **good(1.0))
    failing.update(actual_returncode=1, verdict="INVALID VERDICT")
    validate_matrix(rows + [failing], stages, order, primary, 7)
    refused(rows[:-1], stages, primary, 7, "a missing row")
    refused(rows + [dict(rows[0])], stages, primary, 7, "a duplicate row")
    refused(matrix(primary, stages, 6), stages, primary, 7, "six rounds where seven are required")
    refused(matrix(primary, stages, 8), stages, primary, 7, "eight rounds where seven are required")
    refused(rows + [dict(stage="extra", round=0, order=0, variant="now-base", **good(1.0))],
            stages, primary, 7, "an unexpected stage")
    for field, value in [("actual_returncode", 1), ("launcher_returncode", 124), ("timed_out", True),
                         ("verdict", "rejected"), ("seconds", float("nan")), ("seconds", 0.0), ("order", 8)]:
        bad = [dict(r) for r in rows]
        bad[0][field] = value
        refused(bad, stages, primary, 7, f"{field}={value}")
    bad = [dict(r) for r in rows]
    bad[0], bad[1] = dict(bad[0], variant=bad[1]["variant"]), dict(bad[1], variant=bad[0]["variant"])
    refused(bad, stages, primary, 7, "a misordered variant")

    # The sample's void decision, with a fake runner in place of the timed process.
    def fake(outcome):
        def run(stage, number, position, variant):
            row = dict(stage=stage, round=number, order=position, variant=variant, seconds=1.0,
                       actual_returncode=0, launcher_returncode=0, timed_out=False, verdict="accepted")
            if variant in outcome:
                row.update(outcome[variant])
            return row
        return run

    def sampled(outcome):
        data = {"pairs": {pair: dict(spec) for pair, spec in both.items()}}
        calls = []
        runner = fake(outcome)
        sample_pass(data, order, lambda *cell: calls.append(cell[3]) or runner(*cell),
                    lambda row: f'{row["variant"]} exit {row["actual_returncode"]}')
        return data, calls

    rejected = dict(actual_returncode=1, launcher_returncode=1, verdict="INVALID VERDICT")
    data, calls = sampled({})
    if data["pairs"]["then"]["status"] != "valid" or calls != order:
        raise RuntimeError("a clean sample did not run every variant")
    for outcome in (rejected, dict(rejected, actual_returncode=101), dict(rejected, actual_returncode=-11)):
        data, calls = sampled({"then-rule": outcome})
        if (data["pairs"]["then"]["status"] != "void" or data["pairs"]["now"]["status"] != "valid"
                or "then-rule exit" not in data["pairs"]["then"]["void_reason"]
                or calls != order[:5]):
            raise RuntimeError("a reference pair whose compiler completed without the verdict was not voided")
    for variant, outcome in [("now-base", rejected), ("now-rule", rejected),
                             ("then-base", dict(timed_out=True, launcher_returncode=124, actual_returncode=-15)),
                             ("then-twin", dict(actual_returncode=None, launcher_returncode=1, seconds=None,
                                                verdict="MISSING DIRECT PROCESS RECORD"))]:
        try:
            sampled({variant: outcome})
        except RuntimeError:
            continue
        raise RuntimeError(f"a {variant} failure that is not a completed reference run did not stop the sample")

    # The criterion: claimed 2.0, required 0.5, so O = 1.5 gives margin 0.5.
    if criterion(1.5, 0.25, 2.0, 0.5) != (0.5, True):
        raise RuntimeError("the inclusive required-reduction boundary failed")
    if criterion(1.5000000000000002, 0.25, 2.0, 0.5)[1]:
        raise RuntimeError("a margin just under the required reduction passed")
    if criterion(1.0, 0.9999999999999999, 2.0, 0.5) != (1.0, True):
        raise RuntimeError("a twin range just under the margin failed")
    if criterion(1.0, 1.0, 2.0, 0.5)[1]:
        raise RuntimeError("the strict twin-range boundary failed")
    if criterion(1.9, 0.0, 2.0, 0.5)[1]:
        raise RuntimeError("a small margin passed")

    # The report: overhead and criterion computed from known times.
    def times(now_rule, twin_spread, then_rule=None):
        rows, values = [], {"now-base": 10.0, "now-rule": now_rule, "now-twin": 10.0,
                            "then-base": 9.0, "then-rule": then_rule, "then-twin": 9.0}
        for number in range(7):
            for variant in (order if then_rule is not None else order[:3]):
                seconds = values[variant] + (twin_spread if variant.endswith("twin") and number == 0 else 0.0)
                rows.append(dict(stage="measured", round=number, order=0, variant=variant, seconds=seconds))
        return rows

    text, numbers = render(times(10.5, 0.25, 10.5), both, 7, 2.0, 0.5)
    if numbers["pairs"]["now"]["overhead"] != 0.5 or numbers["pairs"]["now"]["twin_range"] != 0.25:
        raise RuntimeError("report overhead or twin range is wrong")
    if numbers["criterion"] != "MEETS" or numbers["margin"] != 1.5:
        raise RuntimeError("report criterion is wrong for a pass")
    if numbers["reference_difference"] != 1.0 or "exceeds the larger twin range: yes" not in text:
        raise RuntimeError("report reference difference is wrong")
    _, numbers = render(times(11.75, 0.0), {"now": both["now"]}, 7, 2.0, 0.5)
    if numbers["criterion"] != "DOES NOT MEET":
        raise RuntimeError("report criterion is wrong for a failure")
    _, numbers = render(times(10.5, 0.0), {"now": both["now"]}, 1, 2.0, 0.5)
    if not numbers["criterion"].startswith("NOT EVALUATED"):
        raise RuntimeError("a run of fewer than seven rounds was judged against the criterion")
    voided = dict(both, then=dict(both["then"], status="void", void_reason="old compiler rejected the source"))
    text, numbers = render(times(10.5, 0.0), voided, 7, 2.0, 0.5)
    if "VOID. old compiler rejected the source" not in text or "then" not in numbers["void"]:
        raise RuntimeError("a voided reference pair was not reported")

    # Inputs and verdicts.
    env = {"FIRN_TIME_NOW_BASE": sha_a, "FIRN_TIME_NOW_RULE": sha_b, "FIRN_TIME_NOW_SOURCE": sha_b}
    if set(read_pairs(env)) != {"now"}:
        raise RuntimeError("an empty reference pair was not skipped")
    for bad_env in [{}, dict(env, FIRN_TIME_NOW_BASE="aaaaaaa"), dict(env, FIRN_TIME_NOW_BASE=sha_a.upper()),
                    dict(env, FIRN_TIME_NOW_RULE=sha_a), dict(env, FIRN_TIME_THEN_BASE=sha_c)]:
        try:
            read_pairs(bad_env)
        except ValueError:
            continue
        raise RuntimeError("read_pairs accepted invalid inputs: " + repr(bad_env))
    if not accepted("firn: accepted\n"):
        raise RuntimeError("ordinary complete verdict was refused")
    for text in ["other: accepted", "firn: accepted, pending work", "firn: rejected", ""]:
        if accepted(text):
            raise RuntimeError("invalid full-entry verdict accepted")
    rows = [["10", "1", "10", "0.0", "bash", "bash"], ["11", "1", "11", "0.0", "cargo", "cargo build"],
            ["12", "1", "12", "0.0", "python3", "python3 .github/firn-check-time.py rounds"],
            ["13", "1", "13", "25.0", "idle-thing", "x"], ["14", "1", "14", "0.0", "make", "make"]]
    if [r[0] for r in busy_processes(rows, {10})] != ["11", "12", "13", "14"] or busy_processes(rows[:1], set()) != []:
        raise RuntimeError("busy-host classification is wrong")
    print("self-check: valid matrices accepted; missing/duplicate/short/long/failing/timed-out/"
          "nonfinite/misordered rows refused; voided reference pair tolerated and a primary failure refused; "
          "criterion boundaries, report arithmetic, input and verdict validation checked")


if __name__ == "__main__":
    action = sys.argv[1]
    try:
        if action == "one":
            sys.exit(one(*sys.argv[2:]))
        {"init": init, "host": host, "idle": idle, "prepare": prepare, "build": build,
         "sample": sample, "rounds": rounds, "finalize": finalize,
         "cleanup": cleanup, "selfcheck": selfcheck}[action]()
    except Exception as error:
        print(f"UNAVAILABLE: {action}: {error}", file=sys.stderr, flush=True)
        if action != "selfcheck" and "FIRN_TIME_OUT" in os.environ:
            (out() / "acquisition-status.txt").write_text(f"UNAVAILABLE: {action}: {error}\n")
            (out() / "logs" / f"{action}-failure.txt").write_text(repr(error) + "\n")
        sys.exit(1)
