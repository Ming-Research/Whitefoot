---
name: investigation
description: Plan, run and report a Whitefoot investigation or experiment - a measurement, benchmark, performance attribution or agent writer trial - so its result can select or reject a design. Use when starting or extending research/investigations/<name>/ or research/experiments/, or when a claim needs a new measurement. Not for routine test runs, reading existing research or reporting a CI result.
---

# Investigation

An investigation exists to decide something. Its design, measurements and
rejected alternatives live in `research/investigations/<name>/`, reproducible
experiments in `research/experiments/`, and the decision that survives goes to
the design tree through the `design-tree` skill. AGENTS.md's rules for choices
and verification apply throughout; these add what this project's experiments
need.

1. **Performance.** Attribute a loss with a same-source causal comparison,
   before and after the change, and a falsifier. Profile proof cost by
   formation, automatic derivation, certificate checking and fact propagation
   before attributing it to `use`.
2. **Agent writer trials.** Keep four observations apart: whether the program
   and its proofs can be expressed; whether the tested agent writes them with
   the supplied interfaces, context, tools and repair help; whether separately
   written parts meet independent expectations when composed; and whether the
   result meets its runtime cost goal, and why. Model identity and assistance
   are experimental conditions, not ceilings on the language.
