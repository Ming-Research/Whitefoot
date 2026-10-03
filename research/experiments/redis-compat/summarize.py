#!/usr/bin/env python3
"""Summarizes a run directory that run.sh filled.

Reads the output of every unit's last attempt and writes into the run
directory tests.tsv, one row per test with its outcome and the cause of a
failure, and units.tsv, the counts per unit; then prints a report.

usage: summarize.py RUN_DIR [--reference TESTS_TSV]

With --reference, a test that the reference run's tests.tsv reports and this
run does not is counted as not reached and listed so in tests.tsv. README.md
defines every outcome and cause.
"""

import argparse
import collections
import re
import sys
from pathlib import Path

STATUS_RE = re.compile(r"^\[(ok|err|skip|ignore|exception|TIMEOUT)\]: ?(.*)$")
DONE_RE = re.compile(r"^\[\d+/\d+ done\]: ")
OTHER_RE = re.compile(r"^(Testing |\[ready\]: |Cleanup: |Starting test server )")
END_RE = re.compile(r"^\s+The End$")
ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")
OK_RE = re.compile(r"^(.*) \(\d+ ms\)$")
TEST_RE = re.compile(r"^(.*) in (tests/\S+\.tcl)$")
TRACE_RE = re.compile(r"^    (while executing|invoked from within)$")
STATE_RE = re.compile(r"^sock\S* => (.*)$")
REASONS = (
    r"(Not supported on external server|Not supported on singledb"
    r"|Not supported in cluster mode|Not supported in tls mode"
    r"|large memory flag not provided|Tag: \S+ denied"
    r"|Tag: none of the tags allowed)"
)
BLOCK_IGNORE_RE = re.compile("^" + REASONS + "$")
TEST_IGNORE_RE = re.compile("^(.*): " + REASONS + "$")
UNKNOWN_COMMAND_RE = re.compile(r"unknown command '([^']*)'")
UNKNOWN_SUBCOMMAND_RE = re.compile(r"unknown subcommand '([^']*)'(?:\. Try (\S+) HELP)?")
CONNECTION_RE = re.compile(
    r"I/O error reading reply|connection reset|broken pipe|connection refused"
    r'|error (?:reading|writing) "sock|couldn.t open socket',
    re.IGNORECASE,
)
PROTOCOL_RE = re.compile(r"Bad protocol")
REPLY_RE = re.compile(r"^[A-Z][A-Z_]+(?: |$)")

TEST_OUTCOMES = ["passed", "failed", "errored", "skipped", "timed-out", "not-reached"]
EVENT_OUTCOMES = ["skipped-block", "block-error", "exception", "stalled"]
NOT_PASSING = ["failed", "errored", "timed-out", "block-error", "exception", "stalled"]
UNIT_COLUMNS = [
    "unit", "passed", "failed", "errored", "skipped", "timed_out", "not_reached",
    "reference_passed", "skipped_blocks", "block_errors", "ended", "server",
]


def clean(text, limit=120):
    """One printable line of at most limit characters, without surrounding
    whitespace: a few test names end with a space."""
    text = "".join(c if c.isprintable() else "?" for c in text.replace("\t", " ")).strip()
    return text if len(text) <= limit else text[: limit - 3] + "..."


def classify(message, assertion):
    """The cause and its detail for the message of a test that did not pass."""
    line = message.split("\n", 1)[0]
    command = UNKNOWN_COMMAND_RE.search(message)
    if command:
        return "unknown-command", command.group(1).lower()
    sub = UNKNOWN_SUBCOMMAND_RE.search(message)
    if sub:
        name = sub.group(1).lower()
        return "unknown-subcommand", f"{sub.group(2).lower()} {name}" if sub.group(2) else name
    if CONNECTION_RE.search(message):
        return "connection", clean(line)
    if PROTOCOL_RE.search(message):
        return "protocol-error", clean(line)
    if assertion:
        return "wrong-reply", clean(line)
    if REPLY_RE.match(message):
        return "error-reply", clean(line)
    return "other", clean(line)


def error_message(lines):
    """The whole error of a record whose first line is the error without its
    first ten characters, followed by the error's errorInfo."""
    if not lines:
        return ""
    for line in lines[1:]:
        if len(line) >= 10 and line[10:] == lines[0]:
            return line
    return lines[1] if len(lines) > 1 else lines[0]


def read_records(path, prefixes):
    """The status records of one runtest output, each a kind and its lines.
    Every one of prefixes, the run directory's absolute paths, becomes <run>:
    the suite's assertion messages name the files of its copy there."""
    records = []
    current = None
    with open(path, encoding="utf-8", errors="replace") as log:
        for raw in log:
            line = ANSI_RE.sub("", raw.rstrip("\r\n"))
            for prefix in prefixes:
                line = line.replace(prefix, "<run>")
            if END_RE.match(line):
                break
            match = STATUS_RE.match(line)
            if match:
                current = (match.group(1), [match.group(2)])
                records.append(current)
            elif DONE_RE.match(line):
                current = None
                records.append(("done", [line]))
            elif OTHER_RE.match(line):
                current = None
            elif current is not None:
                current[1].append(line)
    return records


def unit_rows(unit, records, hung):
    """The rows (outcome, cause, detail, test) of one unit's last attempt.
    Test names lose surrounding whitespace, as clean() writes them."""
    rows = []
    hung_left = collections.Counter(name.strip() for name in hung)
    for kind, lines in records:
        head = lines[0]
        if kind == "ok":
            match = OK_RE.match(head)
            rows.append(("passed", "", "", (match.group(1) if match else head).strip()))
        elif kind == "skip":
            name = head.strip()
            if hung_left[name] > 0:
                hung_left[name] -= 1
                rows.append(("timed-out", "hang", "timed out in an earlier attempt", name))
            else:
                rows.append(("skipped", "", "by name", name))
        elif kind == "ignore":
            block = BLOCK_IGNORE_RE.match(head)
            test = TEST_IGNORE_RE.match(head)
            if block:
                rows.append(("skipped-block", "", block.group(1), "-"))
            elif test:
                rows.append(("skipped", "", test.group(2), test.group(1).strip()))
            else:
                rows.append(("skipped", "", "", head.strip()))
        elif kind == "err":
            test = TEST_RE.match(head)
            if test:
                body = lines[1:]
                name = test.group(1).strip()
                if any(TRACE_RE.match(line) for line in body):
                    cause, detail = classify(error_message(body), assertion=False)
                    rows.append(("errored", cause, detail, name))
                else:
                    cause, detail = classify("\n".join(body), assertion=True)
                    rows.append(("failed", cause, detail, name))
            else:
                cause, detail = classify(error_message(lines), assertion=False)
                rows.append(("block-error", cause, detail, "-"))
        elif kind == "exception":
            message = lines[1] if len(lines) > 1 else head
            cause, detail = classify(message, assertion=False)
            rows.append(("exception", cause, detail, "-"))
        elif kind == "TIMEOUT":
            for line in lines[1:]:
                state = STATE_RE.match(line)
                if not state:
                    continue
                if state.group(1).startswith("(IN PROGRESS) "):
                    name = state.group(1)[len("(IN PROGRESS) "):].strip()
                    rows.append(("timed-out", "hang", "", name))
                else:
                    rows.append(("stalled", "hang", clean("outside a test: " + state.group(1)), "-"))
    for name, count in hung_left.items():
        rows.extend([("timed-out", "hang", "timed out in an earlier attempt", name)] * count)
    return rows


def ended(records, exit_code):
    kinds = {kind for kind, _ in records}
    if "exception" in kinds:
        return "exception"
    if "TIMEOUT" in kinds:
        return "timeout"
    if "done" in kinds:
        return "done"
    if exit_code in ("124", "137"):
        return "limit"
    return "incomplete"


def read_attempts(run):
    """Per unit, in run order: its attempts as (attempt, exit code, server)."""
    attempts = collections.OrderedDict()
    with open(run / "attempts.tsv", encoding="utf-8") as table:
        next(table)
        for line in table:
            unit, attempt, code, server = line.rstrip("\n").split("\t")
            attempts.setdefault(unit, []).append((attempt, code, server))
    return attempts


def read_reference(path):
    """Per unit, the tests a reference tests.tsv reports, in order, each as
    its name and outcome."""
    tests = collections.defaultdict(list)
    with open(path, encoding="utf-8") as table:
        header = next(table).rstrip("\n").split("\t")
        for line in table:
            row = dict(zip(header, line.rstrip("\n").split("\t")))
            if row["test"] != "-" and row["outcome"] != "not-reached":
                tests[row["unit"]].append((row["test"], row["outcome"]))
    return tests


def table(headers, rows):
    widths = [max(len(str(x)) for x in column) for column in zip(headers, *rows)]
    lines = ["  ".join(str(h).ljust(w) for h, w in zip(headers, widths)).rstrip()]
    for row in rows:
        cells = []
        for value, width in zip(row, widths):
            text = str(value)
            cells.append(text.rjust(width) if isinstance(value, int) else text.ljust(width))
        lines.append("  ".join(cells).rstrip())
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("run", type=Path)
    parser.add_argument("--reference", type=Path)
    args = parser.parse_args()
    run = args.run
    reference = read_reference(args.reference) if args.reference else None
    prefixes = sorted({str(run.absolute()), str(run.resolve())}, key=len, reverse=True)

    all_rows = []
    units = []
    unmatched = 0
    reference_passed = 0
    passed_of_reference = 0
    for unit, attempts in read_attempts(run).items():
        last, code, _ = attempts[-1]
        records = read_records(run / "logs" / f"{unit}.{last}.log", prefixes)
        hung_file = run / "hung" / f"{unit}.txt"
        hung = [n for n in hung_file.read_text(encoding="utf-8").split("\n") if n] if hung_file.exists() else []
        rows = unit_rows(unit, records, hung)
        unit_reference_passed = "-"
        if reference is not None:
            seen = collections.Counter(test for _, _, _, test in rows if test != "-")
            passed = collections.Counter(test for outcome, _, _, test in rows if outcome == "passed")
            unit_reference_passed = 0
            for name, outcome in reference.get(unit, []):
                if seen[name] > 0:
                    seen[name] -= 1
                else:
                    rows.append(("not-reached", "", "", name))
                if outcome == "passed":
                    unit_reference_passed += 1
                    if passed[name] > 0:
                        passed[name] -= 1
                        passed_of_reference += 1
            unmatched += sum(seen.values())
            reference_passed += unit_reference_passed
        counts = collections.Counter(outcome for outcome, _, _, _ in rows)
        servers = [f"{s} in attempt {a}" for a, _, s in attempts if s != "running"]
        units.append([
            unit, counts["passed"], counts["failed"], counts["errored"], counts["skipped"],
            counts["timed-out"], counts["not-reached"] if reference is not None else "-",
            unit_reference_passed, counts["skipped-block"], counts["block-error"],
            ended(records, code), "; ".join(servers) if servers else "running",
        ])
        all_rows.extend((unit,) + row for row in rows)

    with open(run / "tests.tsv", "w", encoding="utf-8") as out:
        out.write("unit\toutcome\tcause\tdetail\ttest\n")
        for row in all_rows:
            out.write("\t".join(clean(str(x), limit=400) for x in row) + "\n")
    with open(run / "units.tsv", "w", encoding="utf-8") as out:
        out.write("\t".join(UNIT_COLUMNS) + "\n")
        for row in units:
            out.write("\t".join(str(x) for x in row) + "\n")

    meta = run / "meta.txt"
    if meta.exists():
        print(meta.read_text(encoding="utf-8").rstrip())
        print()
    totals = collections.Counter(row[1] for row in all_rows)
    print("Totals")
    print(table(["outcome", "count"], [[o, totals[o]] for o in TEST_OUTCOMES + EVENT_OUTCOMES]))
    if reference is not None:
        share = 100.0 * passed_of_reference / reference_passed if reference_passed else 0.0
        print(f"of the {reference_passed} tests the reference passes, this run passes "
              f"{passed_of_reference} ({share:.1f}%)")
        print(f"tests this run reports that the reference does not: {unmatched}")
    ends = collections.Counter(row[10] for row in units)
    print("units by how they ended: " + ", ".join(f"{k} {v}" for k, v in sorted(ends.items())))
    print()
    print("Units")
    print(table(UNIT_COLUMNS, units))
    print()

    reasons = collections.Counter((row[1], row[3]) for row in all_rows if row[1] in ("skipped", "skipped-block"))
    if reasons:
        print("Skipped, by reason")
        print(table(["outcome", "reason", "count"], [[o, r, n] for (o, r), n in reasons.most_common()]))
        print()

    failing = [row for row in all_rows if row[1] in NOT_PASSING]
    print("Causes")
    causes = collections.Counter((row[2], row[1]) for row in failing)
    cause_names = sorted({c for c, _ in causes}, key=lambda c: -sum(v for (k, _), v in causes.items() if k == c))
    print(table(
        ["cause"] + NOT_PASSING + ["total"],
        [[c] + [causes[(c, o)] for o in NOT_PASSING] + [sum(causes[(c, o)] for o in NOT_PASSING)] for c in cause_names],
    ))
    print()

    commands = collections.defaultdict(lambda: collections.Counter())
    command_units = collections.defaultdict(set)
    for unit, outcome, cause, detail, _ in failing:
        if cause == "unknown-command":
            commands[detail][outcome] += 1
            command_units[detail].add(unit)
    if commands:
        print("Unknown commands (units stopped: exceptions; blocks stopped: block errors)")
        ordered = sorted(commands, key=lambda c: (-commands[c]["exception"], -commands[c]["block-error"], -sum(commands[c].values()), c))
        print(table(
            ["command", "units stopped", "blocks stopped", "tests failed", "tests errored", "units"],
            [[c, commands[c]["exception"], commands[c]["block-error"], commands[c]["failed"],
              commands[c]["errored"], len(command_units[c])] for c in ordered],
        ))
        print()

    for cause in ("error-reply", "wrong-reply", "unknown-subcommand", "connection", "protocol-error", "other"):
        details = collections.Counter(row[3] for row in failing if row[2] == cause)
        if details:
            print(f"Most frequent details: {cause}")
            print(table(["count", "detail"], [[n, d] for d, n in details.most_common(15)]))
            print()

    if len(failing) <= 150:
        print("Every test or event that did not pass")
        print(table(["unit", "outcome", "cause", "detail", "test"], failing) if failing else "none")
    else:
        print(f"{len(failing)} tests or events did not pass; tests.tsv lists them")
    return 0


if __name__ == "__main__":
    sys.exit(main())
