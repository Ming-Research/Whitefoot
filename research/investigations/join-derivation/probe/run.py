"""Runs the earlier study's experiment compiler on the generated programs,
once with its counter file and once without, and prints one line per
program and configuration: the largest counts over the obligations of the
function `probe`, and the exit status both ways.

usage: run.py <whitefootc> <programs dir> <output dir> <compiler checkout>"""

import csv
import os
import pathlib
import subprocess
import sys
import time


def check(compiler, source, cwd, counters=None):
    env = dict(os.environ)
    env.pop("WHITEFOOT_E1", None)
    if counters is not None:
        env["WHITEFOOT_E1"] = str(counters)
    start = time.monotonic()
    try:
        done = subprocess.run(
            [str(compiler), "--check", str(source)],
            cwd=cwd, env=env, capture_output=True, text=True, timeout=1500,
        )
        status, text = done.returncode, (done.stdout + done.stderr).strip()
    except subprocess.TimeoutExpired:
        status, text = "timeout", ""
    return status, time.monotonic() - start, text


def main():
    compiler, programs, out, cwd = (pathlib.Path(arg).resolve() for arg in sys.argv[1:5])
    out.mkdir(parents=True, exist_ok=True)
    print("program\tconfig\tstatus_counters\tstatus_plain\trows\tproved\tmax_n1\tmax_product"
          "\tmax_backjump_nodes\tmax_full_nodes\tfull_capped\tsum_judge_ms\tsum_full_ms"
          "\tlast_requirement_shadow\tlast_n1\tlast_backjump_nodes\tlast_full_nodes\tlast_judge_ms\twall_s")
    stop = set()
    for source in sorted(programs.glob("*.wf")):
        family = source.stem.split("-")[0]
        if family in stop:
            continue
        counters = out / (source.stem + ".tsv")
        if counters.exists():
            counters.unlink()
        plain, _, plain_text = check(compiler, source, cwd)
        status, wall, text = check(compiler, source, cwd, counters)
        if status != 0 or plain != 0:
            print(f"# {source.name}: status {status} with counters, {plain} without")
            for line in (text or plain_text).splitlines()[:12]:
                print("#   " + line)
        if status == "timeout" or wall > 900:
            stop.add(family)
        if not counters.exists():
            continue
        names = ("label config scope prelude function record rule kind site ordinary route shadow queries refuted "
                 "root_refuted atoms instances n1 n2 n3 max_cases product backjump_nodes max_depth full_nodes "
                 "full_capped form_ns judge_ns full_ns notes walk_ns batch").split()
        rows = [dict(zip(names, line)) for line in csv.reader(counters.open(), delimiter="\t")]
        rows = [row for row in rows if row.get("function", "").endswith("probe")]
        for config in ("baseline", "remedy", "remedy+join"):
            mine = [row for row in rows if row["config"] == config]
            if not mine:
                continue
            calls = [row for row in mine if "at_most" in row["site"] + row["kind"] + row["rule"] + row["record"]
                     or "below" in row["site"] + row["kind"] + row["rule"] + row["record"]]
            last = max(mine, key=lambda row: (int(row["full_nodes"]), int(row["backjump_nodes"])))
            if calls:
                last = calls[-1]
            print("\t".join(str(item) for item in (
                source.stem, config, status, plain, len(mine),
                sum(row["shadow"] == "proved" for row in mine),
                max(int(row["n1"]) for row in mine),
                max(int(row["product"]) for row in mine),
                max(int(row["backjump_nodes"]) for row in mine),
                max(int(row["full_nodes"]) for row in mine),
                max(int(row["full_capped"]) for row in mine),
                round(sum(int(row["judge_ns"]) for row in mine) / 1e6, 3),
                round(sum(int(row["full_ns"]) for row in mine) / 1e6, 3),
                last["shadow"], last["n1"], last["backjump_nodes"], last["full_nodes"],
                round(int(last["judge_ns"]) / 1e6, 3), round(wall, 2),
            )))
        sys.stdout.flush()


if __name__ == "__main__":
    main()
