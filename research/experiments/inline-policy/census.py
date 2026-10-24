#!/usr/bin/env python3
"""Inline-decision census of the inline-policy experiment (README.md beside
this file).

    census.py reduce --name NAME REMARKS [REMARKS.lto.*.yaml ...]
    census.py self-test

REMARKS is the text file clang-arm.sh collects: the inline remarks LLVM
prints for an ordinary per-module compile. Each *.yaml file holds the remarks
of an LTO link, whose decisions are the final ones; when a program has any,
its census is taken from them alone. A decision is one call site LLVM's inliner
priced: inlined, or not inlined with a reason. The reducer prints, for the
call sites whose callee and caller are both Whitefoot definitions (names that
start with "wf"):

- the decisions by outcome and reason, and the thresholds the priced ones met;
- the priced sites by cost over threshold, in bands around 1: the sites near
  the threshold are those a few instructions move across it;
- the near sites themselves, not inlined within 25% above the threshold or
  inlined within 20% below it, most expensive first.
"""

import argparse
import collections
import re
import sys

TEXT = re.compile(
    r"'(?P<callee>[^']+)' (?P<outcome>inlined into|not inlined into) '(?P<caller>[^']+)'"
    r"(?: because (?P<reason>.*?))?\s*(?:with )?\(cost=(?P<cost>-?\d+|always|never)"
    r"(?:, threshold=(?P<threshold>-?\d+))?\)"
)
BANDS = [(None, 0.5), (0.5, 0.8), (0.8, 1.0), (1.0, 1.1), (1.1, 1.25), (1.25, 1.5), (1.5, 2.0), (2.0, None)]


def parse_text(text):
    for line in text.splitlines():
        m = TEXT.search(line)
        if not m:
            continue
        yield {
            "callee": m["callee"],
            "caller": m["caller"],
            "inlined": m["outcome"] == "inlined into",
            "reason": (m["reason"] or "").strip() or ("inlined" if m["outcome"] == "inlined into" else "?"),
            "cost": m["cost"],
            "threshold": m["threshold"],
        }


def unquote(value):
    value = value.strip()
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "'\"":
        value = value[1:-1].replace("''", "'")
    return value


def parse_yaml(text):
    # One record per '--- !Kind' document; only the fields the census reads.
    for document in re.split(r"^--- !", text, flags=re.M)[1:]:
        kind = document.split("\n", 1)[0].strip()
        if kind not in ("Passed", "Missed"):
            continue
        fields = {}
        for key in ("Callee", "Caller", "Cost", "Threshold", "Reason"):
            m = re.search(rf"^\s*- {key}:\s*(.*)$", document, re.M)
            if m:
                fields[key] = unquote(m.group(1))
        strings = "".join(unquote(s) for s in re.findall(r"^\s*- String:\s*(.*)$", document, re.M))
        if "Callee" not in fields or "Caller" not in fields:
            continue
        reason = "inlined"
        if kind == "Missed":
            m = re.search(r"because (.*?)(?: \(cost=|$)", strings)
            reason = fields.get("Reason") or (m.group(1).strip() if m else "?")
        yield {
            "callee": fields["Callee"],
            "caller": fields["Caller"],
            "inlined": kind == "Passed",
            "reason": reason,
            "cost": fields.get("Cost"),
            "threshold": fields.get("Threshold"),
        }


def priced(decision):
    try:
        return int(decision["cost"]), int(decision["threshold"])
    except (TypeError, ValueError):
        return None


def band_of(ratio):
    for low, high in BANDS:
        if (low is None or ratio > low) and (high is None or ratio <= high):
            return (low, high)
    raise AssertionError(ratio)


def band_name(band):
    low, high = band
    if low is None:
        return f"<= {high}"
    if high is None:
        return f"> {low}"
    return f"({low}, {high}]"


def census(decisions):
    own = [d for d in decisions if d["callee"].startswith("wf") and d["caller"].startswith("wf")]
    outcomes = collections.Counter((d["inlined"], d["reason"]) for d in own)
    thresholds = collections.Counter()
    bands = collections.Counter()
    near = []
    for d in own:
        p = priced(d)
        if not p:
            continue
        cost, threshold = p
        thresholds[threshold] += 1
        if threshold <= 0:
            continue
        ratio = cost / threshold
        bands[(band_of(ratio), d["inlined"])] += 1
        if (not d["inlined"] and 1.0 < ratio <= 1.25) or (d["inlined"] and 0.8 < ratio <= 1.0):
            near.append((ratio, d))
    return own, outcomes, thresholds, bands, near


def report(name, decisions, limit=40):
    own, outcomes, thresholds, bands, near = census(decisions)
    lines = [f"== {name}: {len(own)} decisions between Whitefoot definitions ({len(decisions)} in all)"]
    for (inlined, reason), count in sorted(outcomes.items(), key=lambda kv: -kv[1]):
        lines.append(f"  {'inlined' if inlined else 'not inlined'}: {reason}: {count}")
    lines.append("  thresholds met: " + ", ".join(f"{t}x{c}" for t, c in sorted(thresholds.items())))
    lines.append("  cost/threshold band: inlined / not inlined")
    for band in BANDS:
        lines.append(f"    {band_name(band):>12}: {bands[(band, True)]} / {bands[(band, False)]}")
    lines.append(f"  near the threshold: {len(near)} sites (not inlined <= 1.25, inlined > 0.8)")
    for ratio, d in sorted(near, key=lambda item: -item[0])[:limit]:
        outcome = "inlined" if d["inlined"] else "kept   "
        lines.append(f"    {outcome} {ratio:5.2f} cost={d['cost']} threshold={d['threshold']} {d['callee']} <- {d['caller']}")
    return "\n".join(lines)


def reduce(args):
    decisions = []
    yaml_files = [path for path in args.inputs if path.endswith(".yaml")]
    text_files = [path for path in args.inputs if not path.endswith(".yaml")]
    if yaml_files:
        for path in yaml_files:
            with open(path, errors="replace") as f:
                decisions.extend(parse_yaml(f.read()))
    else:
        for path in text_files:
            with open(path, errors="replace") as f:
                decisions.extend(parse_text(f.read()))
    if not decisions:
        print(f"== {args.name}: no inline remarks found in {' '.join(args.inputs)}")
        return 1
    print(report(args.name, decisions))
    return 0


def self_test():
    text = (
        "remark: <unknown>:0:0: 'wf_a' inlined into 'wf_b' with (cost=200, threshold=225) at callsite wf_b:1:0; [-Rpass=inline]\n"
        "remark: <unknown>:0:0: 'wf_c' not inlined into 'wf_b' because too costly to inline (cost=250, threshold=225) [-Rpass-missed=inline]\n"
        "remark: <unknown>:0:0: 'wf_d' not inlined into 'wf_b' because it should never be inlined (cost=never): noinline function attribute [-Rpass-missed=inline]\n"
        "remark: <unknown>:0:0: 'wf_e' inlined into 'wf_b' with (cost=always): always inline attribute at callsite wf_b:2:0; [-Rpass=inline]\n"
        "remark: <unknown>:0:0: 'memcpy' not inlined into 'wf_b' because too costly to inline (cost=900, threshold=225) [-Rpass-missed=inline]\n"
        "remark: <unknown>:0:0: 'wf_f' not inlined into 'wf_b' because too costly to inline (cost=60, threshold=45) [-Rpass-missed=inline]\n"
    )
    decisions = list(parse_text(text))
    own, outcomes, thresholds, bands, near = census(decisions)
    failures = []
    if len(decisions) != 6 or len(own) != 5:
        failures.append(f"text: {len(decisions)} decisions, {len(own)} own")
    if outcomes[(False, "too costly to inline")] != 2:
        failures.append(f"text: outcomes {outcomes}")
    if thresholds != collections.Counter({225: 2, 45: 1}):
        failures.append(f"text: thresholds {thresholds}")
    if sorted(d["callee"] for _, d in near) != ["wf_a", "wf_c"]:
        failures.append(f"text: near {[d['callee'] for _, d in near]}")
    if bands[(band_of(60 / 45), False)] != 1:
        failures.append(f"text: bands {bands}")
    yaml = (
        "--- !Passed\nPass: inline\nName: Inlined\nFunction: wf_b\nArgs:\n"
        "  - String: ''''\n  - Callee: 'wf_a$x'\n  - String: ''' inlined into '''\n  - Caller: wf_b\n"
        "  - String: ''''\n  - String: ' with '\n  - String: '(cost='\n  - Cost: '10'\n"
        "  - String: ', threshold='\n  - Threshold: '225'\n  - String: ')'\n...\n"
        "--- !Missed\nPass: inline\nName: TooCostly\nFunction: wf_b\nArgs:\n"
        "  - String: ''''\n  - Callee: wf_c\n  - String: ''' not inlined into '''\n  - Caller: wf_b\n"
        "  - String: ''' because '\n  - Reason: too costly to inline\n  - String: ' (cost='\n  - Cost: '240'\n"
        "  - String: ', threshold='\n  - Threshold: '225'\n  - String: ')'\n...\n"
        "--- !Analysis\nPass: inline\nName: X\nFunction: wf_b\n...\n"
    )
    decisions = list(parse_yaml(yaml))
    if [(d["callee"], d["inlined"], d["cost"], d["reason"]) for d in decisions] != [
        ("wf_a$x", True, "10", "inlined"), ("wf_c", False, "240", "too costly to inline")]:
        failures.append(f"yaml: {decisions}")
    for failure in failures:
        print("FAIL", failure)
    if failures:
        return 1
    print("census self-test: passed")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("reduce")
    p.add_argument("--name", required=True)
    p.add_argument("inputs", nargs="+")
    sub.add_parser("self-test")
    args = parser.parse_args()
    if args.command == "reduce":
        return reduce(args)
    return self_test()


if __name__ == "__main__":
    sys.exit(main())
