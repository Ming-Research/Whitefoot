#!/usr/bin/env python3
"""whitefoot conformance test system — spec-anchored, rule-keyed, toolchain-agnostic.

Each case is a canonical `.wf` source (tests/conformance/cases/<id>.wf), or a
module-form case directory (tests/conformance/cases/<id>/) holding a
`modules.wfg` graph and one directory per module [MOD-1, MOD-2], plus a
manifest entry (tests/conformance/manifest.jsonl) declaring the rule id(s) it
exercises and the expected verdict. Cases are driven through a named toolchain
adapter. The active adapter is native, not Python: `compiler/tests/corpus.rs`
checks `accept` and `reject` through the ordinary compiler's complete source
front end, compiles `run` and `unsupported` through the complete target path,
realizes the ARRANGE below as a real invocation, and reduces the outcome to one verdict below
(`make conformance-run`, included by root `make check`). This file stays on the other side of that boundary —
identity, corpus structure, declared rule coverage, and schema validity — so
the corpus keeps outliving any one compiler and no compiler behaviour is
reimplemented here. `ADAPTER` below is the hook for a future non-native
toolchain; with none installed, a Python-side semantic run fails explicitly
rather than reporting an empty success.

Because it tests the LANGUAGE (source -> verdict), not a compiler's internals, this
suite outlives compiler implementations.

Verdict = ("accept",) | ("reject", rule) | ("run", exit) | ("unsupported", why)

Manifest line (JSON):
  {"id": str, "rules": [rule_id...], "expect": EXPECT, "arrange": ARRANGE?,
   "status": "runnable", "doc": str}
  EXPECT = {"kind":"accept"} | {"kind":"reject","rule":R} | {"kind":"run","exit":N}
         | {"kind":"unsupported","why":str}

  status: runnable = must match expect. It is the only status. There is no
          skipped and no expected-failure status: a case whose declared verdict
          the toolchain does not reach is a failing case until the toolchain is
          fixed.

`expect` is the verdict the SPECIFICATION requires. An unimplemented compiler
capability never rewrites `expect` and never excuses a case from running.

The `unsupported` outcome belongs only to a compiler, build/link or invocation
capability failure, never to source-language rejection. No target qualification
or host-specific acceptance class exists. An unimplemented compiler capability
remains a toolchain readiness fact and cannot replace an ordinary source
verdict. A target-layout stop likewise cannot replace `accept`: target
qualification begins after complete source-semantic acceptance [STOR-6].

ARRANGE describes the invocation a `run` case needs; a case that is
never executed must not carry one. Every byte string is lowercase hex so the
corpus can express non-UTF-8 argument and path bytes exactly and diff them
readably:

  ARRANGE = {"argv": ["<hex>"...]?,          the complete native argument vector
             "stdin": "<hex>"?,              standard input body
             "files": [FIXTURE...]?,         fixtures under the initial directory
             "redirect": {"stdout": NAME?, "stderr": NAME?}?}
  FIXTURE = {"path":"<hex>", "bytes":"<hex>"} | {"path":"<hex>", "directory":true}

`argv` is the COMPLETE native argument vector, position 0 included, exactly as
the ordinary Args library value delivers it: `argv[i]` is what the program
reads at position i, and the vector's length is the count the program reads.
This is the only lossless reading. `Args` carries the complete native vector in the selected library API, so a schema that listed only the arguments after the invoked
name would leave the vector's first element — an element a program can count
and read — unstated by a case that purports to fix its invocation. Position 0
is therefore a case's own datum rather than the harness's incidental choice of
where it built the executable, and an adapter sets it explicitly. An absent
`argv` means the adapter's own default vector, which supplies position 0 and
nothing after it.

A redirection NAME is an opaque sink label: two entries naming the same sink
are the same destination, which is how "stdout and stderr redirected to one
target" is stated. An adapter realizes one sink label as one object under the
initial directory, so a case may also name that label as a path and read the
combined bytes back. Absent keys mean the default arrangement — the default
argument vector, an empty standard input, no fixtures, and separate sinks.
"""
import json, re, subprocess, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
CASES = HERE / "cases"
MANIFEST = HERE / "manifest.jsonl"
ACTIVE_SPEC = Path("spec/kernel-spec.md")
# The named native adapter is compiler/tests/corpus.rs, reached through
# `make conformance-run` and therefore root `make check`; this hook stays open for a future non-native
# toolchain. Keeping it explicit prevents a missing compiler, crash, or broad
# exception from becoming `Unsupported`.
ADAPTER = None


def matches(v, expect):
    k = expect["kind"]
    return ((k == "accept" and v[0] == "accept")
            or (k == "reject" and v[0] == "reject" and v[1] == expect["rule"])
            or (k == "run" and v[0] == "run" and v[1] == expect["exit"])
            # `why` is the adapter's prose; the corpus asserts the verdict kind,
            # never a diagnostic's wording.
            or (k == "unsupported" and v[0] == "unsupported"))


def load_manifest():
    """Split manifest lines into cases (have "id") and coverage annotations (have "covered_by")."""
    cases, annots = [], []
    for ln in MANIFEST.read_text().splitlines():
        ln = ln.strip()
        if not ln or ln.startswith("#"):
            continue
        o = json.loads(ln)
        (cases if "id" in o else annots).append(o)
    return cases, annots


def run_cases(cases):
    if ADAPTER is None:
        raise RuntimeError(
            "no active compiler adapter; use `coverage` until an adapter is installed"
        )
    results = []
    for c in cases:
        src = case_sources(c["id"])
        v = ADAPTER(src, c["expect"]["kind"] == "run")
        m = matches(v, c["expect"])
        if v[0] == "unsupported" and c["expect"]["kind"] != "unsupported":
            outcome = "FAIL"                     # runnable means supported
        else:
            outcome = "PASS" if m else "FAIL"
        results.append((c, outcome, v))
    return results


RULE_ID = r"[A-Z]+-[0-9]+[a-z]?(?:\.S[0-9]+)?"


def specification_rules(text):
    """Check Markdown definition/reference integrity without a compiler build.

    Sub-rule anchors are checked individually, but coverage counts their parent.
    This checks citations, not semantic uniqueness or a change's selection ground.
    """
    definitions = {}
    for number, line in enumerate(text.splitlines(), 1):
        match = re.match(rf"^\[({RULE_ID})\]", line)
        if match:
            rule = match[1]
            if rule in definitions:
                raise ValueError(
                    f"specification line {number}: duplicate rule definition [{rule}] "
                    f"(first defined at line {definitions[rule]})"
                )
            definitions[rule] = number
    for number, line in enumerate(text.splitlines(), 1):
        for match in re.finditer(rf"\[({RULE_ID})\]", line):
            if match[1] not in definitions:
                raise ValueError(
                    f"specification line {number}: unresolved rule reference [{match[1]}]"
                )
    return {base_rule(rule) for rule in definitions}


def spec_rule_ids(root=ROOT):
    # The active file is the authority; its bytes need no duplicate identity.
    spec = root / ACTIVE_SPEC
    text = spec.read_bytes().decode("utf-8")
    # Rule ids at line starts. A `[FAM-N.Sk]` sub-id line is an addressable
    # citation anchor inside its parent rule, not a rule of its own: the
    # coverage denominator stays the base rules, and a citation of a sub-id
    # counts toward its parent (see base_rule below).
    return specification_rules(text), spec.name


def base_rule(rule_id):
    """Fold a `[FAM-N.Sk]` sub-id citation onto its parent rule id."""
    return rule_id.split(".", 1)[0]


HEX = re.compile(r"\A(?:[0-9a-f]{2})*\Z")


def hex_errors(label, value, allow_empty=True):
    """A byte string is lowercase hex so the corpus states non-UTF-8 bytes exactly."""
    if not isinstance(value, str) or not HEX.match(value):
        return [f"{label} must be a lowercase even-length hex byte string"]
    if not allow_empty and not value:
        return [f"{label} must not be empty"]
    return []


def arrange_errors(label, arrange):
    """Check one invocation arrangement. Keys are a closed set: an unknown key is a
    silently ignored intention, which is worse in a corpus than a rejected line."""
    if not isinstance(arrange, dict):
        return [f"{label}: arrange must be an object"]
    errors = []
    unknown = sorted(set(arrange) - {"argv", "stdin", "files", "redirect"})
    if unknown:
        errors.append(f"{label}: unknown arrange keys: {' '.join(unknown)}")

    if "argv" in arrange:
        argv = arrange["argv"]
        if not isinstance(argv, list) or not argv:
            errors.append(f"{label}: argv must be a nonempty list")
        else:
            for position, argument in enumerate(argv):
                errors += hex_errors(f"{label}: argv[{position}]", argument)

    if "stdin" in arrange:
        errors += hex_errors(f"{label}: stdin", arrange["stdin"])

    if "files" in arrange:
        files = arrange["files"]
        if not isinstance(files, list) or not files:
            errors.append(f"{label}: files must be a nonempty list")
        else:
            paths = []
            for position, fixture in enumerate(files):
                where = f"{label}: files[{position}]"
                if not isinstance(fixture, dict):
                    errors.append(f"{where} must be an object")
                    continue
                unknown = sorted(set(fixture) - {"path", "bytes", "directory"})
                if unknown:
                    errors.append(f"{where}: unknown fixture keys: {' '.join(unknown)}")
                errors += hex_errors(f"{where}.path", fixture.get("path"), allow_empty=False)
                paths.append(fixture.get("path"))
                is_file, is_directory = "bytes" in fixture, "directory" in fixture
                if is_file == is_directory:
                    errors.append(f"{where} must carry exactly one of bytes, directory")
                elif is_file:
                    errors += hex_errors(f"{where}.bytes", fixture["bytes"])
                elif fixture["directory"] is not True:
                    errors.append(f"{where}.directory must be true")
            duplicate = sorted({path for path in paths if paths.count(path) > 1})
            if duplicate:
                errors.append(f"{label}: duplicate fixture paths: {' '.join(duplicate)}")

    if "redirect" in arrange:
        redirect = arrange["redirect"]
        if not isinstance(redirect, dict) or not redirect:
            errors.append(f"{label}: redirect must be a nonempty object")
        else:
            unknown = sorted(set(redirect) - {"stdout", "stderr"})
            if unknown:
                errors.append(f"{label}: unknown redirect streams: {' '.join(unknown)}")
            for stream, sink in redirect.items():
                if not isinstance(sink, str) or not sink:
                    errors.append(f"{label}: redirect {stream} needs a nonempty sink name")

    return errors


def case_sources(case_id, cases_dir=CASES):
    """The case's source text: one `.wf` record, or a module-form case's
    graph and records keyed by their path below the case directory."""
    directory = cases_dir / case_id
    if (directory / "modules.wfg").is_file():
        return {
            str(path.relative_to(directory)): path.read_text()
            for path in sorted(directory.rglob("*"))
            if path.is_file()
        }
    return (cases_dir / f"{case_id}.wf").read_text()


def case_source_names(cases_dir=CASES):
    """Every case source in the directory: `<id>.wf` files and module-form
    case directories, named `<id>.wf` and `<id>/` respectively."""
    names = {path.name for path in cases_dir.glob("*.wf")}
    names |= {
        f"{path.name}/"
        for path in cases_dir.iterdir()
        if path.is_dir() and (path / "modules.wfg").is_file()
    }
    return names


def validate_manifest(cases, annots, root=ROOT, cases_dir=CASES):
    """Reject malformed or stale corpus structure before reporting coverage."""
    rules, _ = spec_rule_ids(root)
    errors = []

    ids = [case.get("id") for case in cases]
    duplicate_ids = sorted({case_id for case_id in ids if ids.count(case_id) > 1})
    if duplicate_ids:
        errors.append("duplicate case ids: " + " ".join(duplicate_ids))

    actual_sources = case_source_names(cases_dir)
    expected_sources = {
        f"{case_id}/" if f"{case_id}/" in actual_sources else f"{case_id}.wf"
        for case_id in ids
        if isinstance(case_id, str)
    }
    missing_sources = sorted(expected_sources - actual_sources)
    orphan_sources = sorted(actual_sources - expected_sources)
    if missing_sources:
        errors.append("missing case sources: " + " ".join(missing_sources))
    if orphan_sources:
        errors.append("orphan case sources: " + " ".join(orphan_sources))

    valid_statuses = {"runnable"}
    expectation_fields = {
        "accept": {"kind"},
        "reject": {"kind", "rule"},
        "run": {"exit", "kind"},
        "unsupported": {"kind", "why"},
    }
    executed_kinds = {"run"}
    for case in cases:
        case_id = case.get("id", "<missing-id>")
        case_rules = case.get("rules")
        if not isinstance(case_rules, list) or not case_rules:
            errors.append(f"{case_id}: rules must be a nonempty list")
            case_rules = []
        unknown = sorted(
            r for r in set(case_rules) if base_rule(r) not in rules
        )
        if unknown:
            errors.append(f"{case_id}: unknown rules: {' '.join(unknown)}")

        status = case.get("status", "runnable")
        if status not in valid_statuses:
            errors.append(f"{case_id}: invalid status {status!r}")

        expect = case.get("expect")
        kind = expect.get("kind") if isinstance(expect, dict) else None
        if kind not in expectation_fields:
            errors.append(f"{case_id}: invalid expectation {kind!r}")
        elif set(expect) != expectation_fields[kind]:
            fields = " ".join(sorted(expectation_fields[kind]))
            errors.append(
                f"{case_id}: {kind} expectation fields must be exactly: {fields}"
            )
        elif kind == "reject":
            reject_rule = expect.get("rule")
            if reject_rule not in case_rules:
                errors.append(f"{case_id}: reject rule must appear in case rules")
        elif kind == "run" and type(expect.get("exit")) is not int:
            errors.append(f"{case_id}: run expectation requires an integer exit")
        elif kind == "unsupported" and not expect.get("why"):
            errors.append(f"{case_id}: unsupported expectation requires a why")

        if "arrange" in case:
            if kind not in executed_kinds:
                errors.append(f"{case_id}: only a run case may carry an arrange")
            else:
                errors += arrange_errors(case_id, case["arrange"])

        if not case.get("doc"):
            errors.append(f"{case_id}: missing documentation")

    annotation_rules = [annotation.get("rule") for annotation in annots]
    duplicate_annotations = sorted(
        {rule for rule in annotation_rules if annotation_rules.count(rule) > 1}
    )
    if duplicate_annotations:
        errors.append("duplicate annotations: " + " ".join(duplicate_annotations))
    for annotation in annots:
        rule = annotation.get("rule", "<missing-rule>")
        if rule not in rules:
            errors.append(f"annotation {rule}: unknown rule")
        if not annotation.get("covered_by"):
            errors.append(f"annotation {rule}: missing covered_by")
        if not annotation.get("reason"):
            errors.append(f"annotation {rule}: missing reason")

    if errors:
        raise ValueError("invalid conformance manifest:\n  " + "\n  ".join(errors))


def coverage(cases, annots):
    rules, spec_name = spec_rule_ids()
    tagged, pos, neg = set(), set(), set()
    for c in cases:
        tagged |= {base_rule(r) for r in c["rules"]}
        kind = c["expect"]["kind"]
        # Coverage measures the corpus against the specification, so every case
        # counts whatever its toolchain-readiness status is. An `unsupported`
        # expectation is a qualification stop: neither a positive exercise of
        # the rule nor a rejection citing it.
        if kind == "reject":
            neg.add(c["expect"]["rule"])
        elif kind != "unsupported":
            pos |= {base_rule(r) for r in c["rules"]}
    annotated = {a["rule"] for a in annots} & rules
    by_case = tagged & rules
    covered = by_case | annotated
    return rules, spec_name, covered, by_case, annotated, pos, neg, sorted(rules - covered)


def declared_verdicts(text):
    """Every case's declared verdict in one manifest's text, keyed by id."""
    verdicts = {}
    for line in text.splitlines():
        row = line.strip()
        if not row or row.startswith("#"):
            continue
        case = json.loads(row)
        if "id" in case:
            verdicts[case["id"]] = case["expect"]
    return verdicts


def manifest_at(revision):
    """The manifest as of one revision, read through git rather than the tree."""
    path = MANIFEST.relative_to(ROOT)
    result = subprocess.run(
        ["git", "-C", str(ROOT), "show", f"{revision}:{path}"],
        capture_output=True, text=True, check=False,
    )
    if result.returncode != 0:
        raise SystemExit(f"cannot read the manifest at {revision}: {result.stderr.strip()}")
    return result.stdout


def verdict_diff(revision):
    """Reports every declared verdict added, moved, or removed since `revision`.

    This is the one population the adapter cannot see. The adapter compares
    each case's ACTUAL verdict against its DECLARED one, so a verdict that
    moves while its manifest row is edited to follow leaves the adapter green.

    THE SCOPE, because "the migration damaged a case" is three classes and
    this check owns exactly one of them:

      (A) a negative whose violation was a construct the migration deletes.
          It then compiles where its row demands a rejection, so THE ADAPTER
          already catches it and always did. Not this check's.
      (B) a positive whose subject was deleted. It keeps passing and its
          verdict never moves, so no verdict-based check reaches it. Not this
          check's, and not anything's — see the limit below.
      (C) a declared verdict that moves with its row following. This check.

    Reading a green run here as "the migration broke nothing" is therefore
    wrong in class (B)'s direction, which is the direction that has actually
    cost this project cases.

    ITS LIMIT, stated because a checker trusted past its range is worse than
    none: it cannot see an EMPTIED case. A positive whose subject was deleted
    still declares, and still reaches, the verdict it always did; there is
    nothing here to compare. A verdict is the wrong instrument for a question
    about a case's subject, and this check does not answer one.

    A hit is not necessarily a defect. The report makes changed declarations
    visible and supplies one input to rule 4's exact conformance-content record.
    It does not replace the complete source-and-manifest diff that rule needs.
    """
    before, after = declared_verdicts(manifest_at(revision)), declared_verdicts(
        MANIFEST.read_text()
    )
    moved = sorted(k for k in before.keys() & after.keys() if before[k] != after[k])
    gone = sorted(before.keys() - after.keys())
    added = sorted(after.keys() - before.keys())
    return before, after, moved, gone, added



def main():
    cmd = sys.argv[1] if len(sys.argv) > 1 else "all"
    verbose = "-v" in sys.argv
    cases, annots = load_manifest()
    try:
        validate_manifest(cases, annots)
    except ValueError as error:
        print(error, file=sys.stderr)
        raise SystemExit(1) from error
    fail = 0
    if cmd in ("run", "all"):
        if ADAPTER is None:
            print(
                "conformance run: UNAVAILABLE — no active compiler adapter",
                file=sys.stderr,
            )
            fail += 1
            if cmd == "run":
                raise SystemExit(fail)
        else:
            tally = {}
            for c, o, v in run_cases(cases):
                tally[o] = tally.get(o, 0) + 1
                if o == "FAIL":
                    fail += 1
                    print(f"  {o:5} {c['id']:38} want {c['expect']} got {v}")
            print("conformance run: " + "  ".join(f"{k}={tally[k]}" for k in sorted(tally)))
    if cmd == "verdicts":
        if len(sys.argv) < 3:
            raise SystemExit("usage: runner.py verdicts <revision>")
        revision = sys.argv[2]
        before, after, moved, gone, added = verdict_diff(revision)
        for case in moved:
            print(f"  MOVED   {case:38} {before[case]} -> {after[case]}")
        for case in gone:
            print(f"  REMOVED {case:38} {before[case]}")
        for case in added:
            print(f"  ADDED   {case:38} {after[case]}")
        print(
            f"declared verdicts vs {revision}: {len(moved)} moved, {len(gone)} removed, "
            f"{len(added)} added, {len(after)} total"
        )
        # Any declared-set change is relevant to rule 4. Exit nonzero so an
        # unexamined addition is no quieter than a move or removal.
        sys.exit(1 if moved or gone or added else 0)
    if cmd in ("coverage", "all"):
        rules, spec_name, covered, by_case, annotated, pos, neg, uncovered = coverage(cases, annots)
        print(f"coverage ({spec_name}): {len(covered)}/{len(rules)} rules covered "
              f"({len(by_case)} by case [+{len(pos)}/-{len(neg)}], {len(annotated)} by annotation); "
              f"{len(uncovered)} uncovered")
        if uncovered:
            fail += 1
        if verbose and uncovered:
            print("  uncovered: " + " ".join(uncovered))
    sys.exit(1 if fail else 0)


if __name__ == "__main__":
    main()
