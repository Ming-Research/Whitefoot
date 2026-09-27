# Task completion review

The items an independent reviewer checks when a task completes. The
[completion-review skill](skills/completion-review/SKILL.md) owns when the
review runs, who runs it, how findings are routed and where the report goes.
Merge conditions remain in [AGENTS.md](../AGENTS.md#branch-and-main-boundary).

## How to review

Read the task's requested outcome and constraints, the complete diff from the
base to the reviewed revision (including uncommitted and new files, but not
the archived specification copies `make review-scope` lists as excluded), and
the actual validation results. Read changed sections in context, the relevant
[document roles](workflow.md#document-roles), and directly affected
definitions, callers, or cases.
Do not load the whole repository or require a separate review packet.

Judge the artifacts against the task and current owners, not just the author's
summary. Mechanical checks cover their encoded properties; this review checks
meaning, placement, and omitted dependent updates. Neither reconstructs an
unrecorded reason or certifies the soundness of a design argument: mark such a
question `unverified` for the implementing agent rather than inventing missing
evidence or redesigning the project.

Check every group whose trigger applies; `make review-scope` names them from
the changed paths, and a group it cannot decide from paths says so. Mark items
`pass`, `finding`, `unverified`, or `not applicable`; missing evidence is not a
pass. When the task changes a review rule or an expected result, compare its
previous form with the requested change rather than judging only against the
newly edited rule. A finding names its item ID, file/line, the offending text
or missing evidence, and a short reason; quote both sides of a contradiction.
Summarize clean groups together and link detailed evidence instead of copying
logs or this checklist. Later edits invalidate review of the affected content,
not unrelated completed checks.

## A. Scope and repository layout — every change

Source: [repository hygiene](../AGENTS.md#repository-structure-and-hygiene).

- [ ] **A1 — Task fit.** Each changed artifact serves the requested outcome or
  a necessary dependency. Unrelated user work is preserved; the completion
  claim does not silently drop a requirement or leave an advertised fix as a
  stub, TODO, or unconnected implementation.
- [ ] **A2 — New paths.** For each added file or directory, identify its
  consumer, existing home, and removal condition. No unnecessary root entry,
  parallel document, copied implementation, per-task report, or unused helper
  was added. An established directory is not a dumping ground either.
- [ ] **A3 — Connections.** Added scripts and maintained tests have a real
  caller or collection path; documented commands name existing targets.
  Moves/deletions update affected imports, links and gate wiring. Superseded
  live material is updated or removed; frozen history and useful dated
  evidence follow their own retention rules.
- [ ] **A4 — Artifact hygiene.** No scratch output, personal filesystem path,
  credential, or machine-local setup has leaked into the diff. New/modified
  artifacts use English. Tooling uses the project's native path unless its
  different role justifies another tool; `AGENTS.md` remains the single
  agent-instruction source.

## D. Documentation — changed Markdown, comments or examples

- [ ] **D1 — Purpose.** Each added or changed passage serves the containing
  document or code's reader. Check against the
  [document roles](workflow.md#document-roles), including editorial history
  and process instructions inserted into substantive documents. For
  constitutional changes, check that complete clauses state the relevant
  obligations and conditions; a chosen prohibition is not merely a report of
  current implementation behavior.
- [ ] **D2 — References.** Changed references resolve to the intended file,
  heading or symbol, obey the
  [citation boundaries](workflow.md#citation-boundaries), and support their
  claim. A correct relative path does not make an inappropriate citation
  acceptable.
- [ ] **D3 — Current meaning.** Changed claims agree with their owning source
  and directly affected guidance. Update an obsolete standing statement, not
  only a later correction beneath it. Distinguish a goal, proposal, specified
  behavior, implemented capability and dated measurement; historical positions
  are not conflicting current instructions merely because they differ.
- [ ] **D4 — Usability.** Instructions name real commands and prerequisites.
  Changed runnable examples have been checked through the ordinary path;
  fragments have enough surrounding context and are not offered as complete
  programs. No duplicated changing status or version identity needs another
  synchronized update. Do not invent new style conventions during review.

## C. Code and cases — changes under `compiler/`, `lib/` or `tests/`

Source: [compiler rules](../AGENTS.md#compiler-rules) and
[test integrity](../AGENTS.md#specification-and-test-integrity).

- [ ] **C1 — Observable case.** A bug fix has a case that distinguishes the
  faulty behavior from the intended result; a new behavior has coverage for
  its normal use and relevant failure/boundary. Identify the case and result.
  Existing coverage is sufficient if it exercises the change. A pure refactor
  can name its existing coverage; prose-only edits need no invented test.
- [ ] **C2 — Independent expectation.** The expected result follows the task
  contract or specification, not the implementation's current output. For a
  regression, show the case fails before and passes after, or explain why the
  old run is unavailable and how the case detects the fault. A checker's
  negative case fails for the intended reason, not an earlier unrelated error.
- [ ] **C3 — General path.** The diff implements a grammar/semantic rule or
  general runtime operation. No function, project, source shape or test name
  selects a special acceptance/lowering path. No duplicated rule, unexplained
  forwarding layer, or fallback conceals an unsupported capability.
- [ ] **C4 — Safety boundary.** Inspect changed acceptance and proof paths for
  added Rust `unsafe`, weakened contracts, runtime substitutes for required
  proof, impossible-case returns, and timeout/fuel/heuristic acceptance limits.
  Changes to optimization facts retain acceptance and behavior with facts off;
  relevant evidence is supplied when those paths change. If a semantic safety
  question cannot be settled by local inspection and cases, flag it for deeper
  review rather than certifying soundness.
- [ ] **C5 — Architectural fit.** Apply the design skill's
  [G3](../design/skill/SKILL.md#design-checks) to structural choices. Check that
  assessment occurred when making or revising the choice and was explained
  to the owner, rather than supplied retrospectively at completion.

## T. Specification and checks — changes to `spec/kernel-spec.md`, `tests/`, a Makefile or `.github/`

Source: [specification and test integrity](../AGENTS.md#specification-and-test-integrity).

- [ ] **T1 — Language evidence.** For a specification amendment, the outgoing
  active bytes are archived unchanged, released archives are untouched, and
  the active title advances the version. The change declares its
  specification delta (rules, tokens, spellings, exceptions) and
  evidence/minimality selection ground. `make static` checks the archive
  name, its bytes and the title mechanically. Affected cases/verdicts,
  generated syntax, compiler and documentation follow the amendment. For
  changed rules or constitutional premises, apply R3–R4 below. For
  conformance changes, the PR explains the normative expectation and how the
  changed evidence tests it. The conformance runner checks unique rule IDs
  and resolving references; inspect semantic duplication, exception clauses,
  and whether non-authoritative review inventories match their normative
  definitions. The approval boundary remains
  [AGENTS.md rule 4](../AGENTS.md#branch-and-main-boundary).
- [ ] **T2 — Preserved checks.** Every removed, skipped, narrowed, regenerated
  or weakened test/check has a technical reason consistent with the requested
  change. An implementation gap, crash, timeout or unsupported feature has not
  been relabeled as normative source rejection. Check assertions and collection
  wiring as well as filenames and pass counts.
- [ ] **T3 — Effective checks.** New cases actually run. New or changed check
  machinery demonstrates that a representative wrong result or missing input
  is detected. Reusing established machinery needs no new mutation campaign.
  Active build/test/tool paths do not depend on `archive/`.
- [ ] **T4 — Case admission and home.** Identify each added or changed case's
  protected property, meaningful failure and owning group. Additions or expanded
  coverage identify the observation missing from existing cases. The home
  follows the property: normative requirements in conformance,
  complete program behavior in programs, additional implementation obligations
  in compiler/runtime tests. A WF fragment wrapped in `#[test]` or a new
  executable does not justify another case. Merge checks with the same observations.
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

## R. Decisions — changed choices, premises or relevant evidence

Source: [How work proceeds](../AGENTS.md#how-work-proceeds) and the
[design-tree skill](../design/skill/SKILL.md#what-is-a-decision). Applies to
changes under `design/`, `docs/constitution.md`, `spec/kernel-spec.md` or
`research/investigations/`, and to any task that made a material choice
elsewhere. These are checks on observable artifacts, not a claim to know an
agent's internal reasoning or a second design review. A routine fix under
unchanged design can skip this group; absence of a tree diff does not
establish that the group is inapplicable.

- [ ] **R1 — Stated ground.** A material choice has a retrievable explanation
  of its purpose, required properties, assumptions, alternatives actually
  considered, selection reason, and remaining uncertainty. Relevant prior
  objections are addressed. Distinguish deductions, observations, and
  provisional choices; a constitutional citation alone does not select a
  particular mechanism. Flag a substantive question for the implementing
  agent rather than inventing a rationale. Unresolved proposals and assumptions
  have not become settled decisions through wording alone. Before real project
  adoption, internal adaptation costs have not been used to reject a language
  change, and test/example frequency has not been passed off as real usage.
- [ ] **R2 — Discriminating evidence.** An experiment used to select a design
  states what comparison could distinguish it, the conditions and protected
  requirements, and the actual outcome. A claim of a prediction made before
  measurement has an inspectable prior criterion; otherwise label the finding
  exploratory or the timing unverified. A changed requirement, inconclusive
  result, or trial on one model has not been reported as broader success.
- [ ] **R3 — Consumed changes of reason.** For changed objectives, premises,
  rules, cited sources, or evidence meeting a reopening condition, use the diff
  and direct references to check the named affected set. The explanation says
  which choices still stand, stand on different grounds, or need replacement.
  Their current owners agree with the design record maintained under the
  design skill. Remaining questions have a concrete source; a log entry does
  not supersede contradictory standing guidance.
  Do not require an unrelated project-wide sweep.
- [ ] **R4 — Maintained tree.** Added, changed, or retired rules and changed
  grounds have corresponding design records under the design skill, and the
  sources a decision cites resolve and support the stated scope. A log entry
  or a lint success is not proof that a cited argument is true.

## M. Design review — every change

- [ ] **M1 — Design procedure.** Apply the design-tree skill
  (`design/skill/SKILL.md`) to the reviewed scope: its design checks G1–G3,
  correspondence checks DC1–DC4 and structural validation. That skill owns the
  procedure; include its actual results in this completion review.

## V. Validation and handoff — every change

Source: [How work proceeds](../AGENTS.md#how-work-proceeds), the
[investigation skill](skills/investigation/SKILL.md) and the
[merge boundary](../AGENTS.md#branch-and-main-boundary).

- [ ] **V1 — Actual checks.** Applicable checks ran on the delivered content;
  commands, results and limitations are available. Missing tools, unrun tests,
  skips and expected failures are reported accurately. Focused success is not
  described as a complete gate; review does not rerun unaffected green tests.
- [ ] **V2 — Supported claims.** Counts, paths, revisions and quoted results
  were checked. A performance claim names workload, environment and comparison;
  a causal claim has isolating evidence. A historical result or another agent's
  report is not silently presented as a fresh independent measurement.
- [ ] **V3 — Delivery.** The reply/PR describes the current result and remaining
  limitations. Its *Found along the way* section gives every defect or
  opportunity the work exposed a disposition: fixed, recorded in
  `docs/todo.md`, or declined with a reason; report any the reviewer notices
  that it omits. For specification revisions, check the conversation
  explanation required by `AGENTS.md`: affected rules, before/after behavior,
  and selection grounds. Conformance changes explain what changed and their
  selection ground. If merging is requested, verify owner approval and root
  `make check` for the exact merge tree under the existing four rules; neither
  a fast review nor a focused test run substitutes for them.
- [ ] **V4 — Existing PR updated.** The reviewed task changes are committed
  and pushed to the existing PR branch without waiting for a reminder; its
  remote head contains the delivered revision, and its description and
  validation reflect the current diff. The
  reply links the PR. A failed publication is an explicit delivery blocker,
  not a completed update. This check runs after the content review and push.

Use existing checks when applicable: `git diff --check` for patch whitespace;
`make static` for repository invariants, specification archives (immutability
and amendment shape), live spec references, cited review items, entry-document
paths, skill links and design-tree form; `make -C compiler format lint` and the
[focused compiler commands](../README.md#verification) for code. `make static`
does not check document purpose, anchors or the truth of a claim, and compiler
`docs` builds Rust API documentation, not this prose checklist. The root
[Makefile](../Makefile) owns the full gate inventory; the design-tree checks
follow `design/skill/SKILL.md` and are covered by M1.
