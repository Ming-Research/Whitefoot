#!/usr/bin/env python3
"""Byte comparison with the pre-prototype compiler, not self-comparison.

Called only by the explicit experiment workflow after both compilers exist.
The source paths, target and flags are identical for each pair.
"""
import argparse
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
BASE = "ac5f1498a431df3043fe42a5ce38f4eaf750e4c0"
SOURCES = ("parallel/tree.wf", "parallel/range_fold.wf", "parallel/indexed_reductions.wf",
           "compute/mandelbrot.wf", "compute/records.wf", "compute/fir.wf", "compute/stencil.wf",
           "compute/prefix.wf", "compute/histogram.wf")

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--base", required=True)
    p.add_argument("--candidate", required=True)
    p.add_argument("--output", required=True, type=Path)
    args = p.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for name in SOURCES:
        source = ROOT / "tests/programs" / name
        outputs = []
        for arm, compiler in (("base", args.base), ("candidate", args.candidate)):
            output = args.output / (name.replace("/", "-") + "." + arm + ".ll")
            subprocess.run([compiler, "--par", "--emit-llvm", str(source), "-o", str(output)], check=True)
            outputs.append(output.read_bytes())
        if outputs[0] != outputs[1]:
            raise SystemExit(f"--par emission changed for {name}; see {args.output}")
    (args.output / "result.json").write_text(json.dumps(dict(baseline=BASE, result="byte-identical", sources=SOURCES), indent=2) + "\n")

if __name__ == "__main__":
    main()
