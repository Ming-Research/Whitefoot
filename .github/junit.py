#!/usr/bin/env python3
"""Write the JUnit XML that the wf-agents engine reads from a run it asked for.

The engine (wf-agents.toml at the root) runs the fast workflow and the full
workflows by hand and reads what each test became from JUnit XML files in the
artifact `test-results`, never from a log. The compiler's tests run under
libtest, as `cargo test` runs them in the gate, and stable libtest writes no
JUnit; this script reads what libtest and cargo print and writes it.

  junit.py job --job NAME --out FILE [--log FILE]... [--stamped]
               [--tests-step NAME] [--step OUTCOME NAME]...
               [--named FILE] [--listed FILE] [--cases-log FILE]
               [--budget-out SECONDS]

    The results of one job. Each Rust test a log reports is a testcase named
    as libtest names it (`programs::wfgrep::wfgrep_searches_a_real_tree_and_agrees_with_grep`),
    in a testsuite named for its test binary as cargo describes it
    (`tests/corpus.rs`); a failed test holds what libtest printed for it. A
    test binary that cargo built (`Executable`) but never ran, or that ended
    before libtest printed its result line, is a testsuite with no testcase
    and an error, which the engine reads as a unit not built. Each step given
    with `--step` that did not pass is said as a testsuite of the job's name
    with an error naming the step, except the step `--tests-step` where failed
    tests explain its failure: then the engine may run those tests again by
    name. `--stamped` reads logs whose lines begin with a time and a space,
    as gate.yml writes its stage log.

    The fast workflow also gives the names it was asked for, one a line
    (`--named`), what libtest listed of them in each binary before the run
    (`--listed`, a log of `cargo test -- --list`), and the log of its run of
    named conformance cases (`--cases-log`). That run is the corpus test
    walking only the cases named; each case is a testcase named
    `conformance/<id>` from the lines the adapter prints for it
    (compiler/tests/conformance/adapter.rs), and the corpus test's own
    result there is not one. `--budget-out SECONDS` says the run's time for
    its tests, that many seconds, was used up and the test running then was
    stopped. Each named test that was running then, its binary stopped
    before it gave a result, is a testcase with an error whose message
    begins `stopped: the fast check's time ran out`, and each named test
    that had not started is a skipped testcase whose message begins
    `not run: the fast check's time was used up`, as the engine reads them;
    the binaries the stop ended or kept from running are then not said to
    be not built.

  junit.py needs --out FILE

    The results of the jobs a job needs, read from the JSON of the `needs`
    context in the variable NEEDS: a testsuite of each job's key, with an
    error where the job did not pass. For workflows whose jobs report no test
    of their own, so that every run uploads results the engine can read.

  junit.py --self-test

    The script's own tests.
"""

import argparse
import json
import os
import re
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET
from xml.sax.saxutils import escape, quoteattr

# The most characters kept of what one failed test printed: its end, where
# libtest puts the panic message.
KEPT = 65536

COLOUR = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
# Characters XML 1.0 cannot hold.
NOT_XML = re.compile("[^\x09\x0a\x0d\x20-퟿-�\U00010000-\U0010ffff]")
# Cargo's lines for a test binary it built and one it runs, each with the
# binary's path under the target directory's `deps/`.
RUNNING = re.compile(r"^\s+Running (\S.*) \(([^()]*deps[/\\][^()]*)\)$")
EXECUTABLE = re.compile(r"^\s+Executable (\S.*) \(([^()]*deps[/\\][^()]*)\)$")
RESULT = re.compile(r"(?:^|\s)test (\S+)(?: - should panic)? \.\.\. (ok|FAILED|ignored(?:, .*)?)$")
OUTPUT = re.compile(r"^---- (\S+)(?: - should panic)? stdout ----$")
LISTED = re.compile(r"^    (\S+)(?: - should panic)?$")
SUMMARY = re.compile(r"^test result: (ok|FAILED)\.")
COMPILE_ERROR = re.compile(r"^error(\[E\d+\])?: (?!test failed|\d+ targets? failed)")
EXITED = re.compile(r"process didn't exit successfully")
LIST_ENTRY = re.compile(r"^(\S+): test$")
# The lines the conformance adapter prints of a case it was asked for by name.
CASE_STARTED = re.compile(r"(?:^|\s)case (conformance/\S+) started$")
CASE_SAID = re.compile(r"(?:^|\s)case (conformance/\S+) said (.*)$")
CASE_UNKNOWN = re.compile(r"(?:^|\s)case (conformance/\S+): no case of the manifest has this id$")
CASE_PREFIX = "conformance/"
STOPPED = "stopped: the fast check's time ran out"
NOT_RUN = "not run: the fast check's time was used up"


def clean(text):
    return NOT_XML.sub("�", COLOUR.sub("", text))


def lines_of(paths, stamped):
    for path in paths:
        try:
            with open(path, encoding="utf-8", errors="replace") as handle:
                for line in handle:
                    line = line.rstrip("\r\n")
                    if stamped:
                        line = line.split(" ", 1)[1] if " " in line else ""
                    yield COLOUR.sub("", line).rstrip("\r")
        except FileNotFoundError:
            continue


class Binary:
    """A test binary cargo ran: its cases in the order libtest reported them."""

    def __init__(self, name, key):
        self.name = name
        self.key = key
        self.cases = {}
        self.outputs = {}
        self.listed = []
        self.ended = False
        self.why = None
        self.started = []
        self.unknown = []


def parse(lines):
    """The binaries cargo built, those it ran, and the first compiler error."""
    built, ran = [], []
    first_error = None
    binary = None
    section = None  # None, "outputs" or "list" once libtest prints failures:
    output = None
    for line in lines:
        executable = EXECUTABLE.match(line)
        if executable:
            key = executable.group(1) + " " + executable.group(2)
            if key not in [k for _, k in built]:
                built.append((executable.group(1), key))
            continue
        running = RUNNING.match(line)
        if running:
            binary = Binary(running.group(1), running.group(1) + " " + running.group(2))
            ran.append(binary)
            section, output = None, None
            continue
        if first_error is None and COMPILE_ERROR.match(line):
            first_error = line
        if binary is None:
            continue
        if binary.ended:
            if binary.why is None and EXITED.search(line):
                binary.why = line.strip()
            continue
        case = CASE_STARTED.search(line)
        if case:
            binary.started.append(case.group(1))
            continue
        case = CASE_SAID.search(line)
        if case:
            binary.outputs.setdefault(case.group(1), []).append(case.group(2))
            continue
        case = CASE_UNKNOWN.search(line)
        if case:
            binary.unknown.append(case.group(1))
            continue
        if SUMMARY.match(line):
            binary.ended = True
            output = None
            continue
        if line == "failures:":
            section = "outputs" if section is None else "list"
            output = None
            continue
        if section == "outputs":
            header = OUTPUT.match(line)
            if header:
                output = header.group(1)
                binary.outputs[output] = []
            elif output is not None:
                binary.outputs[output].append(line)
            continue
        if section == "list":
            listed = LISTED.match(line)
            if listed:
                binary.listed.append(listed.group(1))
            continue
        result = RESULT.search(line)
        if result:
            word = result.group(2)
            outcome = "passed" if word == "ok" else "failed" if word == "FAILED" else "skipped"
            binary.cases[result.group(1)] = outcome
            continue
        if EXITED.search(line):
            binary.why = line.strip()
    for each in ran:
        for name in each.listed:
            each.cases.setdefault(name, "failed")
    return built, ran, first_error


def listed(paths):
    """Each name libtest listed, with the keys of the binaries that hold it."""
    held = {}
    key = None
    for line in lines_of(paths, False):
        running = RUNNING.match(line)
        if running:
            key = running.group(1) + " " + running.group(2)
            continue
        entry = LIST_ENTRY.match(line)
        if entry and key is not None:
            held.setdefault(entry.group(1), []).append(key)
    return held


def suite(name, cases=(), error=None):
    out = [f"  <testsuite name={quoteattr(clean(name))} tests=\"{len(cases)}\""
           f" failures=\"{sum(1 for _, o, _ in cases if o == 'failed')}\""
           f" errors=\"{(1 if error else 0) + sum(1 for _, o, _ in cases if o == 'stopped')}\""
           f" skipped=\"{sum(1 for _, o, _ in cases if o in ('skipped', 'not run'))}\">"]
    for case, outcome, said in cases:
        head = f"    <testcase name={quoteattr(clean(case))} classname={quoteattr(clean(name))}"
        if outcome == "passed":
            out.append(head + "/>")
        elif outcome == "skipped":
            out.append(head + "><skipped/></testcase>")
        elif outcome == "not run":
            out.append(head + f"><skipped message={quoteattr(clean(said))}/></testcase>")
        elif outcome == "stopped":
            out.append(head + f"><error message={quoteattr(clean(said))}/></testcase>")
        else:
            out.append(head + "><failure message=\"failed\">"
                       + escape(clean(said)) + "</failure></testcase>")
    if error:
        first, _, rest = error.partition("\n")
        out.append(f"    <error message={quoteattr(clean(first))}>"
                   + escape(clean(error)) + "</error>")
    out.append("  </testsuite>")
    return "\n".join(out)


def job_results(job, logs, stamped, steps, tests_step,
                named=(), held=None, cases_log=None, budget_out=None):
    built, ran, first_error = parse(lines_of(logs, stamped))
    # A run of named cases alone runs no binary of the ordinary run.
    ordinary_ran = bool(ran) or not cases_log
    case_run = None
    if cases_log:
        _, case_ran, _ = parse(lines_of([cases_log], stamped))
        for binary in case_ran:
            # The corpus test walked only the named cases: its own result is
            # theirs, said case by case.
            binary.cases = {k: v for k, v in binary.cases.items() if k.startswith(CASE_PREFIX)}
        case_run = case_ran[-1] if case_ran else None
        ran = ran + case_ran
    held = held or {}
    # Under a stop for time, the binary that was running then, and every
    # binary built and not started, is the stop's and not a unit not built.
    stopped_key = None
    if budget_out is not None and ran and not ran[-1].ended:
        stopped_key = ran[-1].key
    stopped_message = (f"{STOPPED}: the tests had {budget_out} s, used up while this test ran"
                       if budget_out is not None else None)
    not_run_message = (f"{NOT_RUN}: the tests had {budget_out} s, used up before this test started"
                       if budget_out is not None else None)
    added = {binary.key: [] for binary in ran}
    reported = {case for binary in ran for case in binary.cases}
    started_keys = {binary.key for binary in ran}
    for name in named:
        if name in reported:
            continue
        if name.startswith(CASE_PREFIX):
            if case_run is None:
                if budget_out is not None:
                    added.setdefault(None, []).append((name, "not run", not_run_message))
                continue
            if name in case_run.unknown:
                continue
            began = name in case_run.started
            if budget_out is not None and case_run.key == stopped_key:
                outcome = ("stopped", stopped_message) if began else ("not run", not_run_message)
                added[case_run.key].append((name,) + outcome)
            elif began or not case_run.ended or case_run.why:
                said = ("the corpus test ended while it ran this case, before its verdict; the job's log says why"
                        if began else "the corpus test ended before it reached this case; the job's log says why")
                added[case_run.key].append((name, "failed", said))
            continue
        if budget_out is None:
            continue
        keys = held.get(name, [])
        if not keys:
            continue
        if stopped_key in keys:
            added[stopped_key].append((name, "stopped", stopped_message))
        elif not any(key in started_keys for key in keys):
            added.setdefault(None, []).append((name, "not run", not_run_message))
    suites = []
    problems = False
    any_failed = False
    for binary in ran:
        cases = []
        for case, outcome in binary.cases.items():
            said = ""
            if outcome == "failed":
                any_failed = True
                said = "\n".join(binary.outputs.get(case, [])).strip()[-KEPT:]
                said = said or "the test failed; libtest printed nothing for it"
            cases.append((case, outcome, said))
        for case in added.get(binary.key, []):
            any_failed = any_failed or case[1] == "failed"
            cases.append(case)
        suites.append(suite(binary.name, cases))
        if not binary.ended and binary.key != stopped_key:
            problems = True
            why = binary.why or "cargo printed no reason"
            suites.append(suite(binary.name, error=f"the test binary ended before libtest printed its result: {why}"))
    for name, key in built:
        if key not in started_keys and budget_out is None and ordinary_ran:
            problems = True
            suites.append(suite(name, error="the test binary was built but did not run: cargo stopped before it"))
    if added.get(None):
        suites.append(suite(job, added[None]))
    explained = (any_failed or budget_out is not None) and not problems
    reasons = []
    for outcome, name in steps:
        if outcome in ("success", "skipped") and not (name == tests_step and outcome == "skipped"):
            continue
        if name == tests_step and outcome == "failure" and explained:
            continue
        if outcome == "skipped":
            reasons.append(f"the step {name} did not run: a step before it failed, as the job's log says")
        elif outcome == "failure":
            reasons.append(f"the step {name} failed")
        else:
            reasons.append(f"the step {name} ended {outcome}")
    if reasons:
        error = reasons[0]
        if first_error:
            error += ": " + first_error
        error += "\n" + "\n".join(reasons[1:]) if len(reasons) > 1 else ""
        suites.append(suite(job, error=error))
    elif not suites:
        suites.append(suite(job))
    return suites


def needs_results(needs):
    suites = []
    for key, value in needs.items():
        result = (value or {}).get("result") or "unknown"
        if result in ("success", "skipped"):
            suites.append(suite(key))
        else:
            suites.append(suite(key, error=f"the job {key} ended in {result}; the run's list of jobs says which step"))
    return suites


def write(path, suites):
    directory = os.path.dirname(path)
    if directory:
        os.makedirs(directory, exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites>\n")
        for each in suites:
            handle.write(each + "\n")
        handle.write("</testsuites>\n")


def main(argv):
    parser = argparse.ArgumentParser(description="write JUnit XML for the wf-agents engine")
    commands = parser.add_subparsers(dest="command", required=True)
    job = commands.add_parser("job")
    job.add_argument("--job", required=True)
    job.add_argument("--out", required=True)
    job.add_argument("--log", action="append", default=[])
    job.add_argument("--stamped", action="store_true")
    job.add_argument("--tests-step")
    job.add_argument("--step", nargs=2, action="append", default=[], metavar=("OUTCOME", "NAME"))
    job.add_argument("--named")
    job.add_argument("--listed", action="append", default=[])
    job.add_argument("--cases-log")
    job.add_argument("--budget-out", type=int)
    needs = commands.add_parser("needs")
    needs.add_argument("--out", required=True)
    args = parser.parse_args(argv)
    if args.command == "job":
        named = [name for name in lines_of([args.named], False) if name] if args.named else []
        write(args.out, job_results(args.job, args.log, args.stamped, args.step, args.tests_step,
                                    named, listed(args.listed), args.cases_log, args.budget_out))
    else:
        write(args.out, needs_results(json.loads(os.environ.get("NEEDS") or "{}")))
    return 0


SAMPLE = """\
   Compiling whitefoot v0.1.0
  Executable unittests src/lib.rs (target/gate/deps/whitefoot-0a1b)
  Executable unittests src/bin/whitefootc.rs (target/gate/deps/whitefootc-2c3d)
  Executable tests/corpus.rs (target/gate/deps/corpus-4e5f)
     Running unittests src/lib.rs (target/gate/deps/whitefoot-0a1b)

running 4 tests
test lexer::tests::reads_a_word ... ok
test semantic::tests::a_slow_one has been running for over 60 seconds
test semantic::tests::rejects_a_loan ... FAILED
test semantic::tests::panics_as_asked - should panic ... ok
test backend::tests::needs_a_host ... ignored, needs io_uring
test semantic::tests::a_slow_one ... ok

failures:

---- semantic::tests::rejects_a_loan stdout ----
thread 'semantic::tests::rejects_a_loan' panicked at src/semantic/tests.rs:9:5:
assertion failed: rejected & <ok>

failures:
    semantic::tests::rejects_a_loan

test result: FAILED. 3 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out; finished in 61.20s

error: test failed, to rerun pass `--lib`
"""


class Tests(unittest.TestCase):
    def log(self, text):
        handle = tempfile.NamedTemporaryFile("w", suffix=".log", delete=False)
        handle.write(text)
        handle.close()
        self.addCleanup(os.unlink, handle.name)
        return handle.name

    def test_each_reported_test_is_a_case_of_its_binary(self):
        built, ran, _ = parse(lines_of([self.log(SAMPLE)], False))
        self.assertEqual(len(built), 3)
        self.assertEqual(ran[0].name, "unittests src/lib.rs")
        self.assertEqual(ran[0].cases, {
            "lexer::tests::reads_a_word": "passed",
            "semantic::tests::rejects_a_loan": "failed",
            "semantic::tests::panics_as_asked": "passed",
            "backend::tests::needs_a_host": "skipped",
            "semantic::tests::a_slow_one": "passed",
        })
        self.assertIn("assertion failed", "\n".join(ran[0].outputs["semantic::tests::rejects_a_loan"]))

    def test_binaries_cargo_did_not_reach_are_not_built(self):
        suites = job_results("unit (ubuntu-24.04)", [self.log(SAMPLE)], False,
                             [("failure", "unit")], "unit")
        text = "\n".join(suites)
        self.assertIn('name="unittests src/bin/whitefootc.rs"', text)
        self.assertIn("did not run: cargo stopped before it", text)
        # The tests do not explain the step's failure while a binary did not run.
        self.assertIn('the step unit failed', text)

    def test_failed_tests_explain_the_tests_step(self):
        log = SAMPLE.replace("  Executable unittests src/bin/whitefootc.rs (target/gate/deps/whitefootc-2c3d)\n", "")
        log = log.replace("  Executable tests/corpus.rs (target/gate/deps/corpus-4e5f)\n", "")
        suites = job_results("unit (ubuntu-24.04)", [self.log(log)], False,
                             [("failure", "unit"), ("success", "Hold every stage to its time budget")], "unit")
        text = "\n".join(suites)
        self.assertNotIn("<error", text)
        self.assertIn('<failure message="failed">', text)
        self.assertIn("&amp; &lt;ok&gt;", text)

    def test_another_failed_step_is_said(self):
        log = self.log("     Running unittests src/lib.rs (target/gate/deps/whitefoot-0a1b)\n"
                       "test a::b ... ok\ntest result: ok. 1 passed; 0 failed\n")
        suites = job_results("unit (ubuntu-24.04)", [log], False,
                             [("success", "unit"), ("failure", "Hold every stage to its time budget")], "unit")
        self.assertIn('message="the step Hold every stage to its time budget failed"', suites[-1])

    def test_a_binary_that_stopped_early_is_not_built(self):
        log = self.log("     Running tests/corpus.rs (target/gate/deps/corpus-4e5f)\n"
                       "test programs::a ... ok\n"
                       "error: test failed, to rerun pass `--test corpus`\n\nCaused by:\n"
                       "  process didn't exit successfully: `corpus` (signal: 11, SIGSEGV)\n")
        suites = job_results("corpus (ubuntu-24.04)", [log], False, [("failure", "corpus")], "corpus")
        text = "\n".join(suites)
        self.assertIn("ended before libtest printed its result", text)
        self.assertIn("SIGSEGV", text)
        self.assertIn('message="the step corpus failed"', text)

    def test_a_build_failure_names_the_first_compiler_error(self):
        log = self.log("error[E0425]: cannot find value `x` in this scope\n"
                       "error: could not compile `whitefoot` (lib test) due to 1 previous error\n")
        suites = job_results("tests", [log], False,
                             [("failure", "Build the tests"), ("skipped", "Run the named tests")],
                             "Run the named tests")
        self.assertIn("the step Build the tests failed: error[E0425]: cannot find value", suites[0])

    def test_stamped_lines_lose_their_time(self):
        log = self.log("1.000      Running unittests src/lib.rs (target/gate/deps/whitefoot-0a1b)\n"
                       "1.100 test a::b ... ok\n2.000 test result: ok. 1 passed\n")
        _, ran, _ = parse(lines_of([log], True))
        self.assertEqual(ran[0].cases, {"a::b": "passed"})

    def test_a_passing_check_writes_an_empty_suite(self):
        self.assertEqual(job_results("static", [], False, [("success", "static")], None),
                         [suite("static")])

    def test_needs_say_each_job_that_did_not_pass(self):
        text = "\n".join(needs_results({"scope": {"result": "success"}, "sanitized": {"result": "failure"}}))
        self.assertIn('name="scope"', text)
        self.assertIn('message="the job sanitized ended in failure', text)

    def test_the_written_file_is_xml(self):
        import xml.etree.ElementTree as ET
        path = os.path.join(tempfile.mkdtemp(), "out", "unit (ubuntu-24.04).xml")
        log = self.log(SAMPLE.replace("assertion failed", "assertion\x07 failed"))
        write(path, job_results("unit", [log], False, [("failure", "unit")], "unit"))
        root = ET.parse(path).getroot()
        self.assertEqual(root.tag, "testsuites")
        self.assertEqual(len(root.findall("./testsuite/testcase")), 5)

    LISTED_LOG = """\
     Running unittests src/lib.rs (target/gate/deps/whitefoot-0a1b)
lexer::tests::reads_a_word: test
backend::tests::slow: test

1 tests, 0 benchmarks
     Running unittests src/bin/whitefootc.rs (target/gate/deps/whitefootc-2c3d)

0 tests, 0 benchmarks
     Running tests/corpus.rs (target/gate/deps/corpus-4e5f)
programs::a: test

1 tests, 0 benchmarks
"""

    def named(self, *names):
        return self.log("".join(name + "\n" for name in names))

    def cases(self, result):
        root = ET.fromstring("<testsuites>" + "".join(result) + "</testsuites>")
        return {case.get("name"): case for case in root.iter("testcase")}

    def test_a_stop_for_time_says_the_running_test_stopped_and_the_rest_not_run(self):
        log = self.log("  Executable unittests src/lib.rs (target/gate/deps/whitefoot-0a1b)\n"
                       "  Executable unittests src/bin/whitefootc.rs (target/gate/deps/whitefootc-2c3d)\n"
                       "  Executable tests/corpus.rs (target/gate/deps/corpus-4e5f)\n"
                       "     Running unittests src/lib.rs (target/gate/deps/whitefoot-0a1b)\n"
                       "running 2 tests\ntest lexer::tests::reads_a_word ... ok\n")
        names = ["lexer::tests::reads_a_word", "backend::tests::slow", "programs::a",
                 "conformance/some-case", "no::such::test"]
        result = job_results("tests", [log], False, [("failure", "Run the named tests")],
                             "Run the named tests", names, listed([self.log(self.LISTED_LOG)]),
                             None, 600)
        cases = self.cases(result)
        self.assertIsNone(cases["lexer::tests::reads_a_word"].find("*"))
        self.assertTrue(cases["backend::tests::slow"].find("error").get("message").startswith(STOPPED))
        for name in ("programs::a", "conformance/some-case"):
            self.assertTrue(cases[name].find("skipped").get("message").startswith(NOT_RUN))
        self.assertNotIn("no::such::test", cases)
        text = "\n".join(result)
        self.assertNotIn("did not run: cargo stopped", text)
        self.assertNotIn("ended before libtest", text)
        self.assertNotIn("the step Run the named tests failed", text)

    def test_named_cases_are_tests_of_their_own(self):
        cases_log = self.log("     Running tests/corpus.rs (target/gate/deps/corpus-4e5f)\n"
                             "case conformance/nope: no case of the manifest has this id\n"
                             "case conformance/good started\ntest conformance/good ... ok\n"
                             "case conformance/bad started\ncase conformance/bad said   Fail bad want Accept\n"
                             "test conformance/bad ... FAILED\n"
                             "test conformance::adapter::the_corpus ... FAILED\n"
                             "test result: FAILED. 0 passed; 1 failed\n")
        build = self.log("  Executable tests/corpus.rs (target/gate/deps/corpus-4e5f)\n")
        result = job_results("tests", [build], False, [("failure", "Run the named tests")],
                             "Run the named tests", ["conformance/good", "conformance/bad", "conformance/nope"],
                             {}, cases_log, None)
        cases = self.cases(result)
        self.assertEqual(set(cases), {"conformance/good", "conformance/bad"})
        self.assertIn("want Accept", cases["conformance/bad"].find("failure").text)
        self.assertNotIn("<error", "\n".join(result))

    def test_a_stop_in_the_cases_stops_the_case_running(self):
        cases_log = self.log("     Running tests/corpus.rs (target/gate/deps/corpus-4e5f)\n"
                             "case conformance/one started\ntest conformance/one ... ok\n"
                             "case conformance/two started\n")
        result = job_results("tests", [], False, [("failure", "Run the named tests")], "Run the named tests",
                             ["conformance/one", "conformance/two", "conformance/three"], {}, cases_log, 60)
        cases = self.cases(result)
        self.assertIsNone(cases["conformance/one"].find("*"))
        self.assertTrue(cases["conformance/two"].find("error").get("message").startswith(STOPPED))
        self.assertTrue(cases["conformance/three"].find("skipped").get("message").startswith(NOT_RUN))


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
    else:
        sys.exit(main(sys.argv[1:]))
