#!/usr/bin/env python3
"""C3's manual CI timing driver; remove with the temporary C3 workflow jobs.

GNU time supplies wall/CPU/RSS. Python only generates the shared inputs, rotates
process order and reduces observations; native time alone cannot coordinate
that panel and its identity/status records. Never used by the correctness gate.
"""

import hashlib
import math
import os
from pathlib import Path
import statistics
import subprocess
import sys


VARIANTS = ("base", "twin", "head")
WIDTHS = (40, 80, 160, 320, 640)


def summarize(rows, path, rounds):
    lines = ["C3 checking time (seconds; RSS in KiB)",
             "All observations retained, including the first/cold round.",
             "input variant n wall-median[min,max] cpu-median[min,max] peak-rss status"]
    medians = {}
    for name in dict.fromkeys(row[0] for row in rows):
        panel = [row for row in rows if row[0] == name]
        complete = all(
            len([row for row in panel if row[3] == variant]) == rounds
            for variant in VARIANTS
        ) and all(row[4] == 0 and row[5] is not None for row in panel)
        for variant in VARIANTS:
            selected = [row for row in rows if row[0] == name and row[3] == variant]
            measured = [row for row in selected if row[4] == 0 and row[5] is not None]
            if not measured:
                lines.append(f"{name} {variant}: no successful measurement; statuses={[row[4] for row in selected]}")
                continue
            wall = [row[5] for row in measured]
            cpu = [row[6] + row[7] for row in measured]
            if complete:
                medians[name, variant] = statistics.median(wall)
            lines.append(
                f"{name} {variant} {len(measured)} "
                f"{statistics.median(wall):.3f}[{min(wall):.3f},{max(wall):.3f}] "
                f"{statistics.median(cpu):.3f}[{min(cpu):.3f},{max(cpu):.3f}] "
                f"{max(row[8] for row in measured)} {[row[4] for row in selected]}"
            )
        if not complete:
            lines.append(f"{name}: incomplete or failed panel; comparative ratios withheld")
            continue
        base = medians.get((name, "base"), 0)
        if base:
            for variant in ("twin", "head"):
                if (name, variant) in medians:
                    lines.append(f"{name} {variant}/base median ratio: {medians[name, variant] / base:.3f}")
        else:
            lines.append(f"{name}: timer resolution cannot resolve a base ratio")
    for low, high in zip(WIDTHS, WIDTHS[1:]):
        earlier = medians.get((f"plain-{low}", "head"), 0)
        later = medians.get((f"plain-{high}", "head"))
        if earlier and later is not None:
            lines.append(f"head {high}/{low} median ratio: {later / earlier:.3f} (prior criterion <= 2.5)")
    lines.append("No performance ratio selects acceptance. Inspect twin spread before attributing a change.")
    path.write_text("\n".join(lines) + "\n")
    print(path.read_text(), flush=True)


def main():
    work = Path(sys.argv[1]).resolve()
    root = Path(__file__).resolve().parents[3]
    rounds = int(os.environ["C3_ROUNDS"])
    maximum = int(os.environ["C3_MAX_ARMS"])
    if rounds < 3 or rounds % 3 or maximum not in WIDTHS:
        raise ValueError("rounds must be a positive multiple of 3; width must be in the fixed series")
    inputs = work / "inputs"
    results = work / "results"
    inputs.mkdir()
    results.mkdir()
    scripts = work / "scripts"
    scripts.mkdir()
    generator = Path(__file__).with_name("series-gen.py")
    for script in (Path(__file__), generator, root / ".github/run-check.pl"):
        (scripts / script.name).write_bytes(script.read_bytes())
    names = []
    for width in WIDTHS:
        if width <= maximum:
            name = f"plain-{width}"
            with (inputs / f"{name}.wf").open("w") as output:
                subprocess.run([sys.executable, str(generator), str(width)], stdout=output, check=True)
            names.append(name)
    if os.environ["C3_NATURAL"] == "true":
        wasm = root / "research/experiments/match-dispatch/wasm"
        for name in ("gen.py", "interp_head.wf", "interp_tail.wf"):
            (scripts / name).write_bytes((wasm / name).read_bytes())
        subprocess.run([sys.executable, str(wasm / "gen.py"), str(inputs / "nat.wf")], check=True)
        names.append("nat")
    with (work / "input-sha256.txt").open("w") as identities:
        for path in sorted(inputs.iterdir()) + sorted(scripts.iterdir()):
            identities.write(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(work)}\n")
    environment = os.environ.copy()
    for key in ("WHITEFOOT_CHECK_WORK", "WHITEFOOT_TEST_TIMINGS"):
        environment.pop(key, None)
    (results / "plan.txt").write_text(
        f"cwd={root}\nrounds={rounds}\ninputs={names}\n"
        "orders=base,twin,head; twin,head,base; head,base,twin\n"
        "per-process timeout=120s; no --cache; counters unset\n"
    )
    rows = []
    failed = False
    with (results / "raw.tsv").open("w", buffering=1) as raw:
        raw.write("input\tround\tposition\tvariant\tstatus\twall_s\tuser_s\tsystem_s\tmaxrss_kib\n")
        for name in names:
            for round_index in range(rounds):
                rotation = round_index % 3
                order = VARIANTS[rotation:] + VARIANTS[:rotation]
                for position, variant in enumerate(order):
                    stem = results / f"{name}-{round_index}-{variant}"
                    with stem.with_suffix(".stdout").open("w") as out, stem.with_suffix(".stderr").open("w") as err:
                        completed = subprocess.run(
                            ["/usr/bin/timeout", "--kill-after=5s", "120s", "/usr/bin/time",
                             "-f", "%e\t%U\t%S\t%M", "-o", str(stem.with_suffix(".time")),
                             str(work / "bin" / variant), "--check", str(inputs / f"{name}.wf")],
                            cwd=root, env=environment, stdout=out, stderr=err,
                        )
                    # A nonzero child status is evidence of a failed check,
                    # never an accepted source verdict or a successful timing.
                    times = (None, None, None, None)
                    time_file = stem.with_suffix(".time")
                    if time_file.exists():
                        last = time_file.read_text().splitlines()
                        if last:
                            fields = last[-1].split("\t")
                            if len(fields) == 4:
                                try:
                                    parsed = (*map(float, fields[:3]), int(fields[3]))
                                    if all(math.isfinite(value) and value >= 0 for value in parsed):
                                        times = parsed
                                except (ValueError, OverflowError):
                                    pass
                    row = (name, round_index, position, variant, completed.returncode, *times)
                    rows.append(row)
                    raw.write("\t".join("missing" if item is None else str(item) for item in row) + "\n")
                    failed |= completed.returncode != 0 or times[0] is None
                    summarize(rows, results / "summary.txt", rounds)
                # Finish the paired round even on failure, then stop scaling.
                if failed:
                    return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
