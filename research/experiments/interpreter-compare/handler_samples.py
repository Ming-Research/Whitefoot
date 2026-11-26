#!/usr/bin/env python3
"""Per-instruction CPU sample shares of two builds of the wasm interpreter v2h
(README.md beside this file).

    handler_samples.py sample --out DIR --cpu N
    handler_samples.py self-test

`sample` reads what `interp.py build` left in DIR (images/v2h-base and
images/v2h-candidate, inputs/v2h-ARM.wf, inputs/coremark.wasm and its
iteration count), runs CoreMark under `perf record -e cpu-clock` once per arm
pinned to CPU N, and writes DIR/handler-shares.tsv and DIR/sample-summary.txt.

An instruction's handler is an arm of the interpreter's `run` loop; the
compiler names its body `wf_run(.body).arm.K`, K counting the arms of `run` in
source order. Every sample is counted: a sample outside every handler (the
loop's own code, the runtime, the kernel, an unknown address) stays in the
denominator under its own row, and a handler no sample fell in is listed with
zero. Two handlers whose code the compiler placed at one address stay one
ambiguous row; nothing is split by name.
"""
import argparse
import collections
import importlib.util
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
INTERP = os.path.join(HERE, "..", "compiler-compare", "interp.py")
SAMPLE_HZ = 20000
ARM_SYMBOL = re.compile(r"wf_run(?:\.body)?\.arm\.(\d+)")
# `perf script -F period,ip,sym,symoff,dso`: period, address, symbol+offset, (dso)
SCRIPT_RECORD = re.compile(r"\s*(\d+)\s+([0-9a-fA-F]+)\s+(.+?)\s+\((.*)\)\s*")


class Invalid(Exception):
    pass


def load_interp():
    spec = importlib.util.spec_from_file_location("interp", INTERP)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def source_arms(text):
    """The arm names of `fn run(`'s match, in source order."""
    start = text.find("\nfn run(")
    if start < 0:
        raise Invalid("the generated source has no `fn run(`")
    end = text.find("\nfn ", start + 1)
    arms = re.findall(r"^      (\w+)\([^\n]*\) => \{", text[start:end if end > 0 else len(text)], re.M)
    if not arms or len(set(arms)) != len(arms):
        raise Invalid(f"the arms of `run` are missing or repeated ({len(arms)} found)")
    return arms


def parse_nm(text):
    """{symbol: (address, size)} of the sized text symbols of
    `nm -S --defined-only --format=posix`."""
    symbols = {}
    for line in text.splitlines():
        fields = line.split()
        if len(fields) < 4 or fields[1] not in ("t", "T", "w", "W"):
            continue
        symbols[fields[0]] = (int(fields[2], 16), int(fields[3], 16))
    return symbols


def handler_map(symbols, arms):
    """{symbol: handler name} for the arm bodies among SYMBOLS. Every arm of
    the source must have a body symbol and no symbol may name an arm past the
    source's: another count means the numbering is not the source order."""
    found = {}
    for name in symbols:
        match = ARM_SYMBOL.fullmatch(name)
        if match:
            found[name] = int(match.group(1))
    indices = set(found.values())
    if indices != set(range(len(arms))):
        raise Invalid(f"the binary has arm bodies {len(indices)} of {len(arms)} source arms "
                      f"(highest {max(indices, default=-1)}): the symbol numbers are not the source order")
    return {name: arms[index] for name, index in found.items()}


def address_groups(symbols, handlers):
    """{address: sorted handler names} of the handler bodies at each address."""
    groups = collections.defaultdict(set)
    for name, handler in handlers.items():
        groups[symbols[name][0]].add(handler)
    return {address: sorted(names) for address, names in groups.items()}


def count_samples(script, binary, symbols, groups):
    """({(kind, label): [samples, period]}, total samples) of a `perf script`
    text. A sample in a handler's code is counted under the handler (several
    names when one address holds several handlers); the others are counted by
    what they are: another function of the binary, another object, unknown."""
    counts = collections.defaultdict(lambda: [0, 0])
    total = 0
    for line in script.splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        match = SCRIPT_RECORD.fullmatch(line)
        if not match:
            raise Invalid(f"unrecognized perf script record: {line!r}")
        period, _, symbol, dso = match.groups()
        name = re.sub(r"\+0x[0-9a-fA-F]+$", "", symbol)
        if name in ("[unknown]", "unknown"):
            key = ("unknown", "[unknown]")
        elif os.path.basename(dso) == binary and name in symbols and symbols[name][0] in groups:
            names = groups[symbols[name][0]]
            key = ("handler" if len(names) == 1 else "ambiguous", " | ".join(names))
        elif os.path.basename(dso) == binary:
            key = ("outside handlers", name)
        else:
            key = ("other object", f"{os.path.basename(dso)}: {name}")
        counts[key][0] += 1
        counts[key][1] += int(period)
        total += 1
    if not total:
        raise Invalid("no samples")
    return counts, total


def share_rows(per_arm, groups_by_arm):
    """[(kind, label, {arm: (samples, percent)})] over every arm's rows plus
    each arm's handlers with zero samples, ordered by the largest share."""
    totals = {arm: total for arm, (_, total) in per_arm.items()}
    keys = set()
    for arm, (counts, _) in per_arm.items():
        keys.update(counts)
        for names in groups_by_arm[arm].values():
            keys.add(("handler" if len(names) == 1 else "ambiguous", " | ".join(names)))
    rows = []
    for key in keys:
        shares = {}
        for arm, (counts, _) in per_arm.items():
            samples = counts[key][0] if key in counts else 0
            shares[arm] = (samples, 100.0 * samples / totals[arm])
        rows.append((key[0], key[1], shares))
    rows.sort(key=lambda row: (-max(share[1] for share in row[2].values()), row[0], row[1]))
    return rows


def format_rows(rows, totals, out):
    arms = sorted(totals)
    print("Share of all cpu-clock samples at %d Hz, pinned; every sample is in the denominator." % SAMPLE_HZ, file=out)
    print("handler = an arm of `run`; ambiguous = several handlers at one address; outside handlers = another", file=out)
    print("function of the interpreter; other object = the runtime, libraries and the kernel.", file=out)
    print("all samples: " + ", ".join(f"{arm} {totals[arm]}" for arm in arms), file=out)
    header = f"{'kind':<17} {'row':<40}" + "".join(f" {arm + ' n':>10} {arm + ' %':>10}" for arm in arms)
    if len(arms) == 2:
        header += f" {'diff pp':>9}"
    print(header, file=out)
    for kind, label, shares in rows:
        line = f"{kind:<17} {label[:40]:<40}"
        for arm in arms:
            line += f" {shares[arm][0]:>10} {shares[arm][1]:>10.3f}"
        if len(arms) == 2:
            line += f" {shares[arms[1]][1] - shares[arms[0]][1]:>+9.3f}"
        print(line, file=out)


def write_tsv(path, rows, totals):
    arms = sorted(totals)
    with open(path, "w") as f:
        f.write("\t".join(["kind", "row"] + [x for arm in arms for x in (f"{arm}_samples", f"{arm}_percent")]) + "\n")
        for kind, label, shares in rows:
            f.write("\t".join([kind, label] + [x for arm in arms
                                               for x in (str(shares[arm][0]), f"{shares[arm][1]:.4f}")]) + "\n")


def run_command(command, cwd, output):
    with open(output, "w") as f:
        done = subprocess.run(command, cwd=cwd, stdout=f, stderr=subprocess.PIPE, text=True)
    if done.returncode != 0:
        raise Invalid(f"{' '.join(command)}: exit {done.returncode}: {done.stderr.strip()[-400:]}")
    with open(output) as f:
        return f.read()


def sample(args):
    interp = load_interp()
    out = os.path.abspath(args.out)
    inputs = os.path.join(out, "inputs")
    with open(os.path.join(inputs, "coremark-iterations")) as f:
        iterations = int(f.read())
    per_arm, groups_by_arm = {}, {}
    for arm in ("base", "candidate"):
        binary = os.path.join(out, "images", f"v2h-{arm}")
        with open(os.path.join(inputs, f"v2h-{arm}.wf")) as f:
            arms = source_arms(f.read())
        symbols = parse_nm(run_command(["nm", "-S", "--no-sort", "--defined-only", "--format=posix", binary],
                                       out, os.path.join(out, f"{arm}-nm.txt")))
        handlers = handler_map(symbols, arms)
        groups = address_groups(symbols, handlers)
        data = os.path.join(out, f"{arm}.perf.data")
        stdout = run_command(["taskset", "-c", str(args.cpu), "perf", "record", "-e", "cpu-clock", "-F", str(SAMPLE_HZ),
                              "-o", data, "--", binary, "coremark.wasm", *interp.COREMARK_ARGUMENTS, str(iterations)],
                             inputs, os.path.join(out, f"{arm}-stdout.txt"))
        # The sample is of a run that computed the right answer.
        interp.validate("coremark", stdout, iterations, {})
        script = run_command(["perf", "script", "-i", data, "--no-demangle", "-F", "period,ip,sym,symoff,dso"],
                             out, os.path.join(out, f"{arm}-script.txt"))
        per_arm[arm] = count_samples(script, os.path.basename(binary), symbols, groups)
        groups_by_arm[arm] = groups
    rows = share_rows(per_arm, groups_by_arm)
    totals = {arm: total for arm, (_, total) in per_arm.items()}
    write_tsv(os.path.join(out, "handler-shares.tsv"), rows, totals)
    with open(os.path.join(out, "sample-summary.txt"), "w") as f:
        format_rows(rows, totals, f)
    with open(os.path.join(out, "sample-summary.txt")) as f:
        print(f.read(), end="")
    return 0


# ---------------------------------------------------------------- self-test

SOURCE = """fn helper(x: u32) -> u32 pure {
  x
}

fn run(code: Slots<Op>) -> status {
  loop {
    match op {
      Nop() => {
      }
      I32Add(a, b) => {
      }
      I32Sub(a, b) => {
      }
    }
  }
}

fn main() {
}
"""

NM = """wf_run T 1000 40
wf_run.body.arm.0 t 1100 20
wf_run.body.arm.1 t 1200 20
wf_run.body.arm.2 t 1200 20
helper t 2000 10
data d 3000 8
"""

SCRIPT = """         100     1104 wf_run.body.arm.0+0x4 (/x/v2h-base)
         100     1204 wf_run.body.arm.1+0x4 (/x/v2h-base)
         100     1208 wf_run.body.arm.2+0x8 (/x/v2h-base)
         100     1002 wf_run+0x2 (/x/v2h-base)
         100     2000 helper (/x/v2h-base)
         100     7fff memcpy+0x10 (/lib/libc.so.6)
         100     ffff [unknown] ([kernel.kallsyms])
"""


def self_test():
    failures = []

    def expect(label, got, wanted):
        if got != wanted:
            failures.append(f"{label}: got {got!r}, wanted {wanted!r}")

    def expect_invalid(label, thunk, reason):
        try:
            thunk()
        except Invalid as error:
            if reason not in str(error):
                failures.append(f"{label}: refused for {str(error)!r}, wanted {reason!r}")
            return
        failures.append(f"{label}: accepted")

    arms = source_arms(SOURCE)
    expect("arms in source order", arms, ["Nop", "I32Add", "I32Sub"])
    expect_invalid("no run", lambda: source_arms("fn main() {\n}\n"), "no `fn run(`")
    expect_invalid("repeated arm", lambda: source_arms(SOURCE.replace("I32Sub(a, b)", "I32Add(a, b)")), "repeated")
    symbols = parse_nm(NM)
    expect("data symbols are not text", "data" in symbols, False)
    handlers = handler_map(symbols, arms)
    expect("handler map", sorted(handlers.values()), ["I32Add", "I32Sub", "Nop"])
    expect_invalid("an arm missing from the binary", lambda: handler_map(
        {k: v for k, v in symbols.items() if k != "wf_run.body.arm.2"}, arms), "2 of 3 source arms")
    expect_invalid("an arm past the source's", lambda: handler_map(dict(symbols, **{"wf_run.body.arm.3": (1, 1)}), arms),
                   "4 of 3 source arms")
    groups = address_groups(symbols, handlers)
    expect("aliases at one address stay together", groups[0x1200], ["I32Add", "I32Sub"])
    counts, total = count_samples(SCRIPT, "v2h-base", symbols, groups)
    expect("all samples counted", total, 7)
    expect("a handler", counts[("handler", "Nop")], [1, 100])
    expect("an ambiguous address", counts[("ambiguous", "I32Add | I32Sub")], [2, 200])
    expect("outside handlers", counts[("outside handlers", "wf_run")], [1, 100])
    expect("another function of the binary", counts[("outside handlers", "helper")], [1, 100])
    expect("another object", counts[("other object", "libc.so.6: memcpy")], [1, 100])
    expect("unknown", counts[("unknown", "[unknown]")], [1, 100])
    expect_invalid("a malformed record", lambda: count_samples("not a record\n", "v2h-base", symbols, groups),
                   "unrecognized")
    expect_invalid("no samples", lambda: count_samples("# header\n", "v2h-base", symbols, groups), "no samples")
    rows = share_rows({"base": (counts, total), "candidate": (counts, total)},
                      {"base": groups, "candidate": groups})
    expect("the largest share first", rows[0][1], "I32Add | I32Sub")
    expect("a handler without samples is listed", ("handler", "Nop") in {(k, l) for k, l, _ in rows}, True)
    zero = share_rows({"base": ({}, 1), "candidate": ({}, 1)}, {"base": groups, "candidate": groups})
    expect("zero rows", {r[1]: r[2]["base"] for r in zero}["Nop"], (0, 0.0))
    for failure in failures:
        print("FAIL " + failure)
    print(f"handler_samples self-test: {'FAILED' if failures else 'passed'}")
    return 1 if failures else 0


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    p = commands.add_parser("sample")
    p.add_argument("--out", required=True)
    p.add_argument("--cpu", required=True, type=int)
    commands.add_parser("self-test")
    args = parser.parse_args(argv)
    try:
        return sample(args) if args.command == "sample" else self_test()
    except (Invalid, OSError) as error:
        print(f"handler_samples.py {args.command}: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
