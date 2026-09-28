# Archive

Superseded or shelved material, kept for the record. Nothing here gates
anything; no tool or test reads from this tree.

Machine-specific locations in retained records are redacted with descriptive
tokens such as `<historical-repository-root>`, `<local-home>`, and
`<local-workdir>`. The tokens preserve provenance roles without retaining a
developer's absolute directory.

- `DECISION_SPRINT.md`, `ROADMAP.md` — pre-consolidation plans, superseded by
  `/THE-PLAN.md` (2026-07-10). Kept because gates-log entries cite them.
- `HANDOVER-2026-07-17.md` and `compiler/PLAN-2026-07-17.md` — the last
  competing handover and compiler roadmap, retired when the owner made
  `/THE-PLAN.md` the sole execution plan.
- `research/validation-harness-plan.md` and
  `research/systems-performance-coverage-FOLLOW-UPS-2026-07-17.md` — retired
  policy and follow-up registers whose live requirements moved into
  `/THE-PLAN.md`.
- `tools/verify_performance_research_status.py` — the old duplicated-status
  verifier, replaced by `/tools/verify_project_state.py`.
- `APPROVALS.md` — the approval ledger, retired 2026-09-06. Its purpose was an
  asynchronous loop: plans were researched, the owner approved them one at a
  time, and approved ones were then executed. That loop is gone — work now runs
  research to implementation to specification — so the merge-time prose it
  carried is read by nothing. Its `ACTIVE-SPEC:` digest chain went with it: the
  chain verified a total order over versions, which nothing consumed for any
  decision and which made two concurrent specification branches structurally
  impossible, since the second to merge had to renumber, re-digest, re-parent
  and re-archive. A specification version's identity is now its own bytes, and
  `make spec-append-only` still forbids a released archive from changing. The
  seven `docs/done/` citations inside it stay as written: it is history now.
- `done/` — the per-batch record, retired 2026-09-06 when `mcts_mem/` became
  the single home for decisions. One hundred records, moved from `docs/done/`
  with their filenames unchanged, so a citation written as `docs/done/0024`
  resolves here as `archive/done/0024`. Nothing live cites them any more: a
  finished task is not evidence, so every citation was removed from the tree
  rather than repointed, and the only remaining ones are inside `APPROVALS.md`
  above, which is itself history. Frozen; not written to again.
- `current-plan.md` — the rolling execution plan, retired 2026-09-06. Its own
  plan was delivered as v0.40 and never replaced, so it survived by having each
  later version appended to it as a changelog. Work is no longer planned in a
  document up front: a selected direction goes to
  `research/investigations/<name>/` and what it settles goes to `mcts_mem/`.
- `research/` — the evidence-first research era: multi-agent debates
  (`debates/`), source papers (`sources/`), feature matrices, synthesis
  notes. This produced the corpus that CONSTITUTION.md and the spec derive
  from; the derivation ledger cites into it.
- `experiments/` — corpus-era measurement studies (noalias collapse vs
  Rust/C, region-effect scatter residual, guarded-plan parallelism). Their
  conclusions are absorbed into the corpus notes and THE-PLAN's evidence
  ledger; the active successors live in `/experiments/`.
- `m3/` — the model-tier authorship harness, shelved per D5 (models improve
  faster than the weak-writer test depreciates) but SHELF-READY: the
  requires-accounting design (section 13.4) names it for the authorship
  experiments, and `trial.py` runs against any model CLI unchanged.
- `research/minimal-systems-capability/` — the superseded B-Strata /
  Candidate B/C / G0-Core capability-research era (2026-07-14..15), suspended by
  D15. Historical evidence and falsifiers for the active
  systems-performance-coverage design; its standalone verifier tools are under
  `verifier-tools/` (inert — not run). ~179MB.
- `research/capability-floor/` — the general-purpose data-structure capability
  floor research (2026-07-13) that preceded and fed the D15 pass.
- `toolchains/self-hosting-2026-07-20/` — the retired Whitefoot wfc, Python
  democ, and tape-era inventory. Their exact Git
  identities and replay instructions are recorded inside. The active compiler
  starts fresh in Rust and imports nothing from this archive.

- `experiments/reference-access-grammar/main.rs` — the retired v0.75 reference
  spelling comparison. The v0.76 compiler and conformance cases replace its
  relabelled-predicate grammar probe. It remains inert evidence; reproduce the
  original runs from revision `44032cac693afe4246cf9e9b872ddf40c2491945`, where
  the driver and its imported generator share their original paths. The live
  comparison is in `research/investigations/contract-surface/OWNERSHIP.md`.
