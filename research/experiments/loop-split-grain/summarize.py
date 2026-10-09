#!/usr/bin/env python3
"""Summarizes measure.sh rows: each arm's median milliseconds at one and four
workers, and per round, each arm's four-worker time over emitted's, with the
twin's ratio as the noise control.  Usage: summarize.py runs.tsv"""
import statistics
import sys

rows = [line.rstrip("\n").split("\t") for line in open(sys.argv[1])][1:]
times = {}
for rnd, arm, workers, ns in rows:
    times.setdefault((arm, workers), {})[int(rnd)] = int(ns) / 1e6
arms = ["emitted", "twin", "direct", "zero", "plain"]
print("arm\tW1 median ms\tW4 median ms\tW4/W1")
for arm in arms:
    w1 = statistics.median(times[(arm, "1")].values())
    w4 = statistics.median(times[(arm, "4")].values())
    print(f"{arm}\t{w1:.1f}\t{w4:.1f}\t{w4 / w1:.3f}")
base = times[("emitted", "4")]
for arm in arms[1:]:
    ratios = sorted(times[(arm, "4")][r] / base[r] for r in base)
    print(f"W4 {arm}/emitted per round: " + " ".join(f"{x:.3f}" for x in ratios))
