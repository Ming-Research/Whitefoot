#!/usr/bin/env python3
"""Rewrites one --par module's overlapped split sites into the experiment's arms.

Usage: arms.py ARM INPUT.ll OUTPUT.ll, ARM one of:
  direct  every overlapped split site calls its chunk, without the budget
          query or the splitter (the sequential world's call, in the
          overlapped world);
  zero    every site keeps the query and calls its chunk when the budget is
          zero, the splitter otherwise.
The sequential world, the splitters and the chunks are left as emitted. A site
is the emitter's two-line pair: a wf__par_split_budget call whose result is the
last argument of the next line's splitter call (emitter/parallel.rs,
emit_loop_split). Each splitter's chunk is the one wf__par_chunk_* it calls.
"""
import re
import sys

QUERY = re.compile(r"^  (%[\w.]+) = call i64 @wf__par_split_budget\(.*\)$")
SPLIT = re.compile(r"^  (%[\w.]+) = call (\S+) @(wf__par_split_[\w.$]+)\((.*), i64 (%[\w.]+)\)$")
DEFINE = re.compile(r"^define .*@(wf__par_split_[\w.$]+)\(")
CHUNK = re.compile(r"call \S+ @(wf__par_chunk_[\w.$]+)\(")


def chunks_of(lines):
    found, current = {}, None
    for line in lines:
        match = DEFINE.match(line)
        if match:
            current = match.group(1)
        elif line.startswith("}"):
            current = None
        elif current:
            called = CHUNK.search(line)
            if called:
                found.setdefault(current, set()).add(called.group(1))
    return {name: callees.pop() for name, callees in found.items() if len(callees) == 1}


def main(arm, source, target):
    lines = open(source).read().split("\n")
    chunk = chunks_of(lines)
    out, sites, i = [], 0, 0
    while i < len(lines):
        query = QUERY.match(lines[i])
        split = SPLIT.match(lines[i + 1]) if query and i + 1 < len(lines) else None
        if not (query and split and split.group(5) == query.group(1) and split.group(3) in chunk):
            out.append(lines[i])
            i += 1
            continue
        result, ty, splitter, arguments = split.group(1), split.group(2), split.group(3), split.group(4)
        direct = f"call {ty} @{chunk[splitter]}({arguments})"
        if arm == "direct":
            out.append(f"  {result} = {direct}")
        else:
            n = sites
            out += [
                lines[i],
                f"  %lsg.zero.{n} = icmp eq i64 {query.group(1)}, 0",
                f"  br i1 %lsg.zero.{n}, label %lsg.direct.{n}, label %lsg.split.{n}",
                f"lsg.direct.{n}:",
                f"  %lsg.a.{n} = {direct}",
                f"  br label %lsg.join.{n}",
                f"lsg.split.{n}:",
                f"  %lsg.b.{n} = " + lines[i + 1].split(" = ", 1)[1],
                f"  br label %lsg.join.{n}",
                f"lsg.join.{n}:",
                f"  {result} = phi {ty} [ %lsg.a.{n}, %lsg.direct.{n} ], [ %lsg.b.{n}, %lsg.split.{n} ]",
            ]
        sites += 1
        i += 2
    if sites == 0:
        sys.exit(f"arms.py: no split site in {source}")
    open(target, "w").write("\n".join(out))
    print(f"arms.py: {arm}: rewrote {sites} site(s)", file=sys.stderr)


if __name__ == "__main__":
    if len(sys.argv) != 4 or sys.argv[1] not in ("direct", "zero"):
        sys.exit(__doc__)
    main(*sys.argv[1:])
