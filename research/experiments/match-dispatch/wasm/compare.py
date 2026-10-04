#!/usr/bin/env python3
"""Run E0's kernels on Silverfir-nano's interpreter and on E0 variants,
interleaved, and compare whole-process cycles for the same work.

    python3 compare.py --nano SF_NANO_CLI --nano-count SF_NANO_CLI_COUNTING \
        --wasm kernels.wasm --e0 E0_BIN_DIR --rounds 10 --tsv nano.tsv
    python3 compare.py ... --summarize nano.tsv

poly is excluded: compiled from C its body folds to constants, so the
wasm program does not repeat vm.c's work. `nop` measures the wasm engine's
start-up, which the summary subtracts from nano's kernels.
"""
import argparse
import random
import re
import statistics
import subprocess
import sys

KERNELS = ["loop", "fib", "sieve", "mandel"]
VARIANTS = ["tailpn-checked", "tailpn-u8", "cellpn-u8", "tailpn-u8v", "cellpn-raw", "switch-u8"]


def timed(cmd):
    proc = subprocess.run(["/usr/bin/time", "-l"] + cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.exit(f"{cmd} exited {proc.returncode}: {proc.stderr[-400:]}")
    cycles = int(re.search(r"(\d+)\s+cycles elapsed", proc.stderr).group(1))
    insns = int(re.search(r"(\d+)\s+instructions retired", proc.stderr).group(1))
    out = proc.stdout.split()
    return out[1] if len(out) > 1 else "", cycles, insns


def command(args, who, kernel):
    if who == "nano":
        return [args.nano, "--interp", args.wasm, kernel]
    return [f"{args.e0}/{who}", kernel]


def run(args):
    pairs = [("nano", k) for k in KERNELS + ["nop"]] + [(v, k) for v in VARIANTS for k in KERNELS]
    checksums = {}
    with open(args.tsv, "a") as tsv:
        for r in range(args.rounds):
            random.shuffle(pairs)
            for who, kernel in pairs:
                checksum, cycles, insns = timed(command(args, who, kernel))
                if checksums.setdefault(kernel, checksum) != checksum:
                    sys.exit(f"checksum mismatch: {who} {kernel}")
                tsv.write(f"{r}\t{who}\t{kernel}\t{cycles}\t{insns}\n")
                tsv.flush()
            print(f"round {r + 1}/{args.rounds} done", file=sys.stderr)


def counts(args):
    out = {}
    for kernel in KERNELS:
        err = subprocess.run([args.nano_count, "--interp-stats", args.wasm, kernel],
                             capture_output=True, text=True, check=True).stderr
        out[("nano", kernel)] = int(re.search(r"native dispatches: (\d+)", err).group(1))
        for access in ["checked", "u8", "u8v", "raw"]:
            text = subprocess.run([f"{args.e0}/count-{access}", kernel],
                                  capture_output=True, text=True, check=True).stdout
            out[(access, kernel)] = int(text.split("dispatches")[1])
    return out


def summarize(args):
    n = counts(args)
    s = {}
    for line in open(args.summarize):
        _, who, kernel, cycles, insns = line.split("\t")
        s.setdefault((who, kernel), []).append((int(cycles), int(insns)))
    nop = statistics.median(c for c, _ in s[("nano", "nop")])
    print(f"nano start-up (nop), median cycles: {nop:.0f}")
    print("median cycles (nano start-up subtracted), ratio to nano, cycles per dispatch")
    print(f"{'':16}" + "".join(f"{k:>34}" for k in KERNELS))
    for who in ["nano"] + VARIANTS:
        cells = []
        for kernel in KERNELS:
            cyc = statistics.median(c for c, _ in s[(who, kernel)])
            if who == "nano":
                cyc -= nop
            base = statistics.median(c for c, _ in s[("nano", kernel)]) - nop
            d = n[("nano", kernel)] if who == "nano" else n[(who.split("-")[1], kernel)]
            cells.append(f"{cyc / 1e6:9.1f}M {cyc / base:6.3f}x {cyc / d:6.3f}c/d")
        print(f"{who:16}" + "".join(f"{c:>34}" for c in cells))
    print("dispatches: " + ", ".join(
        f"{k} nano {n[('nano', k)]} vm {n[('u8', k)]}" for k in KERNELS))


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--nano", required=True)
    p.add_argument("--nano-count", required=True)
    p.add_argument("--wasm", required=True)
    p.add_argument("--e0", required=True)
    p.add_argument("--rounds", type=int, default=10)
    p.add_argument("--tsv", default="nano.tsv")
    p.add_argument("--summarize")
    args = p.parse_args()
    summarize(args) if args.summarize else run(args)


if __name__ == "__main__":
    main()
