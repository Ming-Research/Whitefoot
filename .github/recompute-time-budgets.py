#!/usr/bin/env python3
"""Recompute the Linux columns of .github/time-budgets.txt from recorded runs.

The rule is the one AGENTS.md "Checks" states: for each stage and CPU class,
the budget is 1.1 times the maximum of its most recent recorded runs (at least
20), plus that maximum's distance above the median of those runs, rounded up
to 5 s, with a 10 s floor. This tool applies it to the stage times
that `run-check.pl` prints (`== END <label>: <s> s, exit <n> ==`) in the logs
of CI runs; it changes neither who approves a raise nor the host's own
comparison in `run-check.pl`.

  collect   fetch the job logs of a workflow's finished runs with `gh` (the
            archive of a run attempt's logs, two requests for the attempt
            with its jobs' listing) and keep each stage's time, CPU model and
            logged budget in a samples file (JSON; kept outside the
            repository, it is scratch input);
            --branch keeps one branch's runs, --job the jobs whose names
            start with a prefix (each repeatable); --reserve keeps part of
            the hour's API requests for others and stops before them
  table     print each Linux cell's count, median, maximum, old and new
            budget; with --write, rewrite the table file
  headroom  print, per CPU class, how far above its median a typical run of a
            stage can grow before the budget trips, under each rule
  replay    apply the rule to the recorded runs as a table recomputed every
            few days (or just before each run, --every 0) would have judged
            them, and say how many jobs it turns red, with the times of every
            stage of a job scaled up as a slowdown control; --list names
            each job that went red as it ran and says whether the rule would
            have kept it green, with --branch and --attempt narrowing the
            jobs judged
  against   hold each job that went red as it ran to the budgets the table
            itself gives its CPU model (the largest Linux cell of the stage
            for a model with no column, as run-check.pl does), where `replay`
            judges it by the runs before it; --branch and --attempt narrow
            the jobs judged
  --self-test   check the parsing, the rules, the table rewrite, the replay
            and the judgment against a table on synthetic data (no network)

Cells and columns:
  * A class column with at least 20 recorded runs of a stage is recomputed.
  * A cell with fewer runs keeps its value, except in a column the owner
    added for a model with too few runs (OWNER_COLUMNS: the Xeon): that
    takes the larger of the row's recomputed class values and, if it has
    any run, its own time by the rule: one run shows a class needs more,
    not that it needs less. The row is left alone when no class has enough
    runs. `replay` judges such a cell the same way.
  * Rows named by --keep, the macos and windows columns, and rows with no
    recorded run are not touched.

RULES holds the budget rule the table follows (DEFAULT_RULE) and the fixed
margins it replaced or was compared with (research/investigations/
test-economy/time-budgets.md, "The margin of the budget rule"); `--rule`
selects one for `table`, `headroom` and `replay`.
"""
import argparse
import collections
import concurrent.futures
import contextlib
import datetime
import io
import json
import os
import re
import statistics
import subprocess
import sys
import tempfile
import time
import zipfile

MINIMUM_RUNS = 20
# The columns the owner added for a model with fewer than MINIMUM_RUNS runs
# (decision `owner-added-columns-take-the-larger-value` in
# design/decisions/development/verification.md); an addition is the owner's decision.
OWNER_COLUMNS = ("xeon-8573c",)
BUDGETS = ".github/time-budgets.txt"
DEFAULT_WINDOW = 50
ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
MODEL = re.compile(r"^\S+ model name[ \t]*:[ \t]*(.*?)[ \t]*$", re.M)
END = re.compile(r"^(\S+) == END (\S+): (\d+)\.(\d\d) s, exit (\d+) ==$", re.M)
BUDGET = re.compile(r"== BUDGET (\S+): [\d.]+ s of (\d+) s")
OVER = re.compile(r"== OVER BUDGET: [\w-]+: (\S+) took [\d.]+ s, over its (\d+) s")


def margin_rule(percent):
    """The maximum of the window plus a fixed share of it."""
    def rule(window):
        return max(10, -(-max(window) * (100 + percent) // (100 * 100 * 5)) * 5)
    return rule


def spread_rule(window):
    """1.1 times the maximum plus the maximum's distance above the median.

    On hundredths of a second, in whole seconds: with M the maximum and D the
    median, 1.1 M + (M - D) = (42 M - 10 * 2D) / 20 hundredths, and 5 s is
    500 hundredths, so the rounded value is the integer ceiling of
    (42 M - 10 * 2D) / 10000 times 5. A quiet class keeps a margin close to
    10% and a class that has been seen to vary gets the variation on top.
    """
    ordered = sorted(window)
    twice_median = ordered[len(ordered) // 2] + ordered[(len(ordered) - 1) // 2]
    return max(10, -(-(42 * ordered[-1] - 10 * twice_median) // 10000) * 5)


# The budget rules by name. DEFAULT_RULE is the one the table follows; the
# fixed margins are the former rule (max+10) and the alternatives it was
# compared with, kept so that the comparison can be rerun.
RULES = {"spread": spread_rule, "max+10": margin_rule(10), "max+25": margin_rule(25),
         "max+30": margin_rule(30)}
DEFAULT_RULE = "spread"


def budget_for(window, rule=None):
    """The budget of a window of stage times (hundredths of a second), in whole seconds."""
    return (rule or RULES[DEFAULT_RULE])(window)


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
            out.append(dict(row, job=job_id, run=job["run"], cpu=job["cpu"], attempt=job.get("attempt"),
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


def recompute(table, rows, size, keep=(), owner_columns=OWNER_COLUMNS, rule=None):
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
                new[column] = budget_for([r["cs"] for r in used], rule)
                report.append((label, column, len(used), statistics.median(r["cs"] for r in used) / 100,
                               max(r["cs"] for r in used) / 100, cells[column], new[column]))
            elif used:
                thin[column] = (len(used), budget_for([r["cs"] for r in used], rule), used)
        if not new:
            continue
        largest = max(new.values())
        for column, value in new.items():
            table.set(label, column, value)
        for column in table.classes:
            if column in new or cells.get(column, "-") == "-":
                continue
            own = thin.get(column)
            used = own[2] if own else []
            value = cells[column]
            if column in owner_columns:
                value = max(largest, own[1]) if own else largest
                table.set(label, column, value)
            if used or column in owner_columns:
                report.append((label, column, len(used),
                               statistics.median(r["cs"] for r in used) / 100 if used else None,
                               max(r["cs"] for r in used) / 100 if used else None, cells[column], value))
    return report


def moment(stamp):
    """A log time stamp as a datetime (whole seconds are enough here)."""
    return datetime.datetime.fromisoformat(stamp[:19])


class Judgment:
    """The stages of recorded jobs, each judged by the rule as a table would have.

    `every` > 0 recomputes the table every `every` days with runs: a stage is
    judged against the `size` runs of its class that finished before the start
    of its period. `every` == 0 recomputes it just before each stage. `age`
    days make the table that much older than the stage it judges. The judged
    jobs are those finishing from the day `since` on, of `branch` and of
    `attempt` where given; the runs the rule is applied to are those of every
    branch.

    A stage of a class with fewer than MINIMUM_RUNS earlier runs is judged only
    in an owner column, as `recompute` sets it: by the larger of the budgets of
    the row's other classes that have enough runs and its own runs' budget.
    Any other stage is not judgeable and says why.
    """

    def __init__(self, rows, table, since, size, every, keep=(), rule=None, branch=None,
                 attempt=None, age=0, owner_columns=OWNER_COLUMNS):
        self.rule = rule
        self.size = size
        self.every = every
        self.age = age
        self.table = table
        self.owner_columns = owner_columns
        self.column_of = {model: column for column, model in table.classes.items()}
        labels = [label for label in table.rows if label not in keep]
        rows = [r for r in rows if r["label"] in labels and r["cpu"] in self.column_of and r["exit"] == 0]
        days = sorted({r["day"] for r in rows})
        self.series = collections.defaultdict(list)
        for r in rows:
            self.series[(r["label"], r["cpu"])].append(r)
        self.stamps = {key: [moment(r["ts"]) for r in history] for key, history in self.series.items()}
        # One entry per judged stage: the stage's row, its budget in seconds
        # (None when it cannot be judged), the reason when it cannot, and the
        # kind of budget.
        self.stages = []
        for (label, model), history in self.series.items():
            for r in history:
                if r["day"] < since or (branch is not None and r["branch"] != branch) \
                        or (attempt is not None and r.get("attempt") != attempt):
                    continue
                if every:
                    start = days[(days.index(r["day"]) // every) * every]
                    cutoff = moment(start + "T00:00:00")
                else:
                    cutoff = moment(r["ts"])
                budget, why = self.budget(label, model, cutoff - datetime.timedelta(days=age))
                self.stages.append((r, budget, why))

    def earlier(self, label, model, cutoff):
        history = self.series.get((label, model), [])
        stamps = self.stamps.get((label, model), [])
        # Strictly before the cutoff, which a period's table puts at the
        # start of the period's first day.
        count = sum(1 for stamp in stamps if stamp < cutoff)
        return history[max(0, count - self.size):count]

    def budget(self, label, model, cutoff):
        used = self.earlier(label, model, cutoff)
        if len(used) >= MINIMUM_RUNS:
            return budget_for([h["cs"] for h in used], self.rule), None
        column = self.column_of[model]
        if column not in self.owner_columns:
            return None, f"{len(used)} earlier runs of its class, {MINIMUM_RUNS} needed"
        cells = self.table.cells(label)
        others = []
        for other_model, other_column in self.column_of.items():
            if other_model == model or cells.get(other_column, "-") == "-":
                continue
            window = self.earlier(label, other_model, cutoff)
            if len(window) >= MINIMUM_RUNS:
                others.append(budget_for([h["cs"] for h in window], self.rule))
        if not others:
            return None, "no class of its row has enough earlier runs"
        own = budget_for([h["cs"] for h in used], self.rule) if used else 0
        return max(max(others), own), None

    def jobs(self):
        """{job: [(row, budget, why)]} of the judged stages."""
        found = collections.defaultdict(list)
        for row, budget, why in self.stages:
            found[row["job"]].append((row, budget, why))
        return found


def replay(rows, table, since, size, every, slowdowns, keep=(), rule=None, branch=None, attempt=None,
           age=0, name=None):
    """Jobs over budget when the rule, recomputed every `every` days, judged them.

    A stage is judged by `Judgment`. A job is over when any of its judged
    stages is; a stage that cannot be judged counts for nothing here (`red_jobs`
    names them). Each slowdown scales the times of every stage of the job
    together. Returns {name: {slowdown: (over, jobs)}}.
    """
    judgment = Judgment(rows, table, since, size, every, keep, rule, branch, attempt, age)
    judged = collections.defaultdict(list)
    logged = collections.defaultdict(list)
    for row, budget, _ in judgment.stages:
        if budget is not None:
            judged[row["job"]].append((row["cs"], 100 * budget))
        if row["budget"] is not None:
            logged[row["job"]].append((row["cs"], 100 * row["budget"]))
    title = name or next((n for n, f in RULES.items() if f is (rule or RULES[DEFAULT_RULE])), "rule")
    when = f"recomputed every {every} day(s)" if every else "recomputed before each stage"
    if age:
        when += f", {age} day(s) old"
    result = {}
    for label, jobs in (("table logged when the job ran", logged),
                        (f"rule {title} over {size} runs, {when}", judged)):
        result[label] = {}
        for slowdown in slowdowns:
            over = sum(any(cs * (1 + slowdown) > limit for cs, limit in stages) for stages in jobs.values())
            result[label][slowdown] = (over, len(jobs))
    return result


def red_jobs(judgment):
    """Jobs that went red as they ran, each with what the rule says of it.

    A job went red as it ran when a stage of it took longer than the budget
    its log records. The rule clears it when every one of its judged stages is
    within the rule's budget and none of its red stages is unjudgeable; it
    leaves it red when a judged stage is over; otherwise it cannot judge it.
    Returns [(job, status, [one line per stage that is over a budget or cannot
    be judged])], status being "cleared", "still red" or "not judgeable".
    """
    found = []
    for job, stages in sorted(judgment.jobs().items()):
        red = [row for row, _, _ in stages if row["budget"] is not None and row["cs"] > 100 * row["budget"]]
        if not red:
            continue
        details, over, blind = [], False, False
        for row, budget, why in stages:
            logged_over = row["budget"] is not None and row["cs"] > 100 * row["budget"]
            if budget is None:
                if logged_over:
                    blind = True
                    details.append(f"{row['label']}: {row['cs'] / 100:.1f} s over its logged {row['budget']} s; "
                                   f"not judgeable: {why}")
            elif row["cs"] > 100 * budget:
                over = True
                details.append(f"{row['label']}: {row['cs'] / 100:.1f} s over the rule's {budget} s"
                               + (f" (logged {row['budget']} s)" if row["budget"] is not None else ""))
            elif logged_over:
                details.append(f"{row['label']}: {row['cs'] / 100:.1f} s over its logged {row['budget']} s, "
                               f"within the rule's {budget} s")
        first = stages[0][0]
        found.append((job, "still red" if over else "not judgeable" if blind else "cleared",
                      first["run"], first.get("attempt"), first["branch"], first["cpu"], details))
    return found


def table_budget(table, label, model):
    """What the table holds a stage to on a CPU model, in seconds, as run-check.pl reads it.

    The cell of the model's column; the largest cell of the Linux columns in
    the stage's row for a model with no column. None for a stage with no row
    and for a stage whose cell, or whose every Linux cell, is `-`.
    """
    if label not in table.rows:
        return None
    cells = table.cells(label)
    column = next((c for c, m in table.classes.items() if m == model), None)
    if column is not None:
        return None if cells.get(column, "-") == "-" else int(cells[column])
    linux = [int(cells[c]) for c in table.classes if cells.get(c, "-") != "-"]
    return max(linux) if linux else None


def against_table(rows, table, since, branch=None, attempt=None):
    """The jobs that went red as they ran, judged by the table's own budgets.

    A job went red when a stage of it took longer than the budget its log
    records. It is "within" the table when every red stage is within the
    table's budget for its model, "over" when one is over it and "not
    judgeable" when the table has none for a stage. Returns [(job, status,
    run, attempt, branch, cpu, [one line per red stage])], jobs finishing from
    the day `since` on, of `branch` and `attempt` where given.
    """
    jobs = collections.defaultdict(list)
    for r in rows:
        if r["day"] >= since and r["exit"] == 0 and (branch is None or r["branch"] == branch) \
                and (attempt is None or r.get("attempt") == attempt):
            jobs[r["job"]].append(r)
    found = []
    for job, stages in sorted(jobs.items()):
        red = [r for r in stages if r["budget"] is not None and r["cs"] > 100 * r["budget"]]
        if not red:
            continue
        details, over, blind = [], False, False
        for r in red:
            budget = table_budget(table, r["label"], r["cpu"])
            if budget is None:
                blind = True
                details.append(f"{r['label']}: {r['cs'] / 100:.1f} s over its logged {r['budget']} s; "
                               "the table has no budget for it")
            else:
                over = over or r["cs"] > 100 * budget
                details.append(f"{r['label']}: {r['cs'] / 100:.1f} s over its logged {r['budget']} s, "
                               f"{r['cs'] / budget:.0f}% of the table's {budget} s"
                               + (" (over it)" if r["cs"] > 100 * budget else ""))
        first = red[0]
        found.append((job, "over" if over else "not judgeable" if blind else "within",
                      first["run"], first.get("attempt"), first["branch"], first["cpu"], details))
    return found


def gh(*args, binary=False):
    """Output of `gh api` (bytes when `binary`); a server error (HTTP 5xx) is retried a few times."""
    for attempt in range(1, 5):
        done = subprocess.run(["gh", "api", "--allow-escape-sequences", *args], capture_output=True)
        if done.returncode == 0:
            return done.stdout if binary else done.stdout.decode("utf-8", "replace")
        error = done.stderr.decode("utf-8", "replace")
        if "HTTP 5" not in error or attempt == 4:
            raise SystemExit(f"gh api {' '.join(args)}: {error.strip()}")
        time.sleep(3 * attempt)


def attempt_logs(repo, run, attempt):
    """{job name: log text} of one run attempt, from the one archive GitHub keeps of its logs.

    The archive holds a file `<n>_<job name>.txt` for each job. One request
    gives every job of the attempt, where a log per job costs one request each.
    """
    with zipfile.ZipFile(io.BytesIO(gh(f"repos/{repo}/actions/runs/{run}/attempts/{attempt}/logs", binary=True))) as archive:
        found = {}
        for member in archive.namelist():
            match = re.fullmatch(r"\d+_(.*)\.txt", member)
            if match:
                found[match.group(1)] = archive.read(member).decode("utf-8", "replace")
        return found


def lines(*args):
    return [json.loads(line) for line in gh(*args).splitlines() if line.strip()]


def save(path, samples):
    with open(path + ".tmp", "w") as handle:
        json.dump(samples, handle)
    os.replace(path + ".tmp", path)


REQUESTS_PER_RUN = 10
RUNS_PER_CHECK = 10


def affordable(left, reserve):
    """How many more runs `collect` may fetch with `left` requests of the hour left, `reserve` kept back.

    A run costs, per attempt, a listing of its jobs and the archive of their
    logs (and a log of its own for a job the archive lacks): REQUESTS_PER_RUN is
    an upper estimate that covers a run re-run several times, so the reserve
    survives the runs fetched together with it.
    """
    return max(0, (left - reserve) // REQUESTS_PER_RUN)


def quota_left():
    """Requests the token may still make this hour (the `rate_limit` endpoint is free)."""
    return json.loads(gh("rate_limit"))["resources"]["core"]["remaining"]


def collect(repo, workflows, since, path, limit, until=None, branch=None, named=(), reserve=0):
    """Add the jobs of finished runs to the samples file; a rerun skips known jobs.

    `branch` keeps the runs of one branch; `named` keeps the jobs whose names
    start with one of the prefixes, which leaves the others unread (the
    archive of the attempt's logs is not fetched when none is kept). The runs
    are fetched RUNS_PER_CHECK at a time, and fetching stops before the
    requests left in the hour fall to `reserve`: the quota is shared with the
    engine's own calls, which fail once it is spent.
    """
    try:
        with open(path) as handle:
            samples = json.load(handle)
    except FileNotFoundError:
        samples = {"jobs": {}}
    def fetch(run, workflow):
        found = []
        for attempt in range(1, run["run_attempt"] + 1):
            jobs = lines("--paginate", f"repos/{repo}/actions/runs/{run['id']}/attempts/{attempt}/jobs",
                         "--jq", ".jobs[] | {id, name, conclusion, run_attempt} | tojson")
            wanted = [job for job in jobs
                      if job["conclusion"] in ("success", "failure") and str(job["id"]) not in samples["jobs"]
                      and (not named or job["name"].startswith(tuple(named)))]
            archive = attempt_logs(repo, run["id"], attempt) if wanted else {}
            for job in wanted:
                text = archive.get(job["name"])
                if text is None:
                    text = gh(f"repos/{repo}/actions/jobs/{job['id']}/logs")
                model, rows = parse_log(text)
                found.append((str(job["id"]), {"run": run["id"], "workflow": workflow, "sha": run["head_sha"],
                                               "branch": run["head_branch"], "attempt": job["run_attempt"],
                                               "cpu": model, "rows": rows}))
        return found
    for workflow in workflows:
        runs = lines("--method", "GET", "--paginate", f"repos/{repo}/actions/workflows/{workflow}/runs",
                     "-f", "per_page=100", "-f", "status=completed",
                     "-f", f"created={since}..{until}" if until else f"created=>={since}",
                     *(["-f", f"branch={branch}"] if branch else []),
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
        fetched = 0
        with concurrent.futures.ThreadPoolExecutor(6) as pool:
            while fetched < len(runs):
                left = quota_left()
                chunk = runs[fetched:fetched + min(RUNS_PER_CHECK, affordable(left, reserve))]
                if not chunk:
                    print(f"{workflow}: stopped with {left} requests left and {reserve} kept in reserve;"
                          f" {len(runs) - fetched} runs not fetched"
                          + (f"; go on from --until {runs[fetched - 1]['created_at']}" if fetched else ""),
                          file=sys.stderr)
                    break
                for found in pool.map(lambda run: fetch(run, workflow), chunk):
                    samples["jobs"].update(found)
                fetched += len(chunk)
                save(path, samples)
                print(f"{workflow}: {fetched} of {len(runs)} runs, {left} requests were left", file=sys.stderr, flush=True)
        print(f"{workflow}: {fetched} runs, {len(samples['jobs'])} jobs kept", file=sys.stderr)
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


def headroom(rows, table, labels, size, rules):
    """One record per stage and class with enough runs: how far above its median a run can grow.

    Returns [(label, column, runs, median s, max s, {rule: (budget s, headroom)})],
    the headroom being the budget over the median, less one.
    """
    found = []
    for label in labels:
        for column, model in table.classes.items():
            used = window_of(rows, label, model, size)
            if len(used) < MINIMUM_RUNS:
                continue
            times = [r["cs"] for r in used]
            median = statistics.median(times) / 100
            found.append((label, column, len(used), median, max(times) / 100,
                          {name: (budget_for(times, RULES[name]), budget_for(times, RULES[name]) / median - 1)
                           for name in rules}))
    return found


def print_headroom(found, rules):
    print("| stage | class | runs | median s | max s | " + " | ".join(f"{name} budget (headroom)" for name in rules) + " |")
    print("|---|---|---:|---:|---:|" + "---:|" * len(rules))
    for label, column, count, median, top, by_rule in found:
        print(f"| `{label}` | {column} | {count} | {median:.1f} | {top:.1f} | "
              + " | ".join(f"{by_rule[name][0]} (+{100 * by_rule[name][1]:.0f}%)" for name in rules) + " |")


def print_red_jobs(found, statuses=("cleared", "still red", "not judgeable")):
    print("jobs that went red as they ran (a stage over the budget its log records):")
    for job, status, run, attempt, branch, cpu, details in found:
        print(f"  {status}: job {job}, run {run}, attempt {attempt if attempt is not None else '?'}, "
              f"branch {branch}, {cpu}")
        for line in details:
            print(f"      {line}")
    counts = collections.Counter(status for _, status, *_ in found)
    print(f"  {len(found)} jobs: " + ", ".join(f"{counts[status]} {status}" for status in statuses))


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

    # The rules, exactly, on windows of stage times in hundredths of a second.
    # A window of equal times has no spread: 110.00 s is 110 s, not 115 s;
    # 59.95 rounds to 60 and 60.005 to 65; the floor is 10 s (a 1 s stage
    # would round to 5 s). The spread rule on one run is the former fixed 10%.
    singles = (32370, 10000, 5450, 5455, 100)
    assert [budget_for([m]) for m in singles] == [360, 110, 60, 65, 10]
    assert [RULES["max+10"]([m]) for m in singles] == [360, 110, 60, 65, 10]
    assert RULES["max+25"]([10000]) == 125 and RULES["max+30"]([10000]) == 130
    # The same median and the same 110 s for a window that never varies, but a
    # window with one 150 s run among twenty gets that run's spread on top:
    # 1.1 * 150 + (150 - 100) = 215 s. Order does not matter.
    assert budget_for([10000] * 20) == 110
    assert budget_for([10000] * 19 + [15000]) == 215 and budget_for([15000] + [10000] * 19) == 215
    # An even count's median is the mean of the middle two: 100.005 s here, so
    # 1.1 * 100.01 + 0.005 = 110.016 s, which rounds up to 115 s.
    assert budget_for([10000, 10001]) == 115
    assert RULES["max+30"]([10000] * 19 + [15000]) == 195

    # The columns the shipped tool treats as the owner's exist in the real
    # table; the synthetic table below names its thin column after the first,
    # and `recompute` runs with its default, so a changed or emptied constant
    # changes what is tested.
    assert OWNER_COLUMNS, "no owner column"
    with open(BUDGETS) as handle:
        shipped = Table(handle.read())
    assert set(OWNER_COLUMNS) <= set(shipped.classes), (OWNER_COLUMNS, list(shipped.classes))
    owner = OWNER_COLUMNS[0]
    table_text = "\n".join([
        "# comment", "cpu big-1  Big CPU", "cpu small-2  Small CPU", f"cpu {owner}  Thin CPU", "",
        f"label        big-1  small-2  {owner}  macos  windows",
        "stage/one    50     50       50      60     -",
        "stage/two    30     30       30      40     -",
        "stage/three  20     20       20      25     -",
        "stage/four   20     20       20      25     -",
        "stage/five   -      15       -       25     -",
        "stage/six    40     40       40      50     -",
        "stage/seven  60     60       60      70     -",
        "stage/eight  45     45       45      50     -",
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
        [("stage/one", "Big CPU", 9000 + 100 * i, 1 + i // 3, 0) for i in range(25)]      # max 114.00 s, median 102.00 s
        + [("stage/one", "Big CPU", 99900, 28, 1)]                                          # failed: not counted
        + [("stage/one", "Small CPU", 8000, 5, 0)] * 10                                     # 10 runs: too few
        + [("stage/one", "Thin CPU", 15000, 6, 0)]                                          # one run: 150 s
        + [("stage/two", "Big CPU", 5000, 7, 0)] * 25                                       # kept as is
        + [("stage/four", "Small CPU", 100, 8, 0)] * 25                                     # floor: 10 s
        + [("stage/six", "Big CPU", 4000 + 100 * i, 9 + i // 3, 0) for i in range(15)]    # 15 runs, logged twice
        + [("stage/seven", "Big CPU", 5000, 14, 0)] * 25                                    # 55 s
        + [("stage/seven", "Small CPU", 3000, 14, 0)] * 25                                  # 35 s
        + [("stage/eight", "Big CPU", 7000, 15, 0)] * 25                                    # 77 s -> 80
        + [("stage/eight", "Thin CPU", 4000, 15, 0)] * 3)                                   # 3 runs: 45 s by the rule
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
    report = recompute(table, rows, DEFAULT_WINDOW, keep=("stage/two",))
    cells = table.cells("stage/one")
    # 1.1 * 114.00 + (114.00 - 102.00) = 137.4 -> 140. The 10-run class keeps
    # its 50 s: ten runs set no value. The owner's column, with one run of
    # 150 s, takes the row's recomputed class (140) or its own 150 s -> 165 if
    # higher.
    assert (cells["big-1"], cells["small-2"], cells[owner]) == ("140", "50", "165"), cells
    assert table.cells("stage/two") == {"big-1": "30", "small-2": "30", owner: "30", "macos": "40", "windows": "-"}
    assert table.cells("stage/three") == {"big-1": "20", "small-2": "20", owner: "20", "macos": "25", "windows": "-"}
    # A one-second stage rounds to the 10 s floor; the class without a run keeps 20.
    assert table.cells("stage/four")["small-2"] == "10" and table.cells("stage/four")["big-1"] == "20"
    assert table.cells("stage/five") == {"big-1": "-", "small-2": "15", owner: "-", "macos": "25", "windows": "-"}
    assert table.cells("stage/one")["macos"] == "60" and table.cells("stage/one")["windows"] == "-"
    # Counted once, 15 runs are too few; counted twice they would be 30.
    assert table.cells("stage/six") == {"big-1": "40", "small-2": "40", owner: "40", "macos": "50", "windows": "-"}
    # Two classes have enough runs, 55 s and 35 s by the rule: the class
    # without runs takes the larger of them, not the smaller.
    assert table.cells("stage/seven") == {"big-1": "55", "small-2": "35", owner: "55", "macos": "70", "windows": "-"}
    # A class without a run keeps its value; the owner's column takes the row's
    # recomputed 80 s, not its own three runs' 45 s.
    assert table.cells("stage/eight") == {"big-1": "80", "small-2": "45", owner: "80", "macos": "50", "windows": "-"}
    out = table.text().split("\n")
    old = table_text.split("\n")
    assert out[:6] == old[:6] and out[7:9] == old[7:9] and out[10:12] == old[10:12] and out[14:] == old[14:]
    assert all(len(line.split()) == len(before.split()) for line, before in zip(out, old))
    again = Table(table.text())
    assert recompute(again, rows, DEFAULT_WINDOW, keep=("stage/two",)) and again.text() == table.text()
    assert any(entry[0] == "stage/one" and entry[1] == "big-1" and entry[2] == 25 and entry[6] == 140 for entry in report)

    # A recorded series that is stable: no job is over its recomputed budget
    # without a slowdown, every job is over it at +30%, and none at +5%, so
    # the replay can tell a slow stage from the margin's own noise.
    title = lambda every: f"rule spread over 20 runs, recomputed every {every} day(s)"
    stable = [("stage/one", "Big CPU", 10000 + 100 * (i % 3), 1 + i, 0) for i in range(30)]
    replayed = replay(flatten(sample(stable)), Table(table_text), "2026-10-21", 20, 1, (0, 0.05, 0.30))
    judged = replayed[title(1)]
    assert judged == {0: (0, 10), 0.05: (0, 10), 0.30: (10, 10)}, judged
    # The same series with its newest day 25% slower turns exactly that job red.
    slowed = stable[:-1] + [("stage/one", "Big CPU", 12600, 30, 0)]
    judged = replay(flatten(sample(slowed)), Table(table_text), "2026-10-21", 20, 1, (0,))
    assert judged[title(1)][0] == (1, 10), judged
    # A recompute every 3 days judges a stage on the runs before its period's
    # first day: day 21 belongs to the period of day 19, which has 18 runs
    # before it and is not judged, so 9 jobs remain.
    spaced = replay(flatten(sample(stable)), Table(table_text), "2026-10-21", 20, 3, (0, 0.30))
    assert spaced[title(3)] == {0: (0, 9), 0.30: (9, 9)}, spaced
    # Only days from --from are judged, and the budget a log recorded (120 s:
    # above the slowest stage's 102 s, below it at +30%) is judged as logged.
    late = replay(flatten(sample(stable, budget=120)), Table(table_text), "2026-10-25", 20, 1, (0, 0.30))
    assert late[title(1)] == {0: (0, 6), 0.30: (6, 6)}, late
    assert late["table logged when the job ran"] == {0: (0, 6), 0.30: (6, 6)}, late

    # A noisy class (alternately 100 s and 140 s) and a quiet one (always 120 s)
    # have the same median. The spread rule gives the noisy one 1.1 * 140 + 20
    # = 174 -> 175 s and the quiet one 1.1 * 120 = 132 -> 135 s, where a fixed
    # +30% gives 185 s and 160 s. Ten days are judged in each class, one job a
    # day. At +25% every stage of the noisy class stays within its budget and
    # every stage of the quiet one goes over; at +50% the 140 s runs of the
    # noisy class (five of ten) and all of the quiet one's are over. The fixed
    # margin sees nothing at +25%, which is what the spread rule is for.
    mixed = sample([("stage/one", "Big CPU", 10000 if i % 2 == 0 else 14000, 1 + i, 0) for i in range(30)]
                   + [("stage/one", "Small CPU", 12000, 1 + i, 0) for i in range(30)])
    for job in mixed["jobs"].values():
        job["branch"], job["attempt"] = ("feature", 2) if job["cpu"] == "Big CPU" else ("main", 1)
    mixed_rows = flatten(mixed)
    steps = (0, 0.25, 0.5)
    both = replay(mixed_rows, Table(table_text), "2026-10-21", 20, 1, steps)[title(1)]
    assert both == {0: (0, 20), 0.25: (10, 20), 0.5: (15, 20)}, both
    fixed = replay(mixed_rows, Table(table_text), "2026-10-21", 20, 1, steps, rule=RULES["max+30"], name="max+30")
    assert fixed["rule max+30 over 20 runs, recomputed every 1 day(s)"] == {0: (0, 20), 0.25: (0, 20), 0.5: (15, 20)}, fixed
    # A branch or an attempt narrows the jobs judged, not the runs the rule reads.
    only_noisy = replay(mixed_rows, Table(table_text), "2026-10-21", 20, 1, steps, branch="feature")[title(1)]
    only_quiet = replay(mixed_rows, Table(table_text), "2026-10-21", 20, 1, steps, attempt=1)[title(1)]
    assert only_noisy == {0: (0, 10), 0.25: (0, 10), 0.5: (5, 10)}, only_noisy
    assert only_quiet == {0: (0, 10), 0.25: (10, 10), 0.5: (10, 10)}, only_quiet
    # Recomputing before each stage reads the same runs here; a table seven days
    # old leaves the stages of days 27 to 30 judgeable, 4 jobs of each class.
    before_each = replay(mixed_rows, Table(table_text), "2026-10-21", 20, 0, steps)
    assert list(before_each.values())[1] == both, before_each
    old_table = replay(mixed_rows, Table(table_text), "2026-10-21", 20, 0, (0,), age=7)
    assert list(old_table.values())[1] == {0: (0, 8)}, old_table

    # The headroom of a typical run before the stage trips: the spread rule's
    # 175 s over the noisy class's 120 s median is +46% and the quiet class's
    # 135 s is +12.5%; +30% of the maximum gives 185 s and 160 s.
    found = headroom(mixed_rows, Table(table_text), ["stage/one"], 20, ["spread", "max+30"])
    assert [(entry[1], entry[2], entry[3], entry[4]) for entry in found] == [("big-1", 20, 120.0, 140.0),
                                                                             ("small-2", 20, 120.0, 120.0)], found
    assert [(by["spread"][0], by["max+30"][0]) for *_, by in found] == [(175, 185), (135, 160)], found
    assert abs(found[0][5]["spread"][1] - 175 / 120 + 1) < 1e-9 and abs(found[1][5]["spread"][1] - 0.125) < 1e-9

    # A stage of a class with too few earlier runs is not judged, unless the
    # class has an owner column: that takes the larger of the other classes'
    # budgets (110 s, from the 25 runs of the big class) and its own earlier
    # runs' budget (150 s once two runs of 100 s and 125 s are behind it). A
    # row in which no class has 20 earlier runs cannot be judged at all.
    owner_series = sample([("stage/two", "Big CPU", 10000, 1 + i, 0) for i in range(25)]
                          + [("stage/two", "Thin CPU", 10000, 26, 0), ("stage/two", "Thin CPU", 12500, 27, 0),
                             ("stage/two", "Thin CPU", 10000, 28, 0), ("stage/two", "Small CPU", 10000, 26, 0),
                             ("stage/three", "Thin CPU", 10000, 26, 0)])
    verdicts = {(r["label"], r["cpu"], r["day"]): (budget, why)
                for r, budget, why in Judgment(flatten(owner_series), Table(table_text), "2026-10-26", 20, 1).stages}
    assert verdicts == {
        ("stage/two", "Thin CPU", "2026-10-26"): (110, None),
        ("stage/two", "Thin CPU", "2026-10-27"): (110, None),
        ("stage/two", "Thin CPU", "2026-10-28"): (150, None),
        ("stage/two", "Small CPU", "2026-10-26"): (None, "0 earlier runs of its class, 20 needed"),
        ("stage/three", "Thin CPU", "2026-10-26"): (None, "no class of its row has enough earlier runs"),
    }, verdicts
    # With two other classes that have enough runs, 55 s (50 s) and 35 s (30 s),
    # the owner column takes the larger. A run that finishes exactly at the
    # cutoff is not among the runs before it: the fifth run of the big class,
    # at 00:00:04 of the fifth day, has four earlier runs.
    two_classes = sample([("stage/seven", "Big CPU", 5000, 1 + i, 0) for i in range(25)]
                         + [("stage/seven", "Small CPU", 3000, 1 + i, 0) for i in range(25)]
                         + [("stage/seven", "Thin CPU", 4000, 26, 0)])
    seen = Judgment(flatten(two_classes), Table(table_text), "2026-10-26", 20, 1)
    assert [(r["cpu"], budget, why) for r, budget, why in seen.stages] == [("Thin CPU", 55, None)], seen.stages
    fifth = next(r for r in flatten(two_classes) if r["cpu"] == "Big CPU" and r["day"] == "2026-10-05")
    assert len(seen.earlier("stage/seven", "Big CPU", moment(fifth["ts"]))) == 4, fifth

    # The jobs that went red as they ran (a stage over the budget its log
    # recorded, 110 s here): a 112 s run is cleared, the rule's budget being
    # 115 s; a 126 s run is still red; a run of a class with no history is not
    # judgeable.
    def reds(last, model="Big CPU"):
        series = stable[:-1] + [("stage/one", model, last, 30, 0)] if model == "Big CPU" else [("stage/one", model, last, 30, 0)]
        return red_jobs(Judgment(flatten(sample(series, budget=110)), Table(table_text), "2026-10-30", 20, 1))
    assert [status for _, status, *_ in reds(11200)] == ["cleared"], reds(11200)
    assert [status for _, status, *_ in reds(12600)] == ["still red"], reds(12600)
    assert [status for _, status, *_ in reds(11200, "Small CPU")] == ["not judgeable"], reds(11200, "Small CPU")
    assert reds(10100) == [], "a stage within its logged budget is not a red job"
    # Judging the red jobs by the table's own budgets. Every stage below was
    # logged against 40 s. The table gives `stage/a` 50 s on the big class and
    # 70 s on the small one, so an unmapped model is held to 70 s, the largest;
    # `stage/b` has only the small class's 15 s, so the big class has no
    # budget for it and cannot be judged. A stage exactly at its budget is
    # within it, a stage within its logged budget is no red job, and the day,
    # branch and attempt narrow the jobs judged.
    against_text = "\n".join([
        "cpu big-1  Big CPU", "cpu small-2  Small CPU", "",
        "label    big-1  small-2  macos  windows",
        "stage/a  50     70       100    -",
        "stage/b  -      15       100    -",
        ""])
    against_rows = [("stage/a", "Big CPU", 5000, 20, 0),      # 0: exactly its 50 s: within
                    ("stage/a", "Big CPU", 5001, 20, 0),      # 1: over
                    ("stage/a", "Small CPU", 6900, 20, 0),    # 2: within its 70 s
                    ("stage/a", "Small CPU", 7001, 20, 0),    # 3: over
                    ("stage/a", "Odd CPU", 7000, 20, 0),      # 4: unmapped, held to the largest, 70 s: within
                    ("stage/a", "Odd CPU", 7001, 20, 0),      # 5: over it
                    ("stage/b", "Big CPU", 4500, 20, 0),      # 6: no budget for it: not judgeable
                    ("stage/a", "Big CPU", 3999, 20, 0),      # 7: within its logged 40 s: no red job
                    ("stage/a", "Small CPU", 9000, 20, 1),    # 8: a failed stage is not judged
                    ("stage/a", "Small CPU", 7001, 19, 0),    # 9: before the day judged
                    ("stage/a", "Small CPU", 7001, 21, 0),    # 10: another branch, and a rerun
                    ("stage/a", "Big CPU", 4000, 20, 0)]      # 11: exactly its logged 40 s: no red job
    against_data = sample(against_rows, budget=40)
    against_data["jobs"]["10"].update(branch="other", attempt=2)
    # A second red stage with no budget in the table (the big class has none for
    # `stage/b`): it leaves an over job over and makes a within job not judgeable.
    for job in ("0", "1"):
        against_data["jobs"][job]["rows"].append({"label": "stage/b", "cs": 4500, "exit": 0, "budget": 40,
                                                  "ts": "2026-10-20T00:30:00.0000000Z"})
    against_found = against_table(flatten(against_data), Table(against_text), "2026-10-20")
    judged = {job: status for job, status, *_ in against_found}
    assert judged == {"0": "not judgeable", "1": "over", "2": "within", "3": "over", "4": "within", "5": "over",
                      "6": "not judgeable", "10": "over"}, judged
    assert dict((job, details) for job, *_, details in against_found)["5"] == [
        "stage/a: 70.0 s over its logged 40 s, 100% of the table's 70 s (over it)"], against_found
    assert dict((job, details) for job, *_, details in against_found)["4"] == [
        "stage/a: 70.0 s over its logged 40 s, 100% of the table's 70 s"], against_found
    narrowed = against_table(flatten(against_data), Table(against_text), "2026-10-20", "other", 2)
    assert [(job, status) for job, status, *_ in narrowed] == [("10", "over")], narrowed
    assert against_table(flatten(against_data), Table(against_text), "2026-10-20", "main", 2) == []
    assert [table_budget(Table(against_text), "stage/a", model) for model in ("Big CPU", "Small CPU", "Odd CPU")] == [50, 70, 70]
    assert [table_budget(Table(against_text), label, "Big CPU") for label in ("stage/b", "stage/none")] == [None, None]
    assert table_budget(Table(against_text), "stage/b", "Odd CPU") == 15
    # `collect` leaves the reserve of the hour's requests to others: at 10
    # requests a run, 1,000 left with 920 kept back is 8 runs, and nothing
    # is fetched at or below the reserve.
    assert [affordable(left, 920) for left in (1000, 999, 929, 920, 100, 0)] == [8, 7, 0, 0, 0, 0]
    assert affordable(5000, 1000) == 400
    # The same through `collect` with a stubbed `gh`: 25 runs listed, 60
    # requests left at the start and 20 kept in reserve; every request lowers
    # the count, and fetching stops with the reserve intact and runs unfetched.
    # A run attempt's jobs come from one archive of its logs (a job, a
    # cancelled job that is not read, and a job whose name the archive does not
    # hold, which is read from its own log): three requests besides the listing
    # of the runs, 1 for the jobs, 1 for the archive and 1 for the odd job's log.
    left, requests = [60], []
    runs = [{"id": 100 + i, "head_sha": "s", "head_branch": "main", "run_attempt": 1, "conclusion": "success",
             "created_at": f"2026-10-20T00:{59 - i:02d}:00Z"} for i in range(25)]
    archive = io.BytesIO()
    with zipfile.ZipFile(archive, "w") as made:
        made.writestr("1_unit (x).txt", stamp % 1 + " model name\t: Archive CPU\n" + stamp % 2 + " == END stage/one: 1.00 s, exit 0 ==\n")
        made.writestr("unit (x)/system.txt", "not a job's log")
        made.writestr("2_unit (z).txt", stamp % 1 + " model name\t: Cancelled CPU\n")
    def stub(*args, binary=False):
        request = " ".join(args)
        if request == "rate_limit":
            return json.dumps({"resources": {"core": {"remaining": left[0]}}})
        left[0] -= 1
        requests.append(request)
        if "/workflows/" in request:
            return "\n".join(json.dumps(run) for run in runs)
        if request.endswith("/attempts/1/logs"):
            assert binary
            return archive.getvalue()
        if "/attempts/1/jobs" in request:
            n = len(requests)
            return "\n".join(json.dumps(job) for job in (
                {"id": 10 * n, "name": "unit (x)", "conclusion": "success", "run_attempt": 1},
                {"id": 10 * n + 1, "name": "unit (z)", "conclusion": "cancelled", "run_attempt": 1},
                {"id": 10 * n + 2, "name": "odd/name (y)", "conclusion": "failure", "run_attempt": 1}))
        assert request.endswith("/logs") and "/jobs/" in request, request
        return stamp % 1 + " model name\t: Own-log CPU\n" + stamp % 2 + " == END stage/one: 2.00 s, exit 0 ==\n"
    real_gh = gh
    try:
        globals()["gh"] = stub
        with tempfile.TemporaryDirectory() as scratch, contextlib.redirect_stderr(io.StringIO()):
            collect("o/r", ["gate.yml"], "2026-10-20", os.path.join(scratch, "samples.json"), 25, reserve=20)
            kept = json.load(open(os.path.join(scratch, "samples.json")))["jobs"]
    finally:
        globals()["gh"] = real_gh
    fetched_runs = len({job["run"] for job in kept.values()})
    assert 20 <= left[0] < 20 + REQUESTS_PER_RUN and 0 < fetched_runs < 25, (left, kept)
    assert len(kept) == 2 * fetched_runs and sorted({job["cpu"] for job in kept.values()}) == ["Archive CPU", "Own-log CPU"], kept
    assert sum(r.endswith("/logs") and "/jobs/" in r for r in requests) == fetched_runs, requests
    assert sum(r.endswith("/attempts/1/logs") for r in requests) == fetched_runs and 1 + 3 * fetched_runs == len(requests), requests
    # An attempt with no job worth reading, here one cancelled job, costs only
    # the listing of its jobs.
    requests.clear()
    runs[:] = runs[:1]
    def only_cancelled(*args, binary=False):
        if args == ("rate_limit",):
            return json.dumps({"resources": {"core": {"remaining": 100}}})
        requests.append(" ".join(args))
        if "/workflows/" in requests[-1]:
            return json.dumps(runs[0])
        return json.dumps({"id": 7, "name": "unit (z)", "conclusion": "cancelled", "run_attempt": 1})
    try:
        globals()["gh"] = only_cancelled
        with tempfile.TemporaryDirectory() as scratch, contextlib.redirect_stderr(io.StringIO()):
            collect("o/r", ["gate.yml"], "2026-10-20", os.path.join(scratch, "samples.json"), 25, reserve=0)
    finally:
        globals()["gh"] = real_gh
    assert not any(r.endswith("/logs") for r in requests), requests
    print("recompute-time-budgets self-test passed")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--self-test", action="store_true")
    sub = parser.add_subparsers(dest="command")
    fetch = sub.add_parser("collect")
    fetch.add_argument("--repo", required=True)
    fetch.add_argument("--workflow", action="append", required=True)
    fetch.add_argument("--since", required=True, help="YYYY-MM-DD or YYYY-MM-DDTHH:MM:SSZ")
    fetch.add_argument("--until", help="newest run created, same forms; default now")
    fetch.add_argument("--limit", type=int, default=400, help="newest runs per workflow")
    fetch.add_argument("--branch", help="keep the runs of this branch only")
    fetch.add_argument("--job", action="append", default=[], help="keep the jobs whose names start with this")
    fetch.add_argument("--reserve", type=int, default=1000,
                       help="requests of the hour's 5,000 to leave for others (default 1000); fetching"
                            " stops before the quota falls to it")
    fetch.add_argument("--samples", required=True)
    judge = sub.add_parser("against")
    judge.add_argument("--samples", required=True, nargs="+", help="samples files from collect")
    judge.add_argument("--budgets", default=BUDGETS)
    judge.add_argument("--from", dest="since", required=True, help="first day judged, YYYY-MM-DD")
    judge.add_argument("--branch", help="judge the jobs of this branch only")
    judge.add_argument("--attempt", type=int, help="judge the jobs of this run attempt only")
    for name in ("table", "headroom", "replay"):
        command = sub.add_parser(name)
        command.add_argument("--samples", required=True, nargs="+", help="samples files from collect")
        command.add_argument("--budgets", default=BUDGETS)
        command.add_argument("--window", type=int, default=DEFAULT_WINDOW)
        command.add_argument("--keep", action="append", default=[], help="a stage to leave untouched")
        if name == "headroom":
            command.add_argument("--label", action="append", required=True, help="a stage")
            command.add_argument("--rule", action="append", choices=sorted(RULES), help="default: every rule")
            continue
        command.add_argument("--rule", choices=sorted(RULES), default=DEFAULT_RULE)
        if name == "table":
            command.add_argument("--write", action="store_true")
        else:
            command.add_argument("--from", dest="since", required=True, help="first day judged, YYYY-MM-DD")
            command.add_argument("--every", type=int, default=1,
                                 help="days with runs between recomputes; 0 recomputes before each stage")
            command.add_argument("--age", type=int, default=0, help="days the table is older than the stage it judges")
            command.add_argument("--branch", help="judge the jobs of this branch only")
            command.add_argument("--attempt", type=int, help="judge the jobs of this run attempt only")
            command.add_argument("--list", action="store_true", help="name the jobs that went red as they ran")
            command.add_argument("--slowdown", type=float, action="append", help="default 0 0.1 0.2 0.3")
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if args.command == "collect":
        return collect(args.repo, args.workflow, args.since, args.samples, args.limit, args.until,
                       args.branch, tuple(args.job), args.reserve)
    if args.command is None:
        parser.error("name a command or --self-test")
    rows = flatten(load(args.samples))
    with open(args.budgets) as handle:
        table = Table(handle.read())
    if args.command == "against":
        print_red_jobs(against_table(rows, table, args.since, args.branch, args.attempt),
                       ("within", "over", "not judgeable"))
        return
    if args.command == "headroom":
        rules = args.rule or list(RULES)
        print_headroom(headroom(rows, table, args.label, args.window, rules), rules)
        return
    rule = RULES[args.rule]
    if args.command == "table":
        print_report(recompute(table, rows, args.window, tuple(args.keep), OWNER_COLUMNS, rule))
        if args.write:
            with open(args.budgets, "w") as handle:
                handle.write(table.text())
        return
    keep = tuple(args.keep)
    slowdowns = sorted(set(args.slowdown or [0.0, 0.1, 0.2, 0.3]))
    for name, shares in replay(rows, table, args.since, args.window, args.every, slowdowns, keep, rule,
                               args.branch, args.attempt, args.age, args.rule).items():
        print(name)
        for slowdown, (over, jobs) in shares.items():
            print(f"  +{slowdown * 100:.0f}% slower: {over} of {jobs} jobs over budget ({100 * over / max(jobs, 1):.1f}%)")
    if args.list:
        print_red_jobs(red_jobs(Judgment(rows, table, args.since, args.window, args.every, keep, rule,
                                         args.branch, args.attempt, args.age)))


if __name__ == "__main__":
    main()
