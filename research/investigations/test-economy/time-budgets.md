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

**Budget size.** 1.5 times the slowest run recorded, rounded up to 5 s, and
never under 10 s. For the gate the runs are the six earlier ones and three
of this branch (0692767b3, 4fb0b1555, e8334d394); `check/unit` uses only the
three, because it no longer builds the ordinary library. For `io-hosts` they
are the main runs 36509424727, 36520371632 and 36545788231 and this branch's
three; for `compute-regression`, run 36545746305 and this branch's three. The
spread between runs was up to 3 times for clippy and 1.4–1.7 times for the
builds and cases, so a budget sits above the slowest run seen rather than a
typical one; a stage that grows by half again over its slowest run, or a new
stage without a budget, fails. Slowest runs, in seconds:

| Stage | ubuntu | macOS |
|---|---|---|
| `check/static` | 76.7 | 92.0 |
| `repository-invariants` | 15.0 | 18.9 |
| `compiler/lint` | 59.4 | 60.1 |
| `check/unit` | 187.2 | 215.1 |
| `compiler/test-build-unit` | 105.6 | 157.2 |
| `compiler/test-unit` | 84.7 | 73.3 |
| `check/corpus` | 164.7 | 201.3 |
| `compiler/test-build-corpus` | 78.2 | 104.4 |
| `compiler/test-corpus` | 86.3 | 96.5 |
| `check/runtime` | 12.6 | 7.9 |
| `linux-runtime` | 31.8 | |
| `performance-candidate-compiler` | 88.9 | |
| `performance-baseline-compiler` | 80.9 | |
| `performance-null` | 34.7 | |
| `performance-slow-control` | 41.8 | |
| `performance-comparison` | 33.9 | |

Every other stage's slowest run was under 6.6 s, so its budget is the 10-s
floor.

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
- Runner speed varies; a budget at 1.5 times the observed maximum can still
  fail on an unusually slow runner. Such a failure names the stage, and a
  second run on the same revision separates the runner from the change.
- The corpus stage cannot drop below its longest case, one conformance walk
  (78 s on the local container); splitting that walk across threads is
  recorded in `docs/todo.md`.
- The Windows io-hosts steps have no budget, only step timeouts, and the
  Windows job is now the longest CI job at 251–285 s; recorded in
  `docs/todo.md`.
- The unit job's two builds of one 243,000-line crate remain the largest
  cost and grow with the compiler; the budget makes that growth visible and
  forces the decision when it arrives.
