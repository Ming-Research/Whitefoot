#!/usr/bin/env python3
"""Check that the agent guidance's references resolve, not what it says.

- Review item IDs cited in the guidance (A1, T4-T7, G3, DC2, ...) are defined
  in docs/review-checklist.md or the design-tree skill.
- Repository paths written in backticks in the entry documents exist.

A green run says only that these references resolve. Whether the guidance is
correct, current and well placed is review item D.
"""

from pathlib import Path
import re
import sys
import tempfile
import unittest

CHECKLIST = "docs/review-checklist.md"
DESIGN_SKILL = "design/skill/SKILL.md"
# Documents that cite review items.
CITING = ["AGENTS.md", CHECKLIST, DESIGN_SKILL, ".github/pull_request_template.md"]
# Entry documents whose backticked repository paths must exist. The design-tree
# skill is excluded: it names its roles generically for reuse in any project.
PATHS = ["AGENTS.md", "README.md", "README.zh-CN.md", CHECKLIST,
         ".github/pull_request_template.md"]

ITEM = r"(?:DC|[ACDGMRTV])\d+"
CITATION = re.compile(r"(?<![\w-])(" + ITEM + r")(?:\s*[-–]\s*(" + ITEM + r"|\d+))?(?![\w-])")
CHECKLIST_DEFINITION = re.compile(r"\*\*(" + ITEM + r") —")
SKILL_DEFINITION = re.compile(r"^(" + ITEM + r")\. ", re.M)
BACKTICKED = re.compile(r"`([^`\s]+)`")
PLACEHOLDER = re.compile(r"[<>*{}$]|\bvN\b|YYYY|\.\.\.")
FENCE = re.compile(r"^```.*?^```", re.M | re.S)


def line_of(text, offset):
    return text.count("\n", 0, offset) + 1


def prose(text):
    # Blank fenced blocks while keeping line numbers.
    return FENCE.sub(lambda m: "\n" * m[0].count("\n"), text)


def defined_items(root):
    items = set()
    checklist = root / CHECKLIST
    if checklist.is_file():
        items |= set(CHECKLIST_DEFINITION.findall(checklist.read_text()))
    design = root / DESIGN_SKILL
    if design.is_file():
        items |= set(SKILL_DEFINITION.findall(design.read_text()))
    return items


def cited_items(text):
    for match in CITATION.finditer(text):
        first, last = match[1], match[2]
        if last is None:
            yield match.start(), match[0], [first]
            continue
        prefix, low = re.fullmatch(r"([A-Z]+)(\d+)", first).groups()
        high = int(re.search(r"\d+$", last)[0])
        if last[0].isalpha() and not last.startswith(prefix):
            yield match.start(), match[0], [first, last]
        else:
            yield match.start(), match[0], [prefix + str(n) for n in range(int(low), high + 1)]


def distinct(root, names):
    seen = set()
    for name in names:
        path = root / name
        if path.is_file() and path.resolve() not in seen:
            seen.add(path.resolve())
            yield name, path


def item_findings(root):
    items = defined_items(root)
    findings = []
    for name, path in distinct(root, CITING):
        text = prose(path.read_text())
        for offset, written, ids in cited_items(text):
            for item in ids:
                if item not in items:
                    findings.append(f"{name}:{line_of(text, offset)}: {written} cites undefined review item {item}")
    return findings


def path_findings(root):
    findings = []
    generic = (root / DESIGN_SKILL).resolve()
    for name, path in distinct(root, PATHS):
        if path.resolve() == generic:
            continue
        text = prose(path.read_text())
        for match in BACKTICKED.finditer(text):
            token = match[1]
            if "/" not in token or PLACEHOLDER.search(token):
                continue
            if token.startswith(("http:", "https:", "-")) or "://" in token:
                continue
            if not ((root / token).exists() or (path.parent / token).exists()):
                findings.append(f"{name}:{line_of(text, match.start())}: `{token}` does not exist")
    return findings


def check(root):
    findings = item_findings(root) + path_findings(root)
    for finding in findings:
        print("guidance: " + finding, file=sys.stderr)
    if not findings:
        print("guidance: cited review items and entry-document paths resolve")
    return int(bool(findings))


class GuidanceTests(unittest.TestCase):
    def fixture(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "docs").mkdir()
        (root / "design/skill").mkdir(parents=True)
        (root / CHECKLIST).write_text("- [ ] **A1 — Fit.** x\n- [ ] **A2 — New.** x\n"
                                      "- [ ] **M1 — Design.** x\n")
        (root / DESIGN_SKILL).write_text("Record decisions.\n"
                                         "G1. Decision test.\nDC1. Decisions in code.\n")
        (root / "AGENTS.md").write_text("Use `docs/review-checklist.md`.\n")
        return root

    def test_clean_fixture(self):
        root = self.fixture()
        self.assertEqual(item_findings(root) + path_findings(root), [])

    def test_undefined_item_and_range(self):
        root = self.fixture()
        (root / "AGENTS.md").write_text("Checks A1, G1 and DC1.\nSee M1–M3 and A1-A2.\n")
        self.assertEqual(item_findings(root), [
            "AGENTS.md:2: M1–M3 cites undefined review item M2",
            "AGENTS.md:2: M1–M3 cites undefined review item M3"])

    def test_rule_ids_and_fences_are_not_items(self):
        root = self.fixture()
        (root / "AGENTS.md").write_text("OWN-7, PAR-2 and L0 are rules.\n```\nV9\n```\n")
        self.assertEqual(item_findings(root), [])

    def test_missing_backticked_path(self):
        root = self.fixture()
        (root / "AGENTS.md").write_text("See `tools/` and `conformance/`,\n"
                                        "`spec/kernel-spec-vN.md` and `lib/<name>/`.\n")
        self.assertEqual(path_findings(root), ["AGENTS.md:1: `tools/` does not exist",
                                               "AGENTS.md:1: `conformance/` does not exist"])


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
    else:
        sys.exit(check(Path(__file__).resolve().parent.parent))
