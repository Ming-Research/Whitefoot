# Task completion review

Whitefoot's review items. The reviewer applies them together with the
owner-wide review procedure and checks; [AGENTS.md](../AGENTS.md#review) says
which groups a change gets, and merge conditions are in its
[branch and main boundary](../AGENTS.md#branch-and-main-boundary).

## How to review

Read the relevant [document roles](../AGENTS.md#document-roles) and the
definitions, callers or cases the change directly affects. Mark each item
`pass`, `finding`, `unverified` or `not applicable`; missing evidence is not a
pass. When the task changes a review rule or an expected result, compare its
previous form with the requested change rather than judging only against the
newly edited rule. A question that local inspection and cases cannot settle,
such as the soundness of a design argument, is marked `unverified` for the
implementing agent.

## A. Repository — every change

Source: [repository hygiene](../AGENTS.md#repository-structure-and-hygiene).

- [ ] **A4 — Whitefoot layout.** `AGENTS.md` is this repository's agent
  instruction file. The Rust compiler is checked with its native tools. No
  active source, build, test or tool depends on `archive/`. `README.md` and
  `README.zh-CN.md` change together.

## D. Documentation — changed Markdown, comments or examples

- [ ] **D1 — Purpose.** Each changed passage fits its
  [document role](../AGENTS.md#document-roles), without editorial history or
  process instructions inserted into substantive documents. A constitutional
  change states complete clauses with their obligations and conditions; a
  chosen prohibition is not merely a report of current implementation
  behavior.
- [ ] **D2 — References.** Changed references resolve to the intended file,
  heading or symbol and obey the
  [citation boundaries](../AGENTS.md#citation-boundaries).
- [ ] **D3 — Current meaning.** Changed claims distinguish a goal, proposal,
  specified behavior, implemented capability and dated measurement;
  historical positions are not conflicting current instructions merely
  because they differ.
- [ ] **D4 — Usability.** Instructions name real commands and prerequisites.
  Changed runnable examples have been checked through the ordinary path;
  fragments have enough surrounding context and are not offered as complete
  programs. No duplicated changing status or version identity needs another
  synchronized update.

## C. Code and cases — changes under `compiler/`, `lib/` or `tests/`

Source: [compiler rules](../AGENTS.md#compiler-rules) and
[test integrity](../AGENTS.md#specification-and-test-integrity).

- [ ] **C3 — General path.** The diff implements a grammar or semantic rule or
  a general runtime operation; no function, project, source shape or test name
  selects a special acceptance or lowering path.
- [ ] **C4 — Safety boundary.** Inspect changed acceptance and proof paths for
  added Rust `unsafe`, weakened contracts, runtime substitutes for required
  proof, impossible-case returns, and timeout/fuel/heuristic acceptance limits.
  Changes to optimization facts retain acceptance and behavior with facts off;
  relevant evidence is supplied when those paths change. Flag a semantic
  safety question that local inspection and cases cannot settle for deeper
  review.

## T. Specification and checks — changes to `spec/kernel-spec.md`, `tests/`, the conformance adapter under `compiler/tests/conformance/` and its wiring in `compiler/tests/corpus.rs`, a Makefile or `.github/`

Source: [specification and test integrity](../AGENTS.md#specification-and-test-integrity).

- [ ] **T1 — Language evidence.** A specification amendment follows the
  steps in [AGENTS.md](../AGENTS.md#specification-and-test-integrity), whose
  archive and title `make static` checks mechanically. The change declares
  its specification delta (rules, tokens, spellings, exceptions) and
  evidence/minimality selection ground. Affected cases/verdicts,
  generated syntax, compiler and documentation follow the amendment. For
  conformance changes, the PR explains the normative expectation and how the
  changed evidence tests it. The conformance runner checks unique rule IDs
  and resolving references; inspect semantic duplication, exception clauses,
  and whether non-authoritative review inventories match their normative
  definitions. An implementation gap, crash, timeout or unsupported feature
  has not been relabeled as normative source rejection.
- [ ] **T4 — Case admission and home.** Identify each added or changed case's
  protected property, meaningful failure and owning group. The home follows
  the property: normative requirements in conformance, complete program
  behavior in programs, additional implementation obligations in
  compiler/runtime tests. A WF fragment wrapped in `#[test]` or a new
  executable does not justify another case. Merge checks with the same
  observations.
- [ ] **T5 — Construction and execution.** Identify what each selected path
  builds and runs: compiler/profile, Rust/C test executable, WF compilation,
  native program, or other tool. Additional native builds/runs, configurations
  and repetitions protect a named observation; compatible construction is
  shared. A correctness invocation contains no exploratory timing protocol.
- [ ] **T6 — Research boundary.** Daily CI and the canonical gate have no
  direct or indirect dependency on research programs, scripts, fixtures or
  datasets. Inspect callers, imports, generated inputs and shared helpers,
  not only job names. Useful cases and necessary oracles are extracted into
  formal test ownership; the remaining research stays explicitly invoked.
  Moving a wrapper or copying a whole experiment is not sufficient.
  Check automated dependency-check coverage for changed executable paths;
  inspect dynamic or indirect paths it cannot resolve. Changed checking needs
  representative forbidden-dependency and allowed-citation controls. Keep
  normal complete checkouts; hiding or removing research is not enforcement.
- [ ] **T7 — Local/CI correspondence.** Ordinary correctness CI derives its
  groups from the same Makefile inventory and recipes as local `make check`;
  inspect changed selection, filters and callers for omissions or extra checks.
  Platform qualification and paired performance remain explicit separate
  responsibilities. Report the tested revision and actual groups; matching
  group names alone do not establish matching test selection.
- [ ] **T8 — Time budgets.** A new labeled CI stage has a row in
  `.github/time-budgets.txt` for each host it runs on. A raised budget has the
  owner's decision; a change that makes a stage much faster lowers its budget.
  An overrun reported as runner variance names what the change adds to that
  stage and why it cannot account for the time. A stage kept within its budget
  by removing coverage is a weakened check.

## V. Validation and report — every change

Source: [branch and main boundary](../AGENTS.md#branch-and-main-boundary).

- [ ] **V3 — Specification delivery.** For specification revisions, the
  report shows the affected rules, their before/after behavior and the
  decision card that selected each or why it needed none. Conformance changes
  explain what changed and their selection ground. A PR marked ready has the
  owner's approval of every specification change it carries, recorded in
  `spec/log.md`. A merge has the owner's approval and a passing root
  `make check` for the exact merge tree.

Existing checks: `git diff --check` for patch whitespace; `make static` for
repository invariants, specification archives (immutability and amendment
shape), live spec references, cited review items, entry-document paths and
design-tree form; `make -C compiler format lint` and the
[focused compiler commands](../AGENTS.md#checks) for code. `make static`
does not check document purpose, anchors or the truth of a claim. The root
[Makefile](../Makefile) owns the full gate inventory.
