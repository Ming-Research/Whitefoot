---
name: completion-review
description: Finish a Whitefoot task - run the checks, get one independent review at the depth the change needs, route its findings, and publish the result to the PR. Use when about to mark a PR ready or report a task complete, or when asked for a review, completion review or dcr. Not for opening a PR or pushing work in progress.
---

# Completion review

One review per task, at completion: before the PR is marked ready or the task
is reported done, and whenever the owner asks for `review` or `dcr`. It is also
the design-tree skill's DCR. Opening a PR or pushing progress needs none.
`docs/review-checklist.md` holds the items; this skill owns when the review
runs, who runs it, how deep it goes, how findings are routed and where the
report goes.

## Steps

1. **Validate.** Run the checks the change needs (checklist V1): `make static`
   for any change, the README's focused commands for code, and `make check` or
   the hosted gate on the revision to be merged. Keep the commands, results
   and tested revision for the report.
2. **Scope.** `make review-scope` prints the base and head, the changed areas,
   the depth, the applicable checklist groups and the archived specification
   copies excluded from review input.
3. **Review.** Start a separate, read-only agent that did not implement the
   change, sized by the depth `make review-scope` prints:
   - **full** for any change to code, tests, the specification, gate wiring,
     the design tree or agent guidance: a mid-sized model, every applicable
     group;
   - **light** when only research records or other prose changed: a small
     model, groups A, D, M and V, plus R for a material choice.

   Substantive findings have come from code, specification, test and design
   changes; prose-only reviews have found wording. Give the reviewer the
   prompt below, filled in.
4. **Route every finding** by what resolving it would change:

   | Resolving the finding would change | Action |
   |---|---|
   | a design decision or amendment, a specification rule, or the agreed scope | Present it to the owner with your assessment and recommended response (owner-handoff skill) and wait for direction, including during unattended work. |
   | anything else: form, wording, a broken reference, missing coverage or evidence, placement | Fix it, recheck the affected items and list it in the report. |

   The implementing agent rechecks a wording or mechanical fix itself and says
   so; the reviewer rechecks a substantive fix. A clean review approves no
   amendment and does not make the PR ready.
5. **Review changed content only.** Later edits invalidate the review of the
   content they touch. Merging main without conflicts in reviewed content
   needs no new review, since the gate covers it; a resolved conflict is
   reviewed as changed content, those hunks only.
6. **Publish.** Commit and push, verify that the remote head is the reviewed
   revision, and update the PR description, with the Agent review section in
   the template's Scope / Checks / Findings form; that is the report, with no
   separate review file or audit log. Link the PR in the reply. A failed
   publication is a blocker to report with the changes that remain
   unpublished, not a completed update. Mark the PR ready only when the
   design-tree skill's workflow allows it.
7. **Hand off** with the owner-handoff skill.

## Reviewer prompt

```text
You are reviewing a Whitefoot change you did not write. Do not edit files.
Task outcome and constraints: <...>
Review scope (make review-scope output): <...>
Validation already run: <commands, results, tested revision>

Read the diff from the base (git diff <base>, plus untracked files, without
the excluded paths), the changed sections in context, and "How to review" in
docs/review-checklist.md. Check each listed group. For M1, apply the design
and correspondence checks in design/skill/SKILL.md against the relevant tree
nodes and their ancestors. Do not rerun green suites.

Mark items pass, finding, unverified or not applicable. Report Scope (your
model, base..head, groups checked and skipped), Checks (what you ran) and
Findings (item ID, file:line, quoted text or missing evidence, reason; quote
both sides of a contradiction), or "none within scope".
```
