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
[citation boundaries](#citation-boundaries) permit. `archive/`
keeps superseded material, such as retired research, as frozen historical
evidence and rationale instead of deleting it; its retired per-batch record
`archive/done/` is not written to again and not cited. Process wording in any
historical artifact is superseded by the rules below. Words such as
*validation* or *ratification* in language and design artifacts describe
evidence, not a workflow step.

## How work proceeds

A *material choice* changes accepted behavior, a safety or trust condition, a
shared interface or representation, a significant performance commitment or a
standing project rule. Restoring specified behavior or editing prose without
changing its meaning is routine; task size and file count do not decide which
a change is. Only a material choice between viable alternatives is a design
decision; this is the project's threshold for the `design-tree` skill's
decisions.

1. **Before starting,** read the affected current owners; for a material
   choice, also the relevant constitutional aims and existing decision
   grounds. On resumption, verify the actual worktree and PR state. Settle
   the direction with the owner first, as the `design-tree` skill describes.
2. **While working,** state why each material choice fits its requirements and
   evidence, and record a discriminating experiment's criterion before using
   it to choose; `research/README.md` holds the method for attributing a
   performance loss or proof cost and for agent writer trials. Change the
   code, the specification and the design tree together on a Draft PR; when a
   conclusion or its grounds change, update current guidance and material
   dependents in the same work. Work through to completion, as the skill
   describes.
3. **At completion,** validate (see [Checks](#checks)), run the one
   [review](#review), fix what it finds, and hand the work back as the
   `design-tree` skill describes. After the skill's parts, the handoff
   shows every specification change rule by rule, with its before and after
   behavior and the card that selected it or why it needed none; the
   validation actually run, its revision and what remains unverified; the
   review's scope and the findings it fixed; and what the work found along
   the way. A version number or PR link does not replace this.
4. **After the owner approves** every decision the work needs, including every
   design-tree and specification change, write the log entries and mark the PR
   ready (rule 1 below).

**Weigh a design on its merits alone.** No design judgment, of the language,
the compiler, the runtime or the library, weighs the existing code, tests,
programs or documents a choice would change: not the cost or effort of
migrating them, not how many of them it touches, and not their present use as
evidence of what programs need. Before real projects adopt Whitefoot there is
nothing to keep compatible, and the corpus was written to exercise the
compiler (`design/language.md`). A decision card names no such cost as a
reason, a cost or an option's drawback.

Record reasons when choices settle, not by reconstructing them at completion.
Routine fixes under unchanged design need no decision record.

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
until rule 1 below lets it become ready. Push coherent progress to the same
branch and keep its description and actual validation results current;
publish the reviewed result before reporting completion and link it. Do not
wait for another request to update the PR or leave the reviewable result only
in the local worktree. Updating a work-branch PR never authorizes a merge into
`main`.

**The design tree in this project.** The `design-tree` skill is the one
recurring procedure kept as a skill. It is written for any project and lives
in `design/skill/`; `.agents/skills/` (Codex) and `.claude/skills/` (Claude
Code) hold only links to it, and its body loads when its description matches
the task. Here its roles are:

- live trees: `design/language.md` and `design/compiler.md` with their
  subdirectories;
- change log: `design/log.md`;
- research record: `research/investigations/` and `research/experiments/`;
- maintained TODO: `docs/todo.md`;
- form check: `make design-lint`, part of `make static`;
- readiness check: `make design-ready`, which also requires an approved
  `spec/log.md` entry for a changed specification, run by
  `.github/workflows/design-readiness.yml` on ready PRs and on main.

## Branch and main boundary

These are the complete approval and merge rules:

1. Work-branch changes need no approval, including plans, repository layout,
   the design tree, specifications, conformance evidence, gate wiring, code,
   tests, and documentation, except that new repository-root entries require
   owner approval. A PR becomes ready only after the owner has approved every
   decision it needs, including every design-tree and specification change;
   the approval is recorded in `design/log.md` and `spec/log.md` only then,
   and `make design-ready` checks the records.
2. Every change merged into `main` requires owner approval of the exact
   revision to be merged.
3. The exact revision merged into `main` must pass all repository tests through
   the canonical `make check` entry point before the merge.
4. If the merge changes `spec/kernel-spec.md` or conformance evidence, the
   pull request states what changed and its selection ground, answered against
   the exact revision being merged. The specification's bytes are its
   identity, the released archives are immutable, `spec/log.md` records the
   owner's approval of each change, and git is the history.

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
  and [Checks](#checks) says where each runs.
- **Conformance evidence** is `tests/conformance` case source and manifest
  content, its runner and adapter, gate-integrity tests, and any collection or
  invocation wiring that can change which cases run or how their results are
  read.

No other workflow step, such as a plan, record, audit, rebase method or commit
shape, is an approval or merge precondition.

## Specification and test integrity

- The active specification `spec/kernel-spec.md` is editable on a work branch;
  released `spec/kernel-spec-vN.md` archives are immutable, and
  `compiler/build.rs` derives the active identity from its bytes. A branch's
  amendment lands as one change:
  1. Archive once per branch: with the local `main` current, copy the base's
     active bytes to the archive named by their title token, for a base
     titled `# Kernel Specification v0.69`
     `git show "$(git merge-base main HEAD)":spec/kernel-spec.md > spec/kernel-spec-v0.69.md`.
  2. Retitle the active file to the next version, `v0.70`, or `v1.0` for a
     major revision.
  3. Edit the rules under the bullets below, and record a rule chosen among
     viable alternatives in the design tree.
  4. If `main` advanced the version meanwhile, merging it conflicts on the
     title: keep main's text, reapply this branch's rule changes, archive
     main's active bytes under main's version and retitle to the next one.
  5. After the owner approves, add the `spec/log.md` entry naming every rule
     added, changed or retired.

  `make static` verifies the archive and title, the optional hook from
  `make install-hooks` reports an archive edit earlier, and
  `make design-ready` requires the approval entry. A spec/compiler discrepancy
  is a technical defect; implementation convenience never selects language
  behavior.
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
  fixtures, test names, and file names use English. The one exception is
  `README.zh-CN.md`, the owner's Chinese translation of `README.md`: a change
  to either file changes the other in the same change, and `make static`
  refuses a branch that changes only one.

### Document roles

Each document holds what serves its reader; a brief summary or relevant
technical explanation is useful, duplicating another document's changing
inventory or mixing in the editing conversation is not. A file needs no new
status banner or self-description merely to satisfy this list.

- `README.md`: introduction, getting started and navigation, not a compiler
  inventory, a second specification or task history.
- `AGENTS.md`: goal and priorities, authority, how work proceeds, the approval
  and merge rules, integrity and hygiene rules, checks and review; not
  research narration or a design procedure the `design-tree` skill holds.
- `design/skill/`: the project-independent design-tree procedure; nothing
  specific to Whitefoot.
- `docs/review-checklist.md`: the items a reviewer answers from the diff; not
  language semantics, task outcomes or a procedure stated in full elsewhere.
- `docs/constitution.md`: complete statements of purpose, objectives,
  obligations, prohibitions, tradeoffs and their conditions; not who asked
  for an edit, conversations, progress, maintenance instructions, abbreviated
  labels in place of clauses, per-clause usage checklists or a selected
  mechanism presented as an inevitable consequence of the purpose.
- `spec/kernel-spec.md`: normative syntax, semantics, judgments, boundaries
  and examples; not compiler convenience presented as law or editing history.
  `spec/log.md` holds its approvals.
- `docs/todo.md`: defects, costs, improvement opportunities and their
  validation, removed when resolved; not settled decisions, claims of
  implemented capability or progress logs.
- `docs/patterns.md`: writer problems, usable forms, examples, applicability
  and costs; not acceptance rules, universal performance claims or project
  administration.
- `docs/ideas.md` and `docs/why-whitefoot.md`: candidate mechanisms and
  explanatory essays; not a work queue, invented measurements or contributor
  process inserted into an essay.
- `docs/articles/`: one idea each for readers outside the project, every
  program accepted or rejected as shown by the compiler revision the article
  names; not normative rules, claims no repository file or command
  reproduces, or project process.
- `research/` and `governance/spec-evolution/`: questions, alternatives,
  designs, experiments, results and limitations; not task completion
  presented as evidence, a proposal presented as an implemented rule, or
  daily test implementations and inputs kept in research.
- `design/`: live decisions with their reasons and refused alternatives and
  the approval log; not inventories, transcripts or progress.
- `archive/`: superseded material kept frozen; never edited or depended on.
- The PR description: this change's problem, behavior, grounds, validation,
  limitations and what it found along the way, kept current with the diff;
  not a source of project rules.

### Citation boundaries

- Definitions point to their current owner; technical claims point to the
  specification, source and cases, a relevant design, or reproducible
  evidence, and the linked passage supports the claim.
- The constitution, specification, writer patterns and essays stand without
  the design trees: they do not link to `design/` or use it as authority.
- Maintainer navigation (README, this file, the research index) may point to
  the design trees. Research records and PRs may cite decisions as historical
  rationale, not as language definitions or proof of an empirical claim. A
  tree node may cite specifications, designs and evidence in its reason.
- Historical references may name their historical versions; current guidance
  uses the active specification's stable path. Frozen archives keep their
  historical content.

## Compiler rules

The compiler's implementation rules are its design decisions in
`design/compiler`, each with its reason. Before changing the compiler, read the
subtree you are changing and its ancestors; a decision the tree does not
cover is added to the tree for the owner's approval, never left only in
code. Apply the design-tree skill's
[structural-choice assessment](design/skill/SKILL.md#workflow) when choosing
or revising compiler code structure, including during implementation.

Automatic CI checks current correctness and performance regressions;
exploratory timing runs only when requested. Separate build time from
test/program execution, investigate a stage that exceeds its budget, and
preserve the full gate before merge.

## Checks

- `make static`, before every push and in `gate.yml` on every push:
  repository invariants, the specification archives, the README and its
  translation changed together, prose integrity, guidance references,
  compiler sources over 4,000 lines named in `docs/todo.md`, and the design
  tree's form.
- `make check`, on the revision to merge: the static group plus the compiler
  build, tests, the conformance adapter and the runtime; `make check-groups`
  lists the groups. `gate.yml` runs those groups on Linux and macOS on every
  push, and its green run on the exact revision to be merged, a head current
  with `main`, is that revision's `make check`; run it locally to reproduce a
  failure or when CI is unavailable. It needs `python3`, LLD on Linux
  (`ld.lld`) and the `time` utility.
- `make design-ready`, before marking ready and in `design-readiness.yml` on
  ready PRs and main: approved tree and specification changes.
- CI only: `io-hosts.yml` on every push (Linux io_uring and Windows IOCP),
  `compute-regression.yml` on PRs that touch measured inputs (paired WF-to-WF
  timing), and `io-bench.yml` and `compute-bench.yml` on request, which are
  experiments and never a gate.
- `make install-hooks` optionally reports an edit of a released archive at
  commit.

Focused commands for a compiler change, before `make static`:

```sh
make -C compiler format lint
make -C compiler build        # optimized compiler only
make -C compiler test-build   # construct test executables without running cases
perl .github/run-check.pl <label> cargo test --manifest-path compiler/Cargo.toml --profile gate --locked --offline --lib <filter>
```

Heavy commands run under `perl .github/run-check.pl <label> <command> ...`,
as the `make` targets already do, including commands from other worktrees. It
holds one host-wide lock, leaves Cargo and the test harness at their own
default of every available processor unless `CARGO_BUILD_JOBS` or
`RUST_TEST_THREADS` names fewer, prints wall, user and system time with a
report every 30 seconds, and stops a command after 30 minutes unless
`WHITEFOOT_CHECK_TIMEOUT` gives another limit in seconds. It also compares
each labeled stage with its budget in `.github/time-budgets.txt` without
changing the stage's status: CI records a stage that exceeded its budget or
has none and fails the job in a final verdict step, and a local run only
prints the comparison. When a stage exceeds its budget, look at what the
change adds to that stage, such as cases, fixtures or work on its path, and
at the job's slowest cases and host. Fix a cause you find, or bring the raise
it needs to the owner; re-run the job once when you cannot tell; and when the
change plainly cannot slow the stage, report the overrun as runner variance in
the validation you hand back, where it does not hold the revision back.
Raising a budget is a decision for the owner; lower one in the change that
makes its stage much faster, and give a new labeled CI stage its budget. Inspect an existing owner's PID instead of starting
another heavy command, and after an
uncatchable stop inspect the recorded PID and command before removing a stale
lock. The `gate` Cargo profile builds the Rust compiler with optimization,
debug assertions and overflow checks; it does not change how WF source is
compiled. For a slow compiler test, set `WHITEFOOT_TEST_TIMINGS` to a scratch
TSV path to record the phases of the shared test helpers.

## Review

One review per task, when the work is complete and before the handoff, and
whenever the owner asks for one. Start a separate, read-only agent that did
not implement the change:

- for a change to code, tests, the specification, gate wiring, the design
  tree or agent guidance, a mid-sized model and every applicable group of
  [the review checklist](docs/review-checklist.md), whose M group is the
  `design-tree` skill's design correspondence review;
- when only research records or other prose changed, a small model and
  groups A, D, M and V, plus R for a material choice.

Give it this prompt, filled in:

```text
You are reviewing a Whitefoot change you did not write. Do not edit files.
Task outcome and constraints: <...>
Base and head: <...>; validation already run: <commands, results, revision>.
Read the diff from the base (git diff <base>, plus untracked files, without
the released archives spec/kernel-spec-v*.md), the changed sections in
context, and "How to review" in docs/review-checklist.md. Check each group
whose trigger applies. For M1, apply the design checks and correspondence
checks of design/skill/SKILL.md to the relevant tree nodes and ancestors.
Do not rerun green suites. Report Scope (your model, base..head, groups
checked and skipped), Checks (what you ran) and Findings (item ID, file:line,
quoted text or missing evidence, reason; quote both sides of a
contradiction), or "none within scope".
```

Fix every finding and review again as the `design-tree` skill's workflow
describes; a fix that changes a specification rule is also shown with the
specification changes at handoff. Merging main without conflicts in reviewed
content needs no new review; a resolved conflict is reviewed as changed
content, those hunks only. Then commit and push, verify that the remote head
is the reviewed revision, and fill the PR's review section. A failed
publication is a blocker to report, not a completed update.

## Communication

Describe compiler and language work with precise, neutral technical wording.
Avoid unnecessary security or attack-oriented framing when the task is ordinary
correctness checking; name the concrete rule, failure, and expected behavior.
Retain necessary technical terms and report material risks accurately. Wording
must clarify the work, never conceal its purpose or bypass platform safeguards.

## Data safety

Preserve unrelated user changes in a dirty worktree. Never discard, overwrite,
or rewrite work outside the requested change boundary.
