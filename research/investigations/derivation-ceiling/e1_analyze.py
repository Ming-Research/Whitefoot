"""E1 analysis (research/investigations/derivation-ceiling): reads the shadow
derivation's counter file and prints the tables the record's criteria (1) to
(3) read. Experiment-branch tool; it runs inside the E1 collection test.

Usage: python3 -B e1_analyze.py ROWS.tsv
"""

import collections
import csv
import json
import sys
from pathlib import Path

CONFIGS = ("baseline", "remedy", "remedy+join")
CAP = 1 << 20
SLOW_NS = 1_000_000_000

LINEAR_STEPS = (
    "SourceBound", "SourceDistinct", "SourceGoal", "TransitiveBound",
    "StrengthenedBound", "SubsumedBound", "Equality", "DisequalityFromStrictBound",
    "GoalProjection", "L0Contradiction", "GoalContradiction", "IntegerDomain",
    "ConversionDomain", "AffineConsequence", "GoalNormalization",
    "GoalAffineConsequence", "BooleanIntroduction", "BooleanLiteral",
    "MaterializedBound", "MaterializedDistinct", "MaterializedGoal",
    "RequirementAffineImage", "IntegerExclusion", "implicit", "auto-invariant",
    "loop-induction",
)


QUOTIENT_STEPS = {"UnsignedDivisionProduct"}
ORIGIN_STEPS = {"OriginTransport", "OriginEquality", "OriginProjection", "LetOrigin"}
JOIN_STEPS = {"JoinBound", "JoinDistinct", "JoinGoal", "JoinContradiction",
              "TransportedHeaderRelation"}


def step_names(route):
    return {tag.split(":")[0] for tag in route.split("+")}


def route_class(route):
    """The class DESIGN.md's fixed order gives an ordinary proof's steps, or
    None: class 3 (an S7 result range, items 1 and 2) before class 4 (a
    quotient product for items 3 and 4, the ledger having no separate step
    for item 3's power atom; an origin step for item 5; a written certificate
    for item 8). Item 6's float decoding has no ledger step of its own."""
    names = step_names(route)
    if "OperationFact" in names:
        return "class 3: item 1/2 (S7 fact) not recovered by the remedy"
    if names & QUOTIENT_STEPS:
        return "class 4: item 3 or 4 (quotient product)"
    if names & ORIGIN_STEPS:
        return "class 4: item 5"
    if route == "certificate":
        return "class 4: item 8 (written certificate)"
    return None


def linear_only(route):
    return all(tag.split(":")[0] in LINEAR_STEPS for tag in route.split("+"))


def read(path):
    with open(path, newline="") as handle:
        reader = csv.DictReader(handle, delimiter="\t", quoting=csv.QUOTE_NONE)
        rows = list(reader)
    for row in rows:
        for column in (
            "queries", "refuted", "root_refuted", "atoms", "instances", "n1",
            "n2", "n3", "max_cases", "backjump_nodes", "max_depth",
            "full_nodes", "full_capped", "form_ns", "judge_ns", "full_ns",
            "walk_ns",
        ):
            if row[column] in ("", None):
                row[column] = "0"
            row[column] = int(row[column])
        row["product"] = int(row["product"])
    return rows


def key(row):
    return (row["label"], row["scope"], row["function"], row["record"],
            row["kind"], row["site"])


def proved(row):
    return row["shadow"] == "proved"


def today_proved(row):
    """Ordinary entailment discharges it, or a range clause the baseline proves."""
    if row["kind"] == "range-clause":
        return row["shadow"] == "proved"
    if row["kind"] == "condition":
        return False
    return row["ordinary"] == "proved"


def quantiles(values):
    if not values:
        return "n=0"
    values = sorted(values)
    pick = lambda q: values[min(len(values) - 1, int(q * (len(values) - 1)))]
    return (f"n={len(values)} p50={pick(0.5)} p90={pick(0.9)} p99={pick(0.99)} "
            f"p999={pick(0.999)} max={values[-1]}")


def histogram(values, limit=40):
    counts = collections.Counter(values)
    return " ".join(f"{value}:{counts[value]}" for value in sorted(counts)[:limit])


def consistent(rows):
    """Every batch (one function's shadow walk in one configuration) holds
    one row per obligation record, as many as its total row states."""
    totals = {}
    counted = collections.Counter()
    for row in rows:
        if row["kind"] == "records-total":
            totals[row["batch"]] = int(row["site"])
        elif row["kind"] != "range-clause":
            counted[row["batch"]] += 1
    bad = [batch for batch, total in totals.items() if counted[batch] != total]
    bad += [batch for batch in counted if batch not in totals]
    return bad


def main(path):
    rows = read(path)
    print(f"rows {len(rows)}")
    bad = consistent(rows)
    if bad:
        print(f"record count mismatch in {len(bad)} batches: {sorted(bad)[:10]}")
        sys.exit(2)
    print(f"record counts consistent over {len({row['batch'] for row in rows})} batches")
    rows = [row for row in rows if row["kind"] != "records-total"]
    by_config = {config: {} for config in CONFIGS}
    duplicates = collections.Counter()
    for row in rows:
        if row["prelude"] == "1":
            duplicates["prelude"] += 1
            continue
        table = by_config.setdefault(row["config"], {})
        k = key(row)
        if k in table:
            duplicates[row["config"]] += 1
        table[k] = row
    print("skipped", dict(duplicates))
    for config in CONFIGS:
        table = by_config[config]
        print(f"\n== {config}: {len(table)} obligations")
        summary = collections.Counter()
        for row in table.values():
            kind = row["kind"].split("#")[0]
            summary[(kind, row["ordinary"], row["shadow"].split(":")[0])] += 1
        for (kind, ordinary, shadow), count in sorted(summary.items()):
            print(f"  {kind:28} ordinary={ordinary:10} shadow={shadow:14} {count}")
        notes = collections.Counter(row["shadow"] for row in table.values()
                                    if row["shadow"].startswith(("inconclusive", "not-posed")))
        for note, count in sorted(notes.items()):
            print(f"  note {note} {count}")

    base = by_config["baseline"]
    remedy = by_config["remedy"]
    joined = by_config["remedy+join"]

    print("\n== criterion (1): ordinary-proved, baseline-unproved")
    classes = collections.Counter()
    overlap = collections.Counter()
    sources = collections.Counter()
    ninth = []
    gaps = []
    unrepresentable = []
    for k, row in base.items():
        if row["kind"] in ("range-clause", "condition") or row["ordinary"] != "proved":
            continue
        if proved(row):
            continue
        r = remedy.get(k)
        j = joined.get(k)
        shadow = row["shadow"]
        if r is not None and proved(r):
            label = "class 1: item 1/2 recovered by remedy"
        elif j is not None and proved(j):
            label = "class 2: item 7 recovered by join"
        elif route_class(row["route"]):
            label = route_class(row["route"])
            if label.startswith("class 3"):
                # The fixed order puts an S7 range first; say which of these
                # also rest on a step of another row.
                names = step_names(row["route"])
                also = [name for name, steps in (("quotient", QUOTIENT_STEPS),
                                                 ("origin", ORIGIN_STEPS),
                                                 ("join", JOIN_STEPS)) if names & steps]
                overlap["+".join(also) or "(S7 only)"] += 1
        elif shadow.startswith("inconclusive") or shadow in ("unreached", "pending"):
            label = "class 5: walk gap: " + shadow
            unrepresentable.append(row)
        elif shadow.startswith("not-posed"):
            label = "class 6: not posed: " + row["kind"].split("#")[0]
        elif step_names(row["route"]) & JOIN_STEPS:
            label = "ninth kind: join route not recovered by the weakest-bound join"
            ninth.append(row)
        elif linear_only(row["route"]):
            label = "ninth kind: linear route, facts the walk lacks"
            gaps.append(row)
            sources[tuple(sorted(tag for tag in row["route"].split("+")
                                 if tag.startswith("Source")))] += 1
        else:
            label = "ninth kind: other route"
            ninth.append(row)
        classes[label] += 1
    for label, count in sorted(classes.items()):
        print(f"  {label:70} {count}")
    print(f"  total {sum(classes.values())}")
    print("  class 3 records whose route also has a step of another row:")
    for also, count in sorted(overlap.items()):
        print(f"    {also:30} {count}")
    print("  sources of the ninth kind's linear routes (top 25):")
    for tags, count in sources.most_common(25):
        print(f"    {count:6} {'+'.join(tags) or '(none)'}")
    lost = [k for k, row in base.items() if proved(row) and k in remedy and not proved(remedy[k])]
    print(f"  lost under the remedy: {len(lost)}")
    for k in lost:
        print("    LOST", "\t".join(k), remedy[k]["shadow"], remedy[k]["notes"][:120])

    def conformance_first(row):
        return (row["label"].startswith(("program:", "app:", "probe:")), len(row["label"]))

    def grouped(title, rows, signature):
        groups = collections.defaultdict(list)
        for row in rows:
            groups[signature(row)].append(row)
        print(f"  {title}: {len(rows)} rows in {len(groups)} groups")
        for sig, members in sorted(groups.items(), key=lambda item: -len(item[1])):
            labels = len({row["label"] for row in members})
            print(f"    GROUP {len(members)} rows, {labels} labels: {sig}")
            for row in sorted(members, key=conformance_first)[:3]:
                print("      e.g.", row["label"], row["scope"], row["function"], row["kind"],
                      row["site"].split(" @")[-1], f"n1={row['n1']} atoms={row['atoms']}",
                      row["shadow"])

    def route_signature(row):
        return "+".join(sorted({tag for tag in row["route"].split("+")
                                if not tag.startswith(("Source", "Transitive", "Subsumed",
                                                       "Materialized", "GoalProjection",
                                                       "GoalNormalization"))})) or "(sources only)"

    grouped("ninth kind: join and other routes", ninth, lambda row: (row["kind"].split("#")[0], route_signature(row)))
    grouped("ninth kind: linear routes", gaps,
            lambda row: (row["kind"].split("#")[0], "+".join(sorted(
                tag for tag in row["route"].split("+") if tag.startswith("Source"))) or "(none)"))
    grouped("class 5: walk gaps", unrepresentable,
            lambda row: (row["kind"].split("#")[0], row["notes"][:80]))

    print("\n== soundness review: ordinary unproved, baseline shadow proved")
    flagged = [row for row in base.values()
               if row["kind"] not in ("range-clause", "condition")
               and row["ordinary"] == "unproved" and proved(row)]
    print(f"  {len(flagged)} rows")
    for row in sorted(flagged, key=lambda row: (row["label"], row["function"])):
        print("    FLAG", row["label"], row["scope"], row["function"], row["kind"], row["rule"],
              row["site"].split(" @")[-1], f"n1={row['n1']} atoms={row['atoms']}")
    decided = [row for row in base.values() if row["kind"] == "condition"
               and row["ordinary"] == "decided" and row["shadow"] == "undecided"]
    print(f"  conditions ordinary decides and the shadow does not: {len(decided)}")
    for row in decided:
        print("    COND", row["label"], row["function"], row["site"].split(" @")[-1])

    print("\n== OP-5 conditions (shadow decides more than ordinary?)")
    for config in CONFIGS:
        counter = collections.Counter()
        for row in by_config[config].values():
            if row["kind"] == "condition":
                counter[(row["ordinary"], row["shadow"])] += 1
        print(f"  {config}: " + ", ".join(f"{o}->{s}={c}" for (o, s), c in sorted(counter.items())))

    print("\n== criterion (2): candidate N on today-proved obligations")
    for config in CONFIGS:
        table = by_config[config]
        posed = [row for row in table.values() if today_proved(row) and row["queries"] > 0]
        print(f"  -- {config}: {len(posed)} today-proved posed obligations"
              f" ({sum(1 for row in posed if proved(row))} proved by the shadow)")
        for candidate in ("n1", "n2", "n3", "max_cases", "atoms", "instances",
                          "max_depth", "full_nodes", "backjump_nodes"):
            values = [row[candidate] for row in posed]
            print(f"    {candidate:15} {quantiles(values)}")
        for candidate in ("n1", "n2", "n3"):
            print(f"    {candidate} histogram {histogram([row[candidate] for row in posed])}")
            cover = max((row[candidate] for row in posed if proved(row)), default=0)
            within = [row for row in posed if row[candidate] <= cover]
            worst_time = max(within, key=lambda row: row["judge_ns"] + row["form_ns"], default=None)
            worst_full = max(within, key=lambda row: row["full_nodes"], default=None)
            capped = sum(1 for row in within if row["full_capped"])
            slow = sum(1 for row in within if row["judge_ns"] + row["form_ns"] > SLOW_NS)
            print(f"    {candidate} covering value {cover}: capped={capped} slow>1s={slow}")
            if worst_time:
                print(f"      worst time {(worst_time['judge_ns'] + worst_time['form_ns']) / 1e6:.1f} ms"
                      f" {worst_time['label']} {worst_time['function']} {worst_time['kind']}"
                      f" {candidate}={worst_time[candidate]} full={worst_time['full_nodes']}")
            if worst_full:
                print(f"      worst full analysis {worst_full['full_nodes']} nodes"
                      f" capped={worst_full['full_capped']} {worst_full['label']}"
                      f" {worst_full['function']} {worst_full['kind']} {candidate}={worst_full[candidate]}")
            for value in (2, 4, 8, 16, 32, 64):
                above = sum(1 for row in posed if row[candidate] > value)
                print(f"      above {value}: {above}", end="")
            print()

    print("\n== criterion (5): probes")
    probe_keys = sorted({k for config in CONFIGS for k in by_config[config]
                         if k[0].startswith("probe:") and k[4] != "range-clause"})
    for k in probe_keys:
        cells = []
        for config in CONFIGS:
            row = by_config[config].get(k)
            cells.append(f"{config}={row['shadow'] if row else '-'}"
                         + (f"(n1={row['n1']},n3={row['n3']},cases={row['max_cases']})" if row else ""))
        first = next(by_config[c][k] for c in CONFIGS if k in by_config[c])
        print(f"  {k[0][6:]:34} {k[2]:10} {k[4]:24} {first['rule']:10}"
              f" ordinary={first['ordinary']:10} route={first['route'][:80]} " + " ".join(cells))

    print("\n== criterion (3): joins")
    for config in ("remedy", "remedy+join"):
        table = by_config[config]
        posed = [row for row in table.values() if row["queries"] > 0]
        today = [row for row in posed if today_proved(row)]
        total_ns = sum(row["judge_ns"] + row["form_ns"] for row in posed)
        walk_ns = {}
        for row in table.values():
            walk_ns[(row["label"], row["scope"], row["function"])] = row["walk_ns"]
        print(f"  {config}: posed={len(posed)} proved={sum(1 for row in posed if proved(row))}"
              f" n1>0={sum(1 for row in today if row['n1'] > 0)}"
              f" n3>0={sum(1 for row in today if row['n3'] > 0)}"
              f" derivation_ms={total_ns / 1e6:.0f} walk_ms={sum(walk_ns.values()) / 1e6:.0f}")
        for value in (2, 4, 8, 16, 32):
            print(f"    n1>{value}: {sum(1 for row in today if row['n1'] > value)}"
                  f"  n3>{value}: {sum(1 for row in today if row['n3'] > value)}")

    verdicts(by_config)
    worst_records(by_config)


def declared():
    """Each conformance case's declared verdict, by id."""
    manifest = Path(__file__).resolve().parents[3] / "tests/conformance/manifest.jsonl"
    out = {}
    for line in manifest.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        case = json.loads(line)
        # Rule-coverage lines carry no case.
        if "id" in case and "expect" in case:
            out[case["id"]] = case["expect"]["kind"]
    return out


def verdicts(by_config):
    """Per program, the two verdicts DESIGN.md reads from the file:
    handoff (every record proved by ordinary entailment or the shadow) and
    derivation alone (every posed record proved by the shadow, no condition
    the shadow decides); a record the shadow did not pose keeps its ordinary
    answer and the program is counted apart."""
    expect = declared()
    print("\n== verdicts per program against today's")
    for config in CONFIGS:
        per_label = collections.defaultdict(lambda: {"handoff": True, "alone": True,
                                                     "unposed": False, "today": True})
        for row in by_config[config].values():
            state = per_label[row["label"]]
            kind = row["kind"]
            if kind == "range-clause":
                ok = proved(row)
                state["handoff"] &= ok
                state["alone"] &= ok
                state["today"] &= ok
                continue
            if kind == "condition":
                state["today"] &= row["ordinary"] != "decided"
                state["handoff"] &= row["ordinary"] != "decided"
                state["alone"] &= not row["shadow"].startswith("decided")
                continue
            ordinary_ok = row["ordinary"] == "proved"
            state["today"] &= ordinary_ok
            state["handoff"] &= ordinary_ok or proved(row)
            if row["shadow"].startswith("not-posed"):
                state["unposed"] = True
                state["alone"] &= ordinary_ok
            else:
                state["alone"] &= proved(row)
        table = collections.Counter()
        examples = collections.defaultdict(list)
        for label, state in per_label.items():
            if label.startswith("probe:"):
                continue
            if label.startswith(("program:", "app:")):
                want = "accept"
            else:
                want = {"run": "accept", "accept": "accept", "reject": "reject"}.get(
                    expect.get(label, "?"), "other")
            today = "accept" if state["today"] else "reject"
            for verdict in ("today", "handoff", "alone"):
                got = "accept" if state[verdict] else "reject"
                table[(verdict, want, got)] += 1
                # Against today's verdict read from the same records: a
                # case rejected by a rule outside entailment has no record
                # that says so, and its declared verdict cannot change here.
                if verdict != "today" and got != today:
                    family = label.split(":")[0] if ":" in label else "case"
                    examples[f"{verdict} {family} today={today} reached={got}"].append(label)
            if state["unposed"] and today == "accept" and not state["alone"]:
                table[("alone-with-unposed", want, "reject")] += 1
        print(f"  -- {config}")
        for (verdict, want, got), count in sorted(table.items()):
            print(f"    {verdict:20} declared={want:7} reached={got:7} {count}")
        for name, labels in sorted(examples.items()):
            shown = sorted(labels) if " case " not in name else sorted(labels)[:40]
            print(f"    {name}: {len(labels)}: {' '.join(shown)}")
        families = collections.Counter(
            label.split(":")[0] if ":" in label else "case"
            for label in per_label if not label.startswith("probe:"))
        print(f"    labels by family: {dict(sorted(families.items()))}")


def worst_records(by_config):
    print("\n== slowest today-proved records (form, judge, full analysis)")
    for config in CONFIGS:
        rows = [row for row in by_config[config].values() if today_proved(row)]
        rows.sort(key=lambda row: -(row["form_ns"] + row["judge_ns"]))
        for row in rows[:8]:
            print(f"  {config:12} {row['label'][:50]:50} {row['function'][:24]:24} {row['kind']:16}"
                  f" n1={row['n1']} atoms={row['atoms']} instances={row['instances']}"
                  f" form_ms={row['form_ns'] / 1e6:.1f} judge_ms={row['judge_ns'] / 1e6:.1f}"
                  f" full_ms={row['full_ns'] / 1e6:.1f} full={row['full_nodes']}"
                  f" backjump={row['backjump_nodes']} queries={row['queries']}")


if __name__ == "__main__":
    main(sys.argv[1])
