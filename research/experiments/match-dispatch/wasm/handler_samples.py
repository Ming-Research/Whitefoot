#!/usr/bin/env python3
"""Temporary, wired by v2h-handler-samples.yml; remove before ready.

Exclusive physical-symbol attribution; no percentage filtering or guessed
semantic splits for identical-code aliases. Uses native perf output, not a
second profiler. All samples, including unknown/non-handler samples, count.
"""

import argparse
import collections
import csv
import pathlib
import re
import subprocess

from coremark import EXPECTED


WASMI_CANDIDATES = {
    "u64_select_rsss", "global_get_u64_r", "global_set_u64_s",
    "i32_eq_rsi", "i32_eq_rri", "i32_clz_rs", "i32_ctz_rs", "i32_popcnt_rs",
    "i32_sub_rss", "i32_mul_rss", "i32_bitand_rss", "i32_bitor_rss",
    "i32_bitxor_rss", "i32_shr_rss", "u32_shr_rss", "i32_rotl_rss", "i32_rotr_rss",
}
V2H_CANDIDATES = {
    "Select", "GlobalGet", "GlobalSet", "I32Eqz", "I32EqzA", "I32Clz", "I32Ctz",
    "I32Popcnt", "I32Sub", "I32Mul", "I32And", "I32Or", "I32Xor", "I32ShrS",
    "I32ShrU", "I32Rotl", "I32Rotr",
}

def capture(command, path):
    with path.open("w") as output, path.with_name(path.name + ".stderr.txt").open("w") as stderr:
        subprocess.run(command, stdout=output, stderr=stderr, check=True)
    return path.read_text()


def write_rows(path, header, rows):
    with path.open("w") as output:
        writer = csv.writer(output, delimiter="\t", lineterminator="\n")
        writer.writerow(header)
        writer.writerows(rows)


def inventory(binary, engine, out, arm_names):
    raw = capture(["nm", "-S", "--no-sort", "--defined-only", "--format=posix", str(binary)],
                  out / f"{engine}-nm.txt")
    # Identical command/sort order: only the symbol spelling is demangled.
    pretty = capture(["nm", "-S", "--no-sort", "--defined-only", "--format=posix", "-C", str(binary)],
                     out / f"{engine}-nm-demangled.txt")
    raw_lines, pretty_lines = raw.splitlines(), pretty.splitlines()
    if len(raw_lines) != len(pretty_lines):
        raise RuntimeError("raw/demangled nm inventory mismatch")
    symbols, starts = {}, collections.defaultdict(list)
    for line, demangled in zip(raw_lines, pretty_lines):
        fields = line.split()
        if len(fields) < 3 or fields[1] not in {"t", "T", "w", "W"}:
            continue
        name, kind, address = fields[:3]
        size = int(fields[3], 16) if len(fields) > 3 else 0
        address = int(address, 16)
        # Remove the nm metadata suffix; demangled names can contain spaces.
        pretty_name = demangled.rsplit(" ", 3 if len(fields) > 3 else 2)[0]
        handler = ""
        arm = re.fullmatch(r"wf_run(?:\.body)?\.arm\.(\d+)", name)
        if engine == "v2h" and arm:
            handler = arm_names[int(arm.group(1))]
        elif engine == "wasmi" and re.fullmatch(r"wasmi::engine::executor::handler::exec::[a-z0-9_]+", pretty_name):
            handler = pretty_name.rsplit("::", 1)[1]
        symbols[name] = (address, size, pretty_name, handler)
        starts[address].append(name)
    if not any(item[3] for item in symbols.values()):
        raise RuntimeError(f"no {engine} handler symbols; cannot attribute this backend")
    expected = V2H_CANDIDATES if engine == "v2h" else WASMI_CANDIDATES
    missing = expected - {item[3] for item in symbols.values() if item[1]}
    if missing:
        raise RuntimeError(f"expected {engine} candidate bodies missing: {sorted(missing)}")
    write_rows(out / f"{engine}-symbol-map.tsv",
               ["symbol", "address", "size", "demangled", "handler", "same_address_aliases"],
               [[name, hex(item[0]), item[1], item[2], item[3], " | ".join(starts[item[0]])]
                for name, item in symbols.items()])
    return symbols, starts


def collect(args):
    out = args.out.resolve()
    source = (out / "v2h.wf").read_text()
    start = source.index("\nfn run(") + 1
    end = source.index("\nfn ", start)
    arms = re.findall(r"^      (\w+)\([^\n]*\) => \{", source[start:end], re.M)
    if not arms or len(set(arms)) != len(arms) or "Return" not in arms:
        raise RuntimeError("generated run arm map is missing or ambiguous")
    write_rows(out / "v2h-arm-map.tsv", ["arm", "handler"], enumerate(arms))
    results, totals = [], []
    engines = [("v2h", out / "v2h"), ("wasmi", out / "wasmi-source/target/release/wasmi")]
    inventories = {engine: inventory(binary, engine, out, arms) for engine, binary in engines}
    for engine, binary in engines:
        symbols, starts = inventories[engine]
        # Write symbol identities and the protocol BEFORE sampling begins.
        data = out / f"{engine}.data"
        with (out / f"{engine}-stdout.txt").open("w") as stdout, \
                (out / f"{engine}-record-stderr.txt").open("w") as stderr:
            subprocess.run(["taskset", "-c", "2", "perf", "record", "-e", "cpu-clock",
                            "-F", "20000", "-o", str(data), "--", str(binary),
                            "coremark.wasm", "0x0", "0x0", "0x66", str(args.iterations)],
                           cwd=out, stdout=stdout, stderr=stderr, check=True)
        stdout = (out / f"{engine}-stdout.txt").read_text()
        crcs = dict(re.findall(r"\[0\](crc\w+)\s*:\s*(0x[0-9a-f]+)", stdout))
        expected = dict(EXPECTED, crcfinal={2000: "0x4983", 20000: "0x382f"}[args.iterations])
        if any(crcs.get(name) != value for name, value in expected.items()):
            raise RuntimeError(f"{engine}: CoreMark CRC mismatch or incomplete result: {crcs}")
        capture(["perf", "report", "-i", str(data), "--stdio", "--show-nr-samples",
                 "--no-children", "--no-demangle", "--percent-limit", "0", "--sort", "dso,symbol"],
                out / f"{engine}-report.txt")
        script = capture(["perf", "script", "-i", str(data), "--no-demangle",
                          "-F", "period,ip,sym,symoff,dso"], out / f"{engine}-script.txt")
        counts = collections.defaultdict(lambda: [0, 0])
        # Emit zero-sampled rows for every resolvable handler, not just hot ones.
        for address, names in starts.items():
            handlers = sorted({symbols[name][3] for name in names if symbols[name][3] and symbols[name][1]})
            if handlers:
                label = " | ".join(handlers)
                counts[("handler" if len(handlers) == 1 else "ambiguous-handlers", label)]
        total_samples, total_period, unresolved_executor = 0, 0, 0
        for line in script.splitlines():
            if not line.strip() or line.startswith("#"):
                continue
            match = re.fullmatch(r"\s*(\d+)\s+([0-9a-fA-F]+)\s+(.+?)\s+\((.*)\)\s*", line)
            if not match:
                raise RuntimeError(f"unrecognized perf script record: {line!r}")
            period, ip, symbol, dso = match.groups()
            period = int(period)
            name = re.sub(r"\+0x[0-9a-fA-F]+$", "", symbol)
            kind, label = "other", f"{dso}: {symbol}"
            if pathlib.Path(dso).name == binary.name and name in symbols:
                address, size, pretty, handler = symbols[name]
                names = starts[address]
                handlers = sorted({symbols[alias][3] for alias in names if symbols[alias][3] and symbols[alias][1]})
                if handlers:
                    kind = "handler" if len(handlers) == 1 else "ambiguous-handlers"
                    label = " | ".join(handlers)
                elif "wasmi::engine::executor::" in pretty:
                    kind, label = "unresolved-executor", pretty
                    unresolved_executor += 1
            elif name in {"[unknown]", "unknown"}:
                kind = "unknown"
            counts[(kind, label)][0] += 1
            counts[(kind, label)][1] += period
            total_samples += 1
            total_period += period
        if not total_samples or not total_period:
            raise RuntimeError(f"{engine}: no usable samples")
        for (kind, label), (samples, period) in sorted(counts.items()):
            cutoff = ("zero-sampled" if not samples else
                      "below" if samples * 1000 < total_samples * 3 else "at-or-above")
            results.append([engine, kind, label, samples, total_samples, 100 * samples / total_samples,
                            period, total_period, 100 * period / total_period, cutoff])
        totals.append([engine, total_samples, total_period, unresolved_executor])
    write_rows(out / "handler-shares.tsv",
               ["engine", "classification", "handler_or_symbol", "samples", "all_samples", "sample_percent",
                "period_ns", "all_period_ns", "period_percent", "sample_cutoff_0.3_percent"], results)
    write_rows(out / "totals.tsv", ["engine", "all_samples", "all_period_ns", "unresolved_executor_samples"], totals)
    # Missing mappings are disclosed. They are never silently redistributed
    # onto a nearby handler, and do not turn a zero-sampled row into execution.


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=pathlib.Path)
    parser.add_argument("--iterations", type=int, choices=[2000, 20000], default=2000)
    args = parser.parse_args()
    collect(args)
