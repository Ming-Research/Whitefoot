---
name: owner-handoff
description: Hand work back to the Whitefoot owner in the owner's language - decision cards first, then the result, specification revisions and what the work found along the way. Use when stopping for the owner - a task is done, an amendment needs a ruling, or a finding awaits direction. Not for progress notes while work continues.
---

# Owner handoff

The owner rules on the design and merges. A handoff exists so that the owner
sees every decision without re-reading the work: lead with what needs the
owner and keep supporting detail one step away, in the PR, the amendment or
the investigation. Write in the owner's language; repository artifacts stay
English.

Keep the handoff compact. Each part below starts with its name in bold on a
line of its own, so none reads as part of the card before it. Under it, every
point is a bullet that opens with its gist in bold and continues on the same
line, with any detail in bullets indented beneath it. Never use headings or
tables: headings spread one point over a screen, and a table's narrow columns
bury the reasoning.

1. **Decision cards.** One card for each amendment awaiting a ruling, each
   finding awaiting direction and each other choice the owner must make, or
   "none". A card opens with its number and topic in bold, followed on the
   same line by the recommendation; three indented bullets give the current
   state, the reason and the confidence, a sentence or two each.

   ```markdown
   - **#1 The question the owner decides, in one sentence.** Recommendation:
     the choice proposed and what follows from it.
     - Current state: what the specification, design or implementation does
       now, and the evidence that raised the question; for an amendment or a
       finding about a design decision, the node it concerns.
     - Reason: why that choice fits its requirements and evidence, and what
       it costs.
     - Confidence N/5: 5 when the evidence settles the choice and 1 when it
       rests on judgment alone, then what supports it and what could still
       overturn it.
   ```

   Write the labels and their content in the owner's language. Link the
   amendment or evidence that holds the problem, alternatives, tradeoffs and
   uncertainty in full; the card summarizes them, and an alternative enters
   the card only when it decides the recommendation.
2. **Result.** A few bullets: what changed, the validation actually run (full
   gate or focused, and the tested revision), what remains unverified, and the
   PR link.
3. **Specification revisions.** Whenever `spec/kernel-spec.md` changed: which
   rules changed, their before and after behavior, and why they were selected.
   A version number or PR link does not replace this.
4. **Found along the way.** A few bullets summarizing the PR's section of
   that name: what was fixed, what was recorded in `docs/todo.md` and what was
   declined, with reasons, or one line naming the areas worked in when nothing
   was found. The noticing happens while working, under AGENTS.md's "Fix or
   record what you notice"; this step only reports it.
