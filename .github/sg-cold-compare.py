"""Temporary full-build comparison, wired only to sg-cold-attrib.yml.

Removed with that workflow before merge. Stock Python reads the driver's
structured reports and GNU time output; it does not reproduce compiler logic.
Shell has no native JSON decoder or grouped median operation, so this one
helper owns validation and aggregation instead of duplicating them in steps.
The enclosing run-check.pl owns the heavy command group and its timeout.
"""

import argparse
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys


PHASES = ("front_end_ms", "split_ms", "compile_ms", "link_ms")
COUNTERS = ("analyses_reused", "analyses_recorded", "fragments", "objects_compiled", "objects_reused")
SCHEDULE = ((1, "base"), (1, "twin"), (1, "head"), (2, "head"), (2, "twin"), (2, "base"))
TIME_FORMAT = '{"wall_s":%e,"user_s":%U,"system_s":%S,"max_rss_kib":%M,"command_status":%x}'


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            result.update(block)
    return result.hexdigest()


def number(value, name, positive=False):
    if isinstance(value, bool) or not isinstance(value, (float, int)):
        raise ValueError(f"{name}: expected a number, got {value!r}")
    if not math.isfinite(value) or value < 0 or (positive and value == 0):
        raise ValueError(f"{name}: invalid measurement {value!r}")
    return value


def parse_build(texts):
    reports = []
    for text in texts:
        for line in text.splitlines():
            line = line.strip()
            if not line.startswith("{"):
                continue
            try:
                item = json.loads(line)
            except ValueError:
                if line.startswith('{"build"'):
                    raise ValueError("malformed --report build record") from None
                continue
            if isinstance(item, dict) and "build" in item:
                reports.append(item["build"])
    if len(reports) != 1 or not isinstance(reports[0], dict):
        raise ValueError(f"expected exactly one --report build record, found {len(reports)}")
    report = reports[0]
    for key in PHASES:
        number(report.get(key), key, positive=key in ("compile_ms", "link_ms"))
    for key in COUNTERS:
        value = report.get(key)
        if key not in report or (value is None and key not in ("analyses_reused", "analyses_recorded")):
            raise ValueError(f"missing build counter {key}")
        if value is not None and (isinstance(value, bool) or not isinstance(value, int) or value < 0):
            raise ValueError(f"invalid build counter {key}: {value!r}")
    if report["fragments"] < 2:
        raise ValueError("comparison requires a multi-fragment build")
    if report["objects_compiled"] + report["objects_reused"] < report["fragments"]:
        raise ValueError("object counters do not cover all fragments")
    if "module_reused" not in report or (report["module_reused"] is not False and report["module_reused"] is not None):
        raise ValueError("cold entry module was reused or its status is missing")
    return report


def normalized_phases(report, subtracts_split):
    phases = {phase: report[phase] for phase in PHASES}
    if subtracts_split:
        phases["compile_ms"] += report["split_ms"]
    return phases


def parse_time(text):
    records = []
    for line in text.splitlines():
        if line.startswith("{"):
            try:
                records.append(json.loads(line))
            except ValueError:
                raise ValueError("malformed GNU time record") from None
    if len(records) != 1 or not isinstance(records[0], dict):
        raise ValueError(f"expected one GNU time record, found {len(records)}")
    timing = records[0]
    for key in ("wall_s", "user_s", "system_s", "max_rss_kib", "command_status"):
        number(timing.get(key), key, positive=key == "wall_s")
    if timing["command_status"] != 0:
        raise ValueError(f"GNU time recorded command exit {timing['command_status']}")
    return timing


def spread(values):
    median = statistics.median(values)
    return {
        "n": len(values), "min": min(values), "max": max(values), "median": median,
        "range_over_median_percent": 100 * (max(values) - min(values)) / median if median else None,
    }


def noise_agreement(base, twin):
    # Symmetric: neither control may exceed the other by more than 10%.
    ratio = max(base, twin) / min(base, twin)
    return {"max_over_min": ratio, "within_10_percent": ratio <= 1.10}


def summarize(samples, setup_errors=()):
    errors = list(setup_errors)
    if sorted((s["round"], s["variant"]) for s in samples) != sorted(SCHEDULE):
        errors.append("expected exactly two samples each of base, twin and head")
    for sample in samples:
        errors.extend(f"round {sample['round']} {sample['variant']}: {e}" for e in sample["errors"])
    result = {"sample_count": len(samples), "errors": errors, "spreads": {}, "criteria": {}}
    for variant in ("base", "twin", "head"):
        rows = [s for s in samples if s["variant"] == variant and not s["errors"]]
        if rows:
            result["spreads"][variant] = {
                phase: spread([s["phases_ms"][phase] for s in rows]) for phase in PHASES
            }
            result["spreads"][variant]["wall_s"] = spread([s["time"]["wall_s"] for s in rows])
    if errors:
        for criterion in ("F1", "F3"):
            result["criteria"][criterion] = {"verdict": "INVALID", "reason": "missing or invalid measurements; see errors"}
        return result

    controls = {}
    for phase in ("compile_ms", "link_ms"):
        repeated_controls = [s["phases_ms"][phase] for s in samples if s["variant"] in ("base", "twin")]
        controls[phase] = {
            "rounds": {
                str(round_number): noise_agreement(
                    next(s for s in samples if s["round"] == round_number and s["variant"] == "base")["phases_ms"][phase],
                    next(s for s in samples if s["round"] == round_number and s["variant"] == "twin")["phases_ms"][phase],
                ) for round_number in (1, 2)
            },
            "medians": noise_agreement(
                result["spreads"]["base"][phase]["median"], result["spreads"]["twin"][phase]["median"]
            ),
            "all_control_repeats": noise_agreement(min(repeated_controls), max(repeated_controls)),
        }
    result["noise_controls"] = controls
    ratios = {
        phase: {
            control: result["spreads"]["head"][phase]["median"] / result["spreads"][control][phase]["median"]
            for control in ("base", "twin")
        } for phase in ("compile_ms", "link_ms")
    }
    result["head_over_control_median"] = ratios

    def quiet(phase):
        control = controls[phase]
        return (control["medians"]["within_10_percent"] and control["all_control_repeats"]["within_10_percent"]
                and all(r["within_10_percent"] for r in control["rounds"].values()))

    f1_fast = all(ratio <= 0.50 for ratio in ratios["compile_ms"].values())
    result["criteria"]["F1"] = {
        "verdict": "INCONCLUSIVE" if not quiet("compile_ms") else "PASS" if f1_fast else "FAIL",
        "phase": "compile_ms, including runtime objects (full-build implementation screen)",
        "threshold": "head median <= 50% of both base and twin medians",
        "threshold_met": f1_fast,
        "control_qualified": quiet("compile_ms"),
        "reason": "control spread exceeds 10%" if not quiet("compile_ms") else "threshold comparison completed",
    }
    hashes = {variant: [s["executable_sha256"] for s in samples if s["variant"] == variant] for variant in ("base", "twin", "head")}
    control_bytes_equal = len(set(hashes["base"] + hashes["twin"])) == 1
    all_bytes_equal = len({s["executable_sha256"] for s in samples}) == 1
    f3_fast = all(ratio <= 0.85 for ratio in ratios["link_ms"].values())
    f3_qualified = quiet("link_ms") and control_bytes_equal
    result["criteria"]["F3"] = {
        "verdict": "INCONCLUSIVE" if not f3_qualified else "PASS" if f3_fast and all_bytes_equal else "FAIL",
        "phase": "link_ms (full-build implementation screen)",
        "threshold": "head median <= 85% of both base and twin medians; all six executable bytes identical",
        "threshold_met": f3_fast,
        "all_executable_bytes_equal": all_bytes_equal,
        "control_executable_bytes_equal": control_bytes_equal,
        "control_qualified": f3_qualified,
        "reason": "base/twin executable bytes differ; deterministic control unavailable" if not control_bytes_equal else
                  "link control spread exceeds 10%" if not quiet("link_ms") else "threshold and byte comparisons completed",
    }
    return result


def markdown(summary):
    lines = [
        "# Cold build: base / twin / head", "",
        "Two rounds per compiler are a small spread screen; no automatic extension is made.",
        "Schedule: base, twin, head; head, twin, base. Every sample starts with a fresh object/ThinLTO/proof cache.",
        "compile_ms includes runtime-object compilation and fragment compilation. The existing replay job isolates fragments for the original F1 criterion.",
        "Legacy compile_ms is corrected by adding split_ms only for revisions whose timer subtracts the earlier split; raw reports are retained.",
        "Spread = (maximum - minimum) / median × 100%; base/twin noise qualification requires max/min <= 1.10 in each round, for medians and over all four control repeats.", "",
        "| Compiler | Phase | n | Minimum | Median | Maximum | Spread % |",
        "|---|---|---:|---:|---:|---:|---:|",
    ]
    for variant, phases in summary["spreads"].items():
        for phase, stats in phases.items():
            unit = "s" if phase == "wall_s" else "ms"
            width = stats["range_over_median_percent"]
            lines.append(f"| {variant} | {phase} ({unit}) | {stats['n']} | {stats['min']:.3f} | {stats['median']:.3f} | {stats['max']:.3f} | {width:.2f} |" if width is not None else
                         f"| {variant} | {phase} ({unit}) | {stats['n']} | 0 | 0 | 0 | undefined |")
    for phase, ratios in summary.get("head_over_control_median", {}).items():
        lines.extend(["", f"{phase}: head/base = {ratios['base']:.4f}; head/twin = {ratios['twin']:.4f}."])
    for phase, control in summary.get("noise_controls", {}).items():
        values = "; ".join(f"round {round_number}: {values['max_over_min']:.4f}" for round_number, values in control["rounds"].items())
        lines.append(f"{phase} controls (max/min): {values}; medians: {control['medians']['max_over_min']:.4f}; all four repeats: {control['all_control_repeats']['max_over_min']:.4f}.")
    for name, criterion in summary["criteria"].items():
        lines.extend(["", f"**{name}: {criterion['verdict']}** — {criterion.get('threshold', '')}. {criterion['reason']}."])
        if "all_executable_bytes_equal" in criterion:
            lines.append(f"All six executable bytes identical: {criterion['all_executable_bytes_equal']}; base/twin control identical: {criterion['control_executable_bytes_equal']}.")
    if summary["errors"]:
        lines.extend(["", "Invalid measurements:"] + [f"- {error}" for error in summary["errors"]])
    lines.extend(["", "A valid FAIL is a completed experiment. INVALID means no result; INCONCLUSIVE means the controls do not support attribution.", ""])
    return "\n".join(lines)


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n", encoding="utf-8")


def exact_record(text, pattern, name):
    matches = [match for line in text.splitlines() if (match := re.fullmatch(pattern, line))]
    if len(matches) != 1:
        raise ValueError(f"expected exactly one {name} record, found {len(matches)}")
    return matches[0].groups()


def replay_summary(replay_text, status_text, link_logs, link_text):
    """Judge the original fragment-only and same-input link replay criteria."""
    result = {"criteria": {}}

    def status(label):
        (code,) = exact_record(status_text, re.escape(label) + r" exit (-?\d+)", label + " status")
        if int(code) != 0:
            raise ValueError(f"{label} exited {code}")

    try:
        fragments = {}
        for mode in ("serial", "p4", "serial-again"):
            wall, objects, code = exact_record(replay_text, rf"REPLAY {mode} wall_ms=(\S+) objects=(\S+) exit=(-?\d+)", "REPLAY " + mode)
            status("REPLAY " + mode)
            if int(code) != 0:
                raise ValueError(f"REPLAY {mode} timing recorded exit {code}")
            count = int(objects)
            if count <= 0:
                raise ValueError(f"REPLAY {mode} has no compiled objects")
            fragments[mode] = {"wall_ms": number(float(wall), mode + " wall_ms", positive=True), "objects": count}
        if len({row["objects"] for row in fragments.values()}) != 1:
            raise ValueError("fragment replay object counts differ")
        noise = noise_agreement(fragments["serial"]["wall_ms"], fragments["serial-again"]["wall_ms"])
        ratios = {control: fragments["p4"]["wall_ms"] / fragments[control]["wall_ms"] for control in ("serial", "serial-again")}
        fast = all(ratio <= 0.50 for ratio in ratios.values())
        result["criteria"]["F1"] = {
            "verdict": "INCONCLUSIVE" if not noise["within_10_percent"] else "PASS" if fast else "FAIL",
            "threshold": "p4 replay wall_ms <= 50% of both serial and serial-again; controls agree within 10%",
            "phase": "captured LLVM fragment compilation only", "measurements": fragments,
            "p4_over_control": ratios, "noise_control": noise, "threshold_met": fast,
            "reason": "serial controls exceed 10% noise" if not noise["within_10_percent"] else "fragment-only replay threshold comparison completed",
        }
    except (ValueError, KeyError) as error:
        result["criteria"]["F1"] = {"verdict": "INVALID", "reason": str(error)}

    try:
        compared = ["jobs-all", "jobs-1", "default-again", "trace"]
        links, comparisons = {}, {}
        for variant in ("default", *compared):
            status("link-" + variant)
            wall, user, system, rss = exact_record(link_logs[variant], rf"LINK {variant} wall=(\S+) user=(\S+) sys=(\S+) maxrss_kb=(\S+)", "LINK " + variant)
            links[variant] = {"wall_s": number(float(wall), variant + " wall", positive=True),
                              "user_s": number(float(user), variant + " user"),
                              "system_s": number(float(system), variant + " sys"),
                              "max_rss_kib": number(float(rss), variant + " rss")}
            if variant != "default":
                (comparison,) = exact_record(link_text, rf"link-{variant} (identical|DIFFERS|comparison error exit -?\d+)", "link-" + variant + " comparison")
                if comparison.startswith("comparison error"):
                    raise ValueError(f"link-{variant}: {comparison}")
                comparisons[variant] = comparison == "identical"
        noise = noise_agreement(links["default"]["wall_s"], links["default-again"]["wall_s"])
        ratios = {control: links["jobs-all"]["wall_s"] / links[control]["wall_s"] for control in ("default", "default-again")}
        fast = all(ratio <= 0.85 for ratio in ratios.values())
        all_equal = all(comparisons.values())
        qualified = noise["within_10_percent"] and comparisons["default-again"]
        result["criteria"]["F3"] = {
            "verdict": "INCONCLUSIVE" if not qualified else "PASS" if fast and all_equal else "FAIL",
            "threshold": "jobs-all link wall <= 85% of both default and default-again; all replay executable bytes identical",
            "phase": "ThinLTO links over the same captured objects", "measurements": links,
            "jobs_all_over_control": ratios, "noise_control": noise, "threshold_met": fast,
            "executable_comparisons_to_default": comparisons, "all_executable_bytes_equal": all_equal,
            "reason": "default repeat executable differs; deterministic control unavailable" if not comparisons["default-again"] else
                      "default link controls exceed 10% noise" if not noise["within_10_percent"] else "same-input link threshold and byte comparisons completed",
        }
    except (ValueError, KeyError) as error:
        result["criteria"]["F3"] = {"verdict": "INVALID", "reason": str(error)}
    return result


def report_replay(directory):
    # Missing files become missing records, so both independent criteria still
    # get a verdict and measurement failures are never read as success.
    def read(name):
        try:
            return (directory / name).read_text(encoding="utf-8")
        except FileNotFoundError:
            return ""

    summary = replay_summary(read("replay.txt"), read("status.txt"),
                             {variant: read("link-" + variant + ".log") for variant in ("default", "jobs-all", "jobs-1", "default-again", "trace")},
                             read("link.txt"))
    lines = ["# Original cold-build replay criteria", "",
             "These verdicts use the isolated captured-fragment and same-input link replays; the separate full-build screen includes runtime objects.", ""]
    for criterion, data in summary["criteria"].items():
        lines.extend([f"**{criterion}: {data['verdict']}** — {data.get('threshold', '')}. {data['reason']}.", "",
                      "```json", json.dumps(data, indent=2, allow_nan=False), "```", ""])
    write_json(directory / "replay-summary.json", summary)
    report = "\n".join(lines)
    (directory / "replay-summary.md").write_text(report, encoding="utf-8")
    print(report, flush=True)
    return 2 if any(data["verdict"] == "INVALID" for data in summary["criteria"].values()) else 0


def write_results(output, samples, summary):
    write_json(output / "samples.json", samples)
    write_json(output / "summary.json", summary)
    (output / "summary.md").write_text(markdown(summary), encoding="utf-8")
    fields = ["round", "variant", "status", *PHASES, "raw_compile_ms", *COUNTERS,
              "wall_s", "user_s", "system_s", "max_rss_kib", "executable_sha256", "errors"]
    lines = ["\t".join(fields)]
    for sample in samples:
        report = sample.get("raw_build", {})
        values = {**report, **sample.get("phases_ms", {}), **sample.get("time", {}), **sample,
                  "raw_compile_ms": report.get("compile_ms", ""), "status": "INVALID" if sample["errors"] else "OK",
                  "errors": "; ".join(sample["errors"])}
        lines.append("\t".join(str(values.get(field, "")) for field in fields))
    (output / "samples.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")


def run_sample(round_number, variant, compiler, renderer, output, subtracts_split):
    directory = output / "samples" / f"r{round_number}-{variant}"
    # exist_ok=False prevents an accidentally warm rerun under the same output directory.
    directory.mkdir(parents=True)
    cache = directory / "cache"
    if cache.exists():
        raise ValueError(f"cold cache already exists: {cache}")
    executable = output / "layout-oracle"
    executable.unlink(missing_ok=True)
    command = [str(compiler), "--cache", str(cache), "--fragments", "function", "--report", "--par",
               "--graph", "modules.wfg", "--entry", "layout_oracle", "-o", str(executable)]
    sample = {"round": round_number, "variant": variant, "command": command, "cwd": str(renderer),
              "cache": str(cache), "compile_timer_adds_split": subtracts_split, "errors": []}
    with (directory / "stdout.log").open("wb") as stdout, (directory / "stderr.log").open("wb") as stderr:
        process = subprocess.run(["/usr/bin/time", "-f", TIME_FORMAT, "-o", str(directory / "time.json"), *command],
                                 cwd=renderer, stdout=stdout, stderr=stderr, check=False)
    sample["returncode"] = process.returncode
    if process.returncode != 0:
        sample["errors"].append(f"build command exited {process.returncode}")
    for key, read in (
        ("raw_build", lambda: parse_build([(directory / "stdout.log").read_text(errors="replace"),
                                          (directory / "stderr.log").read_text(errors="replace")])),
        ("time", lambda: parse_time((directory / "time.json").read_text(errors="replace"))),
    ):
        try:
            sample[key] = read()
        except (OSError, ValueError) as error:
            sample["errors"].append(str(error))
    if "raw_build" in sample:
        sample["phases_ms"] = normalized_phases(sample["raw_build"], subtracts_split)
    if executable.is_file() and process.returncode == 0:
        sample["executable_sha256"] = digest(executable)
    else:
        sample["errors"].append("successful executable missing")
    if sample["errors"]:
        print(f"round {round_number} {variant}: INVALID: {'; '.join(sample['errors'])}", flush=True)
    else:
        print(f"round {round_number} {variant}: phases_ms={sample['phases_ms']} raw_compile_ms={sample['raw_build']['compile_ms']} "
              f"time={sample['time']} counters={ {key: sample['raw_build'][key] for key in COUNTERS} } "
              f"executable_sha256={sample['executable_sha256']}", flush=True)
    return sample


def self_test():
    """Synthetic qualification, CI only: wrong/missing inputs cannot pass."""
    checks = 0

    def expect(condition, message):
        nonlocal checks
        checks += 1
        if not condition:
            raise AssertionError(message)

    def rejects(call, message):
        try:
            call()
        except ValueError:
            expect(True, message)
        else:
            expect(False, message)

    build = {"module_reused": False, "front_end_ms": 200, "split_ms": 5, "compile_ms": 100, "link_ms": 100,
             "analyses_reused": 913, "analyses_recorded": 1000, "fragments": 10, "objects_compiled": 23, "objects_reused": 2}
    timing = {"wall_s": 1, "user_s": 2, "system_s": 0.5, "max_rss_kib": 1000, "command_status": 0}
    samples = []
    for round_number, variant in SCHEDULE:
        phases = {phase: build[phase] for phase in PHASES}
        phases["compile_ms"] = 49 if variant == "head" else 102 if variant == "twin" else 100
        phases["link_ms"] = 80 if variant == "head" else 102 if variant == "twin" else 100
        samples.append({"round": round_number, "variant": variant, "phases_ms": phases,
                        "time": timing, "executable_sha256": "same", "errors": []})
    result = summarize(samples)
    expect(result["criteria"]["F1"]["verdict"] == "PASS" and result["criteria"]["F3"]["verdict"] == "PASS", "passing controls")
    for phase, criterion, threshold in (("compile_ms", "F1", 50), ("link_ms", "F3", 85)):
        changed = copy.deepcopy(samples)
        for sample in changed:
            if sample["variant"] == "head":
                sample["phases_ms"][phase] = threshold
        expect(summarize(changed)["criteria"][criterion]["verdict"] == "PASS", "inclusive threshold")
        for sample in changed:
            if sample["variant"] == "head":
                sample["phases_ms"][phase] += 0.01
        expect(summarize(changed)["criteria"][criterion]["verdict"] == "FAIL", "threshold violation must fail")
        changed = copy.deepcopy(samples)
        changed[1]["phases_ms"][phase] = 130
        expect(summarize(changed)["criteria"][criterion]["verdict"] == "INCONCLUSIVE", "noisy control must not pass")
        changed = copy.deepcopy(samples)
        for sample in changed:
            if sample["round"] == 2 and sample["variant"] in ("base", "twin"):
                sample["phases_ms"][phase] = 130
        expect(summarize(changed)["criteria"][criterion]["verdict"] == "INCONCLUSIVE",
               "matched control drift across rounds must not pass")
    changed = copy.deepcopy(samples)
    changed[2]["executable_sha256"] = "different"
    expect(summarize(changed)["criteria"]["F3"]["verdict"] == "FAIL", "changed head executable must fail")
    changed[1]["executable_sha256"] = "different"
    expect(summarize(changed)["criteria"]["F3"]["verdict"] == "INCONCLUSIVE", "nondeterministic control must not pass")
    changed = copy.deepcopy(samples)
    changed[2]["errors"] = ["build command exited 1"]
    expect(summarize(changed)["criteria"]["F1"]["verdict"] == "INVALID", "failed command must not become measured FAIL")
    expect(summarize(samples[:-1])["criteria"]["F3"]["verdict"] == "INVALID", "missing sample must not pass")
    report_text = json.dumps({"build": build})
    expect(parse_build([report_text])["analyses_reused"] == 913, "legitimate within-build reuse")
    expect(normalized_phases(build, True)["compile_ms"] == 105 and normalized_phases(build, False)["compile_ms"] == 100,
           "legacy timer correction must apply only when selected")
    expect(build["compile_ms"] == 100, "raw timer evidence must remain unchanged")
    rejects(lambda: parse_build([""]), "missing report")
    rejects(lambda: parse_build([report_text, report_text]), "duplicate report")
    for bad in (float("nan"), float("inf"), -1):
        invalid = {**build, "compile_ms": bad}
        rejects(lambda: parse_build([json.dumps({"build": invalid})]), "invalid phase")
    incomplete = {key: value for key, value in build.items() if key != "split_ms"}
    rejects(lambda: parse_build([json.dumps({"build": incomplete})]), "missing phase")
    rejects(lambda: parse_time(json.dumps({**timing, "wall_s": float("nan")})), "invalid elapsed time")
    rejects(lambda: parse_time(json.dumps({**timing, "command_status": 1})), "nonzero command status")
    replay = "REPLAY serial wall_ms=100 objects=10 exit=0\nREPLAY p4 wall_ms=49 objects=10 exit=0\nREPLAY serial-again wall_ms=102 objects=10 exit=0"
    statuses = "\n".join([f"REPLAY {mode} exit 0" for mode in ("serial", "p4", "serial-again")] +
                         [f"link-{variant} exit 0" for variant in ("default", "jobs-all", "jobs-1", "default-again", "trace")])
    link_logs = {variant: f"LINK {variant} wall={wall} user=1 sys=0 maxrss_kb=1000" for variant, wall in
                 (("default", 100), ("jobs-all", 80), ("jobs-1", 200), ("default-again", 102), ("trace", 110))}
    comparisons = "\n".join(f"link-{variant} identical" for variant in ("jobs-all", "jobs-1", "default-again", "trace"))
    replay_result = lambda r=replay, s=statuses, logs=link_logs, c=comparisons: replay_summary(r, s, logs, c)["criteria"]
    expect(all(row["verdict"] == "PASS" for row in replay_result().values()), "passing exact replay criteria")
    expect(replay_result(r=replay.replace("wall_ms=49", "wall_ms=51"))["F1"]["verdict"] == "FAIL", "fragment replay threshold violation")
    expect(replay_result(r=replay.replace("wall_ms=102", "wall_ms=130"))["F1"]["verdict"] == "INCONCLUSIVE", "noisy replay control")
    changed_logs = {**link_logs, "jobs-all": link_logs["jobs-all"].replace("wall=80", "wall=86")}
    expect(replay_result(logs=changed_logs)["F3"]["verdict"] == "FAIL", "link replay threshold violation")
    expect(replay_result(r="")["F1"]["verdict"] == "INVALID", "missing fragment replay")
    expect(replay_result(r=replay.replace("wall_ms=49", "wall_ms=nan"))["F1"]["verdict"] == "INVALID", "nonfinite replay wall time")
    expect(replay_result(r=replay.replace("p4 wall_ms=49 objects=10", "p4 wall_ms=49 objects=9"))["F1"]["verdict"] == "INVALID", "incomplete replay object count")
    expect(replay_result(s=statuses.replace("REPLAY p4 exit 0", "REPLAY p4 exit 1"))["F1"]["verdict"] == "INVALID", "failed replay status")
    expect(replay_result(s=statuses.replace("link-jobs-all exit 0", "link-jobs-all exit 1"))["F3"]["verdict"] == "INVALID", "failed link status")
    expect(replay_result(c=comparisons.replace("link-jobs-all identical", "link-jobs-all DIFFERS"))["F3"]["verdict"] == "FAIL", "different optimized executable")
    expect(replay_result(c=comparisons.replace("link-default-again identical", "link-default-again DIFFERS"))["F3"]["verdict"] == "INCONCLUSIVE", "unstable replay byte control")
    expect(replay_result(c=comparisons.replace("link-jobs-all identical", "link-jobs-all comparison error exit 2"))["F3"]["verdict"] == "INVALID", "comparison IO error")
    expect(replay_result(c=comparisons.replace("link-trace identical", ""))["F3"]["verdict"] == "INVALID", "missing trace byte comparison")
    print(f"Synthetic comparison qualification: {checks} controls passed (no builds or measurements).")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--replay-dir", type=Path)
    for name in ("renderer", "base", "twin", "head", "output-dir", "base-revision", "head-revision"):
        parser.add_argument("--" + name)
    parser.add_argument("--base-subtracts-split", action="store_true")
    parser.add_argument("--head-subtracts-split", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.replay_dir is not None:
        return report_replay(args.replay_dir)
    for name in ("renderer", "base", "twin", "head", "output_dir", "base_revision", "head_revision"):
        if getattr(args, name) is None:
            parser.error(f"--{name.replace('_', '-')} is required")
    output = Path(args.output_dir).resolve()
    output.mkdir(parents=True, exist_ok=True)
    for name in ("manifest.json", "samples.json", "samples.tsv", "summary.json", "summary.md", "samples", "layout-oracle"):
        if (output / name).exists():
            parser.error(f"comparison output already exists: {output / name}; use a fresh result directory")
    renderer = Path(args.renderer).resolve()
    compilers = {variant: Path(getattr(args, variant)).resolve() for variant in ("base", "twin", "head")}
    processors = len(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else os.cpu_count()
    source_files = [path for path in renderer.rglob("*") if path.is_file() and path.suffix in (".wf", ".wfm", ".wfg")]
    manifest = {
        "base_revision": args.base_revision, "head_revision": args.head_revision,
        "compiler_binaries": {name: {"path": str(path), "sha256": digest(path)} for name, path in compilers.items()},
        "renderer": str(renderer), "sources": {str(path.relative_to(renderer)): digest(path) for path in sorted(source_files)},
        "platform": platform.platform(), "python": sys.version, "available_processors": processors,
        "schedule": [{"round": r, "variant": v} for r, v in SCHEDULE],
        "legacy_compile_timer_subtracts_split": {"base": args.base_subtracts_split, "twin": args.base_subtracts_split, "head": args.head_subtracts_split},
        "command_settings": ["--fragments", "function", "--report", "--par", "--graph", "modules.wfg", "--entry", "layout_oracle"],
        "cache_condition": "fresh unique cache directory per sample; kernel filesystem cache is not flushed",
        "phase_scope": "compile_ms includes runtime and fragment objects; link_ms includes ThinLTO",
    }
    write_json(output / "manifest.json", manifest)
    setup_errors = []
    if processors != 4:
        setup_errors.append(f"expected a hosted 4-processor runner, found {processors} available processors")
    if manifest["compiler_binaries"]["base"]["sha256"] != manifest["compiler_binaries"]["twin"]["sha256"]:
        setup_errors.append("twin is not an identical copy of the base compiler binary")
    if not (renderer / "modules.wfg").is_file():
        setup_errors.append("renderer modules.wfg is missing")
    samples = []
    if not setup_errors:
        for round_number, variant in SCHEDULE:
            try:
                sample = run_sample(round_number, variant, compilers[variant], renderer, output,
                                    args.head_subtracts_split if variant == "head" else args.base_subtracts_split)
            except (OSError, ValueError) as error:
                sample = {"round": round_number, "variant": variant, "errors": [str(error)]}
                print(f"round {round_number} {variant}: INVALID: {error}", flush=True)
            samples.append(sample)
            if len(samples) == 3:
                print("Round 1 pilot (one observation each; range is not yet established):", flush=True)
                for row in samples:
                    print(f"  {row['variant']}: {row.get('phases_ms', {})}; {row.get('time', {})}; errors={row['errors']}", flush=True)
            write_results(output, samples, summarize(samples, setup_errors))
    summary = summarize(samples, setup_errors)
    write_results(output, samples, summary)
    print(markdown(summary), flush=True)
    return 2 if summary["errors"] else 0


if __name__ == "__main__":
    sys.exit(main())
