#!/usr/bin/env python3
"""Run E0's kernels on Silverfir-nano's interpreter and on vm.c / vm1.c
variants, interleaved, and compare whole-process cycles for the same work.

    python3 compare.py --nano SF_NANO_CLI --nano-count SF_NANO_CLI_COUNTING \
        --wasm kernels.wasm --bin BIN_DIR --variants V1,V2,... --rounds 10 --tsv out.tsv
    python3 compare.py ... --summarize out.tsv

A variant is a binary built by build.sh, optionally followed by `@MODE` for
vm1.c's residency mode (none, acc, accpin). poly is excluded: compiled from C
its body folds to constants, so the wasm program does not repeat vm.c's work.
`nop` measures the wasm engine's start-up, which the summary subtracts from
Silverfir-nano's kernels.
"""
import argparse
import random
import re
import statistics
import subprocess
import sys

KERNELS = ["loop", "fib", "sieve", "mandel"]


def timed(cmd):
    proc = subprocess.run(["/usr/bin/time", "-l"] + cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.exit(f"{cmd} exited {proc.returncode}: {proc.stderr[-400:]}")
    cycles = int(re.search(r"(\d+)\s+cycles elapsed", proc.stderr).group(1))
    insns = int(re.search(r"(\d+)\s+instructions retired", proc.stderr).group(1))
    out = proc.stdout.split()
    # A Whitefoot interpreter prints nothing: its exit status 0 says its
    # checksum equalled vm.c's.
    return out[1] if len(out) > 1 else None, cycles, insns


def command(args, who, kernel):
    if who == "nano":
        return [args.nano, "--interp", args.wasm, kernel]
    binary, _, mode = who.partition("@")
    if binary.startswith("wf"):
        # A Whitefoot interpreter binary is built per kernel (wf/vm.wf).
        return [f"{args.bin}/{binary}-{kernel}"]
    return [f"{args.bin}/{binary}", kernel] + ([mode] if mode else [])


def count_binary(who):
    binary = who.partition("@")[0]
    if binary.startswith("wf"):
        # wf/vm.wf runs vm.c's bytecode in vm.c's u8 form.
        return "count-u8"
    if binary.startswith(("e1-", "e1hb-")):
        return "e1-count"
    return "count-" + binary.split("-")[1]


def run(args):
    variants = args.variants.split(",")
    pairs = [("nano", k) for k in KERNELS + ["nop"]] + [(v, k) for v in variants for k in KERNELS]
    checksums = {}
    with open(args.tsv, "a") as tsv:
        for r in range(args.rounds):
            random.shuffle(pairs)
            for who, kernel in pairs:
                checksum, cycles, insns = timed(command(args, who, kernel))
                if checksum is not None and checksums.setdefault(kernel, checksum) != checksum:
                    sys.exit(f"checksum mismatch: {who} {kernel}")
                tsv.write(f"{r}\t{who}\t{kernel}\t{cycles}\t{insns}\n")
                tsv.flush()
            print(f"round {r + 1}/{args.rounds} done", file=sys.stderr)


def dispatches(args, who, kernel, cache):
    key = "nano" if who == "nano" else count_binary(who)
    if (key, kernel) not in cache:
        if key == "nano":
            err = subprocess.run([args.nano_count, "--interp-stats", args.wasm, kernel],
                                 capture_output=True, text=True, check=True).stderr
            cache[(key, kernel)] = int(re.search(r"native dispatches: (\d+)", err).group(1))
        else:
            out = subprocess.run([f"{args.bin}/{key}", kernel],
                                 capture_output=True, text=True, check=True).stdout
            cache[(key, kernel)] = int(out.split("dispatches")[1])
    return cache[(key, kernel)]


def summarize(args):
    s, order = {}, []
    for line in open(args.summarize):
        _, who, kernel, cycles, insns = line.split("\t")
        if who not in order:
            order.append(who)
        s.setdefault((who, kernel), []).append((int(cycles), int(insns)))
    cache = {}
    nop = statistics.median(c for c, _ in s[("nano", "nop")])
    print(f"Silverfir-nano start-up (nop), median cycles: {nop:.0f}")
    print("median cycles (Silverfir-nano start-up subtracted) | ratio to Silverfir-nano | "
          "cycles per dispatch | instructions per dispatch")
    print(f"{'':26}" + "".join(f"{k:>40}" for k in KERNELS))
    rows = ["nano"] + sorted(w for w in order if w != "nano")
    for who in rows:
        cells = []
        for kernel in KERNELS:
            cyc = statistics.median(c for c, _ in s[(who, kernel)])
            ins = statistics.median(i for _, i in s[(who, kernel)])
            if who == "nano":
                cyc -= nop
            base = statistics.median(c for c, _ in s[("nano", kernel)]) - nop
            d = dispatches(args, who, kernel, cache)
            cells.append(f"{cyc / 1e6:8.1f}M {cyc / base:6.3f}x {cyc / d:6.3f} {ins / d:6.2f}")
        print(f"{who:26}" + "".join(f"{c:>40}" for c in cells))
    print("dispatches: " + ", ".join(
        f"{k} nano {dispatches(args, 'nano', k, cache)}" for k in KERNELS))


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--nano", required=True)
    p.add_argument("--nano-count", required=True)
    p.add_argument("--wasm", required=True)
    p.add_argument("--bin", required=True)
    p.add_argument("--variants", default="")
    p.add_argument("--rounds", type=int, default=10)
    p.add_argument("--tsv", default="nano.tsv")
    p.add_argument("--summarize")
    args = p.parse_args()
    summarize(args) if args.summarize else run(args)


if __name__ == "__main__":
    main()
