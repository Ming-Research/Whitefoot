---
name: design-tree
description: Keep a project's design decisions in a design tree, bring every decision that needs the owner to the owner, and review an implementation against the recorded decisions. Use when a task makes or changes a choice between viable alternatives, edits the design tree or its log, asks the owner to decide anything, hands finished work back to the owner, or reviews design and implementation for correspondence (DCR). Not for implementing a recorded decision unchanged or for a routine fix.
---

# Design tree

A design tree records the decisions a project is built on: what was chosen,
because of what, instead of what. It is organized by concept, not by code
structure. The project's main line holds only decisions the owner approved;
a draft branch may change the tree freely, and the owner's approval, recorded
in the log, is what lets that branch become ready. Git holds history; the log
records the approvals.

The project maps these roles to its own paths:

- Live tree: one file per node, with children in a directory of the same name.
- Change log: one entry per approved change, newest first.
- Research record: where derivations, measurements and comparisons live.
- Maintained TODO: where deferred work is recorded.
- Form check and readiness check: the `lint.py` invocations below.

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

Apply three filters to every tree change:

1. Decision, not description. Remove `Decision:` lines without `because` or
   `instead of`.
2. Not derivable from code. Remove nodes that only restate an interface or
   implementation.
3. Normalize upward. State a shared rule once at its common ancestor instead
   of repeating it in children.

Keep each decision concise: retain the choice, its decisive reason or refused
alternative (or both), and the qualifications needed to preserve its meaning.
Put detailed derivations, measurements, comparisons and implementation
mechanics in the relevant research record and link directly to that section.
A long `Decision:` line is still a long explanation. The tree must explain the
choice without requiring the reader to open the link; the linked record
supplies the supporting detail.

## Changes and approval

On a draft branch, change the live tree directly, in the same work as the
implementation it governs, and keep the two consistent as the work goes.
Writing a decision does not approve it. The owner approves at the end, when
the finished work is handed back (Workflow step 4), and approves only the
decisions shown.

After the owner has ruled on every decision of the branch that needs a
ruling, write one log entry for the approved change and only then mark the
branch ready. The readiness check fails while the tree differs from the base
without such an entry, so unapproved changes cannot reach the main line. A
change made after approval, other than one the owner directed, is shown and
approved again. When the owner refuses a change, revise or revert it; keep a
refused alternative worth remembering as a `Rejected:` item. Approval of the
tree does not authorize a merge; the project's merge rules decide that.

## Log format

Each entry has a `## <date> <title>` heading, a `Nodes:` line listing every
node added, changed or retired, an `Owner-approved:` line identifying the
owner's approval in the owner's words, and a concise `Summary:` paragraph with
the change and its reasons. Write the entry only after that approval; the
field records it and never requests or infers it. The newest entry must be
new on the branch and name every changed node. Cite data and evidence at their
source in the research record instead of reproducing them. When parallel
branches add entries, keep both, newest first.

## Owner decisions

The owner decides in the conversation, in the owner's language. A decision
the owner makes lands in the tree, as a node added, changed or retired or as
a refused option under `Rejected:`, so the decisions awaiting the owner and
the branch's tree changes are one list: the ledger kept in the conversation.
An entry may come from a choice made while working, a review finding, an open
research question or a direction the owner gave in passing. The scope of the
work is agreed before starting (below); a change to it is reported in the
handoff's status, not as an entry.

- **Entries.** Each gets an ID, `Q1`, `Q2` and on, never renumbered or reused.
  It stays open until the owner answers that ID. A discussion that moves past
  an entry leaves it open; superseding or withdrawing one needs the owner's
  agreement too.
- **Restate.** After every owner reply, list every ID with its status, for
  example "Q1, Q2 approved; Q3 approved with a change; Q4, Q5 not yet
  discussed". When the owner states a direction in passing, say which entry
  it became and whether it is taken as a ruling.
- **Before starting.** Discuss every choice that sets the direction of the
  work. While a matter that could change it substantially is unclear, keep
  discussing; do not start.
- **After starting.** Work through to completion. A question that arises is
  sent to the owner with a recommendation and work continues on that
  recommendation; the entry stays open and returns at handoff.
- **Batch.** Bring review findings and other questions once, at handoff, not
  one round at a time.
- Re-read this skill before a handoff; a copy loaded early in a long session
  may predate a change to it.

A handoff presents, in this order:

1. **Status.** For each thing the owner asked for: done, done on a
   recommendation still open (name the ID), changed from the agreed scope
   (how and why), or not started. Research that recommends work is not that
   work.
2. **Decision cards.** One per open decision the branch adds, changes or
   retires, oldest first, each after the cards it depends on, so there is no
   separate list of tree changes. A decision the owner already ruled on
   needs no card; the restated ledger shows it approved. End with one line
   naming every open ID and stating that no other decision is open. A card
   opens with its ID and the question in bold, then three parts:
   - Problem: the problem itself, for a reader who has not seen the work:
     what the component or rule does, what goes wrong or stays open, and the
     concrete evidence. Explain each project term at first use.
   - Options: A, B and on, the recommended one marked. Each says what it
     does and what it costs, then why it is recommended or why not.
   - Confidence N/5: 5 when evidence settles it, 1 when it rests on judgment,
     with the reason and what could overturn it.

   The tree records the ruling: the chosen option becomes the node's
   `Decision:` and each refused option worth remembering a `Rejected:` item,
   with the reasons the card gave. A node edit that changes no decision
   needs no card.
3. The parts the project adds, such as its other approved artifacts, the
   validation run and what the work found along the way. They cite a card by
   its ID instead of repeating its reasons.

Write each part as bullets under its bold name; a table's narrow columns bury
reasoning.

## Workflow

1. Settle the direction with the owner (Owner decisions).
2. Implement, change the tree and validate on a draft pull request. Examine
   responsibilities, interfaces, representations and affected consumers for
   design gaps and clear opportunities for a better design, even when the
   current design is valid. Fix in-scope gaps and selected improvements;
   record deferred ones in the maintained TODO with impact, uncertainty,
   validation criterion and reopening condition, proportional to the work.
   List each in the pull request's *Found along the way* section with its
   disposition.
3. At completion, run DCR once, or the project's review that includes it.
   Fix every finding, including those that change the tree or another
   approved artifact; a fix that changes a decision becomes a ledger entry,
   and one that changes the agreed scope goes into the handoff's status.
   Review again only what a fix changed in behavior, a rule or a decision;
   recheck other fixes yourself.
4. Hand off (Owner decisions). The owner rules on every open card.
5. Write the log entry, mark ready once the readiness check and the project's
   CI pass, and leave the merge to the project's merge rules.

## Design Correspondence Review (DCR)

A separate, read-only reviewer that did not implement the change, normally a
small or mid-sized model with bounded inputs, reads the actual artifacts and
reports scope, revision, findings, evidence and uncertainty. It applies the
design checks G1–G3 and the correspondence checks DC1–DC4 below to the tree
diff, the complete work diff and the relevant existing nodes and ancestors.
A task without tree changes still gets DCR at completion. DCR approves
nothing.

### Design checks

G1. Decision test. Check each added or changed node against the node format
and leanness filters. Report descriptions without decisions, circular refusal
reasons, and choices or grounds that require the source record to understand.

G2. Consistency scan. Check changed nodes against ancestors and siblings,
extending to related decisions as needed. A change governing a whole concept
requires reading its subtree. Report nodes read and conflicts, narrowings, or
broken dependencies, naming both sides.

G3. Architectural fit. Check that structural choices received the Workflow
assessment when made or revised, and that the result is visible to the owner.
Report concrete gaps or clear improvement opportunities left without an
assessment or disposition, including deferred opportunities or their
validation missing from the maintained TODO. Do not demand speculative
generality or reconstruct a missing rationale after coding.

### Correspondence: design and implementation

Inputs: the agreed delivery scope, its design commitments including relevant
existing nodes and ancestors, the complete work diff, resulting artifacts, and
validation. Here, code means whichever artifact implements a decision,
including a specification or configuration. Extend into affected consumers as
needed.

DC1. Decisions in code. For each changed region embodying a design choice,
name its node. Report a choice with no node as a missing tree change;
ordinary implementation steps need no record.

DC2. Contradiction. Report code that contradicts a decision or implements a
refused alternative without a tree change that replaces the decision.

DC3. Orphaned support. For deleted code, identify decisions that lose their
implementation. Report a retired approach missing its rejection rationale,
and rejected approaches still implemented.

DC4. Missing or partial implementation. For each design commitment in scope,
identify support for its required behavior and conditions. Report missing or
partial paths, placeholders, and insufficient evidence; a related function
alone is not proof of completion. Exclude unrelated or explicitly deferred
designs unless the deferral contradicts the agreed scope or completion claim.

## Lint

`lint.py` checks form, not design quality. Its layout has one root node file
and optional child directory per concept, with `log.md` beside the roots.

    python3 -B <skill-directory>/lint.py --root <design-directory> --trees <concept> ... [--base <base>] [--require-approval]

Without `--base` it checks form only. With `--base` it also prints node count,
depth and decision counts against the base, which a tree review reports. With
`--require-approval` it is the readiness check: when the tree differs from the
base, the newest log entry must be new, name every changed node and carry a
nonempty `Owner-approved:`. The field is an assertion that lint cannot
authenticate; the owner reads the log before merging. A `--base` must resolve
to a commit, and a caller must choose one that exposes the changes under
review: for a push to the main line, the revision before the push.

## Translation: run on request

Render the requested tree diff or subtree in the requested language, keeping
node names, paths, and code identifiers untranslated. Do not store the
translation in the repository; the tree is English only.
