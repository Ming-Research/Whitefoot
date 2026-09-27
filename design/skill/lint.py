#!/usr/bin/env python3
"""Structural lint for the design tree: form only. Its messages say what it checks."""
import argparse
import os
import re
import subprocess
import sys

DECISION_MARKERS = (" because ", " instead of ")
LOG_ENTRY = re.compile(r"^## \d{4}-\d{2}-\d{2} \S")
LOG_REQUIRED = ("Nodes:", "Summary:")
OWNER_APPROVED = "Owner-approved:"
FORBIDDEN_HEADINGS = ("## Facts", "## Moves")
DATED_LINE = re.compile(r"^- 20\d\d-\d\d-\d\d")
REJECTED_ITEM = re.compile(r"^- (.+?): rejected because (\S.*)$")
DATE = re.compile(r"\b20[0-9]{2}-[0-9]{2}-[0-9]{2}\b")
FIELDS = ("Decision:", "Rejected:")
AMENDMENT_NODE = re.compile(r"^Node: ([^/\\\x00-\x1f\x7f]+(?:/[^/\\\x00-\x1f\x7f]+)*)$")


class Lint:
    def __init__(self, root, trees):
        self.root = root
        self.trees = trees
        self.errors = []
        self.nodes = {}  # node path (relative to root, no .md) -> lines
        self.decisions = 0
        self.rejected = 0
        self.base_metrics = None

    def err(self, where, msg):
        self.errors.append(f"{where}: {msg}")

    # ---- discovery -----------------------------------------------------

    def discover(self):
        for tree in self.trees:
            self.discover_tree(tree)
        self.discover_amendments()

    def discover_amendments(self):
        self.amendments = {}
        amend_dir = os.path.join(self.root, "amendments")
        if not os.path.isdir(amend_dir):
            return
        for name in sorted(os.listdir(amend_dir)):
            path = os.path.join(amend_dir, name)
            rel = os.path.relpath(path, self.root)
            if not name.endswith(".md") or not os.path.isfile(path):
                self.err(rel, "only amendment files (.md) belong under amendments")
                continue
            self.amendments[name[:-3]] = self.read(path)

    def discover_tree(self, tree):
        tree_md = os.path.join(self.root, tree + ".md")
        tree_dir = os.path.join(self.root, tree)
        if not os.path.isfile(tree_md):
            self.err(tree_md, "missing root node")
            return
        self.nodes[tree] = self.read(tree_md)
        if not os.path.isdir(tree_dir):
            return
        for dirpath, dirnames, filenames in os.walk(tree_dir):
            rel_dir = os.path.relpath(dirpath, self.root)
            parent_md = os.path.join(self.root, rel_dir + ".md")
            if not os.path.isfile(parent_md):
                self.err(rel_dir, "directory has no sibling node file")
            for name in sorted(filenames):
                path = os.path.join(dirpath, name)
                rel = os.path.relpath(path, self.root)
                if not name.endswith(".md"):
                    self.err(rel, "only node files (.md) belong under a tree")
                    continue
                self.nodes[rel[:-3]] = self.read(path)
            dirnames.sort()

    def read(self, path):
        with open(path, "rb") as handle:
            data = handle.read()
        rel = os.path.relpath(path, self.root)
        for number, raw in enumerate(data.split(b"\n"), 1):
            if any(byte > 127 for byte in raw):
                self.err(f"{rel}:{number}", "non-ASCII text; the tree is English only")
        return data.decode("ascii", errors="replace").split("\n")

    # ---- node form -----------------------------------------------------

    def check_nodes(self):
        stems = {}
        for path in self.nodes:
            stem = path.rsplit("/", 1)[-1]
            stems.setdefault(stem, []).append(path)
        for stem, paths in stems.items():
            if len(paths) > 1:
                self.err(stem, "node name used more than once: " + ", ".join(paths))
        for path, lines in sorted(self.nodes.items()):
            self.check_node(path, lines, stems)

    def check_node(self, path, lines, stems, count=True):
        where = path + ".md"
        decisions = 0
        section = None
        rejected = []
        prev = "start"  # start | blank | field | item
        seen_field = False
        for number, line in enumerate(lines, 1):
            loc = f"{where}:{number}"
            if DATE.search(line):
                self.err(loc, "a date; a node holds no history, only the live decision")
            if not line.strip():
                section = None
                prev = "blank"
                continue
            is_field = line.startswith(FIELDS)
            is_item = line.startswith("- ")
            if is_field:
                if prev not in ("blank", "start"):
                    self.err(loc, "a field must be separated from the previous line by a blank line")
                seen_field = True
                prev = "field"
            elif is_item:
                if prev not in ("field", "item"):
                    self.err(loc, "a list item must directly follow its header or the previous item")
                prev = "item"
            else:
                prev = "other"
            if line.startswith("Decision:"):
                decisions += 1
                low = line.lower()
                if not any(marker in low for marker in DECISION_MARKERS):
                    self.err(loc, "decision has neither 'because' nor 'instead of'")
                section = None
                continue
            if line.startswith("Rejected:"):
                section = "rejected"
                continue
            if line.startswith(FORBIDDEN_HEADINGS) or DATED_LINE.match(line):
                self.err(loc, "history belongs in git and the change log, not in a node")
                continue
            if line.startswith("- "):
                body = line[2:]
                if section == "rejected":
                    if not REJECTED_ITEM.match(line):
                        self.err(loc, "rejected entry needs '- <alternative>: rejected because <reason>'")
                    rejected.append(body)
                    continue
                self.err(loc, "list item outside Rejected:")
                continue
            self.err(loc, "line outside the node template")
        if decisions == 0:
            self.err(where, "node has no Decision: line")
        if count:
            self.decisions += decisions
            self.rejected += len(rejected)

    # ---- amendments ---------------------------------------------------

    def check_amendments(self):
        for name, lines in self.amendments.items():
            where = f"amendments/{name}"
            node = AMENDMENT_NODE.fullmatch(lines[0]) if lines else None
            if not node or any(part in (".", "..") or not part.strip()
                               for part in node.group(1).split("/")):
                self.err(where + ".md:1", "an amendment starts with 'Node: <tree path>' naming the node it amends or adds")
                continue
            if len(lines) < 2 or lines[1].strip():
                self.err(where + ".md:2", "a blank line separates the Node line from the fields")
                continue
            self.check_node(where, [""] * 2 + lines[2:], set(), count=False)

    def check_no_amendments(self):
        if os.path.lexists(os.path.join(self.root, "amendments")):
            self.err("amendments", "path exists; design readiness requires its removal")

    # ---- log -----------------------------------------------------------

    def check_log(self):
        path = os.path.join(self.root, "log.md")
        if not os.path.isfile(path):
            self.err("log.md", "missing change log")
            return []
        lines = self.read(path)
        entries = []
        current = None
        for number, line in enumerate(lines, 1):
            if line.startswith("## "):
                if not LOG_ENTRY.match(line):
                    self.err(f"log.md:{number}", "entry heading must be '## YYYY-MM-DD <title>'")
                current = {"line": number, "heading": line, "fields": {}}
                entries.append(current)
                continue
            if current is None:
                continue
            for field in LOG_REQUIRED + (OWNER_APPROVED,):
                if line.startswith(field):
                    current["fields"][field] = line[len(field):].strip()
        first_line = {}
        for entry in entries:
            loc = f"log.md:{entry['line']}"
            for field in LOG_REQUIRED:
                if field not in entry["fields"] or not entry["fields"][field]:
                    self.err(loc, f"entry lacks {field}")
            # A union merge keeps both sides of an entry edited on two branches.
            if entry["heading"] in first_line:
                self.err(loc, f"entry heading repeats log.md:{first_line[entry['heading']]}")
            first_line.setdefault(entry["heading"], entry["line"])
        return entries

    def base_exists(self, base):
        probe = subprocess.run(["git", "rev-parse", "--verify", "--quiet",
                                "--end-of-options", f"{base}^{{commit}}"],
                               cwd=self.root, capture_output=True, text=True)
        return probe.returncode == 0

    def check_diff(self, base, entries):
        names = subprocess.run(["git", "diff", "--name-only", "-z", "--relative", base, "--", "."],
                               cwd=self.root, capture_output=True, text=True,
                               check=True).stdout.split("\0")
        untracked = subprocess.run(
            ["git", "ls-files", "--others", "--exclude-standard", "-z", "--", "."],
            cwd=self.root, capture_output=True, text=True, check=True).stdout.split("\0")
        names = list(dict.fromkeys(name for name in names + untracked if name))
        changed = set()
        for rel in names:
            if any(rel == tree + ".md" or rel.startswith(tree + "/") for tree in self.trees):
                if rel.endswith(".md"):
                    changed.add(rel[:-3])
        if not changed:
            return
        log_rel = "log.md"
        if log_rel not in names:
            self.err("log.md", "tree changed since base but the change log did not")
            return
        if not entries:
            self.err("log.md", "tree changed since base but the change log has no entry")
            return
        newest = entries[0]
        base_log = subprocess.run(["git", "show", f"{base}:./{log_rel}"],
                                  cwd=self.root, capture_output=True, text=True)
        base_headings = set()
        if base_log.returncode == 0:
            base_headings = {line for line in base_log.stdout.splitlines()
                             if line.startswith("## ")}
        if newest["heading"] in base_headings:
            self.err("log.md", "tree changed since base but the newest log entry is not new")
        approval = newest["fields"].get(OWNER_APPROVED)
        if not approval:
            self.err(f"log.md:{newest['line']}",
                     "newest entry for a tree change lacks a nonempty Owner-approved: field")
        nodes = {node.strip() for node in newest["fields"].get("Nodes:", "").split(",")
                 if node.strip()}
        for node in sorted(changed):
            if node not in nodes:
                self.err("log.md", f"changed node {node} is not named in the newest log entry")

    # ---- metrics -------------------------------------------------------

    def measure_base(self, base):
        """The same counts at the review base, so a tree diff review can report net change."""
        listing = subprocess.run(["git", "ls-tree", "-r", "--name-only", "-z", base, "--", "."],
                                 cwd=self.root, capture_output=True, text=True,
                                 check=True).stdout.split("\0")
        counter = Lint(self.root, self.trees)
        paths = []
        for rel in listing:
            if not rel.endswith(".md"):
                continue
            if not any(rel == tree + ".md" or rel.startswith(tree + "/") for tree in self.trees):
                continue
            text = subprocess.run(["git", "show", f"{base}:./{rel}"], cwd=self.root,
                                 capture_output=True, text=True, check=True).stdout
            counter.check_node(rel[:-3], text.split("\n"), set())
            paths.append(rel[:-3])
        return {"nodes": len(paths),
                "depth": max((path.count("/") for path in paths), default=0),
                "decisions": counter.decisions,
                "rejected": counter.rejected}

    def metrics(self):
        depth = max((path.count("/") for path in self.nodes), default=0)
        per_subtree = {}
        for path in self.nodes:
            parts = path.split("/")
            key = parts[0] if len(parts) == 1 else "/".join(parts[:2])
            per_subtree[key] = per_subtree.get(key, 0) + 1
        base = self.base_metrics

        def show(name, value):
            if base is None:
                return f"{name}: {value}"
            return f"{name}: {value} (base {base[name]}, {value - base[name]:+d})"

        print(f"{show('nodes', len(self.nodes))}  {show('depth', depth)}  "
              f"{show('decisions', self.decisions)}  {show('rejected', self.rejected)}  "
              f"amendments: {len(self.amendments)}")
        for name, count in sorted(per_subtree.items()):
            print(f"  {name}: {count}")


def main():
    parser = argparse.ArgumentParser(description="structural lint for the design tree")
    parser.add_argument("--root", default="design")
    parser.add_argument("--trees", nargs="+", required=True,
                        help="top-level concept names to check")
    parser.add_argument("--base", default=None)
    parser.add_argument("--require-no-amendments", action="store_true",
                        help="fail when the amendments path exists")
    args = parser.parse_args()
    lint = Lint(args.root, args.trees)
    lint.discover()
    lint.check_nodes()
    lint.check_amendments()
    if args.require_no_amendments:
        lint.check_no_amendments()
    entries = lint.check_log()
    if args.base is not None:
        if lint.base_exists(args.base):
            lint.check_diff(args.base, entries)
            lint.base_metrics = lint.measure_base(args.base)
        else:
            lint.err("review base", f"{args.base!r} does not resolve to a commit; cannot check tree changes")
    if lint.errors:
        for error in lint.errors:
            print("error: " + error, file=sys.stderr)
        print(f"design lint: {len(lint.errors)} error(s)", file=sys.stderr)
        return 1
    lint.metrics()
    print("design lint: ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
