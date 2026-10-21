#!/usr/bin/env python3
"""Recompute the Linux columns of .github/time-budgets.txt from recorded runs.

The rule is the one AGENTS.md "Checks" states: for each stage and CPU class,
the maximum of its most recent recorded runs (at least 20) plus 10%, rounded
up to 5 s, with a 10 s floor. This tool applies it to the stage times that
`run-check.pl` prints (`== END <label>: <s> s, exit <n> ==`) in the logs of
CI runs; it changes neither the rule nor who approves a raise.

  collect   fetch the job logs of a workflow's finished runs with `gh` and
            keep each stage's time, CPU model and logged budget in a samples
            file (JSON; kept outside the repository, it is scratch input)
  table     print each Linux cell's count, median, maximum, old and new
            budget; with --write, rewrite the table file
  replay    apply the rule to the recorded runs as a table recomputed every
            few days would have judged them, and say how many jobs it turns
            red, with the times of every stage of a job scaled up as a
            slowdown control
  --self-test   check the parsing, the rule, the table rewrite and the
            replay on synthetic data (no network)

Cells and columns:
  * A class column with at least 20 recorded runs of a stage is recomputed.
  * A class with fewer runs (the owner-directed Xeon column) takes the larger
    of the row's recomputed class values and, if it has any run, its own
    time by the rule: one run shows a class needs more, not that it needs
    less. The row is left alone when no class has enough runs.
  * Rows named by --keep, the macos and windows columns, and rows with no
    recorded run are not touched.
"""
import argparse
import collections
import concurrent.futures
import json
import os
import re
import statistics
import subprocess
import sys
import time

MINIMUM_RUNS = 20
DEFAULT_WINDOW = 50
MARGIN_PERCENT = 10
ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
MODEL = re.compile(r"^\S+ model name[ \t]*:[ \t]*(.*?)[ \t]*$", re.M)
END = re.compile(r"^(\S+) == END (\S+): (\d+)\.(\d\d) s, exit (\d+) ==$", re.M)
BUDGET = re.compile(r"== BUDGET (\S+): [\d.]+ s of (\d+) s")
OVER = re.compile(r"== OVER BUDGET: [\w-]+: (\S+) took [\d.]+ s, over its (\d+) s")


def budget_for(max_hundredths):
    """The rule on a maximum in hundredths of a second, in whole seconds."""
    scaled = max_hundredths * (100 + MARGIN_PERCENT)
    return max(10, -(-scaled // (100 * 100 * 5)) * 5)


def parse_log(text):
    """(CPU model or None, one dict per finished stage) of one job's log."""
    text = ANSI.sub("", text)
    model = MODEL.search(text)
    logged = {m.group(1): int(m.group(2)) for m in BUDGET.finditer(text)}
    logged.update({m.group(1): int(m.group(2)) for m in OVER.finditer(text)})
    rows = []
    for m in END.finditer(text):
        ts, label, whole, frac, code = m.groups()
        rows.append({"label": label, "cs": int(whole) * 100 + int(frac), "exit": int(code),
                     "ts": ts, "budget": logged.get(label)})
    return (model.group(1) if model else None), rows


def flatten(samples):
    """Stage rows with their job's fields, repeated logs dropped.

    A rerun attempt lists the earlier attempts' jobs again under new ids, so
    the same stage can appear more than once; it counts once per run, label
    and finishing time.
    """
    seen, out = set(), []
    for job_id, job in samples["jobs"].items():
        for row in job["rows"]:
            key = (job["run"], row["label"], row["ts"])
            if key in seen or job["cpu"] is None:
                continue
            seen.add(key)
            out.append(dict(row, job=job_id, run=job["run"], cpu=job["cpu"],
                            branch=job["branch"], day=row["ts"][:10]))
    out.sort(key=lambda r: r["ts"])
    return out


class Table:
    """.github/time-budgets.txt, parsed far enough to change cells in place."""

    def __init__(self, text):
        self.lines = text.split("\n")
        self.classes = {}
        self.columns = None
        self.rows = {}
        for index, line in enumerate(self.lines):
            if re.match(r"\s*(#|$)", line):
                continue
            tokens = line.split()
            if self.columns is None:
                if tokens[0] == "cpu":
                    self.classes[tokens[1]] = line.split(None, 2)[2].strip()
                    continue
                if tokens[0] != "label":
                    raise SystemExit("the table header must start with label")
                self.columns = tokens[1:]
                continue
            self.rows[tokens[0]] = index

    def cells(self, label):
        tokens = self.lines[self.rows[label]].split()
        return dict(zip(self.columns, tokens[1:]))

    def set(self, label, column, value):
        """Replace one cell; later cells keep their place unless crowded."""
        index = self.rows[label]
        found = [(m.start(), m.group()) for m in re.finditer(r"\S+", self.lines[index])]
        slot = 1 + self.columns.index(column)
        found[slot] = (found[slot][0], str(value))
        line, end = "", 0
        for start, token in found:
            start = max(start, end + 1) if line else start
            line += " " * (start - len(line)) + token
            end = len(line)
        self.lines[index] = line

    def text(self):
        return "\n".join(self.lines)


def window_of(rows, label, model, size):
    ok = [r for r in rows if r["label"] == label and r["cpu"] == model and r["exit"] == 0]
    return ok[-size:]


def recompute(table, rows, size, keep=()):
    """Set the new cells in `table`; return one record per changed-or-checked cell."""
    report = []
    for label in table.rows:
        if label in keep:
            continue
        cells = table.cells(label)
        new = {}
        thin = {}
        for column, model in table.classes.items():
            if cells.get(column, "-") == "-":
                continue
            used = window_of(rows, label, model, size)
            if len(used) >= MINIMUM_RUNS:
                top = max(r["cs"] for r in used)
                new[column] = budget_for(top)
                report.append((label, column, len(used), statistics.median(r["cs"] for r in used) / 100,
                               top / 100, cells[column], new[column]))
            elif used:
                thin[column] = (len(used), budget_for(max(r["cs"] for r in used)), used)
        if not new:
            continue
        largest = max(new.values())
        for column, value in new.items():
            table.set(label, column, value)
        for column in table.classes:
            if column in new or cells.get(column, "-") == "-":
                continue
            own = thin.get(column)
            value = max(largest, own[1]) if own else largest
            used = own[2] if own else []
            report.append((label, column, len(used),
                           statistics.median(r["cs"] for r in used) / 100 if used else None,
                           max(r["cs"] for r in used) / 100 if used else None, cells[column], value))
            table.set(label, column, value)
    return report


def replay(rows, table, since, size, every, slowdowns, keep=()):
    """Jobs over budget when the rule, recomputed every `every` days, judged them.

    A stage is judged against the rule applied to its class's `size` runs
    that finished before the start of its recompute period. A job is over
    when any of its judged stages is. Each slowdown scales the times of every
    stage of the job together. Returns {name: {slowdown: (over, jobs)}}.
    """
    labels = [label for label in table.rows if label not in keep]
    models = set(table.classes.values())
    rows = [r for r in rows if r["label"] in labels and r["cpu"] in models and r["exit"] == 0]
    days = sorted({r["day"] for r in rows})
    series = collections.defaultdict(list)
    for r in rows:
        series[(r["label"], r["cpu"])].append(r)
    judged = collections.defaultdict(list)
    logged = collections.defaultdict(list)
    for (label, model), history in series.items():
        for r in history:
            if r["day"] < since:
                continue
            boundary = days[(days.index(r["day"]) // every) * every]
            before = [h for h in history if h["day"] < boundary][-size:]
            if len(before) >= MINIMUM_RUNS:
                judged[r["job"]].append((r["cs"], 100 * budget_for(max(h["cs"] for h in before))))
            if r["budget"] is not None:
                logged[r["job"]].append((r["cs"], 100 * r["budget"]))
    result = {}
    for name, jobs in (("table logged when the job ran", logged),
                       (f"rule over {size} runs, recomputed every {every} day(s)", judged)):
        result[name] = {}
        for slowdown in slowdowns:
            over = sum(any(cs * (1 + slowdown) > limit for cs, limit in stages) for stages in jobs.values())
            result[name][slowdown] = (over, len(jobs))
    return result


def gh(*args):
    """Output of `gh api`; a server error (HTTP 5xx) is retried a few times."""
    for attempt in range(1, 5):
        done = subprocess.run(["gh", "api", "--allow-escape-sequences", *args],
                              capture_output=True, text=True)
        if done.returncode == 0:
            return done.stdout
        if "HTTP 5" not in done.stderr or attempt == 4:
            raise SystemExit(f"gh api {' '.join(args)}: {done.stderr.strip()}")
        time.sleep(3 * attempt)


def lines(*args):
    return [json.loads(line) for line in gh(*args).splitlines() if line.strip()]


def save(path, samples):
    with open(path + ".tmp", "w") as handle:
        json.dump(samples, handle)
    os.replace(path + ".tmp", path)


def collect(repo, workflows, since, path, limit):
    """Add the jobs of finished runs to the samples file; a rerun skips known jobs."""
    try:
        with open(path) as handle:
            samples = json.load(handle)
    except FileNotFoundError:
        samples = {"jobs": {}}
    def fetch(run, workflow):
        found = []
        for attempt in range(1, run["run_attempt"] + 1):
            jobs = lines("--paginate", f"repos/{repo}/actions/runs/{run['id']}/attempts/{attempt}/jobs",
                         "--jq", ".jobs[] | {id, conclusion} | tojson")
            for job in jobs:
                if job["conclusion"] not in ("success", "failure") or str(job["id"]) in samples["jobs"]:
                    continue
                model, rows = parse_log(gh(f"repos/{repo}/actions/jobs/{job['id']}/logs"))
                found.append((str(job["id"]), {"run": run["id"], "workflow": workflow, "sha": run["head_sha"],
                                               "branch": run["head_branch"], "cpu": model, "rows": rows}))
        return found
    for workflow in workflows:
        runs = lines("--method", "GET", "--paginate", f"repos/{repo}/actions/workflows/{workflow}/runs",
                     "-f", "per_page=100", "-f", "status=completed", "-f", f"created=>={since}",
                     "--jq", ".workflow_runs[] | {id, head_sha, head_branch, run_attempt, conclusion, created_at} | tojson")
        runs = [r for r in runs if r["conclusion"] in ("success", "failure")]
        if runs:
            # The listing is newest first and holds at most 1,000 runs, and
            # --limit keeps the newest: the oldest kept run is the window's start.
            print(f"{workflow}: {len(runs)} finished runs listed, keeping the newest {min(limit, len(runs))},"
                  f" the oldest created {runs[:limit][-1]['created_at']}", file=sys.stderr)
        runs = runs[:limit]
        # A run's jobs are recorded together, so a run with a recorded job is done.
        known = {job["run"] for job in samples["jobs"].values()}
        runs = [r for r in runs if r["id"] not in known]
        with concurrent.futures.ThreadPoolExecutor(6) as pool:
            for done, found in enumerate(pool.map(lambda run: fetch(run, workflow), runs), 1):
                samples["jobs"].update(found)
                if done % 10 == 0:
                    save(path, samples)
                    print(f"{workflow}: {done} of {len(runs)} runs", file=sys.stderr, flush=True)
        print(f"{workflow}: {len(runs)} runs, {len(samples['jobs'])} jobs kept", file=sys.stderr)
    save(path, samples)


def load(paths):
    """The jobs of every samples file, as one."""
    jobs = {}
    for path in paths:
        with open(path) as handle:
            jobs.update(json.load(handle)["jobs"])
    return {"jobs": jobs}


def print_report(report):
    print("| stage | class | runs | median s | max s | old | new |")
    print("|---|---|---:|---:|---:|---:|---:|")
    for label, column, count, median, top, old, new in report:
        shown = "" if median is None else f"{median:.1f}"
        peak = "" if top is None else f"{top:.1f}"
        print(f"| `{label}` | {column} | {count} | {shown} | {peak} | {old} | {new}"
              f"{'' if str(new) == old else ' *'} |")


def self_test():
    stamp = "2026-10-20T10:%02d:00.0000000Z"
    log = "\n".join([
        stamp % 0 + " model name\t: AMD EPYC 7763 64-Core Processor",
        stamp % 1 + " \x1b[36;1m== END check/runtime: 21.85 s, exit 0 ==\x1b[0m",
        stamp % 1 + " == BUDGET check/runtime: 21.9 s of 25 s (linux) ==",
        stamp % 2 + " == END check/corpus: 363.20 s, exit 0 ==",
        stamp % 2 + " == OVER BUDGET: enforced: check/corpus took 363.2 s, over its 360 s linux budget ==",
        stamp % 3 + " == END compiler/lint: 5.00 s, exit 101 ==",
    ])
    model, found = parse_log(log)
    assert model == "AMD EPYC 7763 64-Core Processor", model
    assert [(r["label"], r["cs"], r["exit"], r["budget"]) for r in found] == [
        ("check/runtime", 2185, 0, 25), ("check/corpus", 36320, 0, 360), ("compiler/lint", 500, 101, None)], found
    assert parse_log("no stages here") == (None, [])

    # The rule, exactly: 110.00 s is 110 s, not 115 s; 59.95 rounds to 60 and
    # 60.005 to 65; the floor is 10 s (a 1 s stage would round to 5 s).
    assert [budget_for(m) for m in (32370, 10000, 5450, 5455, 100)] == [360, 110, 60, 65, 10]

    table_text = "\n".join([
        "# comment", "cpu big-1  Big CPU", "cpu small-2  Small CPU", "cpu thin-3  Thin CPU", "",
        "label        big-1  small-2  thin-3  macos  windows",
        "stage/one    50     50       50      60     -",
        "stage/two    30     30       30      40     -",
        "stage/three  20     20       20      25     -",
        "stage/four   20     20       20      25     -",
        "stage/five   -      15       -       25     -",
        "stage/six    40     40       40      50     -",
        "stage/seven  60     60       60      70     -",
        ""])
    def sample(rows, budget=None):
        jobs = {}
        for number, (label, model, cs, day, code) in enumerate(rows):
            jobs[str(number)] = {"run": number, "workflow": "gate.yml", "sha": "0", "branch": "main", "cpu": model,
                                 "rows": [{"label": label, "cs": cs, "exit": code,
                                           "ts": f"2026-10-{day:02d}T00:00:{number % 60:02d}.{number // 60:07d}Z",
                                           "budget": budget}]}
        return {"jobs": jobs}
    data = sample(
        [("stage/one", "Big CPU", 9000 + 100 * i, 1 + i // 3, 0) for i in range(25)]      # max 114.00 s
        + [("stage/one", "Big CPU", 99900, 28, 1)]                                          # failed: not counted
        + [("stage/one", "Small CPU", 8000, 5, 0)] * 10                                     # 10 runs: too few
        + [("stage/one", "Thin CPU", 12000, 6, 0)]                                          # one run: 120 s
        + [("stage/two", "Big CPU", 5000, 7, 0)] * 25                                       # kept as is
        + [("stage/four", "Small CPU", 100, 8, 0)] * 25                                     # floor: 10 s
        + [("stage/six", "Big CPU", 4000 + 100 * i, 9 + i // 3, 0) for i in range(15)]    # 15 runs, logged twice
        + [("stage/seven", "Big CPU", 5000, 14, 0)] * 25                                    # 55 s
        + [("stage/seven", "Small CPU", 3000, 14, 0)] * 25)                                 # 35 s
    # A rerun attempt lists the earlier attempts' jobs again under new job ids,
    # with the same run, stage and time; they count once. A job whose CPU model
    # was not logged is not sampled.
    for number, job in list(data["jobs"].items()):
        if job["rows"][0]["label"] == "stage/six":
            data["jobs"]["again-" + number] = job
    data["jobs"]["no-cpu"] = dict(data["jobs"]["0"], run=999, cpu=None)
    rows = flatten(data)
    assert all(r["cpu"] for r in rows)
    assert sum(r["label"] == "stage/six" for r in rows) == 15
    table = Table(table_text)
    report = recompute(table, rows, 50, keep=("stage/two",))
    cells = table.cells("stage/one")
    # 114.00 s * 1.1 = 125.4 -> 130. The 10-run class takes the row's largest
    # recomputed class (130), the one-run class its own 120 s -> 135 if higher.
    assert (cells["big-1"], cells["small-2"], cells["thin-3"]) == ("130", "130", "135"), cells
    assert table.cells("stage/two") == {"big-1": "30", "small-2": "30", "thin-3": "30", "macos": "40", "windows": "-"}
    assert table.cells("stage/three") == {"big-1": "20", "small-2": "20", "thin-3": "20", "macos": "25", "windows": "-"}
    assert table.cells("stage/four")["small-2"] == "10" and table.cells("stage/four")["big-1"] == "10"
    assert table.cells("stage/five") == {"big-1": "-", "small-2": "15", "thin-3": "-", "macos": "25", "windows": "-"}
    assert table.cells("stage/one")["macos"] == "60" and table.cells("stage/one")["windows"] == "-"
    # Counted once, 15 runs are too few; counted twice they would be 30.
    assert table.cells("stage/six") == {"big-1": "40", "small-2": "40", "thin-3": "40", "macos": "50", "windows": "-"}
    # Two classes have enough runs, 55 s and 35 s by the rule: the class
    # without runs takes the larger of them, not the smaller.
    assert table.cells("stage/seven") == {"big-1": "55", "small-2": "35", "thin-3": "55", "macos": "70", "windows": "-"}
    out = table.text().split("\n")
    old = table_text.split("\n")
    assert out[:6] == old[:6] and out[7:9] == old[7:9] and out[10:12] == old[10:12] and out[13:] == old[13:]
    assert all(len(line.split()) == len(before.split()) for line, before in zip(out, old))
    again = Table(table.text())
    assert recompute(again, rows, 50, keep=("stage/two",)) and again.text() == table.text()
    assert any(entry[0] == "stage/one" and entry[1] == "big-1" and entry[2] == 25 and entry[6] == 130 for entry in report)

    # A recorded series that is stable: no job is over its recomputed budget
    # without a slowdown, every job is over it at +30%, and none at +5%, so
    # the replay can tell a slow stage from the margin's own noise.
    stable = [("stage/one", "Big CPU", 10000 + 100 * (i % 3), 1 + i, 0) for i in range(30)]
    replayed = replay(flatten(sample(stable)), Table(table_text), "2026-10-21", 20, 1, (0, 0.05, 0.30))
    judged = replayed["rule over 20 runs, recomputed every 1 day(s)"]
    assert judged == {0: (0, 10), 0.05: (0, 10), 0.30: (10, 10)}, judged
    # The same series with its newest day 25% slower turns exactly that job red.
    slowed = stable[:-1] + [("stage/one", "Big CPU", 12600, 30, 0)]
    judged = replay(flatten(sample(slowed)), Table(table_text), "2026-10-21", 20, 1, (0,))
    assert judged["rule over 20 runs, recomputed every 1 day(s)"][0] == (1, 10), judged
    # A recompute every 3 days judges a stage on the runs before its period's
    # first day: day 21 belongs to the period of day 19, which has 18 runs
    # before it and is not judged, so 9 jobs remain.
    spaced = replay(flatten(sample(stable)), Table(table_text), "2026-10-21", 20, 3, (0, 0.30))
    assert spaced["rule over 20 runs, recomputed every 3 day(s)"] == {0: (0, 9), 0.30: (9, 9)}, spaced
    # Only days from --from are judged, and the budget a log recorded (120 s:
    # above the slowest stage's 102 s, below it at +30%) is judged as logged.
    late = replay(flatten(sample(stable, budget=120)), Table(table_text), "2026-10-25", 20, 1, (0, 0.30))
    assert late["rule over 20 runs, recomputed every 1 day(s)"] == {0: (0, 6), 0.30: (6, 6)}, late
    assert late["table logged when the job ran"] == {0: (0, 6), 0.30: (6, 6)}, late
    print("recompute-time-budgets self-test passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--self-test", action="store_true")
    sub = parser.add_subparsers(dest="command")
    fetch = sub.add_parser("collect")
    fetch.add_argument("--repo", required=True)
    fetch.add_argument("--workflow", action="append", required=True)
    fetch.add_argument("--since", required=True, help="YYYY-MM-DD")
    fetch.add_argument("--limit", type=int, default=400, help="newest runs per workflow")
    fetch.add_argument("--samples", required=True)
    for name in ("table", "replay"):
        command = sub.add_parser(name)
        command.add_argument("--samples", required=True, nargs="+", help="samples files from collect")
        command.add_argument("--budgets", default=".github/time-budgets.txt")
        command.add_argument("--window", type=int, default=DEFAULT_WINDOW)
        command.add_argument("--keep", action="append", default=[], help="a stage to leave untouched")
        if name == "table":
            command.add_argument("--write", action="store_true")
        else:
            command.add_argument("--from", dest="since", required=True, help="first day judged, YYYY-MM-DD")
            command.add_argument("--every", type=int, default=1, help="days between recomputes")
            command.add_argument("--slowdown", type=float, action="append", help="default 0 0.1 0.2 0.3")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.command == "collect":
        return collect(args.repo, args.workflow, args.since, args.samples, args.limit)
    if args.command is None:
        parser.error("name a command or --self-test")
    rows = flatten(load(args.samples))
    with open(args.budgets) as handle:
        table = Table(handle.read())
    if args.command == "table":
        print_report(recompute(table, rows, args.window, tuple(args.keep)))
        if args.write:
            with open(args.budgets, "w") as handle:
                handle.write(table.text())
        return
    for name, shares in replay(rows, table, args.since, args.window, args.every, sorted(set(args.slowdown or [0.0, 0.1, 0.2, 0.3])),
                               tuple(args.keep)).items():
        print(name)
        for slowdown, (over, jobs) in shares.items():
            print(f"  +{slowdown * 100:.0f}% slower: {over} of {jobs} jobs over budget ({100 * over / max(jobs, 1):.1f}%)")


if __name__ == "__main__":
    main()
