# Research and design evidence

This directory holds investigations and experiments that expose language or
compiler needs and test possible solutions. Treat agent authorship as a
changed design condition: identify what a candidate restriction or capability
should buy, compare plausible alternatives, and retain the observed limits.
The [investigation skill](../docs/skills/investigation/SKILL.md) states how a
performance loss is attributed and which observations an agent writer trial
keeps apart. The active [specification](../spec/kernel-spec.md)
defines the language, the [design trees](../design/) record why the language
and the compiler are the way they are, and [AGENTS.md](../AGENTS.md) defines
the work-branch and merge boundary.

- `investigations/`: a selected question's design, measurements, and rejected
  alternatives. Keep useful evidence here after implementation; an ended task
  does not require relocating it.
- `experiments/`: reproducible measurements and their inputs, comparisons,
  limitations, and maintained harnesses.
- `notes/` and existing documents at this directory's root: bounded design
  questions and supporting analysis.
- [Design trees](../design/): what was settled, why, and which alternatives
  were refused. The [design-tree procedure](../design/skill/SKILL.md) owns
  design proposals, owner rulings, and amendments as implementation evolves.
- [Archive promotion audit](archive-promotion-audit.md): a non-authoritative
  map from historical findings to useful successors and remaining questions.
- [Decision workflow investigation](investigations/decision-workflow/DESIGN.md):
  external practices, the constitutional reassessment and index migration,
  and the evidence still needed to assess the selected workflow.
- [Prior compute-runtime bundle](investigations/compute-runtime/PRIOR-BUNDLE.md):
  the two-runtime comparison behind the 2026-09-09 current-stack selection, the
  grain panel and recursion-frontier evidence, and what the compute scoreboard
  replaced.
- [Compute expression and cost](investigations/compute-model/DESIGN.md):
  blocked and irregular algorithm consumers, independent correctness criteria,
  and the runtime costs that test the compute model after range loans.
- [Fixed-resource execution](investigations/fixed-resource-execution/README.md#deferred-work-and-resumption):
  deferred research into no-heap computations with proved completion and
  storage bounds; retained stack/rank/cleanup evidence, proposals and a
  resumption checkpoint reached from the single TODO topic.
- [Containers over x1](investigations/containers-and-resources/X1-LIBRARY.md):
  complete container operations and representation costs over merged PR #70,
  the restored source-library home, and the next implementation trials.
- [Source certificate checking cost](investigations/proof-certificate-architecture/CHECKING-COST.md):
  separate written-proof length from entering-context size and attribute the
  large `proof_use` cost without changing its accepted rules.
- [Result proof transport](investigations/result-proof-transport/DESIGN.md):
  compare verified result facts across direct matches, named outcomes and
  propagation, including capture, invalidation and composition boundaries.
- [Readable diagnostics](investigations/readable-diagnostics/DESIGN.md): the
  labeled record every compiler stop prints, its text and JSON renderings,
  and the rejected rendering paths.
- [Incremental L0 closure](investigations/proof-certificate-architecture/INCREMENTAL-CLOSURE.md):
  keep fact states' closed cores and insert fresh edges instead of recomputing
  the cubic closure, relaxing only which equal-bound L0 derivation is retained.
- [Automatic operation facts](investigations/automatic-operation-facts/DESIGN.md):
  the inventory of operation-derived facts, corpus and idiom-sweep evidence,
  and the operation table that replaced the idiom menu of [ENT-3], with its
  measured checking cost.
- [Modular compilation](investigations/modular-compilation/DESIGN.md):
  modules for large projects and for an architect agent that owns module
  interfaces while implementer agents write bodies in parallel. Self-contained
  `module.wfm` interfaces sit beside their implementation files, with a fixed
  `pkg::` root qualifier, file-local name aliases, one root file for ordered
  module dependencies, module verdicts that depend only on interfaces,
  persistent proof dependencies, and incremental cross-module optimization
  through object generation, with ordinary final linking. Its
  [source boundary rules](investigations/modular-compilation/LANGUAGE.md)
  select one visibility rule for code and annotations, module-relative
  `readonly`, result-projection contracts and complete formation rules; the
  active specification v0.70 now carries the module grammar and most of those
  rules. A [complete source demo](investigations/modular-compilation/demo/README.md)
  follows a queue through shared libraries, a no-heap entry and a heap-using
  tool.
- [Repair wording](investigations/repair-wording/DESIGN.md): every repair the
  specification prescribes against the compiler's printed text, refuted and
  unproved probes of each, and whether the words belong to the specification
  or the compiler.

Open research questions and evidence links are in [ideas](../docs/ideas.md);
known compiler defects and implementation costs are in [todo](../docs/todo.md).
Dated results state what their recorded program, toolchain, and environment
established; they are not descriptions of current compiler capabilities.
Historical approval or phase language in evidence does not add current
workflow requirements.

Research runs only on explicit request. Neither daily CI nor the canonical
`make check` may compile, execute or import research tools, or depend on their
programs, fixtures or datasets. Useful regression observations are extracted,
with their necessary inputs and oracles, into the formal compiler, program,
conformance or performance test system. Research may reuse those maintained
fixtures; formal tests do not reach back into research. A useful experiment
does not become a permanent test suite merely because it runs.

Keep the remaining models, comparisons and measurements here for their stated
questions and dated evidence. No cleanup or modernization is required merely
to preserve that evidence. A deferred prototype that needs a retired compiler
states that limitation; active tools do not import archived code. Remove
superseded material when it no longer carries useful evidence.

Retained records use role placeholders such as `<repository-root>` and
`<scratch-root>` instead of personal filesystem paths. When a reproducibility
bundle hashes a redacted record, its digest describes the redacted bytes.
