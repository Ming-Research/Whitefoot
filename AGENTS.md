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

A serious research compiler: general enough to implement the real language,
clean enough to evolve, and able to compile nontrivial programs so semantics
and performance ideas can be tested quickly; not an untrusted-input service or
a stable LLVM-scale product. That means one general implementation path,
compiler-independent correctness tests where they help, useful diagnostics,
an executable backend, and real programs that expose language and compiler
weaknesses.

First priorities: reach the next meaningful end-to-end language or
performance experiment, then preserve semantic correctness and required
safety checks. Work that does not help compile a real program, test a
language rule, measure a compiler idea or remove the immediate blocker to one
of those is probably not the next work.

## Authority and reading

The active specification `spec/kernel-spec.md`, including its normative worked
example, defines the language and toolchain judgments, and the conformance
results state what the compiler implements. Compiler behavior, tests, the
Lean model under `proof/`, archived code and design prose do not define the
language.
`docs/constitution.md` owns purpose, objectives, tradeoffs and conditional
design principles.

The research record is `research/investigations/<name>/` and
`research/experiments/`. A claim cites the specification, a conformance case,
a measurement under `research/experiments/`, a design under
`research/investigations/`, or a design-tree decision where the
[citation boundaries](#citation-boundaries) permit. `archive/` keeps
superseded material frozen; `archive/done/` is not written to or cited.
Words such as *validation* or *ratification* in language and design artifacts
describe evidence, not a workflow step.

## How work proceeds

- Before a material choice, also read the relevant constitutional aims; the
  constitution's
  [compatibility clause](docs/constitution.md#compatibility-and-evolution)
  decides when compatibility starts to count.
- `research/README.md` holds the method for attributing a performance loss or
  proof cost and for agent writer trials.
- The completion report also shows every specification change rule by rule,
  with its before and after behavior and the card that selected it or why it
  needed none.
- Programs in this repository were written to exercise the compiler
  (`design/language.md`): a program that fails still witnesses a gap, and
  measuring a change's effect on them is still evidence.
- **Fix or record what you notice**, and list each item in the PR's *Found
  along the way* section. `make static` requires every Rust source file under
  `compiler/src/` over 4,000 lines to be listed in
  `.github/oversized-sources.txt` with the key of the status board item that
  records its split.
- `make design-ready` also requires a new `spec/log.md` entry for a
  changed specification.

## Branch and main boundary

Whitefoot's extra merge preconditions:

1. The gate is the root `make check`: the compiler build, Rust type/lint
   checks, maintained compiler/runtime/program tests, specification and
   guidance checks, conformance structure and coverage, and the full native
   conformance adapter. Formatting and Rust API documentation are authoring
   commands, performance comparison has its own workflow, and research is
   never a gate dependency. `make check-groups` lists the groups and
   [Checks](#checks) says where each runs.
2. Each specification change is recorded in `spec/log.md`.
3. When the merge changes the concurrent map's runtime or its test,
   `map-sanitizers.yml` passes on the exact revision being merged.
4. `sanitizers.yml` passes on the exact revision being merged: no sanitizer
   reports on an accepted program
   ([sanitized builds](design/compiler/sanitized-builds.md)).
5. If the merge changes `spec/kernel-spec.md` or conformance evidence, the
   pull request states what changed and its selection ground, answered
   against the exact revision being merged. The specification's bytes are its
   identity, the released archives are immutable, and git is the history.
6. When the merge changes `spec/kernel-spec.md`, `proof/` or the conformance
   cases, `proof.yml` passes on the exact revision being merged.

**Conformance evidence** is `tests/conformance` case source and manifest
content, its runner and adapter, gate-integrity tests, and any collection or
invocation wiring that can change which cases run or how their results are
read.

## Specification and test integrity

- The active specification `spec/kernel-spec.md` is editable on a work branch;
  released `spec/kernel-spec-vN.md` archives are immutable, and
  `compiler/build.rs` derives the active identity from its bytes. The file
  carries no version and an amendment adds no archive, so two branches never
  write the same line to claim a version. A branch's amendment lands as one
  change:
  1. Edit the rules under the bullets below.
  2. In the same change, add the `spec/log.md` entry at the top, headed
     `## <date> <title>` with no version, naming every rule added, changed or
     retired. Existing entries stay as written, and merging `main` keeps both
     branches' entries (`spec/log.md` merges with git's union driver), the
     branch's first.

  An entry's version is derived from its place in the log:
  `sh .github/spec-version.sh` prints the active version and `--list` every
  entry's, counting up from the nearest entry below that names one (an entry
  names a version only to start a major version such as `v1.0`). Cite a new
  amendment by its date and title until it merges and by its version after.
  A milestone is archived by a separate change that edits no rule and adds
  `spec/kernel-spec-<version>.md`, a copy of the active file.

  `make static` verifies the title, the log's entries and the archives, and
  `make design-ready` requires the new entry. A spec/compiler discrepancy is
  a technical defect; implementation convenience never selects language
  behavior.
- State each normative fact once; use rule-ID cross-references elsewhere.
  Rule IDs have one definition and bracketed references resolve. Express
  conditions as total positive rules or table data, without exception clauses.
- Surface names label checked invariants. Do not borrow backend terms naming
  lowering consequences; borrow another language's convention only after
  comparing and recording semantic differences, and only when meanings match.
- When the spec changes, bring everything derived from it to the newest version
  in the same work: conformance cases and verdicts, the lexer/parser and
  generated syntax data, tests and docs, and the Lean model under `proof/` with
  its coverage list and proofs.
- Conformance verdicts and evidence change only with the specification;
  editing a verdict or regenerating evidence to go green is a governance
  breach. A compiler limitation, internal error, timeout or unimplemented
  feature is not a source-language rejection.
- A language gap is stated as its minimal semantic witness, apart from the
  compiler that exposed it. A project-local issue is fixed in the project, not
  by generalizing the language or compiler. A soundness defect is a
  correctness issue whatever the plan says.
- Specification requirements go in
  `tests/conformance/`, whole-program behavior in `tests/programs/`, and other
  implementation obligations in compiler or runtime tests. Formal tests never
  import `research/`; research may consume formal fixtures.

## Repository structure and hygiene

- Check the Rust compiler with `cargo test`, `cargo clippy` and the workspace
  `forbid(unsafe_code)` lint, never with a Python script that re-implements
  them or a script forked per spec version; Python belongs only to genuinely
  compiler-independent tooling.
- No active source, build, test or tool depends on `archive/`.
- `README.zh-CN.md` is the owner's Chinese translation of `README.md`: a
  change to either changes the other in the same change, and `make static`
  refuses a branch that changes only one.

### Document roles

- `README.md`: introduction and navigation; points to the user guide.
- `docs/guide/`: user documents for readers outside the project;
  `docs/guide/README.md` indexes the guides and owns the checked example
  convention.
- `AGENTS.md`: Whitefoot's goal, authority, process additions, approval and
  merge rules, integrity rules, checks and review.
- `docs/review-checklist.md`: the items a reviewer answers from the diff.
- `docs/constitution.md`: complete statements of purpose, objectives,
  obligations, prohibitions, tradeoffs and their conditions; not
  conversations, progress, maintenance instructions, abbreviated labels,
  per-clause usage checklists or a selected mechanism presented as an
  inevitable consequence of the purpose.
- `spec/kernel-spec.md`: normative syntax, semantics, judgments, boundaries
  and examples; not compiler convenience presented as law or editing history.
  `spec/log.md` holds its change log.
- Whitefoot-kit's [writing guide](https://github.com/Ming-Research/Whitefoot-kit/blob/main/docs/writing.md): writer
  problems, usable forms, examples, applicability and costs, for agents.
- `docs/safety-claims.md`: each safety claim with the specification rules and
  conformance cases it rests on, what the specification does not promise, and
  the trusted base; not a claim the specification does not state.
- `docs/ideas.md` and `docs/why-whitefoot.md`: candidate mechanisms and
  explanatory essays; not a work queue or invented measurements.
- `docs/articles/`: one idea each for readers outside the project, every
  program accepted or rejected as shown by the compiler revision the article
  names.
- `research/` and `governance/spec-evolution/`: questions, alternatives,
  designs, experiments, results and limitations; not a proposal presented as
  an implemented rule, or daily test implementations and inputs.
- `archive/`: superseded material kept frozen.

### Citation boundaries

- Technical claims point to the specification, source and cases, a relevant
  design, or reproducible evidence.
- The constitution, specification and essays stand without
  the design trees: they do not link to `design/` or use it as authority.
- Maintainer navigation (README, this file, the research index) may point to
  the design trees. Research records and PRs may cite decisions as historical
  rationale, not as language definitions or proof of an empirical claim.
- Historical references may name their historical versions; current guidance
  uses the active specification's stable path.

## Compiler rules

The compiler's implementation rules are its design decisions in
`design/compiler`. Exploratory timing runs only when requested; measure
build time apart from test and program execution.

## Checks

- `make static`, run by the static group of `gate.yml`, which adds the
  conformance structure, the performance instrument and the compiler's
  lint check: repository invariants, the specification
  archives, the README and its translation changed together, prose
  integrity, guidance references, Rust sources under `compiler/src/` over
  4,000 lines listed in `.github/oversized-sources.txt`, the safety claims
  page against the specification and the conformance manifest, and the design
  tree's form, read through the `design/skill` submodule.
- `make check`, on the revision to merge: the static group plus the compiler
  build, tests, the conformance adapter and the runtime. Its corpus group
  compiles the example programs in `docs/guide/` and both READMEs under the
  [guide's convention](docs/guide/README.md#writing-examples). `make check-groups`
  lists the groups. `gate.yml` and `io-hosts.yml` check a push to `main`, a
  push to a branch with an open ready pull request, the opening or reopening
  of a ready pull request, the event of marking one ready and a manual run;
  any other push, a draft pull request's included, skips their checks, and
  the event of marking a pull request ready skips them when its head already
  has a successful manual run of the same workflow. `gate.yml` runs the groups
  on Linux only while macOS is off (wf-gate-no-macos); its job `Gate on both
  platforms` passes only when every group is green on Linux for the revision,
  and its job `No temporary workflow` fails while a `tmp-` workflow file
  exists in the checked head, and while it fails the correctness groups and
  `Gate on both platforms` do not run. The success of `Gate on both platforms`
  on the exact revision to be merged, a head current with `main`, is that
  revision's `make check`. It needs `python3`, `jq`, LLD on Linux (`ld.lld`)
  and the `time` utility.
- Writing and the full check: a push to a draft pull request starts no gate,
  so the result for what a change touches comes from a temporary workflow
  (below). Once the review has passed, delete the `tmp-` file, merge `main`
  into the branch, push, and start the full check on that head by hand:
  `gh workflow run gate.yml --ref <branch>`, `gh workflow run io-hosts.yml
  --ref <branch>` and `gh workflow run sanitizers.yml --ref <branch>`. Green
  on that head is the last run: marking the pull request ready then starts no
  second gate, I/O or sanitizer run for it. A red is fixed
  with a temporary workflow again, and the full check is started again once
  the review has seen the fix; a push after a green needs its own run.
  `compute-regression.yml` runs when the pull request is marked ready and
  skips no earlier hand-started run.
- `make design-ready` runs in `design-readiness.yml`.
- CI only: `io-hosts.yml`, on the same revisions as the gate (Linux io_uring
  and Windows IOCP),
  `sanitizers.yml` on the same revisions as the gate (the corpus target's
  programs and running conformance cases, the cases also with `--par`, under
  AddressSanitizer with UndefinedBehaviorSanitizer and under ThreadSanitizer;
  any report fails it),
  `map-sanitizers.yml` on every push to a PR that changes the concurrent
  map's runtime or its test (that test under AddressSanitizer and
  ThreadSanitizer, on the PR's head),
  `proof.yml` on every push to a PR that changes the specification, `proof/`,
  the conformance cases or that workflow, drafts included, and on such a push
  to `main` (the Lean model's sentence coverage, its proofs and its run over
  the conformance cases in its fragment, on the PR's head),
  `compute-regression.yml` on ready PRs that touch measured inputs (paired
  WF-to-WF timing; a draft shows it skipped; each push to `main` there only
  builds and caches that commit's compiler for later PRs' merge-base
  baselines), and `io-bench.yml`, `compute-bench.yml` and
  `firn-check-time.yml` (Firn's check on the 14900K runner) on request, which
  are experiments and never a gate.
- Compiler releases: `compiler-release.yml`, dispatched when a downstream
  project needs a main commit, publishes that commit's compiler as release
  `wf-<12 hex digits>`, with `bundle_toolchain` as
  `wf-<12 hex digits>-bundled`, which carries clang and lld, or an unmerged
  commit's as experiment release `wf-exp-<12 hex digits>`, and
  `compiler-release-cleanup.yml` removes releases older than 30 days each
  week, keeping the newest lightweight release of main and every bundled
  release of main; each file's header gives the commands and contents
  ([downstream releases](design/compiler/downstream-releases.md)).
Every build, check, test and measurement runs in CI, never on a
contributor's workstation. A focused result the existing workflows do not
give, such as one test filter or a compiler probe over a set of cases, comes
from a temporary workflow on the work branch: a file named `tmp-<name>.yml`
that a push triggers, removed before the full check, which fails while one
exists; such a step runs its command under `perl .github/run-check.pl`, for
example `perl .github/run-check.pl <label> cargo test --manifest-path
compiler/Cargo.toml --profile gate --locked --offline --lib <filter>`.

CI runs heavy commands under `perl .github/run-check.pl <label> <command> ...`,
as the `make` targets do. It holds one host-wide lock,
leaves Cargo and the test harness at every available processor unless
`CARGO_BUILD_JOBS` or `RUST_TEST_THREADS` names fewer, prints wall, user and
system time every 30 seconds, and stops a command after 30 minutes unless
`WHITEFOOT_CHECK_TIMEOUT` gives another limit in seconds. It compares each
labeled stage with its budget in `.github/time-budgets.txt` without changing
the command's status. Linux budgets are per CPU model: the maximum of at least
its 20 most recent recorded runs plus 10%, rounded up to 5 s, with a 10 s floor.
Models with fewer runs have no column unless the owner adds one; unknown models
use the stage's largest Linux budget, with a note. macOS overruns are
report-only, and so are the stages named in `WHITEFOOT_BUDGET_REPORT_ONLY`
(comma-separated labels; the merge-base compiler build uses it, since its cache
misses after a runner image change); Windows rows keep their rule.
The final CI verdict fails on Linux and Windows overruns and on a missing row
for any host. On a self-hosted runner, inspect an existing lock owner's PID
instead of starting another heavy command, and after an uncatchable stop
inspect the recorded PID and command before removing a stale lock.

When a stage exceeds its budget, look at what the change adds to it and at the
job's slowest cases and host; fix a cause you find or bring the raise to the
owner, and re-run once when you cannot tell; an overrun the change plainly
cannot cause is reported as runner variance, and an enforced overrun is
cleared by a passing re-run. A raise is the owner's decision; recompute Linux
values by the rule above; Windows raises add 10 s to the stage and its group,
if any. Lower a budget when a change makes its stage much faster, and give a
new labeled stage a budget. The `gate` Cargo profile builds the Rust compiler
with optimization, debug assertions and overflow checks; it does not change
how WF source is compiled. `WHITEFOOT_TEST_TIMINGS=<scratch TSV>`
records the phases of the shared test helpers for a slow compiler test.

`WHITEFOOT_CHECK_WORK=<scratch TSV>` appends the checker's per-join,
snapshot and closure work counters, whose columns
`compiler/src/semantic/entailment/work.rs` documents; it is a temporary
instrument for the many-arm join cost, removed after that fix.

## Review

The completion review checks every applicable group of
[the review checklist](docs/review-checklist.md) for a change to code, tests,
the specification, gate wiring, the design tree or agent guidance, and
groups A, D and V when only research records or other prose changed. The released archives
`spec/kernel-spec-v*.md` are mechanically checked copies left out of the
reviewed diff.
