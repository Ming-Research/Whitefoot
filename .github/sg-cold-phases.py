"""Temporary (sg-cold-attrib.yml only; removed with it): rolls `perf script
-F comm,tid,ip,sym` call stacks of a whitefootc build up by phase and thread.

A sample's phase is the outermost stage entry point on its stack; its context is "module verdicts" when `require_module_verdicts` is on
the stack and "entry" otherwise. Samples of other processes (clang, ld.lld)
are counted by command. perf's own Python bindings are not used, so the
script reads text and runs on the runner's stock python3.
"""

import collections
import sys

# Stage entry points, matched walking from the root of the stack (perf
# prints the leaf first, so the stack is reversed); the outermost stage seen
# names the sample. Each needle has its demangled and its legacy-mangled form.
STAGES = [
    ("lex/parse/canonical", ("canonical_syntax", "16canonical_syntax")),
    ("resolve", ("resolution::resolve", "10resolution7resolve")),
    ("check", ("check_semantics",)),
    ("lowering", ("lower_checked_from",)),
    ("dispatch layout", ("prepare_dispatch_layout",)),
    ("emission", ("emit_prepared_llvm",)),
    ("split", ("split_module",)),
    ("objects and link (waiting)", ("link_cached_objects", "cached_object")),
]
# A sample inside a function analysis is proof work, on whichever thread.
PROOF = ("analyze_function",)


def phase_of(stack):
    proof = any(needle in frame for frame in stack for needle in PROOF)
    for frame in reversed(stack):
        for name, needles in STAGES:
            if any(needle in frame for needle in needles):
                if name == "check" and proof:
                    return "check: proof analysis"
                return name
    return "proof analysis (worker)" if proof else "other"


def main():
    by_phase = collections.Counter()
    by_thread = collections.Counter()
    by_command = collections.Counter()
    header, stack = None, []

    def flush():
        if header is None:
            return
        comm, tid = header
        by_command[comm] += 1
        if not comm.startswith("whitefootc"):
            return
        by_thread[tid] += 1
        context = (
            "module verdicts"
            if any("require_module_verdicts" in frame for frame in stack)
            else "entry"
        )
        by_phase[(context, phase_of(stack))] += 1

    for line in sys.stdin:
        if not line.strip():
            flush()
            header, stack = None, []
        elif line[0] in " \t":
            parts = line.split(None, 1)
            stack.append(parts[1] if len(parts) > 1 else "")
        else:
            flush()
            fields = line.split()
            header, stack = (fields[0], fields[1] if len(fields) > 1 else "?"), []
    flush()

    total = sum(by_command.values()) or 1
    print("samples by command (199 Hz):")
    for comm, count in by_command.most_common():
        print(f"  {comm:20} {count:8} {100 * count / total:5.1f}%")
    front = sum(by_phase.values()) or 1
    print("whitefootc samples by context and phase:")
    for (context, phase), count in sorted(by_phase.items(), key=lambda item: -item[1]):
        print(f"  {context:16} {phase:28} {count:8} {100 * count / front:5.1f}%")
    print("whitefootc samples by thread (the driver thread is the largest):")
    for tid, count in by_thread.most_common(12):
        print(f"  {tid:10} {count:8} {100 * count / front:5.1f}%")


main()
