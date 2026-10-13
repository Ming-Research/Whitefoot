#!/usr/bin/env python3
"""Driver of the call-grain-args experiment: calibrate, build, identity, verify,
size, measure, summarize. Each command is one workflow step; the investigation's
README says what every output means. CI only: this builds and times programs.
Remove with the investigation."""
import argparse
import hashlib
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

import generate
import identity
import stats

HERE = Path(__file__).resolve().parent
EXTENTS = [3, 10, 30, 100, 300, 1000, 10000, 100000, 1000000]
KERNELS = ["mandelbrot", "records", "fir", "quadrature", "stencil"]
FIXTURE_FILES = ["oracle.h", "host-adapter.awk"] + [f"{k}.wf" for k in KERNELS] + \
    [f"{k}_host.ll" for k in KERNELS] + [f"{k}_oracle.c" for k in KERNELS]
PER_ELEMENT_TARGET = 2200
PER_ELEMENT_TOLERANCE = 0.10
PIN = {1: [2], 2: [2, 4], 4: [2, 4, 6, 8]}
SEED = 17
ARMS = {"seq": ("base", ""), "base": ("base", "--par"), "chg": ("cand", "--par")}
SPLIT = re.compile(r"PAR split\s+\S*mix_words")
OMITTED = re.compile(r"omitted offer of \S*mix_words \(static work (\d+) below")


def sh(command, **kwargs):
    capture = kwargs.get("capture_output")
    done = subprocess.run([str(c) for c in command], text=True, **kwargs)
    if done.returncode:
        if capture:
            sys.stderr.write(f"command failed ({done.returncode}): {' '.join(map(str, command))}\n{done.stdout or ''}{done.stderr or ''}")
        raise subprocess.CalledProcessError(done.returncode, command, done.stdout, done.stderr)
    return done


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def load(out, name):
    return json.loads((out / name).read_text())


def save(out, name, value):
    (out / name).write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def roots(a):
    return {"base": (Path(a.base_root), Path(a.base_wfc)), "cand": (Path(a.cand_root), Path(a.cand_wfc))}


def image(out, arm, workload):
    return out / "images" / arm / workload


# ---------------------------------------------------------------- calibrate
def ledger_work(wfc, source, scratch):
    text = identity.compile_llvm(wfc, source, ["--par"], scratch / "calibrate.ll", ledger=True)
    match = OMITTED.search(text)
    if SPLIT.search(text) or not match:
        reason = ("mix_words' own loop is split by main (a permitted loop), so the call offer is not what decides"
                  if SPLIT.search(text) else "main's ledger has no 'omitted offer of mix_words' line")
        print(text)
        raise SystemExit("calibration: " + reason)
    return int(match.group(1))


def cmd_calibrate(a):
    out = Path(a.out)
    gen = out / "gen"
    gen.mkdir(parents=True, exist_ok=True)
    wfc = roots(a)["base"][1]
    points = {}
    for steps in (8, 40):
        (gen / "probe.wf").write_text(generate.source(steps))
        points[steps] = ledger_work(wfc, gen / "probe.wf", gen)
    slope = (points[40] - points[8]) / 32.0
    base = points[8] - slope * 8
    steps = max(1, round((PER_ELEMENT_TARGET * 16 - base) / slope))
    (gen / "pair.wf").write_text(generate.source(steps))
    work = ledger_work(wfc, gen / "pair.wf", gen)
    per_element = work / 16.0
    ok = abs(per_element - PER_ELEMENT_TARGET) <= PER_ELEMENT_TOLERANCE * PER_ELEMENT_TARGET
    save(out, "steps.json", dict(steps=steps, static_work=work, per_element=per_element, probes=points, ok=ok))
    print(f"calibration: steps={steps} static work={work} per element={per_element:.0f} (target {PER_ELEMENT_TARGET}) ok={ok}")
    if not ok:
        raise SystemExit("calibration missed the 2200 +- 10% per-element target; adjust the probe points")


# -------------------------------------------------------------------- build
def cmd_build(a):
    out = Path(a.out)
    steps = load(out, "steps.json")["steps"]
    cand_root = roots(a)["cand"][0]
    fixtures = cand_root / "tests" / "programs" / "compute"
    base_fixtures = roots(a)["base"][0] / "tests" / "programs" / "compute"
    same = all(identity.same_bytes(fixtures / f, base_fixtures / f) for f in FIXTURE_FILES)
    meta = dict(base_rev=sh(["git", "-C", roots(a)["base"][0], "rev-parse", "HEAD"], capture_output=True).stdout.strip(),
                cand_rev=sh(["git", "-C", cand_root, "rev-parse", "HEAD"], capture_output=True).stdout.strip(),
                wfc_sha256={k: sha256(v[1]) for k, v in roots(a).items()}, fixtures_identical=same,
                host_model=next((l.split(":", 1)[1].strip() for l in sh(["lscpu"], capture_output=True).stdout.splitlines()
                                 if l.startswith("Model name")), "unknown"), mode=a.mode)
    for arm, (side, flags) in ARMS.items():
        root, wfc = roots(a)[side]
        sh(["make", "-j4", "-C", HERE, f"BUILD={out / 'images' / arm}", f"ROOT={root}", f"WFC={wfc}",
            f"ARMFLAGS={flags}", f"FIXT={fixtures}", f"GEN={out / 'gen'}", f"STEPS={steps}", "build"]
            + (["reference"] if arm == "seq" else []))
    twins = {}
    for arm in ARMS:
        for workload in ["pair"] + KERNELS:
            twin = image(out, arm + "-twin", workload)
            twin.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(image(out, arm, workload), twin)
            twins[f"{arm}/{workload}"] = identity.same_bytes(image(out, arm, workload), twin)
    meta["twins_identical"] = all(twins.values())
    meta["image_sha256"] = {f"{arm}/{w}": sha256(image(out, arm, w)) for arm in ARMS for w in ["pair"] + KERNELS}
    save(out, "meta.json", meta)
    print("build: images for", ", ".join(ARMS), "and their byte-copy twins; fixtures identical:", same)


# ----------------------------------------------------------------- identity
def cmd_identity(a):
    out = Path(a.out)
    fixtures = roots(a)["cand"][0] / "tests" / "programs" / "compute"
    base_wfc, cand_wfc = roots(a)["base"][1], roots(a)["cand"][1]
    result = dict(llvm=identity.llvm_checks(base_wfc, cand_wfc, KERNELS, fixtures, out / "identity-llvm"))
    raw = lambda arm: out / "images" / arm / "pair.ll.raw"
    result["pair_emission_differs"] = not identity.same_bytes(raw("base"), raw("chg"))
    ledger = lambda arm: (out / "images" / arm / "pair.ll.ledger").read_text()
    result["ledger_base_omits_mix_words"] = bool(OMITTED.search(ledger("base")))
    result["ledger_base_splits_mix_words"] = bool(SPLIT.search(ledger("base")))
    result["ledger_cand_omits_mix_words"] = bool(OMITTED.search(ledger("chg")))
    scratch = out / "identity-llvm"
    identity.compile_llvm(cand_wfc, out / "gen" / "pair.wf", [], scratch / "pair.cand-seq.ll")
    identity.compile_llvm(base_wfc, out / "gen" / "pair.wf", [], scratch / "pair.base-seq.ll")
    result["seq_lowering_unchanged"] = identity.same_bytes(scratch / "pair.cand-seq.ll", scratch / "pair.base-seq.ll")
    clones = {}
    for label in ("base", "chg"):
        clones[label] = {}
        for workload in ["pair"] + KERNELS:
            seq = identity.disassemble(image(out, "seq", workload).with_suffix(".o"))
            par = identity.disassemble(image(out, label, workload).with_suffix(".o"))
            required = ["wf_workload"] if workload == "pair" else []
            pairs, failures = identity.compare_clones(seq, par, required)
            entry = dict(pairs=pairs, failures=failures, compared=sum(v == "identical" for v in pairs.values()))
            diffs = out / "identity-diffs"
            diffs.mkdir(exist_ok=True)
            for name, verdict in pairs.items():
                if verdict == "different":
                    twin = identity.CLONE + name[3:]
                    text = identity.full_diff(seq[name], par.get(twin, par.get(name, [])), "sequential " + name, label + " " + twin)
                    (diffs / f"{label}-{workload}-{name}.diff").write_text(text + "\n")
            if workload == "pair":
                entry["control_overlapped_world_differs"] = "wf_workload" in par and "wf_workload" in seq \
                    and par["wf_workload"] != seq["wf_workload"]
                bad = [n for n, v in pairs.items() if v == "different"]
                if bad:
                    twin = identity.CLONE + bad[0][3:]
                    entry["first_difference"] = identity.first_difference(seq[bad[0]], par.get(twin, par.get(bad[0], [])))
            clones[label][workload] = entry
    result["clones"] = clones
    delta = {}
    for workload in ["pair"] + KERNELS:
        base_par = identity.disassemble(image(out, "base", workload).with_suffix(".o"))
        chg_par = identity.disassemble(image(out, "chg", workload).with_suffix(".o"))
        pairs, failures = identity.compare_objects(base_par, chg_par)
        for name, verdict in pairs.items():
            if verdict == "different":
                (out / "identity-diffs").mkdir(exist_ok=True)
                (out / "identity-diffs" / f"change-vs-main-{workload}-{name}.diff").write_text(
                    identity.full_diff(base_par[name], chg_par[name], "main " + name, "change " + name) + "\n")
        delta[workload] = dict(pairs=pairs, failures=failures, identical=sum(v == "identical" for v in pairs.values()))
    result["change_vs_main"] = delta
    save(out, "identity.json", result)
    for k, v in result["llvm"].items():
        print(f"identity llvm {k}: identical={v['identical']} deterministic={v['deterministic']} control={v['control_detects_difference']}")
    for label, per in clones.items():
        for w, e in per.items():
            print(f"identity clones {label}/{w}: clone vs sequential build: identical={e['compared']} "
                  f"different={sum(v == 'different' for v in e['pairs'].values())} "
                  f"missing={sum(v == 'missing' for v in e['pairs'].values())}")
    for w, e in delta.items():
        print(f"identity change vs main {w}: identical={e['identical']} differing symbols={len(e['failures'])}")


# ------------------------------------------------------------------- verify
def pins(mode, widths):
    if mode == "smoke":
        return {}
    topology = Path("/sys/devices/cpu_core/cpus")
    wanted = {c for w in widths for c in PIN[w]}
    if not topology.exists() or shutil.which("taskset") is None:
        raise SystemExit("pilot and main need the hybrid CPU topology and taskset")
    performance = set()
    for part in topology.read_text().strip().split(","):
        low, _, high = part.partition("-")
        performance.update(range(int(low), int(high or low) + 1))
    siblings = set()
    for cpu in sorted(wanted):
        members = (Path(f"/sys/devices/system/cpu/cpu{cpu}/topology/thread_siblings_list")).read_text().strip()
        if members in siblings:
            raise SystemExit(f"CPU {cpu} shares a core with another selected CPU")
        siblings.add(members)
    if not wanted <= performance:
        raise SystemExit(f"selected CPUs {sorted(wanted)} are not all P-core CPUs {sorted(performance)}")
    return {w: PIN[w] for w in widths}


def pinned(command, cpus):
    return ["taskset", "-c", ",".join(map(str, cpus))] + command if cpus else command


def clean_env(width, extra=None):
    env = {k: v for k, v in os.environ.items() if k not in ("WF_SPLIT_WORK", "WF_SCHED_REPORT") and not k.startswith("WFC_")}
    env["WF_WORKERS"] = str(width)
    env.update({k: str(v) for k, v in (extra or {}).items()})
    return env


def cmd_verify(a):
    out = Path(a.out)
    for arm in ARMS:
        for workload in ["pair"] + KERNELS:
            for width in sorted({1, 4} | set(a.kernel_widths)):
                sh([image(out, arm, workload), "verify"], env=clean_env(width), timeout=600)
    print("verify: every image passes its oracle at the widths measured")


# --------------------------------------------------------------------- size
def measure_once(path, width, env, cpus, arm="x", rnd=0):
    text = sh(pinned([path, "measure", arm, width, rnd], cpus), env=clean_env(width, env), capture_output=True, timeout=900).stdout
    rows = [line.split("\t") for line in text.splitlines()]
    return rows


def cmd_size(a):
    out = Path(a.out)
    extents = [int(x) for x in a.extents]
    cpus = pins(a.mode, [1]).get(1, [])
    sizes = {}
    reference = out / "images" / "seq" / "pair_reference"
    for extent in extents:
        reps = max(1, 400000 // extent) if extent < 10 ** 5 else 1
        history = []
        for _ in range(3):
            warm = max(1, reps // 8)
            rows = measure_once(image(out, "seq", "pair"), 1,
                                dict(WFC_REPS=reps, WFC_WARM_REPS=warm, WFC_EXTENT=extent, WFC_SKIP_CHECK=1, WFC_SAMPLES=1), cpus)
            wall = int(rows[1][5])
            history.append((reps, wall))
            reps = max(1, round(reps * a.target_ms * 1e6 / wall))
        warm = max(1, reps // 8)
        expected = sh([reference, reps, warm, extent, SEED], capture_output=True).stdout.split()
        sizes[str(extent)] = dict(reps=reps, warm_reps=warm, expected=int(expected[0]), expected_warm=int(expected[1]),
                                  sizing=history)
        print(f"size: extent {extent} reps {reps} (sizing {history})")
    save(out, "sizes.json", sizes)


# ------------------------------------------------------------------ measure
def cell_plan(a):
    cells = []
    for extent in [int(x) for x in a.extents]:
        cells.append(dict(cell=f"pair-{extent}", workload="pair", extent=extent, widths=sorted(set(a.pair_widths))))
    if not a.no_kernels:
        for kernel in a.kernels:
            cells.append(dict(cell=kernel, workload=kernel, extent=None, widths=sorted(set(a.kernel_widths))))
    return cells


def run_plan(cell):
    items = [("seq", 1), ("seq-twin", 1)]
    for arm in ("base", "base-twin", "chg", "chg-twin"):
        items += [(arm, w) for w in cell["widths"]]
    return items


def cmd_measure(a):
    out = Path(a.out)
    sizes = load(out, "sizes.json")
    cells = cell_plan(a)
    if a.only:
        wanted = set(json.loads(Path(a.only).read_text()))
        cells = [c for c in cells if c["cell"] in wanted]
    widths = sorted({w for c in cells for w in c["widths"]} | {1})
    cpus = pins(a.mode, widths)
    raw = out / "raw.tsv"
    started = time.time()
    with raw.open("a") as sink, (out / "rounds.tsv").open("a") as loads:
        for rnd in range(a.rounds):
            loads.write(f"{a.attempt}\t{rnd}\t{Path('/proc/loadavg').read_text().split()[0] if Path('/proc/loadavg').exists() else 'na'}\n")
            for cell in [cells[i] for i in order(len(cells), rnd)]:
                plan = run_plan(cell)
                for arm, width in [plan[i] for i in order(len(plan), rnd, rnd // 2)]:
                    env = dict(WFC_SAMPLES=1 if cell["workload"] == "pair" or a.mode == "smoke" else 5)
                    if cell["workload"] == "pair":
                        s = sizes[str(cell["extent"])]
                        env.update(WFC_REPS=s["reps"], WFC_WARM_REPS=s["warm_reps"], WFC_EXTENT=cell["extent"],
                                   WFC_EXPECTED=s["expected"], WFC_EXPECTED_WARM=s["expected_warm"])
                    rows = measure_once(image(out, arm, cell["workload"]), width, env, cpus.get(width, []), arm, rnd)
                    for r in rows:
                        sink.write("\t".join(map(str, [a.attempt, rnd, cell["cell"], r[1], r[2]] + r[4:])) + "\n")
                    sink.flush()
            print(f"measure: attempt {a.attempt} round {rnd + 1}/{a.rounds} done after {time.time() - started:.0f} s", flush=True)


def order(n, rnd, offset=0):
    return stats.balanced_order(range(n), rnd, offset)


# ---------------------------------------------------------------- summarize
def fmt(ci):
    return f"{ci[0]:.3f} [{ci[1]:.3f}, {ci[2]:.3f}]"


def evaluate(table, cell, attempt, num, den, twin, bound, not_slower):
    """twin is (arm, width) pairs (numerator, denominator) of the noise control."""
    ci = table.ci(attempt, cell, num, den)
    twin = table.ci(attempt, cell, twin[0], twin[1])
    state, spread = stats.judge_upper(ci, twin, bound, not_slower)
    return dict(ci=ci, twin=twin, spread=spread, state=state)


def finalize(table, cell, num, den, twin, bound, not_slower):
    """Per attempt readings of one comparison and the combined state."""
    attempts = table.attempts(cell)
    readings = [evaluate(table, cell, t, num, den, twin, bound, not_slower) for t in attempts[:2]]
    state = stats.combine(readings[0]["state"], readings[1]["state"] if len(readings) > 1 else None)
    return readings, state


def one_thread_differences(clones, change_vs_main):
    """Criterion 5 as the owner ruled it: the change may add no difference
    between a sequential clone and the sequential build. A function whose clone
    already differs on main is carried, not judged, when the change leaves that
    clone byte-identical to main's; otherwise it is new. Returns (new, carried)."""
    new, carried = [], []
    for workload, entry in clones["chg"].items():
        base_pairs = clones["base"][workload]["pairs"]
        delta = change_vs_main.get(workload, {}).get("pairs", {})
        for name, verdict in entry["pairs"].items():
            if verdict == "identical":
                continue
            twin = identity.CLONE + name[3:]
            counterpart = twin if twin in delta else name
            if base_pairs.get(name, "identical") != "identical" and verdict != "missing" \
                    and delta.get(counterpart) == "identical":
                carried.append(f"{workload}/{name} ({verdict} on main and in the change, the change's clone identical to main's)")
            else:
                new.append(f"{workload}/{name}: {verdict} in the change, main {base_pairs.get(name, 'absent')}, "
                           f"change vs main clone {delta.get(counterpart, 'absent')}")
        if workload == "pair" and entry["pairs"].get("wf_workload") != "identical":
            new.append(f"pair/wf_workload: required and {entry['pairs'].get('wf_workload', 'absent')}")
    return new, carried


def cmd_summarize(a):
    out = Path(a.out)
    verdict = a.mode == "main"
    table = stats.Table(stats.read_rows(out / "raw.tsv"))
    meta, steps = load(out, "meta.json"), load(out, "steps.json")
    ident = load(out, "identity.json") if (out / "identity.json").exists() else {}
    lines, voids, rerun = [], [], []
    lines.append(f"call-grain-args summary, mode={a.mode} rounds={a.rounds}; base {meta['base_rev']} candidate {meta['cand_rev']}")
    lines.append(f"host: {meta['host_model']}; steps per round {steps['steps']}, static work of mix_words {steps['static_work']} "
                 f"= {steps['per_element']:.0f} units per element")

    # ---- preconditions (void rules)
    pre = [("P1 host is the i9-14900K", "14900K" in meta["host_model"] or a.mode == "smoke"),
           ("P2 the two compilers differ and emit different LLVM for the pair", meta["wfc_sha256"]["base"] != meta["wfc_sha256"]["cand"]
            and ident.get("pair_emission_differs", False)),
           ("P3 main's ledger omits the offer of mix_words and does not split its loop; the change's does not omit it",
            ident.get("ledger_base_omits_mix_words", False) and not ident.get("ledger_base_splits_mix_words", True) and not ident.get("ledger_cand_omits_mix_words", True)),
           ("P4 identity method: deterministic compiles, nonempty output, difference control detected",
            bool(ident.get("llvm")) and all(v["deterministic"] and v["has_code"] and v["control_detects_difference"] for v in ident.get("llvm", {}).values())),
           ("P5 fixtures identical between the two revisions", meta["fixtures_identical"]),
           ("P6 every twin is a byte copy of its arm", meta["twins_identical"]),
           ("P7 calibration within 10% of 2200 units per element", steps["ok"]),
           ("P8 sequential and one-worker rows hand out no work",
            all(g == 0 for (_, _, arm, w, _), g in table.grants.items() if w == 1 or arm.startswith("seq"))),
           ("P9 main rounds even and at least 10", a.mode != "main" or (a.rounds % 2 == 0 and a.rounds >= 10)),
           ("P10 the method finds the pair's overlapped world different from the sequential one (control)",
            bool(ident) and all(ident["clones"][label]["pair"].get("control_overlapped_world_differs", False) for label in ("base", "chg")))]
    lines.append("")
    lines.append("Preconditions (any failure voids every verdict below):")
    for name, ok in pre:
        lines.append(f"  [{'ok' if ok else 'VOID'}] {name}")
        if not ok:
            voids.append(name)

    # ---- per-cell timing table (informational)
    lines.append("")
    lines.append("Cells (median wall seconds of one arm-run; ratios are medians of paired per-round ratios):")
    for cell in table.cells():
        first = table.attempts(cell)[0]
        seq = table.seconds(first, cell, "seq", 1)
        widths = sorted({k[3] for k in table.wall if k[1] == cell and k[2] == "chg"})
        parts = [f"seq {seq:.3f}s"]
        for w in widths:
            ci = table.ci(first, cell, ("chg", w), ("seq", 1))
            bi = table.ci(first, cell, ("base", w), ("seq", 1))
            parts.append(f"W{w}: chg/seq {fmt(ci)} base/seq {fmt(bi)} grants chg {table.mean_grants(first, cell, 'chg', w)} base {table.mean_grants(first, cell, 'base', w)}")
        lines.append(f"  {cell}: " + "; ".join(parts))

    # ---- criterion 1
    cell = "pair-1000000"
    lines.append("")
    c1 = None
    if cell in table.cells():
        readings, state = finalize(table, cell, ("chg", 4), ("chg", 1), (("chg-twin", 4), ("chg", 4)), 0.60, False)
        sec = [table.ci(t, cell, ("chg", 4), ("seq", 1)) for t in table.attempts(cell)[:1]]
        grants = table.mean_grants(table.attempts(cell)[0], cell, "chg", 4)
        c1 = state
        lines.append(f"C1 pair at 1,000,000 elements, change W4/W1 wall, upper end of the 95% interval at most 0.60: {state.upper() if verdict else '(no verdict) ' + state}")
        for t, r in zip(table.attempts(cell), readings):
            lines.append(f"   attempt {t}: ratio {fmt(r['ci'])}, twin spread {r['spread']:.3f} -> {r['state']}")
        lines.append(f"   change W4 / sequential: {fmt(sec[0])}; hand-outs per call of the change at W4: {grants}")
        if state in ("fail", "unresolved", "inconclusive"):
            lines.append("   reading: " + ("offers were handed out, so the call grain is not the cause (rejection result R-C1b)" if grants and grants > 0
                                          else "no work was handed out at W4: the change does not offer these calls (rejection result R-C1a)"))
    else:
        lines.append("C1: cell pair-1000000 not measured in this run")

    # ---- criterion 2
    lines.append("")
    verdict2, detail2 = [], []
    for extent in EXTENTS:
        cell = f"pair-{extent}"
        if cell not in table.cells():
            continue
        readings, state = finalize(table, cell, ("chg", 4), ("seq", 1), (("seq-twin", 1), ("seq", 1)), 1.0, True)
        verdict2.append(state)
        detail2.append(f"   {cell}: " + " | ".join(f"attempt {t}: {fmt(r['ci'])} spread {r['spread']:.3f} {r['state']}"
                                                  for t, r in zip(table.attempts(cell), readings)) + f" => {state}")
        if readings[0]["state"] in ("fail", "unresolved"):
            rerun.append(cell)
    overall2 = "void" if "void" in verdict2 else "fail" if "fail" in verdict2 else "pass" if verdict2 and all(s == "pass" for s in verdict2) else "undecided"
    lines.append(f"C2 pair, every element count, change W4 not slower than the sequential build (95% interval of the ratio not above 1 + twin spread): "
                 f"{overall2.upper() if verdict else '(no verdict) ' + overall2}")
    lines += detail2

    # ---- criterion 3
    lines.append("")
    verdict3, detail3, qualifying = [], [], 0
    for cell in table.cells():
        first = table.attempts(cell)[0]
        for w in sorted({k[3] for k in table.wall if k[1] == cell and k[2] == "base"}):
            ci = table.ci(first, cell, ("base", w), ("seq", 1))
            if ci[2] >= 1.0:
                continue
            qualifying += 1
            readings, state = finalize(table, cell, ("chg", w), ("base", w), (("base-twin", w), ("base", w)), 1.05, False)
            verdict3.append(state)
            detail3.append(f"   {cell} W{w}: main/seq {fmt(ci)}; change/main " + " | ".join(
                f"attempt {t}: {fmt(r['ci'])} spread {r['spread']:.3f} {r['state']}" for t, r in zip(table.attempts(cell), readings)) + f" => {state}")
            if readings[0]["state"] in ("fail", "unresolved"):
                rerun.append(cell)
    few = qualifying < 3
    overall3 = "void" if "void" in verdict3 or few else "fail" if "fail" in verdict3 else "pass" if all(s == "pass" for s in verdict3) else "undecided"
    lines.append(f"C3 cells where main's --par is faster than sequential (upper end below 1): {qualifying}; change within 5% of main "
                 f"(upper end of change/main at most 1.05): {overall3.upper() if verdict else '(no verdict) ' + overall3}"
                 + ("  [fewer than 3 qualifying cells: the set is too thin to decide]" if few else ""))
    lines += detail3

    # ---- criteria 4 and 5
    lines.append("")
    llvm = ident.get("llvm", {})
    c4 = bool(llvm) and all(v["identical"] for v in llvm.values())
    lines.append(f"C4 the five formal kernels' --par LLVM is byte-identical between main and the change: "
                 f"{('PASS' if c4 else 'FAIL') if verdict else '(no verdict) ' + str(c4)}  " +
                 ", ".join(f"{k}={'identical' if v['identical'] else 'DIFFERENT'}" for k, v in llvm.items()))
    clones = ident.get("clones", {})
    new, carried = one_thread_differences(clones, ident.get("change_vs_main", {})) if clones else (["no clone comparison"], [])
    chg_ok = bool(clones) and not new and all(e["compared"] > 0 for e in clones["chg"].values())
    lines.append(f"C5 the change adds no difference between the one-thread path (sequential clones) and the sequential build: "
                 f"{('PASS' if chg_ok else 'FAIL') if verdict else '(no verdict) ' + str(chg_ok)}; sequential lowering unchanged: "
                 f"{ident.get('seq_lowering_unchanged')}")
    for text in new:
        lines.append(f"   new: {text}")
    for text in carried:
        lines.append(f"   already on main, unchanged by the change (not judged): {text}")
    for w, e in ident.get("change_vs_main", {}).items():
        lines.append(f"   change's object vs main's object, {w}: {e['identical']} symbols identical, "
                     f"{len(e['failures'])} not" + (": " + "; ".join(e["failures"][:6]) if e["failures"] else ""))
    for label in ("chg", "base"):
        for w, e in clones.get(label, {}).items():
            if e["failures"]:
                lines.append(f"   {label}/{w}: " + "; ".join(e["failures"][:6]))
                if e.get("first_difference"):
                    lines.append(e["first_difference"])
    t1 = [table.ci(table.attempts(c)[0], c, ("chg", 1), ("seq", 1)) for c in table.cells() if c.startswith("pair-")]
    if t1:
        lines.append("   companion timing, change W1 / sequential, per pair cell: " + ", ".join(fmt(x) for x in t1))
    lines.append("C6 Snowghost's report no longer omits the interning calls and its style output is byte-identical: external, verified by Snowghost.")

    # ---- pilot output and verdict
    lines.append("")
    spreads = []
    for cell in table.cells():
        first = table.attempts(cell)[0]
        if cell.startswith("pair-"):
            spreads.append(stats.twin_spread(table.ci(first, cell, ("seq-twin", 1), ("seq", 1))))
    if spreads:
        lines.append(f"largest seq-twin spread {max(spreads):.3f}; rounds projected for a spread of 1%: {stats.project_rounds(max(spreads), a.rounds)}")
    if voids:
        lines.append("VERDICT: VOID -- " + "; ".join(voids))
    elif not verdict:
        lines.append(f"NO VERDICT: mode {a.mode} decides nothing (wiring" + (", sizing and block count" if a.mode == "pilot" else "") + ").")
    else:
        results = dict(C1=c1, C2=overall2, C3=overall3, C4="pass" if c4 else "fail", C5="pass" if chg_ok else "fail")
        lines.append("VERDICT: " + ", ".join(f"{k} {v}" for k, v in results.items()) + ", C6 external")
    (out / "report.txt").write_text("\n".join(lines) + "\n")
    (out / "rerun.json").write_text(json.dumps(sorted(set(rerun))) + "\n")
    print("\n".join(lines))


def parser():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("command", choices=["calibrate", "build", "identity", "verify", "size", "measure", "summarize"])
    p.add_argument("--out", required=True)
    p.add_argument("--mode", choices=["smoke", "pilot", "main"], required=True)
    p.add_argument("--base-root"), p.add_argument("--cand-root")
    p.add_argument("--base-wfc"), p.add_argument("--cand-wfc")
    p.add_argument("--rounds", type=int, default=2)
    p.add_argument("--attempt", type=int, default=1)
    p.add_argument("--only")
    p.add_argument("--target-ms", type=float, default=1000.0)
    p.add_argument("--extents", nargs="+", default=[str(e) for e in EXTENTS])
    p.add_argument("--pair-widths", nargs="+", type=int, default=[1, 4])
    p.add_argument("--kernel-widths", nargs="+", type=int, default=[1, 2, 4])
    p.add_argument("--no-kernels", action="store_true")
    p.add_argument("--kernels", nargs="+", default=KERNELS, choices=KERNELS)
    return p


COMMANDS = dict(calibrate=cmd_calibrate, build=cmd_build, identity=cmd_identity, verify=cmd_verify,
                size=cmd_size, measure=cmd_measure, summarize=cmd_summarize)

if __name__ == "__main__":
    args = parser().parse_args()
    Path(args.out).mkdir(parents=True, exist_ok=True)
    COMMANDS[args.command](args)
