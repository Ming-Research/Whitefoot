"""Regressions for the structural tree gate and its CI baseline adapter.

The make design-lint caller maintains these fixtures with the checked tools;
replace them when those tools are replaced. No fixture edits the live tree.
"""
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
LINT = REPOSITORY / "design/skill/lint.py"
CI_BASE = REPOSITORY / ".github/design-review-base.sh"
DECISION = "Decision: Keep the fixture because it exercises the gate, instead of an unchecked change.\n"
LOG_HEADER = "# Design tree change log\n\n"
BASE_ENTRY = """## 2026-01-01 Baseline fixture

Nodes: language

Owner-approved: Fixture baseline approval.

Summary: Establish the test tree.
"""


class TreeGateTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="whitefoot-design-lint-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.git("init", "--quiet")
        self.git("config", "user.name", "Design lint fixture")
        self.git("config", "user.email", "design-lint@example.invalid")
        self.write("design/language.md", DECISION)
        self.write("design/log.md", LOG_HEADER + BASE_ENTRY)
        self.base = self.commit("Baseline")
        self.git("branch", "-M", "main")
        self.git("update-ref", "refs/remotes/origin/main", self.base)

    def git(self, *args):
        return subprocess.run(
            ["git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", *args],
            cwd=self.root, text=True, capture_output=True, check=True,
        ).stdout.strip()

    def write(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="ascii")

    def commit(self, message):
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", message)
        return self.git("rev-parse", "HEAD")

    def change_tree(self):
        self.write("design/language.md", DECISION.replace("Keep the fixture", "Change the fixture"))

    def log_change(self, approval="Fixture owner approved this revision.", nodes="language"):
        field = "" if approval is None else f"Owner-approved: {approval}\n\n"
        self.write("design/log.md", f"""# Design tree change log

## 2026-01-02 Changed fixture

Nodes: {nodes}

{field}Summary: Record the fixture change.

{BASE_ENTRY}""")

    def lint(self, base, require_no_amendments=False):
        command = [
            sys.executable, "-B", str(LINT), "--root", "design",
            "--trees", "language", "--base", base,
        ]
        if require_no_amendments:
            command.append("--require-no-amendments")
        return subprocess.run(
            command,
            cwd=self.root, text=True, capture_output=True,
        )

    def ci_base(self, event, ref, before=""):
        return subprocess.run(
            ["sh", str(CI_BASE), event, ref, before],
            cwd=self.root, text=True, capture_output=True,
        )

    def assert_rejected(self, result, diagnostic):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(diagnostic, result.stderr)

    def test_amendment_only_needs_no_tree_approval_log(self):
        self.write("design/amendments/proposal.md", "Node: language/proposal\n\n" + DECISION)
        result = self.lint(self.base)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_readiness_passes_without_an_amendments_path(self):
        result = self.lint(self.base, require_no_amendments=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_readiness_rejects_a_nonempty_amendments_path(self):
        self.write("design/amendments/proposal.md", "Node: language/proposal\n\n" + DECISION)
        self.assert_rejected(
            self.lint(self.base, require_no_amendments=True),
            "amendments: path exists",
        )

    def test_readiness_rejects_an_empty_amendments_path(self):
        (self.root / "design/amendments").mkdir()
        self.assertEqual(self.lint(self.base).returncode, 0)
        self.assert_rejected(
            self.lint(self.base, require_no_amendments=True),
            "amendments: path exists",
        )

    def test_readiness_does_not_hide_form_log_or_approval_checks(self):
        self.write("design/amendments/proposal.md", DECISION)
        self.change_tree()
        result = self.lint(self.base, require_no_amendments=True)
        self.assert_rejected(result, "amendments: path exists")
        self.assertIn("an amendment starts with 'Node: <tree path>'", result.stderr)
        self.assertIn("change log did not", result.stderr)

        self.log_change(approval=None)
        result = self.lint(self.base, require_no_amendments=True)
        self.assert_rejected(result, "amendments: path exists")
        self.assertIn("an amendment starts with 'Node: <tree path>'", result.stderr)
        self.assertIn("nonempty Owner-approved:", result.stderr)

    def test_direct_tree_edit_without_a_new_log_is_rejected(self):
        self.change_tree()
        self.assert_rejected(self.lint(self.base), "change log did not")

    def test_untracked_node_without_a_new_log_is_rejected(self):
        self.write("design/language/new-node.md", DECISION)
        self.assert_rejected(self.lint(self.base), "change log did not")

    def test_recorded_tree_change_passes(self):
        self.change_tree()
        self.log_change()
        result = self.lint(self.base)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_or_empty_approval_field_is_rejected(self):
        self.change_tree()
        for approval in (None, ""):
            with self.subTest(approval=approval):
                self.log_change(approval=approval)
                self.assert_rejected(self.lint(self.base), "nonempty Owner-approved:")

    def test_reused_old_log_entry_is_rejected(self):
        self.change_tree()
        self.write("design/log.md", LOG_HEADER + BASE_ENTRY.replace("Establish", "Update"))
        self.assert_rejected(self.lint(self.base), "newest log entry is not new")

    def test_repeated_log_entry_heading_is_rejected(self):
        self.write("design/log.md", LOG_HEADER + BASE_ENTRY + "\n" + BASE_ENTRY.replace("Establish", "Build"))
        self.assert_rejected(self.lint(self.base), "entry heading repeats log.md:3")

    def test_log_must_name_the_changed_node(self):
        self.change_tree()
        self.log_change(nodes="language/other")
        self.assert_rejected(self.lint(self.base), "language is not named")

    def test_missing_or_empty_explicit_base_fails_closed(self):
        self.change_tree()
        for base in ("missing-review-base", ""):
            with self.subTest(base=base):
                self.assert_rejected(self.lint(base), "review base")

    def test_noncommit_base_fails_closed(self):
        tree = self.git("rev-parse", "HEAD^{tree}")
        self.assert_rejected(self.lint(tree), "review base")

    def test_main_push_checks_before_even_when_origin_main_is_head(self):
        self.change_tree()
        head = self.commit("Unlogged tree edit")
        self.git("update-ref", "refs/remotes/origin/main", head)
        # This is the old CI wiring's vacuous comparison.
        self.assertEqual(self.lint("origin/main").returncode, 0)
        result = self.ci_base("push", "refs/heads/main", self.base)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), self.base)
        self.assert_rejected(self.lint(result.stdout.strip()), "change log did not")

    def test_main_push_rejects_missing_before_instead_of_using_head(self):
        for before in ("", "0" * 40, "missing-before"):
            with self.subTest(before=before):
                self.assert_rejected(
                    self.ci_base("push", "refs/heads/main", before), "cannot resolve",
                )

    def test_main_push_rejects_a_self_comparison(self):
        self.assert_rejected(self.ci_base("push", "refs/heads/main", self.base), "equals HEAD")

    def test_manual_main_run_uses_the_first_parent(self):
        self.change_tree()
        self.commit("Unlogged tree edit")
        result = self.ci_base("workflow_dispatch", "refs/heads/main")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), self.base)

    def test_work_branch_uses_its_fork_point_not_new_main_changes(self):
        self.change_tree()
        self.commit("Work branch edit")
        self.git("checkout", "--quiet", "-b", "new-main", self.base)
        self.write("unrelated.txt", "Main advanced.\n")
        newer_main = self.commit("Unrelated main change")
        self.git("update-ref", "refs/remotes/origin/main", newer_main)
        self.git("checkout", "--quiet", "main")
        result = self.ci_base("push", "refs/heads/work", self.base)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), self.base)

    def test_new_work_branch_can_share_the_main_tip(self):
        result = self.ci_base("push", "refs/heads/work", "0" * 40)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), self.base)

    def test_work_branch_with_no_main_base_fails(self):
        self.git("update-ref", "-d", "refs/remotes/origin/main")
        self.assertNotEqual(self.ci_base("push", "refs/heads/work").returncode, 0)

    def test_pull_request_uses_the_merge_base_of_main(self):
        self.change_tree()
        self.commit("Pull request edit")
        result = self.ci_base("pull_request", "refs/pull/1/merge")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), self.base)

    def test_unhandled_event_fails(self):
        self.assert_rejected(self.ci_base("unknown-event", "refs/heads/main"), "unsupported")


if __name__ == "__main__":
    unittest.main()
