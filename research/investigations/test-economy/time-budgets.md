# Time budgets for daily and CI verification

The owner asked that every command run in daily work and every CI job finish
in a time its content justifies, and that a gate stop ordinary changes from
making them much slower. This record measures where the time goes, states
what is reasonable for each command, records the changes made to reach it and
the gate that holds it, and lists what is left.

Measurements below come from three sources, named where used:

- **Hosted CI.** GitHub-hosted `ubuntu-24.04` (four processors) and
  `macos-14` runners. The per-stage times are the `== END <label>` lines of
  gate runs 36407050293, 36481999094, 36509424729, 36520371629 and
  36545788212 on main and 36556965634 on the workflow-simplification
  branch, all 2026-09-28 to 09-29, and the six runs are of different
  revisions; the history uses all 106 successful `gate.yml` runs on main
  since 2026-09-03.
- **Local container.** The four-core Linux container this work ran in, at
  revision 1355610115's compiler sources, under `setpriv` without
  `dac_override`.
- **Serial profile.** Every unit and corpus case run on one test thread on
  that container, so each gap between completions is one case's wall time.
  That run kept `dac_override`, so three permission-denial corpus cases
  stopped early at their precondition check; their times are lower bounds.

## History: how the gate got slower

Median wall time of a successful main gate run per week: 3.3 min (week of
08-31), 8.1, 9.1, then 5.0 and 5.1 after PR #66 regrouped the jobs on 09-16.
The regrouping cut job-minutes per run from about 44–54 to about 15–17, and
since then the longest job has been `unit`, the only one still growing: on
ubuntu 3.2–3.7 min on 09-22/23 and 4.2–4.6 min on 09-29. Over that week the
compiler's `src` grew from 190,446 to 242,761 lines (+27%) and its `#[test]`
count by 20%. Wall-clock excursions to 12–27 min before the regrouping were
macOS runner queueing, not job time.

## Where the time goes

**Hosted gate, per job and stage** (seconds, range over the six runs; the
unit job still built the ordinary compiler first):

| Job | Stage | ubuntu | macOS |
|---|---|---|---|
| static | `repository-invariants` | 12.7–13.8 | 14.1–15.9 |
| static | `compiler/lint` (clippy, cold) | 20–59 | 18–44 |
| static | whole group | 38–77 | 40–72 |
| unit | `compiler/build` (ordinary library and CLI) | 43–74 | 64–91 |
| unit | `compiler/test-build-unit` | 68–102 | 109–157 |
| unit | `compiler/test-unit` | 51–85 | 58–73 |
| unit | whole group | 162–253 | 238–322 |
| corpus | `compiler/test-build-corpus` | 44–78 | 57–94 |
| corpus | `compiler/test-corpus` | 55–86 | 51–78 |
| corpus | whole group | 100–165 | 109–172 |
| runtime | whole group | 9–12 | 5–8 |

Every other static stage takes under 3 s on ubuntu and 6 s on macOS. Other
workflows: `io-hosts` completion-linux 61–75 s (its `linux-runtime` stage
26–31 s); completion-windows 204–251 s, of which the Rust build and program
cases step is 157–212 s; `compute-regression`, when it compares, 250 s, two
cold compiler builds taking 81 and 76 s of it.

**Compilation dominates and grows with the crate.** The compiler is one
dependency-free crate of 243,000 lines, built with optimization. The unit job
built it twice: once as the ordinary library for the CLI binary and its tests,
once in test mode for the library's own tests. Compilation was 148 of 211 s of
the unit group on ubuntu and 197 of 268 s on macOS in run 36554345394, on the
workflow-simplification branch.

**Unit cases are processor-bound.** Run serially, the 1,870 library cases and
21 CLI cases take 243 s in total; the slowest is 6 s, and the backend modules
that compile and run native programs account for most of it (`ranges` 38.6 s
over 26 cases, `deterministic_target` 24.8 s over 15). The same cases took
126 s on two threads and 63.5 s on four in the local container: close to
linear.

**Corpus cases are bounded by the conformance driver.** Serially the 93 cases
take 227 s, and two of them, the conformance adapter's cases
`a_shared_proof_receipt_cache_reaches_every_declared_source_verdict` (62 s)
and `the_corpus_reaches_its_declared_verdict_through_the_ordinary_compiler_path`
(78 s), each walk every conformance case on one thread. No thread count brings
the corpus stage below its longest walk: 78 s on this container. The hosted
runners are faster, and their corpus stage of 51–86 s is bounded the same way
by the same walk.

**Local runs used half the processors.** `run-check.pl` defaulted Cargo jobs
and the test pool to two, to leave capacity for interactive work and other
agents' commands. The host-wide lock it also takes already lets only one
verification command run at a time, so on the four-core container `make check`
left two processors idle: it took 678 s from a target that had to rebuild the
crate. The removed CI comment recorded the other side of that choice: a cold
`whitefootc` build took 72.6 s at two jobs and 43.1 s at four on a
four-processor host, with peak memory 1.27 and 1.37 GB.

## What is reasonable

Judged by what each command does, on a four-core host:

| Command | Content | Reasonable | Status |
|---|---|---|---|
| `make static` | reads the tree; small scripts, their self-tests and the design lint's tests | under 30 s | 21 s locally for the whole static group, clippy warm |
| `make -C compiler lint` | clippy over every target | under a minute cold, seconds warm | 18–59 s cold in CI, 16 s after an edit |
| `make -C compiler build` / `test-build` after one edit | incremental rebuild of the edited crate | tens of seconds | see [daily loop](#daily-loop) |
| hosted gate, end to end | two cold optimized builds per OS, 1,900 unit and 93 corpus cases, runtime fixtures | the slowest job under 5 min | 4.3–5.7 min before this change, 3.5–3.9 min after |
| `make check` locally | the same, in sequence, on one host | a few minutes warm, under 10 cold | 11.3 min with a rebuild before this change, 3.0 min warm after |
| `io-hosts`, `compute-regression` | platform builds and fixtures; a paired performance comparison | under 5 min each | within |

The gate's time is mostly construction the language project needs (an
optimized compiler with assertions, native programs) and processor-bound
cases, so the aim is to remove work that checks nothing, use the processors
the host has, and then hold each stage to its current cost so growth becomes
a decision instead of a drift.

## Changes

Each criterion below was set in the working session before its measurement
ran, but committed with or after the result, so this record cannot show the
order; read them as exploratory. The Windows trial had no criterion.

### Use every processor locally

`run-check.pl` no longer sets Cargo jobs or the test pool, so both take their
own default, every processor available to the process, which also honors a
container's processor quota; a caller, such as a person sharing the host with
interactive work, still names fewer. On this container Rust's available
parallelism, `nproc` and the online count are all four, and it has no
processor quota, so the measurements below, taken while the wrapper set four
from the online count, are the same setting. Criterion: adopt if unit and corpus
case execution on the four-core container drops by at least 30% against two
threads with every case passing. Result, from the same warm target:
`compiler/test-unit` 126 s → 63.5 s (−50%), `compiler/test-corpus`
122 s → 84 s (−31%, held at the conformance walk), 248 s → 148 s together
(−40%), every case passing. The whole warm `make check` took 182 s.
`gate.yml` had set both to the runner's processor count in a step of its
own; that step is removed, since Cargo and the harness default to the same
count there.

### Run the CLI's tests with the corpus

The CLI's 21 tests link the ordinary library. In the unit group they forced
`compiler/build` and a second build of the library, which the corpus group
builds anyway for its harness. They now run in the corpus group, and the unit
group builds only the library in test mode. Expected: the unit job loses its
`compiler/build` stage, 43–74 s on ubuntu and 64–91 s on macOS, and the
corpus job gains the CLI harness build and its 3–5 s of cases. Local `make
check` builds the same artifacts as before. Criterion: the hosted unit job
falls by at least 40 s on both runners while the corpus job grows by at most
20 s. Result, job wall times of runs 36559339945 and 36560712129 against the
median of the six earlier runs:

| Job | ubuntu before | ubuntu after | macOS before | macOS after |
|---|---|---|---|---|
| unit | 279 s | 209 s, 208 s | 288 s | 235 s, 168 s |
| corpus | 169 s | 128 s, 183 s | 166 s | 191 s, 157 s |

The unit job fell by 53–120 s, meeting the criterion on both runners. The
corpus half was missed on the first sample, 25 s more on macOS; a second
sample was then added, and the two average 13 s less on ubuntu and 8 s more
on macOS, a basis chosen after the first sample. Both corpus samples lie
inside the job's earlier range, and the longest gate job is now under four
minutes.

### Tried and withdrawn: the Windows build on every processor

This trial had no criterion set before it. The Windows program step builds
the compiler at two Cargo jobs. At Cargo's
default of every processor, run 36559339981's build took 175 s against a
step of 157–212 s before, of which the cases take about 18 s: no
measurable gain on one sample, so the step keeps its two jobs. The next run,
back at two jobs, took a 285-s Windows job against 251 s at every processor
and 204–251 s before, so this runner's variance exceeds any difference the
setting makes; reopen with several samples if the Windows job becomes the
longest.

### Kept: optimization level 3 for the `gate` profile

Compilation is the largest and growing cost, so a lower optimization level
for the `gate` profile was measured. Criterion: adopt a lower level only if,
summed over cold construction and case execution of the unit and corpus
groups, it saves at least 20% and neither group's cases run more than 25%
slower. Each level was built from an empty target on the four-core container
with CI's settings (incremental off, four jobs, four test threads); every
case passed at every level.

| `opt-level` | library tests, build | corpus and CLI, build | library cases | corpus and CLI cases | total |
|---|---|---|---|---|---|
| 3 (current) | 125.7 s | 71.0 s | 65.9 s | 83.3 s | 345.9 s |
| 2 | 115.1 s | 74.6 s | 70.3 s | 81.2 s | 341.2 s |
| 1 | 97.5 s | 61.2 s | 75.6 s | 91.0 s | 325.3 s |

Level 1 builds 19% faster but runs the cases 9–15% slower, 6% overall;
level 2 saves 1.4%. Neither meets the criterion, so the profile keeps level
3.

### Stop a program that never finishes

A budget reads a stage's time after the stage ends; a program that never
ends leaves nothing to read. On the spawn branch
(`claude/pensive-ramanujan-bfyunw`), a change to the waiting-context runtime
made four backend unit tests run programs that waited without end: one in
`cost_shape` and three in `deterministic_target`, all on the scripted
deterministic host, which never completed a request the change had started
waiting on. Those tests ran their programs with `Command::output`, which
waits without limit. The local gate was stopped by hand after almost half an
hour, short of the command's 30-minute deadline, which would have stopped
the whole command with the unit suite's summary and failure list unwritten
and the later groups unrun. Stable libtest printed that each test had been
running for over 60 seconds, and never stopped one.

Only the program suites ran each program as an owned process: its own
process group, both outputs drained, and a 60 s deadline
(`compiler/tests/support/process.rs`). The backend unit tests, the two
command-line tool tests that run a built program, and the conformance
adapter waited without limit. All of them now use that owned process and its
one `PROGRAM_DEADLINE`: the backend tests through `BoundedOutput` in
`compiler/src/backend/tests.rs`, the tool's tests through `run_command`, and
the adapter directly. The adapter reports a case whose program ran past the
deadline as `Verdict::Stopped`, which the corpus keeps apart from every
verdict, so the case fails by name. The tests' calls of the host C compiler,
`grep` and `awk` that do not go through `run_command` run no program a test
compiled and still wait without limit.

No choice among alternatives was measured here, so no criterion was set:
the limit is the program suites' existing one. It stops a program that has
run far past any case's time. Run alone, the slowest unit case took 6 s,
compilation included ([where the time goes](#where-the-time-goes)). At
revision 9f4370b4a on the local container, the unit group passed 1,870
tests in 57.0 s and none reached libtest's 60-second report; the corpus
group passed the 21 CLI tests, and 93 tests in 68.4 s, where only the
conformance walk, one test that runs every case, reached it. Four changes
made at 8973802b0, which differs only in the stop message's form, and then
reverted show what the tests observe:

- With `output_within` given an hour instead of its limit,
  `owned_children_capture_both_channels_and_enforce_their_deadline` waited
  30.08 s for a 30-second sleep and failed.
- With `PROGRAM_DEADLINE` at one millisecond, 1 of the 12 `cost_shape` and
  `deterministic_target` tests that run a program failed with `TimedOut`
  (3 in a run of the first version, 3e18aee3d). The others had exited by
  the first poll, 5 ms after the start, since the owned process reads a
  program's exit status before its deadline. The same change stopped the
  corpus group's own runtime compilations, which also use `run_command`,
  before any case ran.
- With `run_command` panicking instead of running its command, both CLI
  tests failed there.
- With every case's program replaced by a 30-second sleep under a 100 ms
  limit, the conformance walk failed in 116 s and reported each of the 467
  cases that run a program as `Stopped`, by name.

The deadline holds only where a test uses the owned process: nothing
refuses a new test that runs its program with `Command::output`, which
`docs/todo.md` records.

## The gate

**Mechanism.** `.github/time-budgets.txt` gives each labeled stage a
wall-time budget per hosted runner class. `run-check.pl` compares every
stage's wall time with its budget when the stage ends and never changes the
stage's own exit status, because some steps read it: the slowdown control of
`compute-regression` expects its command to fail, and the identical-image
control reads any failure as an inconclusive host. `gate.yml`,
`io-hosts.yml` and `compute-regression.yml` name a record file in
`WHITEFOOT_TIME_BUDGET_RECORD`; the wrapper appends to it every stage over its
budget or without one for its host, or an unreadable table, and a final
verdict step in each job, `run-check.pl --budget-verdict`, fails on a
nonempty record after
every stage has run. The record must be an absolute path the wrapper can
open before the stage starts, so a bad record stops the command at once
instead of changing a finished stage's status. Without a record, as in
local runs, the wrapper only
prints the comparison, because a local host is not the runner the budgets
were measured on. The Windows steps do not run under the wrapper and have no
budget; their step timeouts stay at 5 and 8 min, and the Windows and Linux
io-hosts jobs' timeouts go from 30 and 45 min to 10.

**Budget size.** 1.25 times the slowest run recorded, rounded up to 5 s,
and never under 10 s. For the gate the runs are seven of this branch whose
compiler source is identical, so their differences are the runners': gate
runs 36559339945, 36560712129, 36561430806, 36563489487, 36564932966,
36565698118 and 36622323588, at 0692767b3 through a6d011f86. For `io-hosts`
they are the main runs 36509424727, 36520371632 and 36545788231, this
branch's first three, and its runs 36563489571, 36564932651, 36565698067,
36622323301 and 36623591270; for `compute-regression`, run 36545746305 and
this branch's three.

Across the seven identical gate runs a stage's slowest run was 1.3–1.55
times its fastest for the builds and cases, and 1.75 (ubuntu) and 3.0
(macOS) times for clippy. The ubuntu runs clustered near their slowest with
a tail of faster ones: `check/unit` took 181–187 s in four runs and 123–162 s
in three, `check/corpus` 155–159 s in four and 103–125 s in three. Whether
the faster runs had faster machines is not known: the gate's host record did
not print the processor model until this change added it.

The first budgets were 1.5 times the slowest of nine runs of different
revisions, whose spread mixed growth with noise. Measured against identical
source, 1.5 let a typical ubuntu `check/unit` run (181 s) grow by 57%, and
the fastest of the seven (123 s) by more than double, before its stage
tripped. The owner asked whether that margin was too wide and ruled for 1.25
with a judgment on every overrun (the gate, below).

A budget at 1.25 times the slowest of seven samples can still trip on
noise. Leaving each of the seven gate runs out in turn and holding it to
1.25 times the slowest of the other six, no build or case stage tripped; the
clippy stage tripped twice (28.9 s against 25 s on ubuntu, 60.1 s against
50 s on macOS), and with it `check/static` on macOS once (92.0 s against
90 s), in two of the seven runs. An occasional clippy overrun that the
judgment reads as runner variance is expected. Slowest runs, in seconds:

| Stage | ubuntu | macOS |
|---|---|---|
| `check/static` | 49.3 | 92.0 |
| `repository-invariants` | 18.5 | 22.5 |
| `compiler/lint` | 28.9 | 60.1 |
| `check/unit` | 187.2 | 222.6 |
| `compiler/test-build-unit` | 105.6 | 147.7 |
| `compiler/test-unit` | 81.9 | 76.6 |
| `check/corpus` | 159.4 | 201.3 |
| `compiler/test-build-corpus` | 75.7 | 104.4 |
| `compiler/test-corpus` | 84.7 | 96.5 |
| `check/runtime` | 12.6 | 6.6 |
| `linux-runtime` | 37.5 | |
| `performance-candidate-compiler` | 88.9 | |
| `performance-baseline-compiler` | 80.9 | |
| `performance-null` | 34.7 | |
| `performance-slow-control` | 41.8 | |
| `performance-comparison` | 33.9 | |

Every other stage's slowest run was under 6.6 s, so its budget is the 10-s
floor.

**Judging an overrun.** A stage over its budget fails the job's verdict
step, and the author then reads the change against the stage: added cases,
fixtures or work on the stage's path, and the job's ranking of slowest cases
and its host. A cause found is fixed, or its raise goes to the owner; when
the reading is unclear, the job runs once more; and when the change plainly
cannot slow the stage, as a prose-only change cannot slow a build, the
overrun is reported as runner variance in the validation handed back and
does not hold the revision back. AGENTS.md "Checks" states the steps.

**Why this form.**

- Job `timeout-minutes` alone stops a stuck job; set at budget size it kills
  the job mid-stage and loses the results and the ranking of slowest cases,
  and a 15-minute limit on a 4-minute job let the unit job grow by more
  than a quarter in a week unnoticed.
- A per-case limit would need per-case times, which stable Rust's test
  harness does not report; the gate's existing ranking of completion gaps
  gives only a lower bound, which suits diagnosis rather than a verdict.
- A paired comparison with the merge base, as `compute-regression` does for
  WF programs, would build and run everything twice per push to detect what a
  fixed budget detects at no extra cost.
- Deterministic proxies such as the number of native images built are immune
  to runner noise but miss a slower compiler or a slower case, and need
  counters in every harness.

**Diagnosis when a budget trips.** The job summary already lists the ten
largest gaps between case completions, and `WHITEFOOT_TEST_TIMINGS` records
the shared helpers' phases per case.

## Daily loop

One edit, measured in the local container with the new defaults: an unused
function appended to `semantic/entailment/flow/prover.rs`, then reverted.

| Step | Seconds |
|---|---|
| `make -C compiler build` (incremental) | 6.2 |
| `make -C compiler test-build-unit` (incremental) | 12.4 |
| `make -C compiler lint` | 16.4 |
| the 160 `semantic::tests::entailment` cases, built | 5.6 |
| the static group, clippy warm | about 21 |

An appended unused function is a light edit; the
[compiler-architecture measurements](../compiler-architecture/DESIGN.md#f9-changing-the-compiler-costs-minutes-per-edit)
found 9–25 s per incremental rebuild for edits inside existing functions.
Either way an edit reaches a focused result in well under a minute, and the
full local gate, warm, in about three.

## Limits and deferred work

- Budgets are measured on hosted runners; local hosts are not gated.
- The gate measures cold builds only. A change that slows incremental
  rebuilds, the cost of each edit in daily work, passes it.
- Runner speed varies; a budget at 1.25 times the observed maximum trips
  on an unusually slow runner, most often in the clippy stage. The judgment
  above separates the runner from the change. Budgets per kind of runner
  machine would allow a tighter margin if the processor model shows that the
  faster ubuntu runs had faster machines; recorded in `docs/todo.md`.
- The corpus stage cannot drop below its longest case, one conformance walk
  (78 s on the local container); splitting that walk across threads is
  recorded in `docs/todo.md`.
- The Windows io-hosts steps have no budget, only step timeouts, and the
  Windows job is now the longest CI job at 251–285 s; recorded in
  `docs/todo.md`.
- The unit job's two builds of one 243,000-line crate remain the largest
  cost and grow with the compiler; the budget makes that growth visible and
  forces the decision when it arrives.
