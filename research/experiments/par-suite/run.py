#!/usr/bin/env python3
"""Manual Linux experiment; all native work belongs on CI hosts."""
import argparse
import csv
import hashlib
import json
import math
import os
import platform
import re
import shutil
import subprocess
from pathlib import Path

from generate import generate
from summarize import RAW_FIELDS, read_tsv, write_summary

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
OVERRIDES = ("WF_WORKERS", "WF_SPLIT_WORK", "WF_SCHED_REPORT", "WF_STACKS", "WF_IO_HELPERS")


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def clean_environment():
    return {key: value for key, value in os.environ.items() if key not in OVERRIDES}


def command(arguments, log, env=None):
    with Path(log).open("w") as stream:
        stream.write(json.dumps([str(arg) for arg in arguments]) + "\n")
        stream.flush()
        subprocess.run([str(arg) for arg in arguments], cwd=ROOT, env=env,
                       stdout=stream, stderr=subprocess.STDOUT, check=True)


def write_tsv(path, rows):
    with Path(path).open("w", newline="") as stream:
        writer = csv.DictWriter(stream, list(rows[0]), delimiter="\t", extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)


def require_kernel_verdict(status, text):
    match = re.search(r"^VERDICT: (PASS|FAIL) -- [0-9]+ kernel\(s\) adverse at two widths$", text, re.M)
    if status not in (0, 1) or not match or match[1] != ("PASS" if status == 0 else "FAIL"):
        raise RuntimeError("formal-kernel campaign incomplete")


def kernel_campaign(wfc, build, results):
    # Reuse the instrument unchanged, including its oracle checks, warmups,
    # five rounds/five calls, eligible W1/W2/W4, and null qualification.
    env = clean_environment()
    listed = [r["name"] for r in read_tsv(HERE / "real-programs.tsv") if r["kind"] == "kernel"]
    maintained = re.search(r"^KERNELS := (.+)$", (ROOT / "tests/performance/Makefile").read_text(), re.M)
    if not maintained or listed != maintained[1].split():
        raise RuntimeError("real-program kernel list differs from the maintained instrument")
    env["PAR_SUITE_WFC"] = str(wfc)
    for arm in ("seq", "par"):
        env["PAR_SUITE_ARM"] = arm
        command(["make", "-C", ROOT / "tests/performance", "build",
                 f"BUILD={build / ('kernels-' + arm)}",
                 f"WFC={HERE / 'compiler-shim.py'}"], results / f"kernels-{arm}-build.log", env)
    null = subprocess.run(["bash", str(ROOT / "tests/performance/compare.sh"),
                           str(build / "kernels-par"), str(build / "kernels-par"),
                           str(results / "kernels-null")], cwd=ROOT, env=env)
    if null.returncode:
        raise RuntimeError("formal-kernel identical-image control failed; retain evidence, stop")
    comparison = subprocess.run(["bash", str(ROOT / "tests/performance/compare.sh"),
                                 str(build / "kernels-seq"), str(build / "kernels-par"),
                                 str(results / "kernels-seq-par")], cwd=ROOT, env=env)
    # compare.sh has its own performance decision. Record it separately; only
    # a complete reducer output is evidence, regardless of a ratio verdict.
    (results / "kernels-status.txt").write_text(f"compare.sh exit={comparison.returncode}\n")
    verdict = results / "kernels-seq-par/verdict.txt"
    require_kernel_verdict(comparison.returncode, verdict.read_text() if verdict.is_file() else "")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--compiler", type=Path, required=True)
    parser.add_argument("--build-dir", type=Path, required=True, help="must not exist")
    parser.add_argument("--results", type=Path, required=True, help="must not exist")
    parser.add_argument("--rounds", type=int, default=5)
    parser.add_argument("--workers", nargs="+", default=["1", "2", "4", "8", "16", "default"])
    parser.add_argument("--phase", choices=("pilot", "visible", "verdict"), default="visible")
    parser.add_argument("--epsilon", type=float, default=0.02)
    parser.add_argument("--startup-ns", type=int, required=True,
                        help="fixed pre-campaign host allowance; zero is a strict exploratory baseline")
    parser.add_argument("--allowance-source", required=True, help="host calibration identity, or uncalibrated-zero")
    parser.add_argument("--process-timeout", type=int, default=60)
    parser.add_argument("--skip-kernels", action="store_true", help="explicit partial campaign")
    args = parser.parse_args()
    if platform.system() != "Linux":
        parser.error("stage-one runner requires Linux; Apple Silicon qualification remains open")
    if args.rounds < 3 or args.rounds > 100 or args.startup_ns < 0 or not math.isfinite(args.epsilon) or args.epsilon < 0:
        parser.error("require 3..100 rounds and finite nonnegative allowances")
    if not 1 <= args.process_timeout <= 3600:
        parser.error("process timeout must be 1..3600 seconds")
    if len(set(args.workers)) != len(args.workers) or not args.workers or any(
            w != "default" and (not w.isdecimal() or not 1 <= int(w) <= 256 or str(int(w)) != w)
            for w in args.workers):
        parser.error("workers must be distinct canonical positive integers <=256, or default")
    wfc, build, results = (p.resolve() for p in (args.compiler, args.build_dir, args.results))
    if not wfc.is_file() or build.exists() or results.exists() or build == results:
        parser.error("compiler must exist; build and results must be different fresh directories")
    build.mkdir(parents=True)
    results.mkdir(parents=True)
    (results / "logs").mkdir()
    (build / "images").mkdir()
    generated = generate(build / "generated")
    shutil.copy2(build / "generated/manifest.tsv", results / "manifest.tsv")
    selected = [dict(c, kind="generated", expected_exit="0") for c in generated
                if args.phase == "verdict" or c["split"] == "visible"]
    real = read_tsv(HERE / "real-programs.tsv")
    if args.phase == "pilot":
        # Smallest admitted representative of each shape; no held-out timing.
        selected = [next(c for c in selected if c["shape"] == shape and c["family"] == "work"
                         and c["steps"] == 1) for shape in ("flat", "balanced", "skew90", "skew99", "spine", "dag")]
    else:
        for item in real:
            if item["kind"] == "program" and (args.phase == "verdict" or item["split"] == "visible"):
                selected.append(dict(cell="real-" + item["name"], split=item["split"],
                                     kind="program", source=item["source"],
                                     expected_exit=item["expected_exit"]))
    # A common explicit schema lets the reducer demand the entire selection.
    write_tsv(results / "selected.tsv", [{k: c[k] for k in ("cell", "split", "kind", "source", "expected_exit")}
                                          for c in selected])
    config = dict(phase=args.phase, rounds=args.rounds, workers=args.workers,
                  epsilon=args.epsilon, startup_ns=args.startup_ns, allowance_source=args.allowance_source,
                  compiler=str(wfc), compiler_sha256=digest(wfc), host=platform.uname()._asdict(),
                  cpus_online=os.cpu_count(), affinity=sorted(os.sched_getaffinity(0)),
                  default_workers=min(len(os.sched_getaffinity(0)), 64),
                  process_timeout=args.process_timeout, skip_kernels=args.skip_kernels,
                  revision=subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                  source_hashes={c["cell"]: digest((build / "generated" if c["kind"] == "generated" else ROOT) / c["source"])
                                 for c in selected},
                  instrument_hashes={p.name: digest(p) for p in HERE.iterdir() if p.is_file()})
    config["maintained_hashes"] = {str(p.relative_to(ROOT)): digest(p)
                                  for directory in (ROOT / "tests/performance", ROOT / "tests/programs/compute")
                                  for p in directory.iterdir() if p.is_file()}
    (results / "run.json").write_text(json.dumps(config, indent=2) + "\n")
    command(["lscpu"], results / "host.txt")
    command(["make", "-C", HERE, "support", f"BUILD={build}"], results / "support-build.log")
    images = {}
    # Construction order alternates by cell; measured *image* and width order
    # rotates/reverses each round. Construction itself is never in raw.tsv.
    for i, cell in enumerate(selected):
        name = cell["cell"]
        source = (build / "generated" if cell["kind"] == "generated" else ROOT) / cell["source"]
        for arm in (("seq", "par") if i % 2 == 0 else ("par", "seq")):
            image = build / "images" / f"{name}-{arm}"
            if cell["kind"] == "generated":
                cmd = ["make", "-C", HERE, "image", f"BUILD={build}", f"WFC={wfc}",
                       f"SOURCE={source}", f"IMAGE={image}", f"ARM={arm}"]
            else:
                cmd = [wfc, *(["--par", "--par-ledger"] if arm == "par" else []), source, "-o", image]
            command(cmd, results / "logs" / f"build-{name}-{arm}.log", clean_environment())
            images[name, arm] = image
        twin = build / "images" / f"{name}-twin"
        shutil.copy2(images[name, "par"], twin)
        images[name, "twin"] = twin
        if digest(twin) != digest(images[name, "par"]):
            raise RuntimeError("twin copy differs")
    before = {str(p): digest(p) for p in images.values()}
    (results / "images-before.json").write_text(json.dumps(before, indent=2) + "\n")
    expected_checksums = {}
    with (results / "raw.tsv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, RAW_FIELDS, delimiter="\t")
        writer.writeheader()
        for round_number in range(args.rounds):
            for j in range(len(selected)):
                cell = selected[(j + round_number) % len(selected)]
                name = cell["cell"]
                order = [(w, arm) for w in args.workers for arm in ("seq", "par", "twin")]
                offset = (round_number + j) % len(order)
                order = order[offset:] + order[:offset]
                if round_number % 2:
                    order.reverse()
                for width, arm in order:
                    key = f"{name}-{arm}-w{width}-r{round_number}"
                    print(key, flush=True)
                    env = clean_environment()
                    if width != "default":
                        env["WF_WORKERS"] = width
                    metrics = results / "logs" / f"{key}.metrics"
                    output = results / "logs" / f"{key}.out"
                    with output.open("wb") as out, (results / "logs" / f"{key}.err").open("wb") as err:
                        subprocess.run([str(build / "measure"), str(metrics), str(args.process_timeout),
                                        str(images[name, arm])], env=env, stdout=out, stderr=err, check=True)
                    wall, cpu, status = map(int, metrics.read_text().split())
                    content = output.read_bytes()
                    if cell["kind"] == "generated":
                        value = content.strip()
                        checksum = value.decode("ascii") if value.isdigit() else "invalid-output"
                    else:
                        # Maintained programs validate their own answer; retain
                        # status and complete output identity, not a made-up u64.
                        checksum = f"exit:{status};stdout-sha256:{hashlib.sha256(content).hexdigest()}"
                    writer.writerow(dict(cell=name, build=arm, workers=width, round=round_number,
                                         wall_ns=wall, cpu_ns=cpu, exit_status=status, checksum=checksum))
                    stream.flush()
                    if status != int(cell["expected_exit"]) or checksum == "invalid-output":
                        raise RuntimeError(f"failed process {key}; see retained logs")
                    if cell["kind"] == "program" and wall >= 1000000000:
                        raise RuntimeError(f"{key} exceeds the one-second real-program inclusion bound; no silent removal")
                    previous = expected_checksums.setdefault(name, checksum)
                    if previous != checksum:
                        raise RuntimeError(f"checksum mismatch for {key}: {previous} != {checksum}")
    after = {str(p): digest(p) for p in images.values()}
    (results / "images-after.json").write_text(json.dumps(after, indent=2) + "\n")
    if after != before:
        raise RuntimeError("an image changed during measurement")
    print(write_summary(results), end="")
    if args.phase != "pilot" and not args.skip_kernels:
        kernel_campaign(wfc, build, results)


if __name__ == "__main__":
    main()
