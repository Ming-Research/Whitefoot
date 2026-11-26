#!/usr/bin/env python3
"""Check supported repository input references, not compiler semantics.

Recognizes literal paths in source/build/script lines, resolvable relative
paths and symlinks, and explicit manual-only Make/workflow boundaries. It does
not evaluate arbitrary shell, Make or Rust. Completion review owns unresolved
dynamic/transitive inputs. Remove this checker only if that early diagnostic
has a maintained replacement. No checkout content is hidden or removed.
"""

from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest


FORBIDDEN = "research"
SUFFIXES = {".rs", ".c", ".h", ".ll", ".py", ".sh", ".pl", ".ps1",
            ".toml", ".mk", ".yml", ".yaml", ".awk"}
PATH = re.compile(r"(?:[\w.${}()@+-]+/)+(?:[\w.*${}()@+-]+)?")
QUOTED = re.compile(r'''["']([^"'\n]+)["']''')
TARGET = re.compile(r"^([\w./_-]+(?:[ \t]+[\w./_-]+)*):(?!=)")
MANUAL = "# research-boundary: manual-only"


def manual_workflow(text):
    match = re.search(r"^on:(.*?)(?=^\S|\Z)", text, re.M | re.S)
    if not match:
        return False
    block = match.group(1)
    inline = block.splitlines()[0].strip()
    if inline:
        events = re.findall(r"[a-z_]+", inline)
    else:
        events = re.findall(r"^  ([a-z_]+):", block, re.M)
    return bool(events) and set(events) == {"workflow_dispatch"}


def code_lines(path, text):
    # Preserve line numbers while removing source documentation. These forms
    # cover the maintained Rust/C inputs; this is not a general language parser.
    if path.suffix in {".rs", ".c", ".h"}:
        text = re.sub(r"/\*.*?\*/", lambda m: "\n" * m[0].count("\n"),
                      text, flags=re.S)
    for number, line in enumerate(text.splitlines(), 1):
        stripped = line.lstrip()
        if stripped.startswith(("//", ";")):
            continue
        if stripped.startswith("#") and path.suffix not in {".c", ".h", ".rs"}:
            continue
        if path.suffix in {".rs", ".c", ".h"}:
            line = re.sub(r"\s+//.*$", "", line)
        yield number, line


def within_forbidden(path, root):
    try:
        return path.resolve().is_relative_to((root / FORBIDDEN).resolve())
    except (OSError, RuntimeError):
        return False


def manual_make_lines(text):
    marked, names, lines = False, set(), set()
    current = False
    for number, line in enumerate(text.splitlines(), 1):
        if line.strip() == MANUAL:
            marked = True
        match = TARGET.match(line)
        if match:
            current = marked
            marked = False
            if current:
                names.update(match[1].split())
        if current:
            lines.add(number)
    return names, lines


def references(path, root, text):
    if path.suffix in {".yml", ".yaml"} and manual_workflow(text):
        return []
    manual_names, manual_lines = (manual_make_lines(text)
                                  if path.name == "Makefile" else (set(), set()))
    findings = []
    phony_continuation = False
    for number, line in code_lines(path, text):
        if path.name == "Makefile" and (line.startswith(".PHONY:") or phony_continuation):
            phony_continuation = line.rstrip().endswith("\\")
            continue
        if number in manual_lines:
            continue
        # A manually invoked recipe cannot become a daily prerequisite or be
        # called from an automatic recipe while retaining its exemption.
        if not line.startswith(".PHONY:"):
            for name in manual_names:
                if re.search(r"(?<![\w-])" + re.escape(name) + r"(?![\w-])", line):
                    findings.append((number, "automatic reference to manual target " + name))
        quoted_paths = {token for token in QUOTED.findall(line)
                        if "/" in token or "\\" in token or Path(token).suffix}
        tokens = set(PATH.findall(line)) | quoted_paths
        for token in sorted(tokens):
            if re.search(r"(?:^|[/\\])" + FORBIDDEN + r"[/\\]", token):
                findings.append((number, token))
                continue
            # Check literal inputs and directory aliases. Unknown variables
            # stay outside this recognizer and remain part of review T6.
            token = re.sub(r"^\$\((?:ROOT|NATIVE_ROOT)\)/", "", token)
            if any(c in token for c in "$*{}()"):
                continue
            for base in (path.parent, root):
                candidate = base / token
                if within_forbidden(candidate, root):
                    findings.append((number, token + " -> " + str(candidate.resolve().relative_to(root.resolve()))))
                    break
    return findings


# The wf-agents engine's declaration names the directory where its agents keep
# what they make for research (`project.research`): a place they write to,
# which no build, test or workflow reads as an input.
DECLARATION = Path("wf-agents.toml")


def selected(path):
    parts = path.parts
    return path != DECLARATION and (path.name == "Makefile" or path.suffix in SUFFIXES) and (
        len(parts) == 1 or parts[0] in {"compiler", "tests", "lib", ".github"})


def check(root):
    output = subprocess.check_output(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], cwd=root)
    failed = False
    for name in sorted(set(output.decode().split("\0")) - {""}):
        relative = Path(name)
        path = root / relative
        if relative.parts[0] in {"compiler", "tests", "lib", ".github"} and path.is_symlink():
            if within_forbidden(path, root):
                print(f"{name}: symlink input resolves into {FORBIDDEN}", file=sys.stderr)
                failed = True
        if not selected(relative) or not path.is_file():
            continue
        for number, destination in references(path, root, path.read_text()):
            print(f"{name}:{number}: forbidden input: {destination}", file=sys.stderr)
            failed = True
    if not failed:
        print("research inputs: supported formal source/build/workflow references are clear")
    return int(failed)


class ReferenceTests(unittest.TestCase):
    def test_literals_helpers_and_relative_inputs(self):
        root = Path("/tmp/reference-check-fixture")
        for filename, line in [
            ("compiler/src/lib.rs", 'include_str!("../../' + FORBIDDEN + '/case.wf");'),
            ("compiler/Makefile", '\t$(MAKE) -C $(ROOT)/' + FORBIDDEN + '/experiment check'),
            (".github/helper.sh", 'sh ' + FORBIDDEN + '/experiment/run.sh'),
            ("lib/native.mk", 'include ../../' + FORBIDDEN + '/native.mk'),
            ("compiler/src/caller.c", '#include "../../' + FORBIDDEN + '/oracle.c"'),
        ]:
            with self.subTest(filename=filename):
                self.assertTrue(references(root / filename, root, line))

    def test_bare_words_are_not_input_paths(self):
        root = Path("/tmp/reference-check-fixture")
        self.assertFalse(references(root / "checker.py", root,
                                    'FORBIDDEN = "' + FORBIDDEN + '"'))

    def test_comments_remain_citations(self):
        root = Path("/tmp/reference-check-fixture")
        citation = FORBIDDEN + "/investigations/example.md"
        self.assertFalse(references(root / "compiler/src/lib.rs", root,
                                    "// " + citation + "\n/* " + citation + " */"))
        self.assertFalse(references(root / "Makefile", root, "# " + citation))

    def test_symlink_input(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / FORBIDDEN).mkdir()
            (root / FORBIDDEN / "case.wf").write_text("fixture")
            (root / "fixture.wf").symlink_to(root / FORBIDDEN / "case.wf")
            self.assertTrue(references(root / "lib.rs", root, 'include_str!("fixture.wf");'))

    def test_manual_workflow_requires_only_dispatch(self):
        command = "jobs:\n  test:\n    run: sh " + FORBIDDEN + "/run.sh\n"
        root = Path("/tmp/reference-check-fixture")
        path = root / ".github/workflows/manual.yml"
        self.assertFalse(references(path, root, "on:\n  workflow_dispatch:\n" + command))
        self.assertTrue(references(path, root, "on:\n  push:\n  workflow_dispatch:\n" + command))
        self.assertTrue(references(path, root, "on: [pull_request, workflow_dispatch]\n" + command))

    def test_the_engines_declaration_is_no_input(self):
        self.assertFalse(selected(Path("wf-agents.toml")))
        self.assertTrue(selected(Path("other.toml")))
        self.assertTrue(selected(Path("compiler/wf-agents.toml")))

    def test_mixed_make_and_manual_target_call(self):
        root = Path("/tmp/reference-check-fixture")
        text = MANUAL + "\nhistorical:\n\tsh " + FORBIDDEN + "/run.sh\ncheck:\n\ttrue\n"
        self.assertFalse(references(root / "Makefile", root, text))
        self.assertTrue(references(root / "Makefile", root, text + "\t$(MAKE) historical\n"))
        self.assertTrue(references(root / "Makefile", root,
                                   text + "\tsh " + FORBIDDEN + "/other.sh\n"))


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        unittest.main(argv=[sys.argv[0]])
    else:
        sys.exit(check(Path(__file__).resolve().parent.parent))
