#!/usr/bin/env python3
"""Run E0: every variant built by build.sh on every kernel, interleaved.

Each round launches every (binary, kernel) pair once in a shuffled order and
records the process's cycles and instructions retired from `/usr/bin/time -l`
(macOS). The summary divides by the kernel's dispatch count, taken from the
counting build, and reports medians over rounds.

    python3 run.py BIN_DIR --rounds 10 --tsv raw.tsv
    python3 run.py BIN_DIR --summarize raw.tsv
"""
import argparse
import math
import os
import random
import re
import statistics
import subprocess
import sys

KERNELS = ["loop", "fib", "sieve", "mandel", "poly", "floor"]
SHAPES = ["switch", "goto", "tail", "tailpn", "cell", "cellpn"]
ACCESS = ["checked", "u8", "u8v", "raw"]
CONTROLS = ["tailpn-checked-pad64", "tailpn-checked-pad2048"]


def measure(binary, kernel):
    proc = subprocess.run(["/usr/bin/time", "-l", binary, kernel],
                          capture_output=True, text=True)
    if proc.returncode != 0:
        sys.exit(f"{binary} {kernel} exited {proc.returncode}: {proc.stderr}")
    cycles = int(re.search(r"(\d+)\s+cycles elapsed", proc.stderr).group(1))
    insns = int(re.search(r"(\d+)\s+instructions retired", proc.stderr).group(1))
    return proc.stdout.split()[1], cycles, insns


def dispatch_counts(bin_dir):
    counts = {}
    for access in ACCESS:
        for kernel in KERNELS:
            out = subprocess.run([os.path.join(bin_dir, f"count-{access}"), kernel],
                                 capture_output=True, text=True, check=True).stdout
            counts[(access, kernel)] = int(out.split("dispatches")[1])
    return counts


def run(args):
    variants = [f"{s}-{a}" for a in ACCESS for s in SHAPES] + CONTROLS
    pairs = [(v, k) for v in variants for k in KERNELS]
    checksums = {}
    with open(args.tsv, "a") as tsv:
        for r in range(args.rounds):
            random.shuffle(pairs)
            for variant, kernel in pairs:
                checksum, cycles, insns = measure(os.path.join(args.bin_dir, variant), kernel)
                if checksums.setdefault(kernel, checksum) != checksum:
                    sys.exit(f"checksum mismatch: {variant} {kernel}")
                tsv.write(f"{r}\t{variant}\t{kernel}\t{cycles}\t{insns}\n")
                tsv.flush()
            print(f"round {r + 1}/{args.rounds} done", file=sys.stderr)


def summarize(args):
    counts = dispatch_counts(args.bin_dir)
    samples = {}
    for line in open(args.summarize):
        _, variant, kernel, cycles, insns = line.split("\t")
        samples.setdefault((variant, kernel), []).append((int(cycles), int(insns)))
    kernels = [k for k in KERNELS if any(kk == k for _, kk in samples)]
    variants = sorted({v for v, _ in samples}, key=lambda v: (v.split("-")[1], v))
    print("cycles per dispatch, median over launches (spread = (max-min)/median)")
    print(f"{'variant':26}" + "".join(f"{k:>16}" for k in kernels) + f"{'geomean':>10}")
    rows = {}
    for variant in variants:
        access = variant.split("-")[1]
        cells, medians = [], []
        for kernel in kernels:
            cyc = [c for c, _ in samples[(variant, kernel)]]
            d = counts[(access, kernel)]
            med = statistics.median(cyc) / d
            spread = (max(cyc) - min(cyc)) / statistics.median(cyc)
            medians.append(med)
            cells.append(f"{med:7.3f} ±{spread * 100:4.1f}%")
        geo = math.exp(sum(map(math.log, medians)) / len(medians))
        rows[variant] = (medians, geo)
        print(f"{variant:26}" + "".join(f"{c:>16}" for c in cells) + f"{geo:10.3f}")
    print()
    print("instructions per dispatch, median")
    for variant in variants:
        access = variant.split("-")[1]
        vals = [statistics.median([i for _, i in samples[(variant, k)]]) / counts[(access, k)]
                for k in kernels]
        print(f"{variant:26}" + "".join(f"{v:16.2f}" for v in vals))
    print()
    print("ratio to switch of the same access form (geomean of per-kernel medians)")
    for variant in variants:
        shape, access = variant.split("-")[0], variant.split("-")[1]
        base = rows.get(f"switch-{access}")
        if not base:
            continue
        ratios = [m / b for m, b in zip(rows[variant][0], base[0])]
        geo = math.exp(sum(map(math.log, ratios)) / len(ratios))
        print(f"{variant:26}" + "".join(f"{r:16.3f}" for r in ratios) + f"{geo:10.3f}")


def main():
    p = argparse.ArgumentParser()
    p.add_argument("bin_dir")
    p.add_argument("--rounds", type=int, default=10)
    p.add_argument("--tsv", default="raw.tsv")
    p.add_argument("--summarize")
    args = p.parse_args()
    summarize(args) if args.summarize else run(args)


if __name__ == "__main__":
    main()
