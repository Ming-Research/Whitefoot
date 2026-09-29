---
name: owner-handoff
description: Hand work back to the Whitefoot owner in the owner's language - decision cards first, then the result, specification revisions and what the work found along the way. Use when stopping for the owner - a task is done, an amendment needs a ruling, or a finding awaits direction. Not for progress notes while work continues.
---

# Owner handoff

The owner rules on the design and merges. A handoff exists so that the owner
can decide without re-reading the work: lead with what needs the owner, and
make each decision understandable from the handoff alone. Full evidence stays
in the PR, the amendment or the investigation, but the problem, the options
and the grounds are explained in the card itself. Write in the owner's
language; repository artifacts stay English.

Every decision the owner must make lives in one place, the **Decisions**
ledger of the PR description, until the owner rules on it. Conversation
context is lost to compaction and a card numbered per handoff is lost when the
next handoff reuses its number; the ledger is what keeps a decision from
disappearing between them.

- **Entries.** Each decision gets an ID, `Q1`, `Q2` and on, assigned once for
  the PR and never renumbered or reused, whatever its source: an amendment, a
  review finding, an investigation's open question, a choice made while
  implementing, or a direction the owner gave in conversation. An entry holds
  the question in one line, its status and where its ruling is applied.
- **Status.** `open` until the owner rules. `ruled` records the date and the
  owner's words. `superseded by Qn` and `withdrawn` need the owner's explicit
  agreement too: a discussion that moves past a card leaves it open.
- **Reconcile before every handoff.** Walk the owner's messages since the last
  handoff, `design/amendments/`, the open questions of the investigations the
  PR touches, review findings awaiting direction and `docs/todo.md` items
  awaiting the owner; each becomes an entry or updates one. When the owner
  states a direction in passing, the reply says which entry it became and
  whether it is taken as a ruling, and asks when that is unclear.
- **Batch.** Hand off once per finished unit of work, after the review and its
  rechecks, not once per finding. A question that blocks work before then
  still shows the whole open list.

Re-read this file before each handoff: a copy loaded earlier in a long session
may predate a merge that changed it.

Keep the handoff compact. Each part below starts with its name in bold on a
line of its own, so none reads as part of the card before it. Under it, every
point is a bullet that opens with its gist in bold and continues on the same
line, with any detail in bullets indented beneath it; a decision card's parts
open with their labels instead. Never use headings or
tables: headings spread one point over a screen, and a table's narrow columns
bury the reasoning.

1. **Decision cards.** One card for every open ledger entry, oldest first,
   not only the entries this handoff adds, or "none". Order dependent
   entries after the entry they depend on and say which choice changes them.
   A card opens with its ledger ID and the question in bold, then explains
   before it recommends: the problem, the options, and only then the
   recommendation, its reason and the confidence. The cards end with one line
   naming every open ID and stating that the ledger holds no other open
   decision.

   ```markdown
   - **Q7 The question the owner decides, in one sentence.**
     - Problem: what this is about and why it needs a decision now, written
       for a reader who has not seen the PR or the investigation: what the
       component or rule does, what goes wrong or stays open, and the concrete
       evidence (an example program, a measured number, a failing case). For
       an amendment or a finding about a design decision, name the node, its
       current decision and the proposed one. Explain each project term the
       first time it appears.
     - Options: each viable choice, what it would do and what it costs, one
       bullet each; include the refused ones that the owner could reasonably
       prefer.
     - Recommendation: the choice proposed and what follows from it.
     - Reason: why that choice fits its requirements and evidence better than
       the other options.
     - Confidence N/5: 5 when the evidence settles the choice and 1 when it
       rests on judgment alone, then what supports it, what is still
       unmeasured and what could overturn it.
   ```

   Write the labels and their content in the owner's language. The Problem
   is as long as understanding needs, usually a short paragraph or a few
   bullets; the other parts stay brief. Link the amendment or evidence that
   holds the full detail. A card carried from an earlier session is checked
   against its sources before it is presented.
2. **Status.** For each thing the owner asked for in this PR, whether it is
   done, waiting on a ruling (name the ID) or not started, and what remains
   before the PR can merge. Research that recommends work is not that work:
   say which recommendations are implemented and which are not.
3. **Result.** A few bullets: what changed, the validation actually run (full
   gate or focused, and the tested revision), what remains unverified, and the
   PR link.
4. **Specification revisions.** Whenever `spec/kernel-spec.md` changed: which
   rules changed, their before and after behavior, and why they were selected.
   A version number or PR link does not replace this.
5. **Found along the way.** A few bullets summarizing the PR's section of
   that name: what was fixed, what was recorded in `docs/todo.md` and what was
   declined, with reasons, or one line naming the areas worked in when nothing
   was found. The noticing happens while working, under AGENTS.md's "Fix or
   record what you notice"; this step only reports it.
