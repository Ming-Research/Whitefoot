"""Writes the shapes of four families as C and as Rust, and times a check of
each by clang and by rustc as the size grows
(research/investigations/join-derivation/DESIGN.md, version 4).

usage: shapes.py <output directory>"""

import pathlib
import subprocess
import sys
import time

SIZES = [64, 256, 1024, 4096, 16384]


def chain(k, rust):
    args = ", ".join((f"c{i}: bool" if rust else f"int c{i}") for i in range(k))
    body = "".join(f"    if {'c%d' % i if rust else '(c%d)' % i} {{ x = x + 1; }}\n" for i in range(k))
    if rust:
        return f"pub fn probe(start: u64, {args}) -> u64 {{\n    let mut x = start;\n{body}    x\n}}\n"
    return f"unsigned long probe(unsigned long start, {args}) {{\n    unsigned long x = start;\n{body}    return x;\n}}\n"


def guarded(k, rust):
    body = "".join("    if i + 1 < n { i = i + 1; }\n" if rust else "    if (i + 1 < n) { i = i + 1; }\n" for _ in range(k))
    if rust:
        return f"pub fn probe(start: u64, n: u64) -> u64 {{\n    let mut i = start;\n{body}    i\n}}\n"
    return f"unsigned long probe(unsigned long start, unsigned long n) {{\n    unsigned long i = start;\n{body}    return i;\n}}\n"


def sumobl(k, rust):
    if rust:
        body = "".join(f"    s = s + a[{i}] as u64;\n" for i in range(k))
        return f"pub fn probe(a: &[u8; {k}]) -> u64 {{\n    let mut s = 0u64;\n{body}    s\n}}\n"
    body = "".join(f"    s = s + a[{i}];\n" for i in range(k))
    return f"unsigned long probe(const unsigned char a[{k}]) {{\n    unsigned long s = 0;\n{body}    return s;\n}}\n"


def limitchain(k, rust):
    if rust:
        body = "".join(f"    n = n + d[{i}] as u64;\n    if i + 1 < n {{ i = i + 1; }}\n" for i in range(k))
        return f"pub fn probe(start: u64, limit: u64, d: &[u8; {k}]) -> u64 {{\n    let mut i = start;\n    let mut n = limit;\n{body}    i\n}}\n"
    body = "".join(f"    n = n + d[{i}];\n    if (i + 1 < n) {{ i = i + 1; }}\n" for i in range(k))
    return f"unsigned long probe(unsigned long start, unsigned long limit, const unsigned char d[{k}]) {{\n    unsigned long i = start;\n    unsigned long n = limit;\n{body}    return i;\n}}\n"


SHAPES = {"chain": chain, "guarded": guarded, "sumobl": sumobl, "limitchain": limitchain}
TOOLS = {
    "clang-syntax": (False, ["clang", "-fsyntax-only"]),
    "clang-O0": (False, ["clang", "-O0", "-c", "-o", "/dev/null"]),
    "clang-O2": (False, ["clang", "-O2", "-c", "-o", "/dev/null"]),
    "rustc-check": (True, ["rustc", "--edition", "2021", "--crate-type", "lib", "--emit", "metadata", "-o", "/dev/null"]),
    "rustc-O0": (True, ["rustc", "--edition", "2021", "--crate-type", "lib", "--emit", "obj", "-o", "/dev/null"]),
    "rustc-O2": (True, ["rustc", "--edition", "2021", "--crate-type", "lib", "-C", "opt-level=2", "--emit", "obj", "-o", "/dev/null"]),
}


def main():
    out = pathlib.Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    print("row,shape,k,tool,status,best_seconds_of_3")
    for name, make in SHAPES.items():
        for tool, (rust, command) in TOOLS.items():
            for k in SIZES:
                if name == "chain" and k > 4096:
                    continue
                source = out / f"{name}-{k}.{'rs' if rust else 'c'}"
                source.write_text(make(k, rust))
                best, status = None, 0
                for _ in range(3):
                    start = time.monotonic()
                    try:
                        done = subprocess.run(command + [str(source)], capture_output=True, timeout=600)
                        status = done.returncode
                    except subprocess.TimeoutExpired:
                        status = "timeout"
                    spent = time.monotonic() - start
                    best = spent if best is None else min(best, spent)
                    if status != 0:
                        break
                print(f"shape,{name},{k},{tool},{status},{best:.3f}", flush=True)
                if status != 0 or best > 120:
                    break


if __name__ == "__main__":
    main()
