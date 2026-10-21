#!/usr/bin/env python3
"""Check docs/safety-claims.md against the specification and the conformance manifest.

The page names, per safety claim, the specification rules the claim rests on
and the conformance cases that check them. A green run says:

- every rule a row names is defined in the active specification;
- every case a row names is in the manifest, cites the row's rule, and has the
  verdict the column promises (a rejecting case declares a rejection citing the
  rule; an accepting case declares accept or run);
- every "other evidence" key is the `covered_by` of the manifest annotation of
  that rule;
- every claim named in CLAIMS has rows, a rejecting case and an accepting case;
- every quotation in the not-claimed table occurs verbatim in the rule it cites;
- every `make` target the reproduction section names exists;
- the trusted-base counting method still resolves to files.

It does not say that the compiler reaches each declared verdict (the gate's
conformance run does), that the sampled cases are the best ones, or that the
sizes quoted on the page are current: they are a dated snapshot, and
`--tcb` prints the present figures with the method the page states.
"""

from pathlib import Path
import json
import re
import sys
import tempfile
import unittest

PAGE = "docs/safety-claims.md"
MANIFEST = "tests/conformance/manifest.jsonl"
SPEC = "spec/kernel-spec.md"
MAKEFILE = "Makefile"
DRIVER = "compiler/src/bin/whitefootc.rs"
BACKEND = "compiler/src/backend"

# The claims the page must make, as the headings of its Claims section.
CLAIMS = (
    "No memory corruption",
    "No data races",
    "No uninitialized reads",
    "No silent overflow",
    "No runtime traps",
)

RULE = r"[A-Z]+-[0-9]+[a-z]?"
RULE_START = re.compile(rf"^\[({RULE})\]")
CODE = re.compile(r"`([^`]+)`")
QUOTE = re.compile("“(.+)”")
INLINE_TEST_MODULE = re.compile(r"^(\s*)(?:pub(?:\([a-z]+\))? )?mod \w+ \{$")


def base_rule(rule):
    return rule.split(".", 1)[0]


def spec_rules(text):
    """Map each rule id defined at a line start to the text of its paragraph run."""
    blocks, current = {}, None
    for line in text.splitlines():
        start = RULE_START.match(line)
        if start:
            current = start[1]
            blocks[current] = [line]
        elif line.startswith("#"):
            current = None
        elif current is not None:
            blocks[current].append(line)
    return {rule: " ".join(" ".join(lines).split()) for rule, lines in blocks.items()}


def manifest_records(text):
    cases, annotations = {}, {}
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        record = json.loads(line)
        if "id" in record:
            cases[record["id"]] = record
        else:
            annotations[record["rule"]] = record
    return cases, annotations


def table_cells(line):
    return [cell.strip() for cell in line.strip().strip("|").split("|")]


def is_separator(cells):
    return all(re.fullmatch(r":?-{3,}:?", cell) for cell in cells)


def parse_page(text):
    """The page's rows: claim rows by claim heading, not-claimed rows, make targets."""
    claims, headings, not_claimed, make_targets = {}, [], [], []
    section = claim = None
    seen_header = {}
    for number, line in enumerate(text.splitlines(), 1):
        if line.startswith("## "):
            section, claim = line[3:].strip(), None
            continue
        if line.startswith("### "):
            claim = line[4:].strip()
            if section == "Claims":
                headings.append(claim)
                claims.setdefault(claim, [])
            continue
        if section == "Reproducing":
            make_targets += [(number, target) for target in re.findall(r"`make ([a-z][a-z-]*)`", line)]
        if not line.startswith("|"):
            continue
        cells = table_cells(line)
        if is_separator(cells):
            continue
        key = (section, claim)
        if not seen_header.get(key):
            seen_header[key] = True
            continue
        if section == "Claims" and claim is not None:
            claims[claim].append((number, cells))
        elif section == "Not claimed":
            not_claimed.append((number, cells))
    return claims, headings, not_claimed, make_targets


def claim_findings(root):
    findings = []

    def add(number, message):
        findings.append(f"{PAGE}:{number}: {message}")

    page = (root / PAGE).read_text()
    rules = spec_rules((root / SPEC).read_text())
    cases, annotations = manifest_records((root / MANIFEST).read_text())
    makefile = (root / MAKEFILE).read_text()
    claims, headings, not_claimed, make_targets = parse_page(page)

    for title in CLAIMS:
        if title not in headings:
            findings.append(f"{PAGE}: the Claims section has no heading '### {title}'")
    for title, rows in claims.items():
        if title in CLAIMS and not rows:
            findings.append(f"{PAGE}: claim '{title}' has no rule rows")

    for title, rows in claims.items():
        rejecting = accepting = 0
        for number, cells in rows:
            if len(cells) != 5:
                add(number, f"a claim row has {len(cells)} cells, expected 5 "
                    "(rule, contribution, rejecting cases, accepting cases, other evidence)")
                continue
            named = CODE.fullmatch(cells[0])
            if not named or not re.fullmatch(RULE, named[1]):
                add(number, f"the first cell must be one rule id in code font, got {cells[0]!r}")
                continue
            rule = named[1]
            if rule not in rules:
                add(number, f"{rule} is not defined in {SPEC}")
                continue
            rejects, accepts, others = (CODE.findall(cell) for cell in cells[2:5])
            if not (rejects or accepts or others):
                add(number, f"{rule} names no case and no other evidence")
            seen = set()
            for case_id in rejects + accepts:
                if case_id in seen:
                    add(number, f"{rule} names {case_id} twice")
                seen.add(case_id)
            for case_id in rejects:
                case = cases.get(case_id)
                if case is None:
                    add(number, f"{rule}: rejecting case {case_id} is not in the manifest")
                    continue
                expect = case["expect"]
                if expect["kind"] != "reject" or base_rule(expect.get("rule", "")) != rule:
                    add(number, f"{rule}: {case_id} declares {expect}, not a rejection citing {rule}")
                    continue
                rejecting += 1
            for case_id in accepts:
                case = cases.get(case_id)
                if case is None:
                    add(number, f"{rule}: accepting case {case_id} is not in the manifest")
                    continue
                kind = case["expect"]["kind"]
                if kind not in ("accept", "run"):
                    add(number, f"{rule}: {case_id} declares {kind}, not accept or run")
                elif rule not in {base_rule(cited) for cited in case["rules"]}:
                    add(number, f"{rule}: {case_id} no longer cites {rule} (it cites {' '.join(case['rules'])})")
                else:
                    accepting += 1
            for key in others:
                annotation = annotations.get(rule)
                if annotation is None or annotation.get("covered_by") != key:
                    found = annotation.get("covered_by") if annotation else "no annotation"
                    add(number, f"{rule}: other evidence '{key}' does not match the manifest annotation ({found})")
        if title in CLAIMS and rows and not rejecting:
            findings.append(f"{PAGE}: claim '{title}' names no rejecting case")
        if title in CLAIMS and rows and not accepting:
            findings.append(f"{PAGE}: claim '{title}' names no accepting case")

    if not not_claimed:
        findings.append(f"{PAGE}: the Not claimed section has no rows")
    for number, cells in not_claimed:
        if len(cells) != 3:
            add(number, f"a not-claimed row has {len(cells)} cells, expected 3 (topic, quotation, rule)")
            continue
        quote, named = QUOTE.search(cells[1]), CODE.fullmatch(cells[2])
        if not quote or not named:
            add(number, "a not-claimed row needs a “quotation” and one rule id in code font")
            continue
        rule = named[1]
        if rule not in rules:
            add(number, f"{rule} is not defined in {SPEC}")
        elif " ".join(quote[1].split()) not in rules[rule]:
            add(number, f"the quotation does not occur verbatim in {rule}")

    if not any(target == "conformance-run" for _, target in make_targets):
        findings.append(f"{PAGE}: the Reproducing section does not name `make conformance-run`")
    for number, target in make_targets:
        if not re.search(rf"^{re.escape(target)}:", makefile, re.M):
            add(number, f"`make {target}` is not a Makefile target")

    return findings


def without_inline_test_modules(source):
    """Source lines outside `#[cfg(test)] mod name { ... }` blocks.

    The sources are rustfmt-formatted, so such a block ends at the first later
    line equal to the module's own indentation and a closing brace.
    """
    lines, kept, index = source.splitlines(), [], 0
    while index < len(lines):
        opening = INLINE_TEST_MODULE.match(lines[index + 1]) if (
            lines[index].strip() == "#[cfg(test)]" and index + 1 < len(lines)) else None
        if not opening:
            kept.append(lines[index])
            index += 1
            continue
        index += 2
        while index < len(lines) and lines[index] != opening[1] + "}":
            index += 1
        index += 1
    return kept


def is_test_source(relative):
    parts = relative.parts
    name = parts[-1]
    return "tests" in parts[:-1] or name == "tests.rs" or name.endswith(("_test.rs", "_tests.rs"))


def newline_count(path):
    return path.read_bytes().count(b"\n")


def runtime_paths(root):
    """The runtime unit files the driver stages, and the findings that stop a count."""
    driver = root / DRIVER
    units = list(dict.fromkeys(re.findall(r'unit\(\s*"([^"]+)"', driver.read_text()))) if driver.is_file() else []
    problems = [] if units else [f"no runtime unit was found in {DRIVER}"]
    paths = [root / BACKEND / unit for unit in units]
    problems += [f"runtime unit {path.relative_to(root)} does not exist" for path in paths if not path.is_file()]
    return [path for path in paths if path.is_file()], problems


def tcb(root):
    """(label, files, lines) per counted part, the system headers, and the findings that stop a count."""
    compiler = root / "compiler"
    sources = [path for path in sorted((compiler / "src").rglob("*.rs"))
               if not is_test_source(path.relative_to(compiler / "src"))]
    build = compiler / "build.rs"
    if build.is_file():
        sources.append(build)
    problems = [] if sources else ["no compiler Rust sources were found under compiler/src"]
    paths, unit_problems = runtime_paths(root)
    c_family = [path for path in paths if path.suffix in (".c", ".h")]
    llvm_ir = [path for path in paths if path.suffix == ".ll"]
    headers = sorted({name for path in c_family
                      for name in re.findall(r"^\s*#\s*include\s*<([^>]+)>", path.read_text(), re.M)})
    rows = [
        ("Compiler (Rust)", len(sources),
         sum(len(without_inline_test_modules(path.read_text())) for path in sources)),
        ("Runtime (C and headers)", len(c_family), sum(newline_count(path) for path in c_family)),
        ("Runtime (LLVM IR)", len(llvm_ir), sum(newline_count(path) for path in llvm_ir)),
    ]
    return rows, headers, problems + unit_problems


def tcb_table(rows, headers):
    lines = [f"{label}: {files} files, {count:,} lines" for label, files, count in rows]
    lines.append(f"System headers the runtime includes: {len(headers)}: {', '.join(headers)}")
    return "\n".join(lines)


def check(root):
    findings = claim_findings(root)
    rows, headers, problems = tcb(root)
    findings += problems
    for finding in findings:
        print("safety claims: " + finding, file=sys.stderr)
    if findings:
        return 1
    print("safety claims: every row resolves against the specification and the manifest")
    print(tcb_table(rows, headers))
    return 0


SPEC_FIXTURE = """# Kernel Specification v0.1

## 1. Scope

[SCOPE-3] Accepted programs have no undefined behavior.
The source outcome model leaves resource availability outside it.

[OP-4] A subscript carries
the bound i < len.

[OWN-1] Every value has exactly one owner.

[CAP-1] A race condition is not excluded.
"""

MANIFEST_FIXTURE = "\n".join([
    "# fixture",
    json.dumps({"id": "op4-neg", "rules": ["OP-4"], "expect": {"kind": "reject", "rule": "OP-4"},
                "status": "runnable", "doc": "x"}),
    json.dumps({"id": "op4-pos", "rules": ["OP-4", "OWN-1"], "expect": {"kind": "accept"},
                "status": "runnable", "doc": "x"}),
    json.dumps({"id": "own1-neg", "rules": ["OWN-1"], "expect": {"kind": "reject", "rule": "OWN-1"},
                "status": "runnable", "doc": "x"}),
    json.dumps({"id": "own1-run", "rules": ["OWN-1"], "expect": {"kind": "run", "exit": 0},
                "status": "runnable", "doc": "x"}),
    json.dumps({"rule": "CAP-1", "covered_by": "ownership-tests", "reason": "x"}),
]) + "\n"


FIXTURE_ROWS = {
    "No memory corruption": ("OP-4", "`op4-neg`", "`op4-pos`", ""),
    "No data races": ("CAP-1", "", "", "`ownership-tests`"),
    "No uninitialized reads": ("OP-4", "`op4-neg`", "`op4-pos`", ""),
    "No silent overflow": ("OP-4", "`op4-neg`", "`op4-pos`", ""),
    "No runtime traps": ("OWN-1", "`own1-neg`", "`own1-run`", ""),
}


def page_fixture(rows=None):
    rows = {**FIXTURE_ROWS, **(rows or {})}
    sections = ["# Safety claims", "", "## Claims", ""]
    for title in CLAIMS:
        rule, rejects, accepts, other = rows[title]
        sections += [f"### {title}", "",
                     "| Rule | Contribution | Rejecting cases | Accepting cases | Other evidence |",
                     "|---|---|---|---|---|",
                     f"| `{rule}` | text | {rejects} | {accepts} | {other} |", ""]
    sections += ["## Not claimed", "",
                 "| Topic | Specification | Rule |", "|---|---|---|",
                 "| Resources | “leaves resource   availability outside it” | `SCOPE-3` |", "",
                 "## Reproducing", "", "Run `make conformance-run`.", ""]
    return "\n".join(sections)


MAKEFILE_FIXTURE = "conformance-run:\n\t@true\n"


class ClaimTests(unittest.TestCase):
    def fixture(self, page=None, manifest=None, spec=None, makefile=None):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for name, text in ((PAGE, page or page_fixture()), (MANIFEST, manifest or MANIFEST_FIXTURE),
                           (SPEC, spec or SPEC_FIXTURE), (MAKEFILE, makefile or MAKEFILE_FIXTURE)):
            path = root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
        return root

    def findings(self, **parts):
        return claim_findings(self.fixture(**parts))

    def assertOne(self, findings, fragment):
        self.assertEqual(len(findings), 1, findings)
        self.assertIn(fragment, findings[0])

    def edit_case(self, case_id, change):
        lines = []
        for line in MANIFEST_FIXTURE.splitlines():
            if line.startswith("{"):
                record = json.loads(line)
                if record.get("id") == case_id:
                    change(record)
                line = json.dumps(record)
            lines.append(line)
        return "\n".join(lines) + "\n"

    def test_clean_fixture(self):
        self.assertEqual(self.findings(), [])

    def test_unknown_rule(self):
        page = page_fixture({"No data races": ("NOPE-9", "", "", "`ownership-tests`")})
        self.assertOne(self.findings(page=page), "NOPE-9 is not defined")

    def test_deleted_case(self):
        manifest = "\n".join(line for line in MANIFEST_FIXTURE.splitlines() if '"op4-pos"' not in line)
        findings = self.findings(manifest=manifest)
        self.assertTrue(findings and all("op4-pos is not in the manifest" in f or "names no accepting" in f
                                         for f in findings), findings)

    def test_accepting_case_loses_the_rule(self):
        manifest = self.edit_case("own1-run", lambda r: r.update(rules=["OP-4"]))
        self.assertOne(self.findings(manifest=manifest), "own1-run no longer cites OWN-1")

    def test_rejecting_case_changes_its_rule(self):
        manifest = self.edit_case("op4-neg", lambda r: r.update(expect={"kind": "reject", "rule": "OWN-1"}))
        findings = self.findings(manifest=manifest)
        self.assertTrue(any("op4-neg declares" in f for f in findings), findings)

    def test_rejecting_case_becomes_accept(self):
        manifest = self.edit_case("own1-neg", lambda r: r.update(expect={"kind": "accept"}))
        findings = self.findings(manifest=manifest)
        self.assertTrue(any("own1-neg declares" in f for f in findings), findings)

    def test_accepting_column_names_a_rejection(self):
        page = page_fixture({"No runtime traps": ("OWN-1", "`own1-neg`", "`own1-neg`", "")})
        findings = self.findings(page=page)
        self.assertTrue(any("own1-neg declares reject, not accept or run" in f for f in findings), findings)

    def test_claim_without_a_rejecting_case(self):
        page = page_fixture({"No memory corruption": ("OP-4", "", "`op4-pos`", "")})
        self.assertOne(self.findings(page=page), "claim 'No memory corruption' names no rejecting case")

    def test_row_without_evidence(self):
        page = page_fixture({"No data races": ("CAP-1", "", "", "")})
        findings = self.findings(page=page)
        self.assertTrue(any("CAP-1 names no case and no other evidence" in f for f in findings), findings)

    def test_annotation_changes(self):
        manifest = MANIFEST_FIXTURE.replace("ownership-tests", "renamed")
        self.assertOne(self.findings(manifest=manifest), "'ownership-tests' does not match")

    def test_missing_claim_heading(self):
        page = page_fixture().replace("### No data races", "### Data races")
        findings = self.findings(page=page)
        self.assertTrue(any("no heading '### No data races'" in f for f in findings), findings)

    def test_quotation_no_longer_in_the_rule(self):
        spec = SPEC_FIXTURE.replace("leaves resource availability", "leaves nothing")
        self.assertOne(self.findings(spec=spec), "does not occur verbatim in SCOPE-3")

    def test_quotation_spans_lines_and_spaces(self):
        spec = SPEC_FIXTURE.replace("leaves resource availability", "leaves\nresource  availability")
        self.assertEqual(self.findings(spec=spec), [])

    def test_missing_make_target(self):
        self.assertOne(self.findings(makefile="other:\n"), "`make conformance-run` is not a Makefile target")

    def test_real_page_goes_red_when_a_cited_case_changes(self):
        real = Path(__file__).resolve().parent.parent
        page, manifest, spec, makefile = (
            (real / name).read_text() for name in (PAGE, MANIFEST, SPEC, MAKEFILE))
        self.assertEqual(claim_findings(self.fixture(page, manifest, spec, makefile)), [])
        claims, _, _, _ = parse_page(page)
        rows = [cells for entries in claims.values() for _, cells in entries]
        row = next(cells for cells in rows if CODE.findall(cells[3]))
        rule, case_id = CODE.fullmatch(row[0])[1], CODE.findall(row[3])[0]
        changed = []
        for line in manifest.splitlines():
            if line.startswith("{") and json.loads(line).get("id") == case_id:
                record = json.loads(line)
                record["rules"] = [other for other in record["rules"] if base_rule(other) != rule] or ["SCOPE-2"]
                line = json.dumps(record)
            changed.append(line)
        findings = claim_findings(self.fixture(page, "\n".join(changed) + "\n", spec, makefile))
        self.assertTrue(any(f"{case_id} no longer cites {rule}" in f for f in findings), findings)

    def test_inline_test_modules_are_not_counted(self):
        source = "fn a() {}\n#[cfg(test)]\nmod tests {\n    fn t() {\n    }\n}\nfn b() {}\n"
        self.assertEqual(without_inline_test_modules(source), ["fn a() {}", "fn b() {}"])

    def test_test_source_paths(self):
        for name in ("backend/tests/a.rs", "backend/tests.rs", "x_test.rs", "x_tests.rs"):
            self.assertTrue(is_test_source(Path(name)), name)
        self.assertFalse(is_test_source(Path("backend/emitter.rs")))

    def test_tcb_counts_and_reports_unresolved_methods(self):
        root = self.fixture()
        (root / "compiler/src/bin").mkdir(parents=True)
        (root / "compiler/src/a.rs").write_text("fn a() {}\n")
        (root / "compiler/src/b_tests.rs").write_text("fn t() {}\n")
        (root / "compiler/src/tests").mkdir()
        (root / "compiler/src/tests/c.rs").write_text("fn t() {}\n")
        (root / "compiler/src/backend").mkdir()
        (root / "compiler/src/backend/x.c").write_text("#include <stdio.h>\n#include \"local.h\"\nint y;\n")
        (root / DRIVER).write_text('const A: &[U] = &[unit("x.c", X), unit("gone.c", G)];\n')
        rows, headers, problems = tcb(root)
        self.assertEqual([row[1:] for row in rows[:2]], [(1, 1), (1, 3)])
        self.assertEqual(headers, ["stdio.h"])
        self.assertEqual(problems, ["runtime unit compiler/src/backend/gone.c does not exist"])
        (root / DRIVER).write_text("fn main() {}\n")
        self.assertIn(f"no runtime unit was found in {DRIVER}", tcb(root)[2])


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
    elif sys.argv[1:] == ["--tcb"]:
        counted, system, trouble = tcb(Path(__file__).resolve().parent.parent)
        print(tcb_table(counted, system))
        for item in trouble:
            print("safety claims: " + item, file=sys.stderr)
        sys.exit(int(bool(trouble)))
    elif not sys.argv[1:]:
        sys.exit(check(Path(__file__).resolve().parent.parent))
    else:
        sys.exit("usage: check-safety-claims.py [--self-test | --tcb]")
