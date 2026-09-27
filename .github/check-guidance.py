#!/usr/bin/env python3
"""Check that the agent guidance's references resolve, not what it says.

- Review item IDs cited in the guidance (A1, T4-T7, G3, DC2, ...) are defined
  in docs/review-checklist.md or the design-tree skill.
- Repository paths written in backticks in the entry documents exist.
- Every skill is discoverable by both supported agents: each entry under
  .agents/skills (Codex) and .claude/skills (Claude Code) is a link to the same
  skill directory elsewhere in the project, and its SKILL.md names that entry
  and says when to use it and when not to.
- The workflow map names every CI workflow, skill and guidance document, so it
  cannot silently fall behind the process it maps.

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
MAP = "docs/workflow.md"
# Documents that cite review items. Skills are added from the skill directory.
CITING = ["AGENTS.md", CHECKLIST, MAP, DESIGN_SKILL,
          ".github/pull_request_template.md"]
# Entry documents whose backticked repository paths must exist. The design-tree
# skill is excluded: it names its roles generically for reuse in any project.
PATHS = ["AGENTS.md", "README.md", CHECKLIST, MAP,
         ".github/pull_request_template.md"]
# Paths that exist only in some states of the tree.
TRANSIENT = {"design/amendments/"}
SKILL_ROOTS = (".agents/skills", ".claude/skills")

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


def skill_files(root):
    found = {}
    directory = root / SKILL_ROOTS[0]
    if directory.is_dir():
        for entry in sorted(directory.iterdir()):
            skill = entry / "SKILL.md"
            if skill.is_file():
                found.setdefault(skill.resolve(), skill.relative_to(root).as_posix())
    return sorted(found.values())


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
    # A skill linked into a skill directory is read once, under its first name.
    seen = set()
    for name in names:
        path = root / name
        if path.is_file() and path.resolve() not in seen:
            seen.add(path.resolve())
            yield name, path


def item_findings(root):
    items = defined_items(root)
    findings = []
    for name, path in distinct(root, CITING + skill_files(root)):
        text = prose(path.read_text())
        for offset, written, ids in cited_items(text):
            for item in ids:
                if item not in items:
                    findings.append(f"{name}:{line_of(text, offset)}: {written} cites undefined review item {item}")
    return findings


def path_findings(root):
    findings = []
    generic = (root / DESIGN_SKILL).resolve()
    for name, path in distinct(root, PATHS + skill_files(root)):
        if path.resolve() == generic:
            continue
        text = prose(path.read_text())
        for match in BACKTICKED.finditer(text):
            token = match[1]
            if "/" not in token or token in TRANSIENT or PLACEHOLDER.search(token):
                continue
            if token.startswith(("http:", "https:", "-")) or "://" in token:
                continue
            if not ((root / token).exists() or (path.parent / token).exists()):
                findings.append(f"{name}:{line_of(text, match.start())}: `{token}` does not exist")
    return findings


def frontmatter(text):
    if not text.startswith("---\n"):
        return None
    end = text.find("\n---\n", 4)
    if end < 0:
        return None
    fields = {}
    for line in text[4:end].splitlines():
        key, colon, value = line.partition(":")
        if colon and not line.startswith((" ", "\t")):
            fields[key.strip()] = value.strip()
    return fields


def skill_findings(root):
    findings = []
    entries = {}
    for skill_root in SKILL_ROOTS:
        directory = root / skill_root
        entries[skill_root] = ({e.name: e for e in directory.iterdir()}
                               if directory.is_dir() else {})
    codex, claude = (entries[r] for r in SKILL_ROOTS)
    for name in sorted(set(codex) | set(claude)):
        if name not in codex or name not in claude:
            missing = SKILL_ROOTS[0] if name not in codex else SKILL_ROOTS[1]
            findings.append(f"skill {name}: no {missing}/{name} entry; both agents must discover every skill")
            continue
        if not (codex[name].is_symlink() and claude[name].is_symlink()):
            findings.append(f"skill {name}: both entries must link to a skill directory kept in the project")
            continue
        if codex[name].resolve() != claude[name].resolve():
            findings.append(f"skill {name}: {SKILL_ROOTS[0]}/{name} and {SKILL_ROOTS[1]}/{name} resolve to different directories")
            continue
        home = codex[name].resolve()
        inside = [p for p in (root.resolve(), *(root.resolve() / r for r in SKILL_ROOTS))
                  if home == p or p in home.parents]
        if inside != [root.resolve()]:
            findings.append(f"skill {name}: its directory must sit in the project, outside the agents' skill directories")
            continue
        skill = codex[name] / "SKILL.md"
        if not skill.is_file():
            findings.append(f"skill {name}: {skill.relative_to(root).as_posix()} does not exist")
            continue
        fields = frontmatter(skill.read_text())
        if fields is None:
            findings.append(f"skill {name}: SKILL.md must open with '---' frontmatter")
            continue
        if fields.get("name") != name or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name) or len(name) > 64:
            findings.append(f"skill {name}: frontmatter name must equal the entry name in lowercase-hyphen form")
        description = fields.get("description", "")
        if not description or len(description) > 1024:
            findings.append(f"skill {name}: frontmatter needs a description of at most 1024 characters")
        elif "Use when" not in description or "Not for" not in description:
            findings.append(f"skill {name}: the description must say when to use the skill ('Use when') and when not ('Not for')")
    return findings


def map_findings(root):
    path = root / MAP
    if not path.is_file():
        return [f"{MAP} does not exist"]
    text = path.read_text()
    expected = [(name, f"document {name}") for name in ("AGENTS.md", "README.md")]
    for document in sorted((root / "docs").glob("*.md")):
        expected.append((f"docs/{document.name}", f"document docs/{document.name}"))
    for workflow in sorted((root / ".github/workflows").glob("*.yml")):
        expected.append((workflow.name, f"workflow {workflow.name}"))
    skills = root / SKILL_ROOTS[0]
    for skill in sorted(skills.iterdir()) if skills.is_dir() else []:
        expected.append((f"`{skill.name}`", f"skill {skill.name}"))
    return [f"{MAP}: {what} is not on the map" for token, what in expected
            if token not in text]


def check(root):
    findings = (item_findings(root) + path_findings(root) + skill_findings(root)
                + map_findings(root))
    for finding in findings:
        print("guidance: " + finding, file=sys.stderr)
    if not findings:
        print("guidance: cited review items, entry-document paths, skill links and the workflow map resolve")
    return int(bool(findings))


DESCRIPTION = "Do a thing. Use when the thing is due. Not for other things."


class GuidanceTests(unittest.TestCase):
    def fixture(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        (root / "docs").mkdir()
        (root / "design/skill").mkdir(parents=True)
        (root / CHECKLIST).write_text("- [ ] **A1 — Fit.** x\n- [ ] **A2 — New.** x\n"
                                      "- [ ] **M1 — Design.** x\n")
        (root / DESIGN_SKILL).write_text("---\nname: design-tree\ndescription: " + DESCRIPTION + "\n---\n"
                                         "Keep proposals in `amendments/`.\n"
                                         "G1. Decision test.\nDC1. Decisions in code.\n")
        (root / "AGENTS.md").write_text("Use `docs/review-checklist.md`.\n")
        for skill_root in SKILL_ROOTS:
            (root / skill_root).mkdir(parents=True)
        (root / ".agents/skills/design-tree").symlink_to("../../design/skill")
        (root / ".claude/skills/design-tree").symlink_to("../../.agents/skills/design-tree")
        return root

    def test_clean_fixture(self):
        root = self.fixture()
        self.assertEqual(item_findings(root) + path_findings(root) + skill_findings(root), [])

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
        (root / "AGENTS.md").write_text("See `tools/` and `conformance/`, `design/amendments/`,\n"
                                        "`spec/kernel-spec-vN.md` and `lib/<name>/`.\n")
        self.assertEqual(path_findings(root), ["AGENTS.md:1: `tools/` does not exist",
                                               "AGENTS.md:1: `conformance/` does not exist"])

    def test_workflow_map_names_every_workflow_skill_and_document(self):
        root = self.fixture()
        (root / ".github/workflows").mkdir(parents=True)
        (root / ".github/workflows/gate.yml").write_text("on: push\n")
        (root / ".github/workflows/bench.yml").write_text("on: workflow_dispatch\n")
        (root / MAP).write_text("AGENTS.md README.md docs/review-checklist.md "
                                "docs/workflow.md gate.yml bench.yml\n")
        self.assertEqual(map_findings(root), [f"{MAP}: skill design-tree is not on the map"])
        (root / MAP).write_text("AGENTS.md docs/workflow.md `design-tree` gate.yml\n")
        self.assertEqual(map_findings(root), [
            f"{MAP}: document README.md is not on the map",
            f"{MAP}: document docs/review-checklist.md is not on the map",
            f"{MAP}: workflow bench.yml is not on the map"])

    def test_skill_bodies_live_in_the_project_behind_links(self):
        root = self.fixture()
        inline = root / ".agents/skills/inline"
        inline.mkdir()
        (inline / "SKILL.md").write_text("---\nname: inline\ndescription: " + DESCRIPTION + "\n---\n")
        (root / ".claude/skills/inline").symlink_to("../../.agents/skills/inline")
        (root / "vague").mkdir()
        (root / "vague/SKILL.md").write_text("---\nname: vague\ndescription: Helps with things.\n---\n")
        for skill_root in SKILL_ROOTS:
            (root / skill_root / "vague").symlink_to("../../vague")
        self.assertEqual(skill_findings(root), [
            "skill inline: both entries must link to a skill directory kept in the project",
            "skill vague: the description must say when to use the skill ('Use when') and when not ('Not for')"])

    def test_skill_missing_for_one_agent(self):
        root = self.fixture()
        (root / "extra").mkdir()
        (root / "extra/SKILL.md").write_text("---\nname: extra\ndescription: " + DESCRIPTION + "\n---\n")
        (root / ".agents/skills/extra").symlink_to("../../extra")
        self.assertEqual(skill_findings(root), [
            "skill extra: no .claude/skills/extra entry; both agents must discover every skill"])

    def test_skill_links_disagree_or_misname(self):
        root = self.fixture()
        for name, declared in (("one", "one"), ("two", "other")):
            (root / name).mkdir()
            (root / name / "SKILL.md").write_text(f"---\nname: {declared}\ndescription: {DESCRIPTION}\n---\n")
        (root / ".agents/skills/one").symlink_to("../../one")
        (root / ".claude/skills/one").symlink_to("../../two")
        (root / ".agents/skills/two").symlink_to("../../two")
        (root / ".claude/skills/two").symlink_to("../../two")
        self.assertEqual(skill_findings(root), [
            "skill one: .agents/skills/one and .claude/skills/one resolve to different directories",
            "skill two: frontmatter name must equal the entry name in lowercase-hyphen form"])


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
    else:
        sys.exit(check(Path(__file__).resolve().parent.parent))
