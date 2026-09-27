# Whitefoot — agent instructions

Whitefoot is a programming language designed as a harness for AI agents.
Accepted programs make memory corruption, data races, uninitialized reads,
silent overflow and every other unproved partial operation unrepresentable:
each partial operation is admitted only after machine proof of its domain,
with no writer-accessible unsafe escape or runtime trap. Acceptance uses no
SMT; automatic derivation is specification-fixed, deterministic and
terminating, and no timeout, machine speed, solver state or work budget
selects acceptance. Harder proofs arrive as explicit finite `use` steps inside
a local `invariant`, checked as written. Proofs are erased before lowering and
may authorize check removal, optimization and parallel independence without
adding runtime branches, locks, dependencies or scheduling edges.

## Project goal

The target is a serious research compiler: general enough to implement the
real language, clean enough to evolve, and capable of compiling nontrivial
programs so we can test semantics and performance ideas quickly. It is not an
untrusted-input service or a stable LLVM-scale product.

“Good enough” means a real compiler rather than a source-shaped demo: one
general implementation path, compiler-independent correctness tests where they
help, useful diagnostics, an executable backend, and real programs that expose
language and compiler weaknesses.

When priorities conflict, use this order:

1. reach the next meaningful end-to-end language or performance experiment;
2. preserve semantic correctness and required safety checks;
3. keep the implementation understandable and easy to change;
4. add only the evidence needed to trust the current result; and
5. defer robustness, infrastructure, and polish that no current experiment
   needs.

If work does not help compile a real program, test a language rule, measure a
compiler idea, or remove the immediate blocker to one of those outcomes, it is
probably not the next work.

## Authority and reading

The [workflow map](docs/workflow.md) shows the development loop, who decides
what, where each check runs, and every document's role.

The active specification `spec/kernel-spec.md`, including its normative worked
example, defines the language and toolchain judgments, and the conformance
results state what the compiler implements. `design/` holds the decisions with
their reasons and refused alternatives. `docs/constitution.md` owns purpose,
objectives, tradeoffs and conditional design principles; a concrete choice
needs its own grounds, not just a constitutional ancestor.

Work is not planned in a document up front: a selected direction gets
`research/investigations/<name>/` for its design, measurements and rejected
alternatives, and its surviving decision goes to the design tree. Read only
the material relevant to the task, and do not turn historical research into an
implied implementation requirement. Compiler behavior, tests, archived code
and design prose do not define the language.

A finished task is not evidence: a claim cites the specification, a
conformance case, a measured result under `research/experiments/`, a design
under `research/investigations/`, or a design-tree decision where the
[citation boundaries](docs/workflow.md#citation-boundaries) permit. `archive/`
keeps superseded material, such as retired research, as frozen historical
evidence and rationale instead of deleting it; its retired per-batch record
`archive/done/` is not written to again and not cited. Process wording in any
historical artifact is superseded by the rules below. Words such as
*validation* or *ratification* in language and design artifacts describe
evidence, not a workflow step.

## How work proceeds

Follow the four occasions below. A *material choice* changes accepted
behavior, a safety or trust condition, a shared interface or representation,
a significant performance commitment or a standing project rule. Restoring
specified behavior or editing prose without changing its meaning is routine;
task size and file count do not decide which a change is.

1. **Start or resume:** read the affected current owners; for a material
   choice, also the relevant constitutional aims and existing decision
   grounds. On resumption, verify the actual worktree and PR state.
2. **Choose:** state why a material choice fits its requirements and evidence;
   record a discriminating experiment's criterion before using it to choose,
   and load the `investigation` skill when a choice needs a new measurement,
   benchmark or trial.
3. **Update:** when a conclusion or its grounds change, update current guidance
   and material dependents in the same work. A design revision is an
   owner-ruled tree change or a pending amendment: load the `design-tree`
   skill whenever a task makes, proposes or applies a design decision or edits
   `design/`.
4. **Finish:** load the `completion-review` skill before marking a PR ready or
   reporting completion (checks, one independent review, finding routing,
   publication), and the `owner-handoff` skill whenever you stop for the
   owner.

Routine fixes under unchanged design need no decision record. Record reasons
when choices settle, not by reconstructing them at task completion.

**Fix or record what you notice.** Work in one place exposes defects in
others: a bug, an awkward interface or architecture, duplicated logic, a file
or function grown past what one reader can hold, a stale document or test.
When you notice one, fix it in the same change if it is small and within the
files you are changing; otherwise add an item to `docs/todo.md` before moving
on, with its impact, the change you would make and when to reopen it. List
each in the PR's *Found along the way* section with its disposition. Low
priority defers the work, never the record: a finding kept only in the
conversation is lost. `make static` requires every compiler source file over
4,000 lines to be named in the Code structure section of `docs/todo.md`.

**Verify with observations that could have come out otherwise.** A passing
result is evidence only if a wrong result would have failed it. Prefer an
observation that separates two hypotheses over one merely consistent with the
hypothesis you hold; make each new check fail once for each way it can fail;
never check a transform against its own output. Read an exit code directly,
not through a pipe. Resolve every commit id, path, count and measurement with
a tool when you write it, and never copy one forward. Another agent's or a
reviewer's report is a lead to verify, not evidence. A green result reached by
weakening a requirement does not answer the original question.

Use a PR as the owner's ongoing review surface from the start, as a Draft
until the design-tree workflow makes it ready. Push coherent progress to the
same branch and keep its description and actual validation results current;
publish the reviewed result before reporting completion and link it. Do not
wait for another request to update the PR or leave the reviewable result only
in the local worktree. Updating a work-branch PR never authorizes a merge into
`main`.

Before stopping work, explain the task's specification revisions in the
conversation: which rules changed, their before/after behavior, and why those
changes were selected. A version number or PR link does not replace this.

Recurring procedures are skills: `design-tree`, `spec-amendment`,
`investigation`, `completion-review` and `owner-handoff`. Their bodies live in
the project, in `docs/skills/` and `design/skill/`; `.agents/skills/` (Codex)
and `.claude/skills/` (Claude Code) hold only links to them. Each skill's
description stays in context and its body loads when its step above arrives,
never at session start.

## Branch and main boundary

These are the complete approval and merge rules:

1. Work-branch changes need no approval, including plans, repository layout,
   specifications, conformance evidence, gate wiring, code, tests, and
   documentation, except that new repository-root entries require owner
   approval, changes to the live design tree require the owner's ruling under
   the design-tree skill, and a review finding whose resolution would change a
   design decision or amendment, a specification rule, or the agreed scope
   requires the owner's direction. Unapproved design choices remain
   amendments while implementation continues.
2. Every change merged into `main` requires owner approval of the exact
   revision to be merged.
3. The exact revision merged into `main` must pass all repository tests through
   the canonical `make check` entry point before the merge.
4. If the merge changes `spec/kernel-spec.md` or conformance evidence, the
   pull request states what changed and its selection ground, answered against
   the exact revision being merged. There is no separate ledger: the
   specification's bytes are its identity, the released archives are
   immutable, and git is the history.

- **Work branch** is any branch other than `main`; its work, including edits
  to a specification, conformance evidence or these rules, proceeds within
  rule 1's boundaries.
- **Exact revision** is the complete tree that will enter `main`. If that tree
  changes after approval or after its successful test run, rules 2 and 3 apply
  to the new revision.
- **All repository tests** is the root `make check` target: the compiler
  build, Rust type/lint checks, maintained compiler/runtime/program tests,
  specification and guidance checks, conformance structure and coverage, and
  the full native conformance adapter. Formatting and Rust API documentation
  are authoring commands, performance comparison has its own workflow, and
  research is never a gate dependency. `make check-groups` lists the groups
  and the [workflow map](docs/workflow.md#checks) says where each runs.
- **Conformance evidence** is `tests/conformance` case source and manifest
  content, its runner and adapter, gate-integrity tests, and any collection or
  invocation wiring that can change which cases run or how their results are
  read.

No other workflow step, such as a plan, record, audit, rebase method or commit
shape, is an approval or merge precondition.

## Specification and test integrity

- The active specification `spec/kernel-spec.md` is editable on a work branch;
  released `spec/kernel-spec-vN.md` archives are immutable, and
  `compiler/build.rs` derives the active identity from its bytes. An amendment
  lands as one change, the outgoing bytes archived and the title advanced;
  load the `spec-amendment` skill before editing `spec/kernel-spec.md`.
  `make check` verifies the archive and title, and the optional hook from
  `make install-hooks` reports an archive edit earlier. There is no
  candidate state: a branch carrying an amendment is merge-ready when its gate
  is green. A spec/compiler discrepancy is a technical defect; implementation
  convenience never selects language behavior.
- State each normative fact once; use rule-ID cross-references elsewhere.
  Rule IDs have one definition and bracketed references resolve. Express
  conditions as total positive rules or table data, without exception clauses.
- Surface names label checked invariants. Do not borrow backend terms naming
  lowering consequences; borrow another language's convention only after
  comparing and recording semantic differences, and only when meanings match.
- When the spec changes, bring everything derived from it to the newest version
  in the same work: conformance cases and verdicts, the lexer/parser and
  generated syntax data, tests, and docs. Beyond the archive and title, this
  consistency is your responsibility and is deliberately not machine-enforced.
- Do not silently weaken derived material to make a check pass. Editing a
  conformance verdict, deleting a failing test, or regenerating evidence to go
  green is a governance breach even though no script blocks it. Add ordinary
  compiler tests freely.
- Never delete, disable, ignore, narrow, or unwire a test or check merely to
  make `make check` green. A deliberately retired test must leave an honest
  technical explanation in the same change.
- Compiler capability, an internal error, a timeout, or an unimplemented
  feature is not a source-language rejection and must not rewrite normative
  expectations.
- A language gap is stated as its minimal semantic witness, apart from the
  compiler that exposed it. A project-local issue is fixed in the project, not
  by generalizing the language or compiler. A soundness defect is a
  correctness issue whatever the plan says.
- A test case earns its place with an observation no existing case makes and a
  failure that means something. Specification requirements go in
  `tests/conformance/`, whole-program behavior in `tests/programs/`, and other
  implementation obligations in compiler or runtime tests. Formal tests never
  import `research/`; research may consume formal fixtures.

## Repository structure and hygiene

The repository root and every established directory are a curated, closed set,
so that the active `spec/`, the `compiler/` and the guidance in `docs/` are
found first. Follow this by judgment and keep moving; it is a standing rule,
not a reason to pause on every file.

- Do not add a repository-root entry without owner approval. Put new material
  in the existing directory that owns its kind; if none fits, ask.
- Every new file, directory, script, or document earns its place before it is
  created: name the compiler capability or experiment it serves, its existing
  home, and the condition under which it is removed. If you cannot name all
  three, do not create it.
- No bulk dumps: do not add many scripts or documents in one change and leave
  them unmaintained. A script ships wired to a caller, a gate target or an
  explicit one-shot deleted after use; a document ships into an existing home
  and is kept current or deleted. Material with no owner and no reader is rot
  the moment it lands.
- Prefer native tooling. Check the Rust compiler with `cargo test`,
  `cargo clippy` and the workspace `forbid(unsafe_code)` lint, never with a
  Python script that re-implements them or a script forked per spec version.
  Python belongs only to genuinely compiler-independent tooling. A new
  script must justify why the native path cannot do the job; if it cannot, it
  does not ship.
- Supersede in place: when new material replaces old, update, merge, or delete
  the old in the same change, and do not accumulate parallel versions, stale
  dossiers, or abandoned experiments beside their replacements. Frozen
  archives and useful dated evidence keep their history under their own
  rules.
- Keep important folders, such as `spec/`, `compiler/`, `tests/` and the
  research directories, as clean as the root. Do not undertake structural
  churn that no current work needs, and never relocate a load-bearing path
  merely for tidiness: paths are pinned by the spec, tests, oracle scripts and
  gates. Prefer a clear map, a good name and a stated purpose over relocation.
- No active source, build, test, or tool may depend on `archive/`.
- New and modified repository artifacts, identifiers, comments, diagnostics,
  fixtures, test names, and file names use English.

## Compiler rules

The compiler's implementation rules are its design decisions in
`design/compiler`, each with its reason. Before changing the compiler, read the
subtree you are changing and its ancestors; a decision the tree does not cover
is an amendment, never an edit to the tree. Apply the design-tree skill's
[structural-choice assessment](design/skill/SKILL.md#workflow) when choosing or
revising compiler code structure, including during implementation.

Automatic CI checks current correctness and performance regressions;
exploratory timing runs only when requested. Use the guarded verification
targets in README, or wrap other local heavy builds, suites and benchmarks
with `perl .github/run-check.pl <label> <command> ...`, including commands
from other worktrees. Inspect an existing owner's PID instead of starting
another heavy command. Separate build time from test/program execution,
investigate a stage that exceeds its observed cost, and preserve the full gate
before merge.

## Communication

Describe compiler and language work with precise, neutral technical wording.
Avoid unnecessary security or attack-oriented framing when the task is ordinary
correctness checking; name the concrete rule, failure, and expected behavior.
Retain necessary technical terms and report material risks accurately. Wording
must clarify the work, never conceal its purpose or bypass platform safeguards.

## Data safety

Preserve unrelated user changes in a dirty worktree. Never discard, overwrite,
or rewrite work outside the requested change boundary.
