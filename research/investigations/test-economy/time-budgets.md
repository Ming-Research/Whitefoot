# Time budgets for daily and CI verification

The owner asked that every command run in daily work and every CI job finish
in a time its content justifies, and that a gate stop ordinary changes from
making them much slower. This record measures where the time goes, states
what is reasonable for each command, records the changes made to reach it and
the gate that holds it, and lists what is left.

Measurements below come from three sources, named where used:

- **Hosted CI.** GitHub-hosted `ubuntu-24.04` (4 vCPU) and `macos-14`
  (3 cores) runners. The per-stage times are the `== END <label>` lines of
  gate runs 36407050293, 36481999094, 36509424729, 36520371629 and
  36545788212 on main and 36556965634 on the workflow-simplification
  branch, all 2026-09-28 to 09-29; the history uses all 106 successful
  `gate.yml` runs on main since 2026-09-03.
- **Local container.** The four-core Linux container this work ran in, at
  revision 1355610115's compiler sources, under `setpriv` without
  `dac_override`.
- **Serial profile.** Every unit and corpus case run on one test thread on
  that container, so each gap between completions is one case's wall time.

## History: how the gate got slower

Median wall time of a successful main gate run per week: 3.3 min (week of
09-03), 8.1, 9.1, then 5.0 and 5.1 after PR #66 regrouped the jobs on 09-16.
The regrouping cut job-minutes per run from about 44–54 to about 17, and
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
| unit | `compiler/build` (ordinary library and CLI) | 43–74 | 65–91 |
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
the unit group on ubuntu and 197 of 268 s on macOS in run 36554345394.

**Unit cases are processor-bound.** Run serially, the 1,870 library cases and
21 CLI cases take 244 s in total; the slowest is 6 s, and the backend modules
that compile and run native programs account for most of it (`ranges` 38.6 s
over 26 cases, `deterministic_target` 24.8 s over 15). The same cases took
126 s on two threads and 63.5 s on four in the local container: close to
linear.

**Corpus cases are bounded by the conformance driver.** Serially the 93 cases
take 227 s, and two of them, the conformance adapter's cases
`a_shared_proof_receipt_cache_reaches_every_declared_source_verdict` (62 s)
and `the_corpus_reaches_its_declared_verdict_through_the_ordinary_compiler_path`
(78 s), each walk every conformance case on one thread. No thread count brings
the corpus stage below that 78 s, which is why it stays at 55–86 s on four
hosted processors.

**Local runs used half the processors.** `run-check.pl` defaulted Cargo jobs
and the test pool to two, to leave capacity for other agents' commands. The
host-wide lock it also takes already lets only one verification command run at
a time, so on the four-core container `make check` left two processors idle:
it took 678 s from a target that had to rebuild the crate.

## What is reasonable

Judged by what each command does, on a four-core host:

| Command | Content | Reasonable | Status |
|---|---|---|---|
| `make static` | reads the tree; about 21 small scripts and the design lint's tests | under 30 s | 21 s locally |
| `make -C compiler lint` | clippy over every target | under a minute cold, seconds warm | 20–59 s cold in CI, 0.2 s warm |
| `make -C compiler build` / `test-build` after one edit | incremental rebuild of the edited crate | tens of seconds | see [daily loop](#daily-loop) |
| hosted gate, end to end | two cold optimized builds per OS, 1,900 unit and 93 corpus cases, runtime fixtures | the slowest job under 5 min | 4.5–5.7 min before this change |
| `make check` locally | the same, in sequence, on one host | a few minutes warm, under 10 cold | 11.3 min cold before this change |
| `io-hosts`, `compute-regression` | platform builds and fixtures; a paired performance comparison | under 5 min each | within |

The gate's time is mostly construction the language project needs (an
optimized compiler with assertions, native programs) and processor-bound
cases, so the aim is to remove work that checks nothing, use the processors
the host has, and then hold each stage to its current cost so growth becomes
a decision instead of a drift.

## Changes

Each change's criterion was recorded before its measurement.

### Use every processor locally

`run-check.pl` now sets Cargo jobs and the test pool to the host's online
processors unless the caller sets them. Criterion: adopt if unit and corpus
case execution on the four-core container drops by at least 30% against two
threads with every case passing. Result, from the same warm target:
`compiler/test-unit` 126 s → 63.5 s (−50%), `compiler/test-corpus`
122 s → 84 s (−31%, held at the conformance walk), 248 s → 148 s together
(−40%), every case passing. The whole warm `make check` took 182 s. CI
already set both to the runner's processors in `gate.yml`; that step is
removed because the wrapper now does the same.

### Run the CLI's tests with the corpus

The CLI's 21 tests link the ordinary library. In the unit group they forced
`compiler/build` and a second build of the library, which the corpus group
builds anyway for its harness. They now run in the corpus group, and the unit
group builds only the library in test mode. Expected: the unit job loses its
`compiler/build` stage, 43–74 s on ubuntu and 65–91 s on macOS, and the
corpus job gains the CLI harness build and its 5 s of cases. Local `make
check` builds the same artifacts as before. Criterion: the hosted unit job
falls by at least 40 s on both runners while the corpus job grows by at most
20 s. Result, run 36559339945 against the six earlier runs:

| Job | ubuntu before (median) | ubuntu after | macOS before (median) | macOS after |
|---|---|---|---|---|
| unit, whole group | 249 s | 187 s | job 288 s | job 235 s |
| corpus, whole group | 145 s | 103 s | job 166 s | job 191 s |

The unit job met the criterion on both runners. The corpus job fell on
ubuntu and rose 25 s on macOS, inside its earlier range of 125–196 s; one
sample cannot separate the CLI harness from runner variance; the next run adds a second sample.

### Tried and withdrawn: the Windows build on every processor

The Windows program step builds the compiler at two Cargo jobs. At Cargo's
default of every processor, run 36559339981's build took 175 s against a
step of 157–212 s before, of which the cases take about 18 s: no
measurable gain on one sample, so the step keeps its two jobs.

### Optimization level of the `gate` profile

Measurement in progress.

## The gate

**Mechanism.** `.github/time-budgets.txt` gives each labeled stage a
wall-time budget per hosted runner class. `run-check.pl` compares every
stage's wall time with its budget when the stage ends. With
`WHITEFOOT_TIME_BUDGETS=enforce`, which `gate.yml`, `io-hosts.yml` and
`compute-regression.yml` set, a stage over its budget, or a stage with no
budget for its host, is recorded, the remaining stages still run, and the
top-level command then fails and lists every overrun. Without it, as in local
runs, the wrapper only prints the comparison, because a local host is not the
runner the budgets were measured on. The Windows steps do not run under the
wrapper; their `timeout-minutes` bound them, tightened to 2 and 6 min from 5
and 8, and the Windows and Linux io-hosts jobs from 30 and 45 min to 10.

**Budget size.** About 1.5 times the slowest of the six measured runs, rounded
up, and never under 10 s. The factor covers the observed spread between runs
of the same revision (up to 2.9 times for clippy, typically under 1.3 for the
builds and cases); a stage that grows by half again, or a new stage without a
budget, fails. Growth below that accumulates until a later change crosses the
line, and that change's author then either removes the cost or asks the owner
to raise the budget.

**Why this form.**

- Job `timeout-minutes` alone stops a stuck job; set at budget size it kills
  the job mid-stage and loses the results and the ranking of slowest cases,
  and a 15-minute limit on a 4-minute job let the unit job grow for weeks.
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
| `make static` | about 21 |

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
- The corpus stage cannot drop below its longest case, the 78-s conformance
  walk; splitting that walk across threads is recorded in `docs/todo.md`.
- The unit job's two builds of one 243,000-line crate remain the largest
  cost and grow with the compiler; the budget makes that growth visible and
  forces the decision when it arrives.
