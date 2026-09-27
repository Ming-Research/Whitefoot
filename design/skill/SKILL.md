---
name: design-tree
description: Record, propose or apply design decisions in the project's design tree, and define the design checks of Design Correspondence Review (DCR). Use when a task chooses between viable alternatives the tree should record (a new or changed language rule included), edits the design tree, its amendments or its log, applies an owner's ruling, or reviews an implementation against recorded decisions. Not for implementing a recorded decision unchanged or for a routine fix.
---

# Design tree

A design tree records the decisions a project is built on: what was chosen,
because of what, instead of what. It is organized by concept, not by code
structure. It holds only owner-approved decisions; unapproved choices remain
amendments beside it. Git holds history; the log provides concise traceability.

Use project-appropriate locations and concept names for three roles:

- Live tree: one file per node, with children in a directory of the same name.
- Amendments: proposed additions, replacements, or retirements of decisions.
- Change log: one short entry per approved tree change, newest first.

## Node format

A node is a file named for the decision it owns, holding one or more
`Decision:` lines and an optional `Rejected:` list, without dates, standalone
facts, measurements, or progress. Cite evidence in a reason instead. Name
events by what happened, not by date. Each field occupies one line, with a
blank line between fields; list items directly follow their header.

A `Decision:` line states the choice, its reason after `because`, and the
alternative after `instead of`. At least one must be present; a line with
neither is a description, not a decision. Write for a reader who has not
seen the source record, expanding compressed terminology.

`Rejected:` lists refused alternatives as `- <alternative>: rejected because
<reason>`, one per line. Give a discriminating reason, and do not re-propose
an alternative without explaining what changed.

## What is a decision

A choice between viable alternatives is a decision, even when the selection
seems obvious. An implementation step with only one viable way needs no
record. Start coarse; the owner tunes the threshold when the tree grows too
fine or too thin.

A reason states its kind of ground. A deduction names its premises and only
the conclusion they entail; an empirical reason names what was observed and
under which conditions; a provisional choice names its reason, uncertainty
and reopening condition. A constitutional principle or one measurement shows
that a choice fits, not that it is the only possible one. Keep an open
question open: name an assumption used to proceed and how it will be checked,
and never record a proposal or an agent's default as a settled decision.

## Keeping the tree lean

Apply three filters to every proposed tree revision:

1. Decision, not description. Remove `Decision:` lines without `because` or
   `instead of`.
2. Not derivable from code. Remove nodes that only restate an interface or
   implementation.
3. Normalize upward. State a shared rule once at its common ancestor instead
   of repeating it in children.

Keep each decision concise: retain the choice, its decisive reason or refused
alternative (or both), and the qualifications needed to preserve its meaning.
Put detailed derivations, measurements, comparisons and implementation
mechanics in the relevant existing `research/` record and link directly to
that section. A long `Decision:` line is still a long explanation. The tree
must explain the choice without requiring the reader to open the link;
the linked record supplies the supporting detail.

Every tree diff review reports node count, depth, and net change; the lint
prints them against the review base.

## Amendments

Keep all proposed tree revisions in `design/amendments/`, the sole temporary
amendment directory. Research and implementation continue on the Draft PR;
only live-tree edits wait for the owner's explicit approval. Present the
complete revision, naming the nodes and decisions added, replaced or retired.
Approval to perform the work, or approval preceding that proposal, does not
authorize its tree edit or approval log entry.

An amendment file starts with `Node: <tree path>`, then a blank line and the
node form. Identify any decision replaced or retired and explain why. Keep
amendments current so the owner can review the complete outstanding revision.

When approved, apply only the revision shown to the owner, make its approval
the newest log entry with the required `Owner-approved:` field, and remove the
accepted amendment. When rejected, add a log entry naming the node, what was
proposed, and why it was refused, remove the amendment, and adjust the design
and implementation to the ruling. Later revisions, including material changes
to an approved proposal, must be shown again and receive their own approval.
After all rulings are applied, remove the amendment directory itself. Removing
or relocating an unresolved proposal does not resolve its required ruling.

## Log format

The log records every ruling on the tree, each approved revision and each
refused amendment, and anything else a later reader must be able to find.
Each entry has a `## <date> <title>` heading, a `Nodes:` line listing every
node touched or ruled on, and a concise `Summary:` paragraph with the
conclusion, its reasons, and the ruling it records. An entry that accompanies
a live-tree change also has `Owner-approved: <approval>` between `Nodes:` and
`Summary:`. Its value briefly identifies the owner's explicit approval of the
proposed revision; the field is an assertion about that approval, not a place
for the agent to request it or infer it from the task. The approval entry must
be the newest entry and must name every changed node. Refused amendments do
not use `Owner-approved:`. Cite data, measurements, and evidence at their
source under the project's research record instead of reproducing them. Git
supplies the detailed history. When parallel branches add entries, retain
both, newest first.

## Workflow

1. Research, implement and validate continuously on a Draft PR. Keep proposed
   tree changes in amendments; pending approval does not block this work.
2. Once the agreed implementation and evidence are ready, run DCR against the
   live tree, amendments and implementation together. Present the reviewed
   proposals, and any findings awaiting direction, to the owner, then await
   the ruling.
3. Apply the ruling to the tree, log and affected implementation; remove the
   resolved amendments and their directory. Recheck affected work and run CI.
4. Only after all proposals are resolved and the final revision's required CI
   is green, mark the PR ready and await the owner's merge. A new pending tree
   revision returns the PR to Draft. Design approval does not authorize merge.

During design and implementation, examine responsibilities, interfaces,
representations and affected consumers for both design gaps and clear
opportunities for a better design or greater capability, even when the current
design is valid. Surface each concrete opportunity in the existing PR or
investigation, stating its expected benefit, cost, affected scope and uncertainty;
recommend addressing, deferring or declining it, with reasons. Fix in-scope gaps
and selected improvements. Record deferred gaps and opportunities in the
project's maintained TODO, including those whose benefit or feasibility is
unverified: validation is itself a task. State their impact, uncertainty,
validation criterion, deferral reason and reopening condition. Reconsider as
implementation reveals new information or later work touches these opportunities.
Keep this proportional to the current work.

List each concern and opportunity in the PR's *Found along the way* section
as you find it, with its disposition and reason, and summarize them when
handing back work; when none were found, one line naming the assessed scope
suffices. This does not replace amendment or DCR explanations.

## Design Correspondence Review (DCR)

Run the bidirectional review below when asked for `dcr` and at Workflow step 2,
before submitting the completed work for owner ruling. Amendments are review
inputs at this point, not a reason to refuse DCR. Opening a Draft PR or
publishing intermediate progress needs no review. A task without proposed
tree changes still needs DCR before completion. Where the project's completion
review includes these checks, running it is the DCR; no second DCR is required
just to apply the exact reviewed and approved revision.

Use a separate, read-only reviewer that did not implement the change,
normally a small or mid-sized model with bounded inputs. It reads actual
artifacts and reports scope, revision, findings, evidence, and uncertainty.

Route each finding by what resolving it would change. A finding whose
resolution would change a design decision or amendment, a specification
rule, or the agreed scope goes to the owner with the primary agent's
assessment and recommended response, and waits for direction, including
during unattended work. Fix every other
finding (form, wording, a broken reference, missing evidence or coverage),
recheck the affected items and report it. DCR never authorizes a tree change.

Before awaiting owner input, give a self-contained handoff in the owner's
language. Lead with one decision card per amendment and per finding awaiting
direction, laid out as the project's handoff procedure prescribes (in
Whitefoot, the owner-handoff skill), naming the node, the current and proposed
decision, and the recommended ruling with its reason. Report the DCR revision
and scope and the findings fixed; say when none were found within scope. Links
and amendment counts support this account but do not replace it. A clean DCR
does not approve the proposals or make the Draft PR ready. Recheck affected
items after directed changes, reusing unaffected review. Tests and merge rules
belong to the project.

## Lint

Use project structural validation. The bundled `lint.py` checks form, not
design quality. Its layout has one root node file and optional child
directory per concept, with `amendments/` and `log.md` beside the roots.
Supply the project's paths, live concept names, and review base; amendments
may propose new concepts. When the live tree differs from the base, the
newest log entry must itself be new, name every changed node, and contain a
nonempty `Owner-approved:` field; pending amendments need no log entry.
The field records an assertion; lint cannot authenticate the owner's approval.
An explicit `--base` must resolve to a commit or lint fails. Omitting it checks
form only, without checking tree changes against a prior revision. A CI caller
must choose a base that exposes the changes under review; for a main push,
comparing the updated main ref with itself checks no changes.

    python3 -B <skill-directory>/lint.py --root <design-directory> --trees <concept-name> --base <review-base>

In Whitefoot, `make design-lint` checks form during draft work. The separate
`make design-ready` uses `--require-no-amendments` and fails if
`design/amendments/` exists, even empty. CI runs this readiness check on
pull requests that are ready for review and on main
(`.github/workflows/design-readiness.yml`); a draft shows it skipped, so
pending proposals never turn draft CI red. It must pass before the PR
becomes ready.

## Design checks

Use these during discussion and DCR; discussion needs neither finished
implementation nor an independent gate at every exchange.

G1. Decision test. Check each added or changed node or amendment against
the node format and leanness filters. Report descriptions without decisions,
circular refusal reasons, and choices or grounds that require the source
record to understand.

G2. Consistency scan. Check changed nodes and amendments against ancestors
and siblings, extending to related decisions as needed. A change governing a
whole concept requires reading its subtree. Report nodes read and conflicts,
narrowings, or broken dependencies, naming both sides.

G3. Architectural fit. Check that structural choices received the Workflow
assessment when made or revised, and that the result is visible to the owner.
Report concrete gaps or clear improvement opportunities left without an
assessment or disposition, including deferred opportunities or their validation
missing from the maintained TODO. Do not demand speculative generality or
reconstruct a missing rationale after coding.

## Correspondence: design and implementation

Inputs: the agreed delivery scope, its design commitments including relevant
existing nodes and ancestors, proposed revisions, the complete work diff,
resulting artifacts, and validation. Do not limit review to the tree diff.
Here, code means whichever artifact implements a decision, including a
specification or configuration. Extend into affected consumers as needed.

DC1. Decisions in code. For each changed region embodying a design choice,
name its node or amendment. Report unrecorded choices as missing amendments;
ordinary implementation steps need no record.

DC2. Contradiction. Report code that contradicts a decision or implements a
refused alternative, naming any amendment proposing that change. Without an
amendment it is drift.

DC3. Orphaned support. For deleted code, identify decisions that lose their
implementation. Report a retired approach missing its rejection rationale,
and rejected approaches still implemented.

DC4. Missing or partial implementation. For each design commitment in scope,
including existing nodes and pending revisions, identify support for its
required behavior and conditions. Report missing or partial paths,
placeholders, and insufficient evidence; a related function alone is not
proof of completion. Exclude unrelated or explicitly deferred designs unless
the deferral contradicts the agreed scope or completion claim.

## Translation: run on request

Render the requested tree diff or subtree in the requested language, keeping
node names, paths, and code identifiers untranslated. Do not store the
translation in the repository; the tree is English only.
