#!/usr/bin/env python3
"""Runs CoreMark under several wasm interpreters, alternating launches, and
reports each one's median score.

    python3 coremark.py --module coremark.wasm --iterations 2000 --launches 5 \
        --tsv run-wasm-v1.tsv wf=./wasm-interp nano="sf-nano-cli --interp"

Each NAME=COMMAND runs as COMMAND MODULE 0x0 0x0 0x66 ITERATIONS. A launch
counts only when CoreMark's list, matrix and state CRCs equal the 2K
performance run's (0xe714, 0x1fd7, 0x8e3a) and every launch of the run
agrees on the final CRC.
"""

import argparse
import re
import shlex
import statistics
import subprocess
import sys

EXPECTED = {"crclist": "0xe714", "crcmatrix": "0x1fd7", "crcstate": "0x8e3a"}


def launch(command, module, iterations):
    argv = shlex.split(command) + [module, "0x0", "0x0", "0x66", str(iterations)]
    out = subprocess.run(argv, capture_output=True, text=True, check=False).stdout
    score = re.search(r"Iterations/Sec\s*:\s*([0-9.]+)", out)
    crcs = dict(re.findall(r"\[0\](crc\w+)\s*:\s*(0x[0-9a-f]+)", out))
    if score is None or any(crcs.get(k) != v for k, v in EXPECTED.items()):
        sys.exit(f"{command}: no valid CoreMark result:\n{out}")
    return float(score.group(1)), crcs["crcfinal"]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--module", required=True)
    parser.add_argument("--iterations", type=int, required=True)
    parser.add_argument("--launches", type=int, default=5)
    parser.add_argument("--tsv")
    parser.add_argument("engines", nargs="+", help="NAME=COMMAND")
    args = parser.parse_args()
    engines = [e.split("=", 1) for e in args.engines]
    scores = {name: [] for name, _ in engines}
    finals = set()
    rows = []
    for r in range(args.launches):
        for name, command in engines:
            score, final = launch(command, args.module, args.iterations)
            scores[name].append(score)
            finals.add(final)
            rows.append(f"{r}\t{name}\t{args.iterations}\t{score}\t{final}\n")
    if len(finals) != 1:
        sys.exit(f"launches disagree on crcfinal: {sorted(finals)}")
    if args.tsv:
        with open(args.tsv, "w") as tsv:
            tsv.write("launch\tengine\titerations\tscore\tcrcfinal\n")
            tsv.writelines(rows)
    base = statistics.median(scores[engines[-1][0]])
    for name, _ in engines:
        s = scores[name]
        med = statistics.median(s)
        spread = (max(s) - min(s)) / med
        print(f"{name}\tmedian {med:.1f}\tspread {spread:.1%}\tratio to {engines[-1][0]} {med / base:.3f}")


if __name__ == "__main__":
    main()
