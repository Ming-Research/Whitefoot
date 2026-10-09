#!/usr/bin/env python3
"""Strict paired process summary. Performance verdicts do not fail this script."""
import argparse
import csv
import json
import math
import re
from collections import defaultdict
from pathlib import Path
from statistics import median

RAW_FIELDS = ("cell", "build", "workers", "round", "wall_ns", "cpu_ns", "exit_status", "checksum")
# A generated cell prints its checksum in decimal; a real program is
# identified by its exit status and the hash of its standard output (run.py).
CHECKSUM = re.compile(r"[0-9]+|exit:[0-9]+;stdout-sha256:[0-9a-f]{64}")


def read_tsv(path):
    with Path(path).open(newline="") as stream:
        return list(csv.DictReader(stream, delimiter="\t"))


def summarize(rows, cells, widths, rounds, epsilon, startup_ns, default_workers=None):
    if not math.isfinite(epsilon) or epsilon < 0 or startup_ns < 0 or rounds < 3:
        raise ValueError("invalid allowance or fewer than three rounds")
    expected = {(c, b, w, r) for c in cells for b in ("seq", "par", "twin")
                for w in widths for r in range(rounds)}
    samples = {}
    checksums = defaultdict(set)
    for row in rows:
        # csv.DictReader files an extra field under None and leaves a missing
        # one None, so the exact field set rejects both.
        if set(row) != set(RAW_FIELDS) or any(value is None for value in row.values()):
            raise ValueError(f"malformed raw row: {row}")
        key = (row["cell"], row["build"], row["workers"], int(row["round"]))
        if key not in expected or key in samples:
            raise ValueError(f"unexpected or duplicate sample: {key}")
        wall, cpu, status = (int(row[k]) for k in ("wall_ns", "cpu_ns", "exit_status"))
        if wall <= 0 or cpu < 0 or status != int(cells[key[0]].get("expected_exit", 0)):
            raise ValueError(f"failed or malformed process: {key}")
        if not CHECKSUM.fullmatch(row["checksum"]):
            raise ValueError(f"missing or malformed checksum: {key}")
        samples[key] = (wall, cpu)
        checksums[key[0]].add(row["checksum"])
    if samples.keys() != expected:
        raise ValueError(f"incomplete matrix: {len(expected - samples.keys())} missing rows")
    if any(len(values) != 1 for values in checksums.values()):
        raise ValueError("checksum mismatch")
    result = []
    for cell, metadata in cells.items():
        for width in widths:
            seq = [samples[cell, "seq", width, r][0] for r in range(rounds)]
            par = [samples[cell, "par", width, r][0] for r in range(rounds)]
            twin = [samples[cell, "twin", width, r][0] for r in range(rounds)]
            ratio = median(p / s for p, s in zip(par, seq))
            # Conservative maximum paired identical-image discrepancy, in
            # T_seq units; the median would hide a noisy individual round.
            spread = max(abs(t - p) / s for t, p, s in zip(twin, par, seq))
            allowance = median(epsilon + startup_ns / s for s in seq)
            excess = median((p - (1 + epsilon) * s - startup_ns) / s
                            for p, s in zip(par, seq))
            if width == "1" or (width == "default" and default_workers == 1):
                verdict, reason = "inconclusive", "W1 diagnostic; H1 concerns W>=2"
            elif spread > allowance:
                verdict, reason = "inconclusive", "twin spread exceeds allowance"
            elif excess <= 0:
                verdict, reason = "pass", "within allowance"
            elif excess > spread:
                verdict, reason = "fail", "excess exceeds twin spread; rerun required"
            else:
                verdict, reason = "inconclusive", "excess within twin spread"
            cpus = {b: median(samples[cell, b, width, r][1] / samples[cell, b, width, r][0]
                              for r in range(rounds)) for b in ("seq", "par", "twin")}
            result.append(dict(cell=cell, split=metadata["split"], workers=width,
                               ratio=ratio, seq_ns=median(seq), twin_spread=spread,
                               allowance=allowance, verdict=verdict, reason=reason,
                               seq_cpu_wall=cpus["seq"], par_cpu_wall=cpus["par"],
                               twin_cpu_wall=cpus["twin"]))
    return sorted(result, key=lambda row: row["ratio"], reverse=True)


def write_summary(directory):
    directory = Path(directory)
    config = json.loads((directory / "run.json").read_text())
    cells = {r["cell"]: r for r in read_tsv(directory / "selected.tsv")}
    table = summarize(read_tsv(directory / "raw.tsv"), cells, config["workers"],
                      config["rounds"], config["epsilon"], config["startup_ns"], config["default_workers"])
    with (directory / "summary.tsv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, list(table[0]), delimiter="\t")
        writer.writeheader()
        writer.writerows(table)
    # Only the verdict phase measures the held-out cells on the dedicated
    # host; any other phase is exploratory and says so first.
    lines = [] if config["phase"] == "verdict" else [
        f"Exploratory {config['phase']} phase: these labels decide nothing."]
    lines += [f"H1 cell observations: epsilon={config['epsilon']}, d={config['startup_ns']} ns",
             f"Allowance provenance: {config['allowance_source']} (uncalibrated-zero is exploratory)",
             "A fail requires one independent rerun before refutation; W1 is diagnostic.",
             "Process intervals include startup; kernel call intervals are separate.",
             "Worst 20 by median paired T_W/T_seq:",
             "cell split workers ratio twin_spread seq_cpu/wall par_cpu/wall verdict"]
    for row in table[:20]:
        lines.append(f"{row['cell']} {row['split']} {row['workers']} {row['ratio']:.6f} "
                     f"{row['twin_spread']:.6f} {row['seq_cpu_wall']:.3f} "
                     f"{row['par_cpu_wall']:.3f} {row['verdict']}")
    text = "\n".join(lines) + "\n"
    (directory / "summary.txt").write_text(text)
    return text


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("results", type=Path)
    print(write_summary(parser.parse_args().results), end="")
