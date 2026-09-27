---
name: spec-amendment
description: Amend the Whitefoot kernel specification as one change - archive the outgoing version, retitle the next, bring conformance cases, syntax data, tests and docs along, and explain the rule changes. Use when a task edits spec/kernel-spec.md or merges main into a branch that already amends it. Not for reading or citing the specification.
---

# Specification amendment

AGENTS.md owns the specification-integrity rules. This procedure lands an
amendment as one change; `make static` checks its archive and title.

1. **Archive once per branch.** Copy the base's active bytes to the archive
   named by their title token. For a base titled `# Kernel Specification v0.69`:

   ```sh
   git show "$(git merge-base main HEAD)":spec/kernel-spec.md > spec/kernel-spec-v0.69.md
   ```

   Keep the local `main` ref current first. Never edit or remove an archive.
2. **Retitle** the active file to the next version: `v0.70`, or `v1.0` for a
   major revision. `compiler/build.rs` derives the identity and grammar tables
   from the bytes; nothing else records the version.
3. **Edit the rules.** State each normative fact once, as total positive rules
   or table data without exception clauses; define each rule ID once and keep
   bracketed references resolving; a surface name labels a checked invariant.
   A rule chosen among viable alternatives is a language decision: record it
   with the `design-tree` skill.
4. **Bring derived material along in the same work:** conformance cases,
   verdicts and manifest entries (a changed expectation states its normative
   ground in the PR), the parser, lexer and generated syntax data, compiler
   tests, examples in `docs/patterns.md` and elsewhere, and live references to
   renamed or retired rule IDs. Archives keep their text.
5. **Resolve a collision.** If main advanced the version while this branch was
   open, merging main conflicts on the title. Keep main's text, reapply this
   branch's rule changes, archive main's active bytes under main's version, and
   retitle to the version after it.
6. **Check.** `make static` verifies the archive name, its bytes and the title
   (`spec-archives`), and rule IDs, references and coverage (`conformance`).
   Run `make check` for the full effect. A green run does not show that derived
   material follows the new rules; review item T1 checks that.
7. **Report.** The PR states what changed and its selection ground (AGENTS.md
   rule 4). The owner handoff explains which rules changed, their before and
   after behavior, and why they were selected.
