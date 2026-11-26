"""E1 analysis (research/investigations/derivation-ceiling): reads the shadow
derivation's counter file and prints the tables the record's criteria (1) to
(3) read. Experiment-branch tool; it runs inside the E1 collection test.

Usage: python3 -B e1_analyze.py ROWS.tsv
"""

import collections
import csv
import sys

CONFIGS = ("baseline", "remedy", "remedy+join")
CAP = 1 << 20
SLOW_NS = 1_000_000_000

ROUTE_ITEMS = {
    "s7-result-bound": "item 1/2 not recovered",
    "interval-product": "item 1/2 not recovered",
    "power-atom": "item 3",
    "step7-quotient": "item 4",
    "origin-view": "item 5",
    "float-decode": "item 6",
    "certificate-fold": "item 8",
}


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


def main(path):
    rows = read(path)
    print(f"rows {len(rows)}")
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
    ninth = []
    for k, row in base.items():
        if row["kind"] in ("range-clause", "condition") or row["ordinary"] != "proved":
            continue
        if proved(row):
            continue
        r = remedy.get(k)
        j = joined.get(k)
        shadow = row["shadow"]
        if r is not None and proved(r):
            label = "item 1/2 recovered by remedy"
        elif j is not None and proved(j):
            label = "item 7 recovered by join"
        elif row["route"] in ROUTE_ITEMS:
            label = ROUTE_ITEMS[row["route"]]
        elif shadow.startswith("not-posed"):
            label = "not posed: " + row["kind"].split("#")[0]
        elif shadow.startswith("inconclusive") or shadow in ("unreached", "pending"):
            label = "walk gap: " + shadow
        else:
            label = "ninth kind"
            ninth.append(row)
        classes[label] += 1
    for label, count in sorted(classes.items()):
        print(f"  {label:50} {count}")
    print(f"  total {sum(classes.values())}")
    lost = [k for k, row in base.items() if proved(row) and k in remedy and not proved(remedy[k])]
    print(f"  lost under the remedy: {len(lost)}")
    for k in lost[:20]:
        print("    LOST", "\t".join(k), remedy[k]["shadow"])
    print(f"  ninth kind rows (first 120 of {len(ninth)}):")
    for row in ninth[:120]:
        print("    NINTH", row["label"], row["function"], row["kind"], row["rule"],
              row["site"][:120], row["route"], f"n1={row['n1']} atoms={row['atoms']}")

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
        for candidate in ("n1", "n3"):
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


if __name__ == "__main__":
    main(sys.argv[1])
