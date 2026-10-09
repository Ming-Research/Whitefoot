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

Only the program suites, and the Windows-only native suite, ran each
program as an owned process: its own
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
the limit is the program suites' existing one. It stops only a program that
has run far past any recorded program's time. Run alone, the slowest unit
case took 6 s, compilation included
([where the time goes](#where-the-time-goes)), and every conformance case's
program exited within about 5 ms (the fourth change below). At
revision 9f4370b4a on the local container, the unit group passed 1,870
tests in 57.0 s and none reached libtest's 60-second report; the corpus
group passed the 21 CLI tests, and 93 tests in 68.4 s, where libtest's
report appeared only for
`the_corpus_reaches_its_declared_verdict_through_the_ordinary_compiler_path`,
one of the two tests that walk every conformance case. Five changes
made at 8973802b0, which differs only in the stop message's form, and then
reverted show what the tests observe:

- With `output_within` given an hour instead of its limit,
  `owned_children_capture_both_channels_and_enforce_their_deadline` waited
  30.08 s for a 30-second sleep and failed.
- With `PROGRAM_DEADLINE` at one millisecond, 1 of the 12 `cost_shape` and
  `deterministic_target` tests that run a program failed with `TimedOut`
  (3 in a run of the first version, 3e18aee3d). The others had exited by
  the first poll after the deadline, about 5 ms after the start, since the
  owned process reads a program's exit status before its deadline. The same change stopped the
  corpus group's own runtime compilations, which also use `run_command`,
  before any case ran.
- With `run_command` panicking instead of running its command, both CLI
  tests failed there.
- With the adapter's limit alone at one millisecond, the conformance walk
  still passed: each of the 467 cases that run a program had exited by the
  first poll after that limit. With every case's program then replaced by a
  30-second sleep under a 100 ms limit, the walk failed in 116 s and
  reported each of the 467 as `Stopped`, by name.

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

**`check/runtime` on macOS, raised to 20 s.** The concurrent map's test
(`compiler/src/backend/concurrent_map_test.c`, three builds) took the stage
to 15.3 s at `2adfc64c0`; once its locked-read and narrowed-hash builds ran
only the tests their change reaches, it took 13.2 s at `ec0bb2e99` and
12.4 s at `c5cdcfc9f`. 1.25 times 13.2 s, rounded up to 5 s, is 20 s, which
the owner approved. On ubuntu the stage took 21.1 s at `2adfc64c0`, which
changed no runtime code, and 20.3 s at `ec0bb2e99`, passed at `cbee9f341`
and `c5cdcfc9f`, and took 16.4 s locally, so its budget stays at 20 s and
the two overruns read as runner variance.

**`check/runtime` lowered to 10 s on ubuntu and 15 s on macOS.** The group
compiled its C one file at a time, about two thirds of the stage, and the
map test's builds held the ubuntu stage at 16.2 to 21.1 s through PR #202.
At `32bf48bd3` the group builds its executables with every processor before
it runs its cases one at a time, and the stage took 5.91 s on ubuntu and
9.33 s on macOS. 1.25 times those, rounded up to 5 s and not under 10 s,
gives 10 s and 15 s.

**`check/runtime` on ubuntu, raised to 15 s.** The stage took 11.8 s,
10.0 s and 10.7 s of its 10 s on ubuntu at the three heads of PR #208 before
`cea9188d4`, whose halved statements brought it back under. The shared-state
redesign's runtime (`keyed_table.c`, holds kept in a statement's frame,
exchanges of two tables' entries, counts under a whole hold and guard
watches, with their tests in `concurrent_map_test.c` and
`shared_object_test.c`) took the stage from 25.96 s and 24.06 s to 26.49 s
and 25.96 s on the 14900K, pinned to four CPUs, about 5% more. 1.25 times
the slowest ubuntu run, 11.8 s, rounded up to 5 s, is 15 s, the next step
above 10 s, which the owner approved on 2026-10-03 as a slight raise; macOS
stays at 15 s.

**`compiler/test-corpus` on ubuntu, raised to 145 s.** The stage took
114.2 s of its 110 s on ubuntu at `5d641c49d`, the shared-state redesign
merged with main, with all 22 and 128 tests passing, against 102.9 s for
22 and 115 tests on main at `a1de2b1ba`: the redesign adds 13 corpus tests,
among them firn's connection and server command cases. 1.25 times that
run, rounded up
to 5 s, is 145 s, which the owner approved on 2026-10-03 and confirmed on
2026-10-04 as a slight raise; macOS stays at 125 s.

**`check/corpus` on ubuntu, raised to 265 s.** With firn's second batch of
commands (PR #212) the corpus group took 207.5 s and 209.4 s of its 200 s
on ubuntu at `c8168dc18` and `825244c60`, every test passing (136 at the
second), against 193.3 s at the redesign's head `cb05ed6ae`; the batch adds
firn's string, key, list, set, hash and sorted-set network cases and their
replay suites, and `compiler/test-corpus` inside the group stayed within its
145 s (125.1 s). 1.25 times the slower run, rounded up to 5 s, is 265 s,
which the owner approved on 2026-10-04 (Q37, "37 agreed", translated from Chinese); macOS stays at 255 s.

**Every gate stage recomputed over main's runs on mixed processors.** On
2026-10-05, 42 gate runs failed with 64 failed jobs, and 59 of those jobs
failed only their verdict step: each stage's own checks passed, and the
stage took longer than its budget. The other five were a clippy refusal of
dead code, the English-artifact check, each on both hosts, and a two-map
audit of the concurrent map that main has since fixed. The host record's
processor model shows why the overruns came and went. The hosted ubuntu
runners drew six processor models, and on one model a stage took about
1.6 times as long as on another (medians over the 70 ubuntu unit jobs and 71
corpus jobs of the last 100 gate runs, all branches; seconds):

| Processor | unit jobs | `compiler/test-build-unit` | `compiler/test-unit` | `compiler/test-corpus` |
|---|---:|---:|---:|---:|
| AMD EPYC 7763 | 45 | 121 | 105 | 128 |
| AMD EPYC 9V45 | 11 | 85 | 65 | 80 |
| AMD EPYC 9V74 | 6 | 124 | 98 | 103 |
| Intel Xeon Platinum 8370C | 5 | 121 | 100 | 129 |
| Intel Xeon Platinum 8573C | 2 | 107 | 82 | 106 |
| Intel Xeon 6973P-C | 1 | 97 | 66 | 104 |

The EPYC 7763 ran most jobs, and on it `compiler/test-unit` had reached its
105 s budget: main took 107.3 s at `a23a3f1f4`, 111.3 s at `bfe5d652b` and
106.9 s at `97b477e14`, so every branch failed that stage whenever it drew
that processor, whatever it changed. The September budgets came from seven
runs whose processor was not recorded; at that time `compiler/test-unit` took
58–81 s over 1,880 cases, and on 2026-10-05 it took 65 s on the 9V45 and
105 s on the 7763 over 1,964. Part of the difference is growth: one
witness test added in PR #231 took 11.8 s alone, which PR #239 splits. The
macOS runners, all `Apple M1 (Virtual)` with three processors, varied as
widely within one model: `compiler/test-unit` took 56–137 s on main's
revisions. PR #227's faster checker did not shorten `compiler/test-unit`
(107.4 s and 103.8 s on the 7763 after it), whose slowest cases compile,
link and run native programs; it shortened firn's cold front end from 24.2 s
to 3.7 s on the 14900K.

Each gate stage's budget is again 1.25 times its slowest run, rounded up to
5 s, now over the 22 gate runs from 2026-10-05 08:27 to 2026-10-06 00:07
UTC whose head is a commit of main, so that no branch's own cost enters the
sample: runs 37283770911, 37284883461, 37291288915, 37298227841,
37300734558, 37302833541, 37303357044, 37376326396, 37379405859,
37379598126, 37379967022, 37380266404, 37381952614, 37381969387,
37381975146, 37383619959, 37383653548, 37389666607, 37390062583,
37390583584, 37391117333 and 37392246769. Every slowest ubuntu run was on an
EPYC 7763. Seconds:

| Stage | ubuntu slowest | ubuntu budget | macOS slowest | macOS budget |
|---|---:|---|---:|---|
| `check/static` | 66.8 | 65 → 85 | 99.8 | 120 → 125 |
| `compiler/lint` | 43.4 | 40 → 55 | 58.5 | 80 → 75 |
| `design-lint` | 1.7 | 10 | 9.9 | 10 → 15 |
| `check/unit` | 249.9 | 235 → 315 | 315.2 | 280 → 395 |
| `compiler/test-build-unit` | 138.3 | 135 → 175 | 212.2 | 185 → 270 |
| `compiler/test-unit` | 111.3 | 105 → 140 | 137.4 | 100 → 175 |
| `check/corpus` | 240.8 | 265 → 305 | 261.7 | 255 → 330 |
| `compiler/test-build-corpus` | 107.8 | 95 → 135 | 128.4 | 135 → 165 |
| `compiler/test-corpus` | 139.7 | 145 → 175 | 137.3 | 125 → 175 |
| `check/runtime` | 12.9 | 15 → 20 | 14.4 | 15 → 20 |

The owner approved this table on 2026-10-05. The other gate stages keep
their budgets, which their slowest runs stay under. These values replace the separate raises proposed in PRs #230
(`compiler/test-unit` 115 s and `check/unit` 250 s on ubuntu) and #237 (the
macOS column over 29 to 30 runs of 2026-10-04 and 2026-10-05), which drop
their budget edits. A budget that covers the slowest processor lets a change
grow a stage on the fastest by about twice before it trips, so a branch
still reads its stage times against main's on the same processor; a budget
column per processor model remains in `docs/todo.md`.

**`performance-candidate-compiler` raised to 130 s.** `compute-regression`
builds the candidate compiler first and the merge-base compiler second, each
cold at two Cargo jobs in its own target directory, and the first build is
the slower one whichever compiler it builds. Across the 30 runs before 2026-10-06
01:00 UTC that built both, the candidate took longer than the baseline every time, by a median of 12%. In
PR #239's run 37394287196, whose two compilers build the same `whitefootc`
source because the PR changes only a test file, the candidate took 120.9 s
and the baseline 90.7 s. Two dispatched runs on an EPYC 7763 of main's
source separate the build order from the compiler built: in run 37395372973,
in the workflow's order, the first build took 101.6 s and the second 93.2 s;
in run 37395370468, with the two build steps swapped, the first took 107.2 s
and the second 93.2 s. The owner set the budget directly to 130 s on
2026-10-05, about 10 s over that slowest run, in the same ruling that fixed
every later raise at 10 s; `performance-baseline-compiler` keeps 105 s, over its slowest run of 97.3 s. Why the first cold build is
slower was not measured; an untimed warm-up before both builds would remove
the difference instead of covering it.

**The candidate's images, back to 10 s.** `performance-candidate-images`
builds the five formal kernels with the candidate compiler right after the
two cold compiler builds. On PRs #278 and #289 it overran its 10-s floor on
first attempts, at 10.6 to 21.3 s, while every re-run passed at 2.2 to 5.5 s,
the paired comparison passed every time, and the identical baseline stage
right after it took about 2 s; the owner raised the budget to 20 s on
2026-10-09. A temporary workflow (run 37890815236, twelve hosted jobs after
the same two compiler builds) then timed the image build twice per job after
three preparations: with none the first build took 4.3 to 28.7 s, after a
`sync` (itself 0.0 to 0.5 s) 2.8 to 16.6 s, and after a one-file clang
warm-up (itself 2.7 to 8.8 s) 4.9 to 12.8 s, while the second build took 2.0
to 3.4 s in every job. The first build's extra cost is therefore neither
writeback nor clang's start alone; the likeliest reading is the native
toolchain's first use paging in from a cold disk, an inference the
experiment does not measure, and it did not find which construction phase
owns the delay or whether a smaller preparation would suffice. A whole build
was sufficient in every sampled job, so `compute-regression.yml` builds the
images once, untimed, before the timed stages; the stage then took 2.7 s in
run 37892608427, and its budget returns to the 10-s floor (PR #300). The
first-use cost now falls in the untimed warm-up and no budget covers it.

**The placement control's two stages, 10 s and 45 s.** The code-placement
change (PR #252) added `performance-shifted-images`, which links the
candidate's images again behind 32 bytes of padding, and
`performance-placement-control`, which compares them with the candidate's.
The shifted images took 1.8 s at `d4f67df6`, and the control 26.1 s at
`1bfe9449` and 34.0 s at `d4f67df6`. The shifted images take the
10-s floor; 1.25 times 34.0 s, rounded up to 5 s, is 45 s, the budget of
`performance-null`, the same campaign over the same images.

**Judging an overrun.** A stage over its budget fails the job's verdict
step, and the author then reads the change against the stage: added cases,
fixtures or work on the stage's path, and the job's ranking of slowest cases
and its host. A cause found is fixed, or its raise goes to the owner; an
approved raise adds 10 s to the stage and to the group stage that contains
it, if any, so an overrun of more than 10 s still needs its cause found. When
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
