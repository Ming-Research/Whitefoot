"""E1 compile-time measurement (research/investigations/derivation-ceiling,
criterion (4)). Experiment-branch tool, run by tmp-e1-time.yml on the
dedicated 14900K runner; it is reached later through the retention tag.

Stages, each a subcommand, all writing under $E1_OUT:
  prepare   build the experiment compiler (gate and release profiles) and
            the baseline main compiler (gate profile)
  redis     fetch Redis 7.0.15, check its SHA-256, unpack it
  count     non-blank lines of what each build compiles
  rounds    warmup and measured one-core rounds, interleaved
  multicore reference pair on every processor, one round each
  profile   perf profile of one one-core Firn build, where perf runs
  summary   medians, ranges and the verdict quantity

Environment: E1_OUT, E1_CPU (default 2), E1_ROUNDS (default 5),
E1_WARMUP (default 1), E1_BASE (baseline main commit).
"""

import collections
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import tarfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
OUT = Path(os.environ.get("E1_OUT", "/tmp/e1-time"))
CPU = os.environ.get("E1_CPU", "2")
ROUNDS = int(os.environ.get("E1_ROUNDS", "5"))
WARMUP = int(os.environ.get("E1_WARMUP", "1"))
BASE = os.environ.get("E1_BASE", "0b8b4ad348b9e75aafdf3f3dbaa034c7aba78635")
REDIS_VERSION = "7.0.15"
REDIS_SHA256 = "98066f5363504b26c34dd20fbcc3c957990d764cdf42576c836fc021073f4341"
REDIS_URL = f"https://download.redis.io/releases/redis-{REDIS_VERSION}.tar.gz"
FIRN_ARGS = ["--graph", "apps/firn/modules.wfg", "--entry", "firn", "--report"]


def log(*parts):
    print(*parts, flush=True)


def run(argv, **kwargs):
    log("+", " ".join(str(a) for a in argv))
    return subprocess.run([str(a) for a in argv], check=True, **kwargs)


def compilers():
    return {
        "exp-gate": OUT / "target-exp/gate/whitefootc",
        "exp-release": OUT / "target-exp/release/whitefootc",
        "main-gate": OUT / "target-main/gate/whitefootc",
    }


def prepare():
    OUT.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_INCREMENTAL="0")
    manifest = ROOT / "compiler/Cargo.toml"
    run(["cargo", "fetch", "--locked", "--manifest-path", manifest], env=env)
    for profile in ("gate", "release"):
        run(["cargo", "build", "--manifest-path", manifest, "--profile", profile, "--bin",
             "whitefootc", "--locked", "--offline", "--target-dir", OUT / "target-exp"], env=env)
    tree = OUT / "main-tree"
    if not tree.exists():
        run(["git", "-C", ROOT, "worktree", "add", "--detach", tree, BASE])
    run(["cargo", "fetch", "--locked", "--manifest-path", tree / "compiler/Cargo.toml"], env=env)
    run(["cargo", "build", "--manifest-path", tree / "compiler/Cargo.toml", "--profile", "gate",
         "--bin", "whitefootc", "--locked", "--offline", "--target-dir", OUT / "target-main"], env=env)
    for name, path in compilers().items():
        log(name, path, path.exists())


def redis():
    OUT.mkdir(parents=True, exist_ok=True)
    archive = OUT / f"redis-{REDIS_VERSION}.tar.gz"
    if not archive.exists():
        # download.redis.io refuses urllib's client (HTTP 403); curl, as
        # research/experiments/redis-compat/run.sh fetches it, is served.
        run(["curl", "-fsSL", "-o", f"{archive}.part", REDIS_URL])
        Path(f"{archive}.part").rename(archive)
    digest = subprocess.run(["sha256sum", archive], check=True, capture_output=True,
                            text=True).stdout.split()[0]
    if digest != REDIS_SHA256:
        sys.exit(f"redis archive digest {digest} != {REDIS_SHA256}")
    target = OUT / "redis-src"
    if target.exists():
        shutil.rmtree(target)
    target.mkdir()
    with tarfile.open(archive) as tar:
        try:
            tar.extractall(target, filter="data")
        except TypeError:
            tar.extractall(target)
    makefile = (redis_tree() / "src/Makefile").read_text()
    log("redis optimization line:", re.findall(r"^OPTIMIZATION\?=.*$", makefile, re.M))


def redis_tree():
    return OUT / "redis-src" / f"redis-{REDIS_VERSION}"


def nonblank(path):
    try:
        text = Path(path).read_text(errors="replace")
    except OSError:
        return 0
    return sum(1 for line in text.splitlines() if line.strip())


def time_redis(pin):
    tree = redis_tree()
    subprocess.run(["make", "distclean"], cwd=tree, check=True, capture_output=True)
    jobs = "-j1" if pin else f"-j{os.cpu_count()}"
    prefix = ["taskset", "-c", CPU] if pin else []
    start = time.monotonic()
    done = subprocess.run(prefix + ["make", jobs], cwd=tree, capture_output=True, text=True)
    seconds = time.monotonic() - start
    if done.returncode != 0:
        log(done.stdout[-4000:], done.stderr[-4000:])
        sys.exit("redis build failed")
    return {"seconds": seconds}


def time_firn(compiler, join, pin, extra_env=None):
    scratch = OUT / "firn-out"
    scratch.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env.pop("WHITEFOOT_E1", None)
    if join:
        env["WHITEFOOT_E1_TIMING"] = "1"
        env["WHITEFOOT_E1_JOIN"] = join
    else:
        env.pop("WHITEFOOT_E1_TIMING", None)
    env.update(extra_env or {})
    prefix = ["taskset", "-c", CPU] if pin else []
    output = scratch / "firn"
    if output.exists():
        output.unlink()
    start = time.monotonic()
    done = subprocess.run(prefix + [str(compiler), *FIRN_ARGS, "-o", str(output)], cwd=ROOT,
                          env=env, capture_output=True, text=True)
    seconds = time.monotonic() - start
    if done.returncode != 0 or not output.exists():
        log(done.stdout[-4000:], done.stderr[-4000:])
        sys.exit(f"firn build failed with {compiler}")
    row = {"seconds": seconds}
    match = re.search(r"E1-TIMING ordinary_ms=([\d.]+) walk_ms=([\d.]+) shadow_ms=([\d.]+)",
                      done.stderr)
    if match:
        row.update(ordinary_ms=float(match[1]), walk_ms=float(match[2]),
                   shadow_ms=float(match[3]))
    for line in done.stdout.splitlines() + done.stderr.splitlines():
        if line.startswith('{"build"'):
            row["report"] = json.loads(line)["build"]
    return row


VARIANTS = {
    "redis": lambda pin: time_redis(pin),
    "firn-selector": lambda pin: time_firn(compilers()["exp-gate"], "selector", pin),
    "firn-weakest": lambda pin: time_firn(compilers()["exp-gate"], "weakest", pin),
    "firn-main": lambda pin: time_firn(compilers()["main-gate"], None, pin),
    "firn-selector-release": lambda pin: time_firn(compilers()["exp-release"], "selector", pin),
}


def record(stage, round_, variant, row):
    row["loadavg"] = Path("/proc/loadavg").read_text().split()[:3]
    with open(OUT / "times.jsonl", "a") as handle:
        handle.write(json.dumps({"stage": stage, "round": round_, "variant": variant, **row}) + "\n")
    log(stage, round_, variant, json.dumps(row))


def rounds():
    names = list(VARIANTS)
    for index in range(WARMUP + ROUNDS):
        stage = "warmup" if index < WARMUP else "round"
        # Rotate the order so no variant always runs first.
        order = names[index % len(names):] + names[:index % len(names)]
        for variant in order:
            record(stage, index, variant, VARIANTS[variant](True))


def multicore():
    for variant in ("redis", "firn-selector", "firn-main"):
        record("multicore", 0, variant, VARIANTS[variant](False))


def std_closure(graph_text):
    """The std modules a graph names, closed over lib/std/modules.wfg."""
    named = set(re.findall(r"std::((?:collections::)?\w+)", graph_text))
    edges = {}
    for line in (ROOT / "lib/std/modules.wfg").read_text().splitlines():
        match = re.match(r"pkg::([\w:]+):\s*\[(.*)\];", line.strip())
        if match:
            edges[match[1]] = re.findall(r"pkg::([\w:]+)", match[2])
    todo, seen = list(named), set()
    while todo:
        module = todo.pop()
        if module not in seen:
            seen.add(module)
            todo.extend(edges.get(module, []))
    return sorted(seen)


def runtime_c_units(requires_heap):
    """The runtime .c units a Linux build compiles: the driver's
    non-Windows compile lists (compiler/src/bin/whitefootc.rs,
    runtime_units), with ordinary_values.c and, where the emitted module
    names the allocator, heap.c."""
    driver = (ROOT / "compiler/src/bin/whitefootc.rs").read_text()
    units = ["ordinary_values.c"] + (["heap.c"] if requires_heap else [])
    for group in ("FLOOR", "CORE", "COMPLETION"):
        match = re.search(r'#\[cfg\(not\(target_os = "windows"\)\)\]\s*const ' + group
                          + r"_COMPILE_UNITS: &\[&str\] =\s*&\[(.*?)\];", driver, re.S)
        if not match:
            sys.exit(f"no {group}_COMPILE_UNITS list in whitefootc.rs")
        units += re.findall(r'"([^"]+\.c)"', match[1])
    return units


def count():
    counts = {}
    # Firn: its .wf and .wfm sources, and every std module its graph reaches.
    firn_files = [p for p in (ROOT / "apps/firn").rglob("*") if p.suffix in (".wf", ".wfm")]
    firn = sum(nonblank(p) for p in firn_files)
    graph = (ROOT / "apps/firn/modules.wfg").read_text()
    std_modules = std_closure(graph)
    std_files = [p for module in std_modules
                 for p in (ROOT / "lib/std" / module.replace("::", "/")).rglob("*")
                 if p.suffix in (".wf", ".wfm")]
    std = sum(nonblank(p) for p in std_files)
    # Runtime C units: the driver compiles a fixed list with /usr/bin/clang;
    # whether heap.c joins it is read from the emitted module, by the test
    # the driver applies (compiler/src/backend/runtime.rs).
    module = OUT / "firn-out/firn.ll"
    module.parent.mkdir(parents=True, exist_ok=True)
    env = {k: v for k, v in os.environ.items() if not k.startswith("WHITEFOOT_E1")}
    run([compilers()["exp-gate"], "--graph", "apps/firn/modules.wfg", "--entry", "firn",
         "--emit-llvm", "-o", module], cwd=ROOT, env=env, capture_output=True)
    text = module.read_text()
    requires_heap = any(name in text for name in (
        "@wf__heap_take(", "@wf__heap_give(", "@wf__heap_retake(", "@wf__empty_window"))
    units = {name: nonblank(ROOT / "compiler/src/backend" / name)
             for name in runtime_c_units(requires_heap)}
    missing = [name for name, lines in units.items() if lines == 0]
    if missing:
        sys.exit(f"runtime units not found: {missing}")
    counts["firn"] = {"files": len(firn_files), "wf": firn, "std_modules": std_modules,
                      "std_files": len(std_files), "std": std, "runtime_units": units,
                      "runtime": sum(units.values()),
                      "total": firn + std + sum(units.values())}
    # Redis: the .c of every object one build makes, and every .h under
    # src/ and deps/.
    tree = redis_tree()
    subprocess.run(["make", "distclean"], cwd=tree, check=True, capture_output=True)
    run(["taskset", "-c", CPU, "make", "-j1"], cwd=tree, capture_output=True)
    sources = set()
    for obj in list((tree / "src").rglob("*.o")) + list((tree / "deps").rglob("*.o")):
        stem = re.sub(r"(\.pic|\.sym)$", "", obj.stem)
        candidate = obj.with_name(stem + ".c")
        if candidate.exists():
            sources.add(candidate)
        else:
            counts.setdefault("redis_unmatched_objects", []).append(str(obj.relative_to(tree)))
    c_lines = sum(nonblank(p) for p in sources)
    headers = [p for d in ("src", "deps") for p in (tree / d).rglob("*.h")]
    h_lines = sum(nonblank(p) for p in headers)
    counts["redis"] = {"c_files": len(sources), "c": c_lines, "h_files": len(headers),
                       "h": h_lines, "total": c_lines + h_lines}
    (OUT / "counts.json").write_text(json.dumps(counts, indent=1))
    log(json.dumps(counts, indent=1))


def profile():
    perf = shutil.which("perf")
    paranoid = Path("/proc/sys/kernel/perf_event_paranoid")
    log("perf:", perf, "paranoid:", paranoid.read_text().strip() if paranoid.exists() else None)
    if not perf:
        log("PROFILE UNAVAILABLE: no perf on this runner")
        return
    data = OUT / "perf.data"
    env = dict(os.environ, WHITEFOOT_E1_TIMING="1", WHITEFOOT_E1_JOIN="selector")
    done = subprocess.run([perf, "record", "-F", "499", "-o", str(data), "--", "taskset", "-c",
                           CPU, str(compilers()["exp-gate"]), *FIRN_ARGS, "-o",
                           str(OUT / "firn-out/firn-perf")], cwd=ROOT, env=env,
                          capture_output=True, text=True)
    log("perf record exit", done.returncode, done.stderr[-2000:])
    if done.returncode != 0:
        log("PROFILE UNAVAILABLE: perf record failed")
        return
    report = subprocess.run([perf, "report", "-i", str(data), "--no-children", "--stdio",
                             "--sort", "comm,dso,symbol", "--percent-limit", "0"],
                            capture_output=True, text=True)
    (OUT / "perf-report.txt").write_text(report.stdout)
    categories = collections.Counter()
    top = []
    for line in report.stdout.splitlines():
        match = re.match(r"\s*([\d.]+)%\s+(\S+)\s+(\S+)\s+\[(.)\]\s+(.*?)\s*$", line)
        if not match:
            continue
        share, comm, dso, symbol = float(match[1]), match[2], match[3], match[5]
        categories[profile_category(comm, dso, symbol)] += share
        top.append(f"{share:6.2f}% {comm} {dso} {symbol[:160]}")
    log("PROFILE CATEGORIES (self time, share of all samples)")
    for name, share in categories.most_common():
        log(f"  {share:6.2f}%  {name}")
    log("PERF REPORT TOP 60")
    log("\n".join(top[:60]))
    log("PERF REPORT END")


ALLOCATION = ("malloc", "free", "_int_", "__libc_", "cfree", "realloc", "calloc", "__rdl_", "__rust_alloc",
              "__rust_dealloc", "__rust_realloc", "unlink_chunk", "finish_grow", "tcache",
              "malloc_consolidate", "sysmalloc", "brk", "mmap", "munmap")
COPYING = ("memmove", "memcpy", "memcmp", "memset", "__memmove", "__memcpy", "__memcmp",
           "__memset", "bcmp")


def profile_category(comm, dso, symbol):
    """One self-time sample's category. Named functions of the derivation
    (range_judgment) and of ordinary entailment are their algorithms;
    allocation, copying, standard-container code and hashing are
    implementation overhead wherever they are called from."""
    if not comm.startswith("whitefootc"):
        return f"process {comm} (LLVM, link and other children)"
    bare = symbol.lstrip("<")
    if any(bare.startswith(name) for name in ALLOCATION):
        return "overhead: allocation"
    if any(name in symbol for name in COPYING):
        return "overhead: copying"
    if bare.startswith(("core::hash", "hashbrown", "<std::hash", "std::collections::hash")) \
            or "core::hash::" in symbol.split(" as ")[0]:
        return "overhead: hashing"
    if bare.startswith(("alloc::", "core::")):
        if "range_judgment" in symbol:
            return "overhead: containers and clones in the derivation"
        if "entailment" in symbol:
            return "overhead: containers and clones in ordinary entailment"
        return "overhead: containers and clones elsewhere"
    if bare.startswith("whitefoot::semantic::range_judgment") or \
            symbol.startswith("<whitefoot::semantic::range_judgment"):
        return "algorithm: derivation (range_judgment)"
    if bare.startswith("whitefoot::semantic::entailment") or \
            symbol.startswith("<whitefoot::semantic::entailment"):
        return "algorithm: ordinary entailment"
    if "whitefoot::" in symbol:
        return "compiler: other phases (syntax, resolution, lowering, emission)"
    return f"other in whitefootc ({dso})"


def summary():
    rows = [json.loads(line) for line in (OUT / "times.jsonl").read_text().splitlines()]
    counts = json.loads((OUT / "counts.json").read_text()) if (OUT / "counts.json").exists() else {}
    measured = [r for r in rows if r["stage"] == "round"]
    lines = ["E1 TIME SUMMARY", f"rounds {ROUNDS} warmup {WARMUP} cpu {CPU}"]

    def stats(variant, field):
        values = [r[field] for r in measured if r["variant"] == variant and field in r]
        if not values:
            return None
        return statistics.median(values), min(values), max(values), len(values)

    for variant in VARIANTS:
        for field in ("seconds", "ordinary_ms", "walk_ms", "shadow_ms"):
            s = stats(variant, field)
            if s:
                lines.append(f"{variant:24} {field:12} median {s[0]:.2f} range {s[1]:.2f}..{s[2]:.2f} n={s[3]}")
        reports = [r["report"] for r in measured if r["variant"] == variant and "report" in r]
        for key in ("front_end_ms", "split_ms", "compile_ms", "link_ms"):
            values = [rep[key] for rep in reports if key in rep]
            if values:
                lines.append(f"{variant:24} {key:12} median {statistics.median(values):.1f}")
    redis = stats("redis", "seconds")
    if redis and counts:
        redis_kloc = counts["redis"]["total"] / 1000
        firn_kloc = counts["firn"]["total"] / 1000
        lines.append(f"KLOC firn {firn_kloc:.2f} redis {redis_kloc:.2f}")
        redis_rate = redis[0] / redis_kloc
        for variant in ("firn-selector", "firn-weakest", "firn-selector-release"):
            per_round = [r for r in measured if r["variant"] == variant and "ordinary_ms" in r]
            if not per_round:
                continue
            alone = statistics.median(r["seconds"] - r["ordinary_ms"] / 1000 for r in per_round)
            whole = statistics.median(r["seconds"] for r in per_round)
            wf_kloc = (counts["firn"]["wf"] + counts["firn"]["std"]) / 1000
            lines.append(
                f"{variant:24} derivation-alone {alone:.2f} s; verdict ratio per KLOC "
                f"{(alone / firn_kloc) / redis_rate:.2f}; whole-time ratio per KLOC "
                f"{(whole / firn_kloc) / redis_rate:.2f}; raw ratio {alone / redis[0]:.2f}; "
                f"ratio per Whitefoot-source KLOC (reference) {(alone / wf_kloc) / redis_rate:.2f}")
        main = stats("firn-main", "seconds")
        if main:
            lines.append(f"firn-main ratio per KLOC {(main[0] / firn_kloc) / redis_rate:.2f}")
    multi = {r["variant"]: r["seconds"] for r in rows if r["stage"] == "multicore"}
    if multi:
        lines.append("multicore (reference): " + json.dumps(multi))
    text = "\n".join(lines)
    (OUT / "summary.txt").write_text(text + "\n")
    log(text)
    step = os.environ.get("GITHUB_STEP_SUMMARY")
    if step:
        with open(step, "a") as handle:
            handle.write("```\n" + text + "\n```\n")


def everything():
    """Every stage under one hold of the host's check lock."""
    prepare()
    redis()
    count()
    rounds()
    if os.environ.get("E1_MULTICORE") == "1":
        multicore()
    profile()
    summary()


if __name__ == "__main__":
    {"prepare": prepare, "redis": redis, "count": count, "rounds": rounds,
     "multicore": multicore, "profile": profile, "summary": summary,
     "all": everything}[sys.argv[1]]()
