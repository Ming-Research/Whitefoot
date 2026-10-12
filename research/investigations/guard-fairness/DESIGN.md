# Guard fairness: woken watchers before newcomers

## Question

A statement `atomic p = &s when guard { ... }` whose guard reads false
registers a watch on the object, releases it and parks; a statement that
writes the object wakes every watch on it as it ends
(`compiler/src/backend/completion/bridge.c`, the guard-watch section;
design node `compiler/waiting-contexts/state-locks`). Nothing orders a woken
watcher against a statement that begins after the write. A context that
releases the object and at once begins another guarded statement on it is
still running on its driver, while the watchers it woke are queued, so it
takes the object first; drivers do not preempt, so it can do so for its
whole run.

Firn's reproducer (Firn-wf branch `exp/engine-probe`,
`research/experiments/guard-fairness/`, run 38052700124 on the native
14900K, compiler wf-78223721f77d, two drivers on CPUs 2 and 4, holds of about
6.6 µs) measured, at 50 contexts of 1,000 acquisitions each, a p99 wait of
13–19 µs but a longest wait of 505–515 ms against a 512–521 ms run: a waiter
was overtaken until the context ahead of it finished. Firn's script engine,
with network I/O between acquisitions, saw 6.5 percent of checkouts wait
1–10 ms.

The specification permits this: [WAIT-2] promises that a guarded statement
takes effect only when its guard is true at every point from some point on,
and here the guard is true only between one statement's release and the
next acquisition. So this is a quality-of-implementation question, not a
conformance defect; strengthening [WAIT-2] is a separate decision (status
board card "guard fairness", option B).

## Proposal

After a statement writes an object and wakes watches on it, each context it
woke has one turn: until every woken context has made its next attempt on
the object, a statement that was not woken and holds no other object does
not take it, but waits as for a held object (spin, then park on the
object's queue). A woken context's attempt is its first acquisition of the
object after it resumes; the object counts outstanding turns and admits
newcomers again when the count reaches zero, waking a newcomer parked
meanwhile. Turns are granted per write, so a woken context whose guard is
false again registers its watch and waits for the next write with no turn
held.

- A newcomer that already holds another object, or a table's entries, is
  exempt: a woken context may need that object first in the global lock
  order, and the newcomer waiting for its turn would then deadlock.
- A woken context that never returns to the object (its statement's guard
  also read another unit, written first, and its retry took another path)
  still ends its turn: the turn is counted only for watches woken through
  this object's list, and a context's retry of the same statement always
  reacquires the object its guard read.
- While turns remain, an unlock wakes a parked context that holds a turn
  (or is exempt) before the head of the object's queue: a woken watcher that
  found the object held parks there, and a refused newcomer woken first
  would park again with the object free, leaving no unlock to wake the turn
  holder (the first candidate deadlocked `tests/programs/shared_objects.wf`
  on one driver this way, gate run 38058067347).
- The waiting cancellation-state update [PRE-2] is exempt: it is no guarded
  statement, and its firing must not wait for the guards it woke.
- Waking all watchers in registration order and the 64-pass yield rule stay
  as they are.

What this changes for the reproducer: the releaser, beginning its next
statement, finds turns outstanding and parks behind the woken watchers, so
it yields its driver; one woken watcher takes the object and writes `out`,
which wakes no one else (they are not watching yet: their attempt found the
guard false and re-registered). The other woken contexts' attempts end their
turns.

What it does not change: woken watchers still race among themselves, so a
waiter can lose to other waiters; the guarantee is that no context that was
not waiting overtakes a woken one between a write and its attempt.

## Comparison, fixed before it measures

On the native 14900K, the reproducer (copied into
`research/experiments/guard-fairness/` with its source named), two drivers
pinned to two physical performance cores, N = 2, 8 and 50 contexts, 1,000
acquisitions each, two passes interleaved, with the base compiler (the main
revision this branch starts from) and the candidate.

- Fairness: at N = 50 the candidate's longest wait is at most
  4 × N × mean hold + 1 ms in both passes, where the base's was about the
  whole run.
- Cost: the candidate's run time at N = 2, 8 and 50 is at most 1.10 times
  the base's (a waiter's turn makes the releaser park, which costs a
  context switch per acquisition under contention).
- Uncontended cost: a program with one context and no waiter runs within
  1 percent of the base (the check is one load on the acquisition path).

A failing fairness criterion rejects this mechanism in favour of an ordered
hand-off (the oldest watcher first, a turn passed on when its guard is
false); a failing cost criterion goes to the board with both numbers.

## Result: turns are fairer and much slower; both criteria fail

[Run 38062025238](https://github.com/Ming-Research/Whitefoot/actions/runs/38062025238)
(temporary workflow; native i9-14900K, 15:10 UTC; two drivers on CPUs 2 and
4, each its own physical performance core; compilers built from base
fe5589ec5 and the candidate at ebcdb5a1d; K = 1,000 acquisitions per
context, two passes, order reversed on the second). Times in ns; hold is the
mean hold.

| N | Arm | p50 | p99 | longest wait | mean hold | run time |
|---:|---|---:|---:|---:|---:|---:|
| 2 | base | 20 / 20 | 24 / 22 | 7,368,149 / 7,302,648 | 6,649 / 6,617 | 14,108,220 / 14,025,145 |
| 2 | candidate | 9,837 / 9,857 | 11,070 / 10,660 | 20,909 / 18,941 | 6,626 / 6,658 | 17,174,776 / 17,214,194 |
| 8 | base | 20 / 20 | 27,065 / 36,061 | 57,329,258 / 57,149,924 | 6,627 / 6,625 | 64,059,613 / 63,887,741 |
| 8 | candidate | 133,465 / 132,373 | 315,254 / 322,130 | 529,431 / 482,948 | 6,658 / 6,626 | 156,708,018 / 156,995,699 |
| 50 | base | 9,298 / 9,500 | 13,877 / 12,306 | 509,763,217 / 512,568,002 | 6,608 / 6,616 | 516,570,415 / 519,382,432 |
| 50 | candidate | 938,469 / 1,155,881 | 4,254,573 / 5,845,534 | 8,935,088 / 17,058,937 | 6,616 / 6,622 | 1,309,439,194 / 1,501,823,278 |

- Fairness criterion (longest wait at N = 50 at most 4 × N × hold + 1 ms,
  about 2.3 ms): **fails**. The longest wait falls from about 510 ms to 8.9
  and 17.1 ms, 30 to 57 times shorter, but stays four to seven times over
  the bound.
- Cost criterion (run time at most 1.10 times base): **fails**: 1.22 at
  N = 2, 2.45 at N = 8, 2.54 and 2.89 at N = 50. The base ran 50,000
  acquisitions in about 516 ms, close to 50,000 × 6.6 µs of holds, because
  one context kept the object on its driver without switching; with turns
  each acquisition hands the object to a context that must be scheduled,
  about 26 to 30 µs per acquisition at N = 50.

This probe takes the object again at once after releasing it, with no work
between, so it is the case where fairness costs most; firn's engine has
network I/O between checkouts. By the pre-registered reading the fairness
failure points to an ordered hand-off, but an ordered hand-off switches at
least as often as turns do, so the cost criterion would fail at least as
badly on this probe. The choice between fairness and throughput under this
contention is the owner's (status board card "guard fairness").

## Result on firn's engine: turns are far worse

Measured by firn's session ([Firn-wf run 38065358945](https://github.com/Ming-Research/Firn-wf/actions/runs/38065358945),
native 14900K, 2026-10-10 15:52–15:57 UTC): the limiter-script workload
(EVALSHA token bucket, every request checking out firn's one shared script
engine under a guard), pipeline depth 1, server on CPUs 2 and 4 (two
drivers), client on CPUs 6–15 (ten threads), two passes of five seconds with
the order reversed on the second, AOF off. Control: firn main 15293c5 pinned
to release wf-fe5589ec5f45 (this branch's base); turns: the same tree pinned
to experiment release wf-exp-5f6dca744b8f (this branch at 5f6dca744); a
second control build as the noise control. Both pinned trees pass firn's
gate. Requests per second and p50 / p99 in ms, pass 1 / pass 2 (the client
reports no maximum). The Redis line is the host's Ubuntu 26.04 package,
which the run's tool check reports as `Redis server v=8.0.5`, not firn's
reference 7.0.15:

| Line | Connections | Rate | p50 | p99 |
|---|---:|---|---|---|
| Redis 8.0.5 | 8 | 228,770 / 228,835 | 0.029 / 0.029 | 0.059 / 0.058 |
| Redis 8.0.5 | 50 | 231,140 / 231,900 | 0.202 / 0.201 | 0.410 / 0.408 |
| control | 8 | 159,068 / 159,481 | 0.018 / 0.018 | 0.793 / 0.742 |
| control | 50 | 211,012 / 209,346 | 0.087 / 0.071 | 1.500 / 1.628 |
| control twin | 8 | 159,278 / 159,617 | 0.018 / 0.018 | 0.724 / 0.699 |
| control twin | 50 | 208,578 / 205,304 | 0.062 / 0.086 | 1.679 / 1.587 |
| turns | 8 | 21,281 / 21,292 | 0.346 / 0.346 | 0.873 / 0.874 |
| turns | 50 | 4,411 / 4,394 | 7.959 / 7.879 | 51.211 / 52.503 |

The control and its twin agree within about 2 percent. Turns cut throughput
7.5 times at 8 connections and 47 times at 50, and raise p99 at 50
connections from about 1.6 ms to about 52 ms: when every request takes the
one engine, turns make each acquisition a context switch, so the queue of
woken watchers is served one switch at a time while new requests wait
behind it. Turns as built are rejected on the workload that motivated
them; any further candidate is measured the same way before it is
proposed.

## Age-bounded turns, fixed before it measures

The owner selected option C on card gran-guard-fairness: normally a releaser
may take the object again immediately, and a write wakes all watchers to
race. Only a watcher whose statement has waited longer than T at that write
receives a turn. Age starts at the first false-guard watch registration of
this statement execution and survives wakes and false-guard retries; it is
not time since the latest wake. T is the compile-time constant
`WF_GUARD_TURN_AGE_NS`, provisionally 1,000,000 ns (1 ms): a hypothesis that
short contention can retain throughput while long waits force a retry,
not a measured optimum. The existing turn consumption, parked-turn-holder
priority, bounded acquisition handoff, earlier-object/table-entry exemptions
and cancellation-state-update exemption remain. Only registration and a
write waking watchers read the existing monotonic clock; uncontended
acquisition adds at most the existing relaxed turn-count load.

The candidate is compared with the base on firn's probe on the native
14900K, two drivers on CPUs 2 and 4, N = 2, 8 and 50, K = 1,000, two passes
with reversed arm order. In each pass, longest wait at N = 50 must be at
most T + 4 x N x mean hold + 1 ms, and run time at every N must be at most
1.10 x base. Firn's session measures its engine with the same settings as
[run 38065358945](https://github.com/Ming-Research/Firn-wf/actions/runs/38065358945),
including the control twin and reversed second pass: throughput must be at
least 0.95 x control at both 8 and 50 connections, and p99 at 50 connections
must be at most 1.5 x control, in each pass. Either the probe criteria or the
engine criteria failing rejects this candidate. These criteria precede its
implementation and measurement; no result for age-bounded turns is claimed.

## Age-bounded turns' results

[Run 38091930357](https://github.com/Ming-Research/Whitefoot/actions/runs/38091930357),
job `run`, 2026-10-10 23:04:33–23:04:55 UTC, native i9-14900K;
`WF_DRIVERS=2`, pinned by `taskset` to CPUs 2 and 4. Candidate implementation:
[f032209e4](https://github.com/Ming-Research/Whitefoot/commit/f032209e426b7181a6d0211a345b7650c3bc1220);
workflow revision:
[187bce72d](https://github.com/Ming-Research/Whitefoot/commit/187bce72dab4509bdf8de8a6767b618ff0ac04b0).
The workflow builds the base at
[fe5589ec5](https://github.com/Ming-Research/Whitefoot/commit/fe5589ec5f4584c6fd235faa17e2e3173a7d651e).
Numbers below are recomputed from the supplied extraction of that run's log.

The output order in [probe.wf](../../experiments/guard-fairness/probe.wf)
is N, K, count, p50, p99, maximum, mean hold, runtime, checksum.
All five time fields are **nanoseconds**. Wait is statement start to
successful checkout; hold is checkout to the clock sample in the returning
statement, including work and any delay acquiring that statement's object.
Runtime spans spawning through joining all contexts, excluding aggregation
and sorting. K = 1,000 and count = N × K: 2,000, 8,000 and 50,000.
Percentiles use ranks floor((count + 1)/2) and ceil(0.99 × count).
“Ideal occupied time” is count × reported mean hold: a serialized-work
reference without gaps, not a separately measured arm or a cost criterion;
integer mean rounding can omit less than count ns of summed hold.

Pass 1 (N order 2, 8, 50):

| N | Arm | p50 wait (ns) | p99 wait (ns) | Longest wait (ns) | Mean hold (ns) | Runtime (ns) | Ideal occupied time (ns) | Runtime/base (×) |
|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 2 | base | 20 | 23 | 7,313,079 | 6,645 | 14,090,068 | 13,290,000 | 1.000000 |
| 2 | candidate | 20 | 26 | 1,010,188 | 6,651 | 14,711,892 | 13,302,000 | 1.044132 |
| 8 | base | 20 | 24 | 56,616,366 | 6,645 | 63,381,741 | 53,160,000 | 1.000000 |
| 8 | candidate | 20 | 1,087,582 | 1,248,861 | 6,624 | 79,215,191 | 52,992,000 | 1.249811 |
| 50 | base | 9,309 | 13,712 | 501,930,556 | 6,609 | 508,735,759 | 330,450,000 | 1.000000 |
| 50 | candidate | 4,302,411 | 17,734,108 | 40,189,036 | 6,683 | 5,261,528,440 | 334,150,000 | 10.342360 |

Pass 2 (N order 50, 8, 2):

| N | Arm | p50 wait (ns) | p99 wait (ns) | Longest wait (ns) | Mean hold (ns) | Runtime (ns) | Ideal occupied time (ns) | Runtime/base (×) |
|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 50 | base | 9,261 | 11,346 | 495,085,270 | 6,613 | 501,882,503 | 330,650,000 | 1.000000 |
| 50 | candidate | 4,202,378 | 17,920,919 | 47,835,201 | 6,678 | 5,262,993,370 | 333,900,000 | 10.486505 |
| 8 | base | 20 | 24 | 55,969,371 | 6,624 | 62,704,067 | 52,992,000 | 1.000000 |
| 8 | candidate | 20 | 1,086,279 | 1,215,744 | 6,629 | 80,170,300 | 53,032,000 | 1.278550 |
| 2 | base | 20 | 24 | 7,306,578 | 6,639 | 14,079,508 | 13,278,000 | 1.000000 |
| 2 | candidate | 20 | 30 | 1,010,617 | 6,648 | 14,757,741 | 13,296,000 | 1.048172 |

- **Fairness fails both passes.** Using the candidate's mean hold and
  T = 1,000,000 ns, pass 1's bound is
  1,000,000 + 4 × 50 × 6,683 + 1,000,000 = **3,336,600 ns**;
  40,189,036 ns is **12.044907×** the bound. Pass 2's bound is
  **3,335,600 ns**; 47,835,201 ns is **14.340809×** the bound.
  Longest waits improve **12.489241× / 10.349811×** over base.
- **Cost fails both passes.** N = 2 passes at **1.044132× / 1.048172×**;
  N = 8 fails at **1.249811× / 1.278550×**;
  N = 50 fails at **10.342360× / 10.486505×**.
  The limit is **1.10×** base at every N; satisfying N = 2 cannot offset
  either failure.
- Every arm reports the expected count, with equal base/candidate checksums
  in both passes: N = 2, `17338884850046946832`; N = 8,
  `10724673450211648456`; N = 50, `12413229600044691665`.
  These support comparable completed work, not fairness.
  The preliminary N = 2, K = 100 pilots each report 200 acquisitions and
  checksum `12845862288366202069`; runtimes
  1,474,983 / 1,466,001 ns give 0.993910×, outside the registered full passes.

**Protocol limitation.** [run.sh](../../experiments/guard-fairness/run.sh)
and the raw labels show base then candidate in both passes: only N order
reverses. Thus the registered reversed arm order was not implemented, and
there is no control twin in this probe. The observed failures reject the
candidate under the fixed thresholds; they do not isolate order effects or
supply a measured causal breakdown.

**What the code establishes.**
In [bridge.c](../../../compiler/src/backend/completion/bridge.c),
`wf_watch_wake_locked` still unlinks and wakes every watch; it samples
one clock per nonempty object wake and grants a turn to every watcher aged
past T. `guard_started` survives false retries and resets on successful
statement release, so an unsuccessful aged watcher can receive turns on
successive writes. Age is checked at writes, not by a deadline timer.
`wf_shared_took_locked` consumes a turn **before** guard evaluation:
a retry can see `out == 1`, fail, and watch again. Both the checkout's
write of `out = 1` and the return's write of `out = 0` wake watches.
Consequently turns restrict newcomers and enable earlier attempts, explaining
a route to the observed shorter tail, but neither order successful
checkouts nor establish the proposed wall-time bound.

`wf_shared_wake_locked` scans the acquisition queue for a parked turn
holder or an exempt holder while turns remain; absent one, it selects the
head. Its two-vain-wakes handoff bounds acquisition overtaking, not
successful guard execution. Ready turn holders enter ordinary driver
queues, and holders race rather than being ordered by age. In
`wf_shared_acquire_as`, the optimistic hint checks holders only: with a
free object but outstanding turns, a refused context can repeatedly take
and drop the object lock through the 256-spin loop before parking.
The bound's mean-hold term does not account for gaps before checkout,
including false retries and delays scheduling or acquiring a turn.

**Attribution hypotheses.** At N = 8 the unchanged 20 ns median and
roughly 1 ms candidate p99 are consistent with mostly immediate reuse
interrupted by aged-waiter retries. At N = 50, p50 exceeds 4 ms and runtime
per successful checkout rises from 10,175 / 10,038 ns to
105,231 / 105,260 ns, while mean hold rises only 1.12% / 0.98%.
Candidate occupied time is about 0.334 s of each 5.26 s run.
Thus increased measured hold cannot account for the loss. A plausible
feedback is that more contexts age past T, generating batches of turns;
losers remain aged, and their repeated wake/retry cycles keep newcomers
parking. Wake-all already exists in base: amplification of its frequency
and failed retries, lock traffic on turn denial, queue scans, and latency
scheduling turn holders are competing cost explanations. Their relative
contributions, and whether a few unusually late holders dominate the
maximum, were **not measured**. Extra clock calls under the watch lock
are another hypothesis; the data do not establish that they dominate.

A separating CI measurement can count existing `wf__shared_seen` events
(WOKEN, HANDED, RESUMED) and `wf__watch_seen` events (WRITTEN, EARLY),
adding per-driver counts of watches visited, turns granted, failed guarded
retries, turn-denied lock attempts and acquisition-queue entries scanned.
Normalize by successful checkout: growth in visits/retries identifies herd
amplification, and denied-lock attempts identify the optimistic-loop cost.
Sample grant-to-acquisition and handoff-to-resume delay, with turn count and
holder state, to distinguish queued-holder latency from retry CPU work.
Count and sample time in `wf__guard_clock_ns`: clock cost sufficient to
explain the excess supports that hypothesis; negligible accumulated cost
rejects it. Compare count-only and sampled runs with the uninstrumented
control to quantify instrumentation effects. These are proposed diagnostics,
not results of this run.

The pre-registered rule **rejects this candidate** on the probe in both
passes. **Firn's engine run is not needed** to decide that rejection.

## Step B: oldest-only handoff, registered before measurement

The owner selected option B on 2026-10-11: first count the costs of the
baseline and rejected age-bounded turns alongside the next candidate, then
measure oldest-only handoff. This section registers the comparison before
any run of that candidate. No new performance result is claimed. This is a
runtime scheduling experiment under [WAIT-2], with no specification change.

**Question and falsifier.** Does reserving one aged guard's next attempt,
passing that reservation after a false retry, remove the repeated wake-all
and bulk-turn costs sufficiently to bound the probe's tail without losing
more than ten percent runtime? A longest wait above 3,300,000 ns at N = 50,
or runtime above 1.10 times the baseline at any measured N, rejects this
candidate when the twin control is valid. Improved median or fewer retries
cannot compensate for either failure.

**Arms and construction.** Use this revision's compiler, the same unchanged
`probe.wf`, optimization settings and runtime source for all arms. Build-time
runtime modes select no guard fairness (`baseline`), rejected age-bounded
turns (`age`), and oldest-only handoff (`oldest`). `twin` is a byte-for-byte
copy of the baseline executable, checked with `cmp` and recorded with its
SHA-256 digest. This is a same-source mechanism comparison; the no-fairness
arm is not claimed to reproduce every byte of the historical compiler.
The research Makefile emits the probe LLVM once, then includes the maintained
`compiler/runtime.mk` native runtime rules with C11,
`-O2 -falign-functions=64`, full LTO and LLD, matching the driver's prior
`--full-lto` optimization settings without a new compiler option.
The age reference retains its bulk-turn admission and optimistic spin loop
so the attribution compares the rejected mechanism rather than a repaired
version of it. Normal builds select oldest-only with counters compiled out.

**Placement, scale and order.** All probe execution runs through the
temporary CI workflow on the native i9-14900K (`self-hosted`, `14900k`), with
`WF_DRIVERS=2` and `taskset -c 2,4`, matching the previous two-core placement.
Record the revision, compiler/tool versions, executable hashes and CPU
sibling topology. Begin with two N = 2, K = 100 pilots for each arm and
retain their runtime spread before the full comparison. A pilot that cannot
finish within ten seconds stops expansion; full runs have a 120-second
watchdog, whose expiry invalidates the evidence rather than changing a
performance criterion. The registered full experiment is
two passes of K = 1,000 successful checkouts per context:

| Pass | N order | Arm order within each N |
|---|---|---|
| 1 | 2, 8, 50 | oldest, baseline, age, twin |
| 2 | 50, 8, 2 | twin, age, baseline, oldest |

Arms are interleaved within N, and both orders reverse; the experiment does
not run all baselines first. Keep every raw row. Each row must contain
N × K completed checkouts, and all arms at that N must have equal checksums.
Missing rows, a failed executable, unequal completed work or different twin
bytes invalidate the comparison. These checks concern comparable work, not
fairness.

**Decision rule.** First require twin/runtime-baseline in [0.95, 1.05] for
every pass/N pair. Five percent is a preselected noise tolerance, not a
measured precision claim. A twin failure makes the comparison inconclusive;
it cannot establish a candidate pass. With valid controls, oldest-only passes
only if its N = 50 maximum wait is at most 3,300,000 ns in both passes and
its runtime/baseline is at most 1.10 at every N in both passes. Ratios use
the baseline row of the same pass and N; no averaging across passing and
failing cells. The age reference has no vote in candidate acceptance.
Only a passing uninstrumented probe warrants asking firn to measure its
engine. Firn's engine remains a separate required consumer comparison before
adopting the mechanism; a probe pass does not establish engine performance.

**Counters, separately from judgment.** Build instrumented baseline, age
and oldest arms separately, then run one diagnostic sweep at N = 2, 8, 50,
K = 1,000, on the same pinned drivers after their small pilots. Record total
guard notifications (including registration-before-park wakes), failed
retries that register and park again, object-lock acquisitions refused by
a turn/reservation, and fairness-clock reads. Print raw counts and counts
per successful checkout beside that invocation's latency percentiles.
The failed-retry count covers guards holding an ordinary object, including
this probe, and increments only when the false retry actually parks. An
early intervening wake is a notification but not another park. Table-only
successful guards have no bridge completion hook to clear retry attribution,
so their false retries are excluded rather than miscounted. The runtime
counts generic events without inspecting this probe's state;
normalization uses the probe's N × K, not all successful object holds (the
probe also takes the object to return each checkout). Output, aggregation
and any added clock samples must stay outside the uninstrumented timing
judgment. Clock reads here mean calls for fairness bookkeeping, not the
probe's own latency clock. Wake-to-run latency is omitted unless a separate
sampling mode can quantify its perturbation; count-only instrumentation
cannot establish scheduler delay or clock-call cost.

**Attribution reading.** Herd amplification is supported only if age's
notifications and failed retries per successful checkout rise above baseline
at N = 50 and oldest lowers both alongside an uninstrumented runtime
improvement. Refused-lock grabs isolate the spin-loop hypothesis: their
reduction in oldest supports reduced admission traffic, not a claim that
this traffic alone caused the whole regression. Clock-call counts show
frequency, not duration; a high count alone does not establish clock cost.
If the corresponding counts do not grow in age or do not fall in oldest,
that explanation is falsified for this workload even if timing improves.
Counters cannot apportion elapsed time among overlapping effects. Compare
diagnostic runtimes with the uninstrumented rows only to disclose observer
perturbation; diagnostics judge no performance criterion. No timing or
counter result had been collected when this protocol was registered; the
results below apply it without changing its thresholds.

### Reservation lifetime and progress obligations

A reservation authorizes an attempt, not a successful checkout. It remains
owned through reacquisition and ends only when the statement releases that
object after either registering its false guard or executing its block.
False retries retain their first-registration age. The notification sweep
visits remaining watches oldest first, including younger watches whose guard
the original write might have enabled; each failed watcher gets at most one
attempt in a sweep. A sweep retains its initiating monotonic clock sample
as an age cutoff, so later new statements cannot indefinitely extend it;
equal-age ties use context identity. Only a finite number of registrations
can occur in the clock tick containing that cutoff. This avoids an all-false
set continuously waking itself without another write. Writes by exempt
holders while a reservation is live preserve a pending
notification for watches already tried or newly registered beyond the
sweep cutoff. Untried watches within the sweep already observe the latest
state. Pending writes do not restart the oldest watch ahead of those
remaining watches.

The ordinary acquisition queue gets progress between sweeps. A live guard
reservation prevents a newcomer from receiving an ordinary handed hold;
when no reservation is live, the existing bounded acquisition handoff can
run before a new guard sweep. Earlier-object holders, table-entry holders
and cancellation-state updates retain their exemptions, since a reserved
retry may need their earlier locks. The experiment tests no-overtaking for
ordinary newcomers without these pre-existing obligations. This does not
promise strict first-come successful guard execution.

Cancellation and deadlines do not asynchronously discard an atomic retry
([WAIT-2], [PRE-2]). For example, a guard equivalent to
`available || cancellation_state.fired` may finish after firing and release
its reservation normally; a guard that does not observe cancellation keeps
waiting. A timeout is an outcome of a host wait, not a timer attached to an
atomic guard. A context cannot enter that host wait until its atomic
statement completes. Every normal block exit releases its holds, including
the reserved object; the context must hold no reservation when it finishes.
Thus cancellation and timeout cannot silently remove the only scheduled
reservation owner. Adding a future asynchronous atomic-abandonment API would
require an explicit reservation-transfer contract; none is introduced here.
The existing lowering retries the same saved guard targets before evaluating
the guard, so a selected reservation's object is always retaken even for a
multi-object condition.

A multi-object false retry can release B before releasing its reserved A.
If B is written in that interval, its notification wakes that already
reserved attempt without installing a second reservation, then continues
selecting from B's remaining watches. Thus one write can also notify an
early retry already reserved elsewhere, but creates at most one new
reservation. The shared watch lock serializes reservation publication and
retirement across objects; object locks remain outside it in the lock order.
The deterministic cross-unit test fixes that release/write order with a
native-thread acknowledgement and checks both notifications, the early wake
and fresh acquisitions after both contexts exit. At registration, its
execution, like the ordinary single-object ordering tests, remained to be
validated in CI; the results below distinguish the owner's CI report from
the downloaded evidence.

## Oldest-only handoff results

Temporary workflow run [38140292139](https://github.com/Ming-Research/Whitefoot/actions/runs/38140292139)
measured revision `ba0e7c5e73d5e304b85c122ed1303d6f2530056a` (implementation
`d5eebbd9011c008542e441499d2d6598abe70479`, followed by the owner's workflow
repairs). The machine was the native i9-14900K, Linux 7.0.0-38-generic,
two drivers pinned to CPUs 2 and 4 (different physical performance cores;
their sibling lists were 2–3 and 4–5). Construction used clang and LLD
22.1.8, C11, `-O2 -falign-functions=64` and full LTO. All arms used the
same emitted LLVM, SHA-256
`3a8f2c6388cb238c44a35498927a4b2736e787fea4069d51c6327dfd0b8f0c09`.
The construction record gives identical baseline/twin executable hashes.
The executables themselves are not included in the downloaded evidence.

Evidence is the run's `guard-fairness-evidence` artifact: construction and
run logs, `gf-timing/verdict.txt`, `gf-timing/measurements.csv`,
`gf-counters/{measurements.csv,counters.csv}`, both saved LLVM files and
all raw stdout/stderr. The separate `guard-fairness-negative-controls`
artifact records the five intended failures. All 32 timing and 17
diagnostic raw outputs agree with their CSV records. Every full row
completed N × 1,000 checkouts with equal checksums across arms. Both N and
arm orders match the registration. Two small timing pilots per arm took
1.462–1.494 ms, so expansion met the registered pilot limit.

### Registered verdict

These are the six lines of the uninstrumented verdict, with maximum wait
converted from ns to ms. Cost passes only at a ratio at most 1.10; the twin
control passes only in [0.95, 1.05]. The registered maximum-wait test applies
at N = 50, although the smaller-N maxima are also shown.

| Pass | N | Twin/base runtime | Oldest/base runtime | Oldest max wait, ms | Cost | Twin |
|---:|---:|---:|---:|---:|---|---|
| 1 | 2 | 0.996407 | 1.043602 | 1.009142 | pass | pass |
| 1 | 8 | 1.008035 | 1.153808 | 1.036914 | fail | pass |
| 1 | 50 | 1.018313 | 1.608172 | 1.848877 | fail | pass |
| 2 | 2 | 0.995460 | 1.045513 | 1.009429 | pass | pass |
| 2 | 8 | 0.988462 | 1.080677 | 1.039376 | pass | pass |
| 2 | 50 | 0.992909 | 1.613733 | 1.820695 | fail | pass |

**Reject under the registered rule.** Both N = 50 waits meet 3.3 ms,
against baseline maxima of 530.965 and 533.353 ms, but both N = 50 cost
ratios and the first N = 8 ratio fail. The twin agrees in every cell;
averaging orders cannot erase a failing cell. This establishes the sampled
tail improvement, not a universal wait bound. Under this protocol it does
not warrant asking firn to measure its engine. Accepting the observed cost
would require a new owner decision, rather than reclassifying this result.

The owner reports that the hosted build, positive tests and TSan passed.
Their execution logs are not in these downloaded artifacts. The five
negative outputs do contain the intended failures: baseline permits newcomer
overtaking; age wakes two guards instead of the oldest one; disabling false
handoff lets the newcomer run before both false attempts; disabling successful
release leaves a turn at context exit; a zero threshold blocks the young
watch. The output files alone do not establish the child exit codes; the
owner reports that CI checked all five expected failures.

### Diagnostic counters

Counts are whole-process events divided by successful guarded checkouts,
not individually tagged acquisitions. These are the separate instrumented
N = 2, 8, 50 runs, K = 1,000; they have no vote in acceptance.

| N | Arm | Woken/checkout | False re-parks/checkout | Refused lock grabs/checkout | Fairness clock reads/checkout |
|---:|---|---:|---:|---:|---:|
| 2 | baseline | 0.499500 | 0.499000 | 0 | 0 |
| 2 | age | 0.973000 | 0.965500 | 0.227500 | 0.980500 |
| 2 | oldest | 0.978000 | 0.970500 | 0.007000 | 0.985500 |
| 8 | baseline | 3.622125 | 3.468375 | 0 | 0 |
| 8 | age | 6.608500 | 6.405625 | 44.680500 | 1.202125 |
| 8 | oldest | 6.755500 | 6.663500 | 0.052750 | 1.142250 |
| 50 | baseline | 34.634460 | 33.716780 | 0 | 0 |
| 50 | age | 45.057140 | 44.041980 | 1592.548280 | 1.994040 |
| 50 | oldest | 48.603000 | 47.604520 | 2.007360 | 2.892760 |

At N = 50 oldest has 40.3% more wakes and 41.2% more false re-parks than
baseline, and 7.9%/8.1% more than age. Thus the registered explanation
that oldest reduces herd notifications and false retries is falsified for
this diagnostic workload. Refused-turn lock grabs fall 99.874% from age
to oldest: the admission storm is removed, while the notification work is
not. Clock reads increase 45.1% over age; that is frequency, not clock cost.
The uninstrumented N = 50 age runtime is 5.309/4.958 s, oldest
0.865/0.872 s and baseline 0.538/0.540 s. Removing the admission storm
accompanies a large improvement over age, but does not recover baseline.

The diagnostic N = 50 runtime exceeds the two corresponding
uninstrumented runtimes by 8.9–9.4% for baseline, 12.1–13.0% for oldest
and 24.5–33.3% for age. These are separate invocations, so their differences
also include run variation. They disclose material observer perturbation,
not a paired estimate of instrumentation overhead. No wake-to-run timestamps,
CPU profile, wake-path split or lock-duration samples were collected.

### Attribution from the code

Oldest-only means one outstanding reserved attempt at a time, not one
notification in total per write. In `wf_watch_wake_locked`, a write whose
oldest watcher is older than 1 ms starts a sweep with that watcher alone.
When it reads false and re-registers, `wf_guard_release_locked` passes the
same notification to the next oldest watcher in the original age cutoff.
This includes young watchers in that cohort; it is necessary to avoid
losing a write which enabled a different condition. The sweep can therefore
resume every watcher, serially, without any further write. The below-T
path still broadcasts. Exempt writes during a sweep can also request a
rescan of watches already tried or registered beyond its cutoff.

This probe writes both `out = 1` at successful checkout and `out = 0` at
return. The checkout write starts notifications while every other
`out == 0` guard is false. Failed reserved attempts pass the reservation
onward. The holder does its mixing work outside the physical object hold;
its separate unguarded return statement has no earlier hold and is a
newcomer to the reservation. It can therefore wait behind the false-attempt
sweep before writing `out = 0`. This is a direct source explanation for
approximately N notifications per checkout even in the oldest-only regime.
The measured 48.603 wakes and 47.605 false parks at N = 50 fit it. The
aggregate counters cannot say what fraction came from that sweep, below-T
broadcasts or rescans; they do not prove that broadcasts dominate.

Against baseline, the extra false attempts require more driver resumes,
object acquisitions, watch registration and parking. Baseline allows the
running holder to return and check out again before notified contexts run;
writes can occur while those contexts are detached from the watch list.
Oldest prevents this overtaking and serializes reservation transfer.
Furthermore `wf_guard_oldest` scans the linked watch list on each false
transfer, skipping already visited watches which re-registered. A sweep of
M false watchers can do quadratic list-search work under the global watch
lock. The counters measure neither these comparisons nor lock contention.
These are source-supported additional costs, not separately timed causes.

The measured extra N = 50 runtime is 6.540/6.631 µs per checkout.
Mean reported hold grows from 6.613/6.611 µs to 11.556/11.719 µs, although
the mixing workload is unchanged. Crucially, the saved LLVM samples
`returned = now()` **before** its lazy acquisition of the return statement's
object. Consequently this interval includes work and scheduling delay
before that sample, but can exclude reservation waiting after it. It is
neither physical lock occupancy nor the complete `out == 1` residence time.
It cannot quantify return-lock delay or partition the slowdown into
computation and synchronization. More false retries, serialized sweeps,
list searches and delayed return are credible contributors to the 1.61×
cost; these data cannot assign an elapsed-time share to any of them or
establish clocks as the dominant cause.

### Related spinner progress gap (static, not a timing result)

`wf__shared_take` only spins, periodically yielding the host thread, and
may borrow an unclaimed handed physical hold. It does not use the guard
reservation or the ordinary parked acquisition queue's missed-wake
escalation. No call to it appears in the saved probe LLVM, so it is not
the explanation for this measured regression.

It should share a progress obligation and acquisition accounting, but
cannot simply join this reservation queue: a caller already holding map
entries or an earlier object must keep running, and the selected guard
may need those earlier locks. Parking that caller or making it await the
selected guard can deadlock. The handed-hold borrowing case must also be
preserved when the grantee is ready on the caller's own driver.

A minimal contested schedule is X holding map entry E and attempting O,
while Y on another driver repeatedly executes finite unguarded statements
on O. Let E precede O in the permitted lock order. If Y wins every free
interval, X has no queue entry or accumulated missed-acquisition state
that forces a later admission. This is a pre-existing, statically identified
gap in the [WAIT-2] eventual-effect argument, not an observed failing run.
Unlike the fluctuating guarded condition in the probe, X's unguarded
statement is covered by that guarantee.

There is also an entry-path gap: `wf__atomic_group_take` uses this spinner
for every ordinary member, including the first when no earlier target is
held. Such a newcomer can bypass a guard reservation without needing the
earlier-lock exemption. A one-driver context looping over finite
unguarded grouped atomics also bypasses the `wf__shared_acquire` context-pass
yield; yielding the host thread cannot resume another ready coroutine on
that driver. A proposed next progress investigation needs a same-type
two-object atomic-loop witness alongside the mixed map/object contention
witness, with no guards required. Both findings are deferred for a separate
progress design and CI witnesses; this results change implements no fix
and changes no specification rule.

## Step D: remaining-path attribution, registered before measurement

The owner selected a diagnostic comparison before choosing another fairness
mechanism. The question is whether the remaining oldest-only cost is
associated mainly with below-threshold broadcasts, searches while passing
a reservation, or false-condition reservation passes. This is an attribution
run of the existing implementation, not a new candidate or a relaxation of
the oldest-only rejection above. No step D result is claimed here.

### Observations and construction

Keep `probe.wf`, the three runtime modes, all uninstrumented build flags,
the timing arm order, K = 1,000, N = 2, 8, 50 and both reversed orders
unchanged. Baseline's twin remains a byte-identical copy. On the native
i9-14900K run two drivers pinned to CPUs 2 and 4. Dispatch input
`path_diagnostics` defaults to false. When enabled it additionally builds
counter-on baseline, age and oldest executables and interleaves their runs
with the uninstrumented arms. Diagnostic rows remain in their own CSV and
are never supplied to the timing judge. Retain small pilots and raw records.
Allow about three minutes for the 14900K job, excluding the hosted compiler
build, queueing and tool installation; this is a scheduling estimate, not a
new timing observation.

Under `WF_GUARD_COUNTERS` retain the existing four totals and additionally
record these paths. Event counts are normalized by completed guarded
checkouts, which are the successful takes of the logical pool resource;
the return statement's physical acquisition is not another denominator.
Counts include generic startup/teardown, just as the earlier counters did.

- Below-T batch wakes: notifications issued by the ordinary-object
  broadcast path after a valid oldest age at most T was observed. Count
  notifications, not writes; a batch with M watchers contributes M. Report
  grant-bypass and invalid-clock/age fallback batches separately so they
  cannot masquerade as below-T broadcasts. Baseline and age have their own
  broadcast behavior and do not establish a mode-2 threshold classification.
- First by-age selections: initial age-qualified sweep selections. These
  count starts, not the watches subsequently visited by the same notification.
  A multi-object watcher already reserved elsewhere can receive this
  notification without installing a new reservation here. False handoff
  and rescan are different stages; a rescan can initiate another by-age
  selection.
- False-condition passes: a reserved retry that registers a false guard
  transfers its reservation to another watcher. Successful read-only
  release is not a false condition. Failed retries that finish a sweep
  without a recipient are not passes.
- Rescans: deferred notifications actually revisited after a sweep,
  distinguished from repeatedly setting its pending bit. A rescan is not
  necessarily a broadcast: it can begin another aged selection.
- Pass-search comparisons: watched links examined by oldest selection
  while advancing an existing reservation, including links excluded by
  cutoff/cursor and unsuccessful searches. Initial oldest selection is
  excluded. Count links examined rather than individual Boolean/identity
  tests, making the unit independent of short-circuit evaluation.

The diagnostic-only return observer uses a generic runtime entry marking
the next ordinary acquisition. A research LLVM observer transform marks the
first real return acquisition after `hold_work`, once before its retry loop;
it rejects missing or ambiguous sites. It preserves the original emitted
LLVM separately and never changes the counter-off timing IR. The runtime
does not inspect `out`, function identity or this probe's state.

Sample the first and every 64th marked acquisition in each context. At
entry to the real `wf__shared_acquire`, take a monotonic timestamp; preserve
it through spin, fairness yield, parking and retries; take the end timestamp
after acquisition succeeds. This interval measures acquisition-call wall
time, including suspension and scheduling, not just mutex wait or CPU work.
It excludes the work preceding that call and the return block itself.
Report marked acquisitions, successful samples, total sampled ns, maximum
sampled ns and clock failures. Report sampled mean as total/sample count;
do not divide sampled ns by all successful checkouts or treat the sample
maximum as the maximum of all return waits. Return sampling clock reads
are separate from the existing fairness-clock counter. Sampling can alias
the workload's periodic behavior and adds observer cost; its findings are
diagnostic, not uninstrumented latency bounds.

### Reading fixed before the run

At N = 50, compare the below-T, initial-selection and false-pass counts and
normalize search work both by successful checkout and by actual
false-condition pass. For this single-object probe those three counts
describe different notification stages, alongside the separately recorded
grant/clock fallbacks; rescan executions and search work overlap them.
They are not a complete partition of arbitrary multi-object guard
notifications or successful read-only forwarding. Check the
N = 2 and 8 rows for the proposed scaling. A larger count denotes event
frequency, not a fraction of elapsed time; no collection of these counters
can apportion overlapping scheduler and synchronization cost.

- If below-T batch notifications are the largest measured notification
  category, the next comparison is option B: one-at-a-time notification
  in every regime, with false-condition forwarding retained for progress.
  This pattern refutes the claim that aged sweeps account for most wakes.
  Strict turns' prior engine losses remain an objection to test, not a
  numerical prediction for that new variant.
- If pass-search links per pass grow with N and links per checkout grow
  approximately as N squared, while below-T batches are not the principal
  source of notifications, investigate fixing the lookup representation
  before changing notification semantics. High search work supports the
  quadratic-work hypothesis; it does not alone establish that searches
  dominate runtime. A counter-off paired version with cheaper selection
  is the eventual cost falsifier.
- If false-condition passes are the largest notification category, with
  approximately N passes per checkout, investigate option C: condition
  eligibility sufficient to avoid scheduling false retries. This remains
  relevant even if lookup work is also large; reducing lookup cost alone
  leaves those resumes. Field dependencies alone cannot distinguish this
  probe's `out = 1` and `out = 0` writes. Large sampled return waits support
  the proposed enabling-writer delay; small samples weaken that explanation
  but cannot exclude unsampled tails.

These patterns may overlap. If both false passes and quadratic searches
are prominent, a lookup-only comparison isolates the smaller representation
change first; it does not establish that condition filtering is unnecessary.
If no pattern separates, or fallback/rescan work dominates, bring that
observation back before selecting a mechanism. Diagnostics themselves have
no fairness or cost acceptance verdict. Every later candidate must still
pass the uninstrumented N = 50 maximum of 3.3 ms, runtime/base at most
1.10 at every N in both orders, and twin/base in [0.95, 1.05] in every
cell before asking firn for an engine comparison.

### Existing main divergence

This comparison keeps the branch's original base `fe5589ec5f45`. Inspection
of the cached `origin/main` at `89ddad8866e24aacadfb42217a594ad58996cff7`
finds substantial driver-service changes since that base. A read-only
three-way merge of the committed branch head and that cached main reports
two conflicts in `bridge.c`: main arms a context wake gate before shared
parking, whereas this branch adds an exemption argument to that parking
call; main also replaces the inline driver resume loop with
`wf_context_run`. A future integration must reconcile wake publication
with reservations and sample-field ownership. No merge, rebase or fetch
was performed; this comparison cannot establish applicability to any
newer remote main.

## Ordered wait queue: prediction registered before implementation

The owner reports path-attribution run 38178470077 on revision
`52d22226d2c769d2aedad5475464e21c799ef554`, native i9-14900K: at N = 50,
oldest/base runtime was 1.608/1.665 and twin/base 0.998/0.967, retaining
the rejection. Per checkout, oldest counted approximately 48.4 notifications
(baseline 24.9), 20.6 below-T batch notifications, 26.3 false-condition
passes, zero rescans and 1,289 passing-search link examinations. Sampled
mean return acquisition time was approximately 5.4 µs, against baseline
21 ns, with a sampled maximum of 19 µs. At N = 8, below-T batches
dominated at 6.42 notifications per checkout while passes and passing
searches were near zero. These are the owner's supplied results; their raw
artifact has not been downloaded or independently checked in this checkout.

The owner selected removal of the quadratic passing search before changing
notification semantics. Keep mode 2's age/identity order, 1 ms threshold,
oldest-only reservation, age-preserving false retry, finite cutoff, below-T
batch notifications, cross-unit notification and progress exemptions.
Maintain an ordered wait queue under the existing locks and select the
next eligible watcher directly. Baseline and age runtime arms are unchanged.
Ordered registration may still require comparisons; moving the old entire
passing scan to every false re-registration would not support the intended
cost attribution and must be disclosed if it occurs.

**Registered prediction.** At N = 50, passing-search link examinations per
successful checkout will be at most four times the number of false-condition
reservation passes plus four, replacing approximately 1,289 examinations
per checkout. The sampled mean return-acquisition wait will be at most
2.7 µs (half the reported 5.4 µs) in the diagnostic run. With agreeing
twins, an uninstrumented oldest/base runtime ratio at most 1.25 in both
N = 50 orders will support search as a main contributor by substantially
closing the gap toward 1.10. Ratios from 1.50 through 1.70 in both orders,
despite meeting the comparison-count prediction, will refute search as the
main source of the approximately 1.6× cost. Other outcomes are mixed or
inconclusive for that attribution. A failing cost verdict stays a rejection:
acceptance still requires N = 50 maximum wait at most 3.3 ms, runtime/base
at most 1.10 at every N in both orders, and twin/base in [0.95, 1.05] in
every cell. Diagnostic timing and the prediction's intermediate 1.25 ratio
cannot establish acceptance.

Run the same K = 1,000, N = 2, 8, 50 protocol on two drivers pinned to
CPUs 2 and 4 of the native 14900K, with unchanged interleaved timing arms
and their reversed second order. Enable the existing `path_diagnostics`
input so separate counter builds and the sampled return observer run in
the same dispatch. Keep the baseline twin and all five deliberately broken
variants' intended failures, and require clean TSan. No ordered-queue
measurement is claimed here. The branch stays on its existing base without
merging main.

### Ordered-queue implementation and verification scope

Only mode 2 sorts ordinary-object watch links by the existing
`(guard_started, integer context identity)` key. A reservation leaves an
uncounted link owned by its context at the selected position. The shared
watch lock protects the order, back links and anchor, while reservation
installation and retirement also hold the reserved object's lock. The
anchor stores its context directly: retaining a pointer to a detached watch
would race with the selected context reinitializing that watch outside the
watch lock. The private link union preserves its size; neither the public
watch-frame ABI nor the fixed shared-object header grows. No new lock is
introduced.

A reserved false retry inserts its real link immediately after its anchor.
Passing starts there, skips that one same-key retry if present, and selects
the next eligible link or stops at the cutoff. Notification detaches every
real link of a watcher, including links on other units; the existing back
pointers automatically update an anchor whose successor was removed. An
already-reserved cross-unit notification keeps its original anchor and
uses the selected link's successor only during the same locked operation.
Successful release removes the anchor before age reset or context exit.
The cancellation alternative succeeds and retires through that same path;
a host timeout still does not asynchronously abandon an atomic guard.

Direct successor selection and same-object reserved false re-registration
are constant-time in the number of waiters. Finding the selected watch's
link still costs its number of watched units, as does unlinking those units.
New or unreserved registrations search for their ordered insertion point,
including retries from below-T batches; those can still incur linear work
per registration. The passing-search counter retains its meaning and counts
real successor candidates, including self/cutoff rejection, not anchors,
initial selection, insertion or repair comparisons. Thus a low passing
count alone cannot establish that total watch-lock work is now linear.

The original missing-clock policy can change an age from zero to a valid
sample while earlier links are still registered on other units. Mode 2
then moves those prior inline and chunk links forward under the watch lock,
restoring their new key order without changing the clock-recovery rule.
This rare repair also visits table links and can change their broadcast
order while preserving the notification set; tables otherwise retain
prepend registration and broadcast behavior. The repair is not performed
on every false retry. Baseline and age retain their original registration,
selection and removal paths.

The existing native `handoff` case now uses four distinct-age watchers to
exercise interior successors and repeated age-preserving false insertions.
The private `cancel-test` ordered fixture supplies an independent equal-age
identity oracle, removes a successor through another unit, tests the cutoff
and checks final anchors, back links and counts. Its clock-recovery fixture
registers five objects plus a table (including overflow watch chunks),
arranges a zero age, then registers a sixth object through the production
path; prior links must move behind the older valid ages without changing
counts. Both fixtures run in the ordinary cancellation test, and the
temporary workflow runs their isolated selector natively and under TSan.
The existing newcomer, age, cancellation and cross-unit selectors and all
five deliberately broken variants remain wired. Native construction,
intended mutant failures, race checking and performance are still awaiting
CI; source inspection and fixture design are not executed evidence.

### Ordered-queue results

The owner supplied rounded results from two native 14900K runs of the
registered protocol, both on revision
`0a36bb15eee7ee26071d0a438db71015c3e69a1f`.
[Run 38182220053](https://github.com/Ming-Research/Whitefoot/actions/runs/38182220053)
reported N = 50 oldest/base and twin/base ratios of 1.832 and 1.089 in
pass 1, and 1.691 and 1.045 in pass 2. Its reported verdict file says
`inconclusive`: pass 1's twin/base ratio exceeds the registered upper
control bound of 1.05. Its timing comparison is excluded from the verdict,
rather than averaged with another run or retaining only its second order.
[Run 38182569301](https://github.com/Ming-Research/Whitefoot/actions/runs/38182569301)
supplies the decisive timing comparison below, with reported verdict file
`reject`. These
are owner-reported values; the raw artifacts are not locally available and
have not been fetched or independently checked. Build, test, intended
mutant-failure and ThreadSanitizer statuses were not supplied, so this
section establishes no execution result for those checks.

| Pass | N | Oldest/base runtime | Twin/base runtime | Oldest maximum wait (ms) |
| --- | ---: | ---: | ---: | ---: |
| 1 | 2 | 1.045 | 1.0015 | — |
| 1 | 8 | 1.185 | 1.0049 | — |
| 1 | 50 | 1.629 | 1.0065 | 1.307 |
| 2 | 2 | 1.047 | 1.0012 | — |
| 2 | 8 | 1.120 | 0.9883 | — |
| 2 | 50 | 1.631 | 1.0067 | 1.300 |

A dash means that value was not supplied. The twin ratios are listed in
pass-major order: N = 2, 8, 50 for pass 1, then N = 2, 8, 50 for pass 2.
All six controls lie within [0.95, 1.05]. The N = 50 maximum wait satisfies
3.3 ms in both orders, and N = 2 satisfies the runtime criterion. The
candidate remains **rejected**: runtime/base exceeds 1.10 at N = 8 and
N = 50 in both orders. Neither the passing tail nor cheaper successor
selection relaxes that acceptance rule.

The supplied N = 50 diagnostic event totals, normalized per successful
checkout, are:

| Oldest-only event | Events per checkout |
| --- | ---: |
| Waiters notified | 48.61 |
| Spurious retries | 47.61 |
| Lock grabs on refused turns | 0.82 |
| Batch notifications at or below T | 28.94 |
| Initial age-qualified selections | 1.21 |
| False-condition reservation passes | 18.46 |
| Rescans | 0 |
| Passing-search candidate links examined | 37.31 |

Baseline notified 25.37 waiters per checkout, leaving 48.61 − 25.37 = 23.24
extra notifications in oldest-only. Its sampled mean return-acquisition
wait was 26.9 ns, versus baseline's 17.3 ns, with an oldest-only sampled
maximum of 146 ns. Those latency summaries describe the sampled diagnostic
acquisitions, not every return or the uninstrumented timing run.

The reported passing count satisfies the registered bound:
37.31 ≤ 4 × 18.46 + 4 = 77.84 links per checkout. The sampled mean also
satisfies the 2.7 µs prediction: 26.9 ns is below 2,700 ns. Despite those
improvements, both N = 50 runtime ratios, 1.629 and 1.631, remain in the
registered [1.50, 1.70] refutation range with agreeing twins. This refutes
the prediction that the old passing search was the main source of the
approximately 1.6× runtime cost on this probe. The comparison does not
support the intermediate 1.25 ratio, much less the 1.10 acceptance limit.

The diagnostic counts still show substantial below-T batch notifications
and false-condition passes. They identify remaining retry activity but
cannot apportion elapsed time between those paths or explain each of the
23.24 extra notifications. Registration and clock-repair comparisons are
excluded from the passing-search counter, so the low passing count does
not establish that all comparison cost disappeared. Without raw matched
diagnostic and timing rows, instrumentation perturbation also remains
unquantified here. The result supports cheaper passing and short sampled
return waits while leaving the total runtime cost and the candidate's
rejection intact.

## Wake only guards that may hold: design study

The owner selected option A after the ordered-queue rejection: a guard
known to be still false stays registered, neither woken nor handed the
reservation. This is a study at revision `2859dec3e`; nothing is implemented
or measured. **[code]** marks a reading of this checkout, **[data]** the
tables above, **[inf]** this study's inference.

**What a guard depends on.** [code] The guard is lowered inline in the
waiting frame and re-evaluated by the waiter after each wake
(`compiler/src/lowering/builder/atomic.rs:633-718`). A watch holds only
links (`bridge.c:1220-1226`). The guard writes no path and calls no waiting
function (`spec/kernel-spec.md:2308`), so it cannot call `now`, which writes
its clock (`spec/kernel-spec.md:2586`). A spawn takes only value parameters
(`spec/kernel-spec.md:2260`). [inf] A guard is therefore a function of its
targets' states and of locals that stay fixed while it waits. A guard that
read false under the hold becomes true only after a write to a target, never
through time.

### 1. Mechanism

**Design A: registered wake tests.**

*Registration.* For each target X, lowering derives a wake test T_X: a
condition over X's state that the guard implies. It puts the guard in
negation normal form through `band`, `bor` and `bnot`, a finite structural
walk like [ENT-3]'s signed decomposition (`spec/kernel-spec.md:3344`).

- An atom is kept when it compares an integer, `Bool` or enum-tag scalar at
  a fixed offset in X's state (field selections and fixed-size array steps)
  with one of: a constant, a value computed from locals at registration, or
  another such scalar of X.
- Every other atom becomes `true`. Since the formula is monotone, guard ⇒ T_X.
- A test holds at most four atoms. Larger guards are weakened
  deterministically: conjuncts are dropped, and an oversized disjunction
  becomes `true`.
- Lowering runs per monomorphized instance (`compiler/src/driver.rs:457`),
  so offsets are constants in generic code too.

Each descriptor (offset, width and signedness, operator, operand value) is
stored by value in the watch before it is linked. `T_X = true` means always
eligible, which is today's behavior. The probe's test is `out == 0` at a
fixed offset.

*Release.* The releaser evaluates the tests on the new state, under the
object lock and `wf_watch_lock` as today's sweep is
(`bridge.c:3056-3125`). Watches with byte-identical tests share a *bucket*,
an intrusive age-ordered list that needs no allocation. Each bucket costs one
evaluation, and the oldest eligible watch is the oldest head among the
eligible buckets.

- **Above T (1 ms):** reserve the oldest eligible watch
  (`bridge.c:3016-3054`).
- **Below T:** wake the oldest eligible watch only, with a *soft
  notification*. Newcomers may still overtake, so these are not the rejected
  strict turns. A false retry, or a success that wrote nothing, forwards the
  notification to the next eligible watch inside the cutoff, as the reserved
  pass does (`bridge.c:3636-3672`).
- **A successful write** restarts its object's sweep from the oldest
  eligible watch.

*Invariant [inf].* A registered watch outside every active sweep's untried
remainder has a false guard in the current state. Registration establishes
this. A skipped watch keeps it, because a false T_X makes its guard false.
Writes to the watch's other targets are judged by its links there.

A guard that becomes persistently true therefore lies in the sweep after
the last write before that point. Sweeps are finite (bounded by the cutoff)
and age-ordered, and each older eligible watch that succeeds leaves. So the
waiter gets an attempt after finitely many writes [WAIT-2]. The derivation
is syntax-directed, and the runtime reads descriptors only: nothing names a
field, value or program.

**Alternative B: the releaser runs the guard.** The guard is outlined as a
function over the states and a record of the locals it reads, copied at
registration. The releaser calls it for each waiter whose targets it holds.

- **Sound.** The releaser holds exclusive access, and the guard writes
  nothing. The number of evaluations is unobservable
  (`spec/kernel-spec.md:2318`), and which waiter goes first is an execution
  input (`spec/kernel-spec.md:2252`).
- **Exact** for single-object guards.
- **Cost [inf].** It makes N indirect calls on cold records per write, under
  the lock, on the critical path: about 2.5–7.5 µs at N = 50 at 50–150 ns
  each, with two writes per take. That is the size of the cost being removed.
  Opaque guards cannot share an evaluation.
- **Limits.** A guard over an object the releaser does not hold must wake,
  because the lock order forbids taking it (state-locks, second decision).
  Cost, allocation and divergence move to a context that did not write the
  guard.

**Alternative C: field dependencies only.** This is cheap, but it gains
nothing on the probe: both statements write `out` (step D).

**Variant A1:** A that wakes every eligible watch below T. The sweep after
`out = 1` empties, and the herd after `out = 0` stays.

### 2. Hard cases

- **Several objects.** A foreign atom becomes `true`: conjunctions still
  filter, while `bor(q^.ready, c^.fired)` gives `T_Q = true`. Freezing
  foreign atoms at their registration values loses wakes. Take `band(a, b)`
  with both false: setting b is skipped, because T_B uses a frozen false a;
  setting a is then skipped too. Caching foreign truth bits that each writer
  updates is sound, and is deferred until cancellable waits show a cost.
- **Cancellation and deadlines.** Firing writes the cancellation state,
  whose link tests `fired`. A deadline is a host-wait outcome, never a guard
  input ("Reservation lifetime" above).
- **Waiter lifetime.** Test values are copied, not referenced. They are read
  only while linked under `wf_watch_lock`, and unlinked under it before the
  owner reinitializes its watch (`bridge.c:1198-1201`, `2950-2976`). Nothing
  abandons a parked retry.
- **Unbounded dependencies.** Helper calls (user results have no [ENT-3]
  origin, `spec/kernel-spec.md:3325`), contents of `Box`/`Slots`, entries
  under computed keys and floats give `true`: today's behavior, still in age
  order. Map watches keep waking the whole map. The callee's verified
  relations [FN-9] could serve helper guards; that is deferred.
- **Fairness.** FIFO by age, then identity, among eligible watches.
  Ineligible watches keep their place.
- **Queue and reservation.** The ordered queue moves into the buckets, with
  the anchor in the reserved watch's bucket. A pass after an exact test cannot
  be white. An exempt write marks a rescan only for an eligible watch
  outside the cursor.

**Found along the way [code].** Every object unlock passes `write = 1`
(`compiler/src/backend/emitter/shared.rs:958`). Every release, read-only
blocks included, therefore starts a sweep, and the read-only forwarding
(`bridge.c:3645-3650`) runs only after false guards. A per-target write flag
from the block's footprint is small and belongs in the candidate.

### 3. Specification impact

None. Evaluating a consequence of the guard is covered by
`spec/kernel-spec.md:2318`. A skipped statement waits on a false guard as
[SHARE-3] requires (`2316-2317`). Choosing which eligible waiter attempts
first chooses the atomic order, which is a [WAIT-2] input (`2252`). Progress
(`2254`) follows from the invariant. B needs no change either. Strong
fairness stays rejected (language/waiting/shared-objects).

### 4. Cost estimate

- **Compiler [inf]:** derivation and operand classification (about 300
  lines), one IR operation, the emitter, and the write flag.
- **Runtime [inf]:** buckets, evaluation and soft notification (about 400
  lines of C). `WF_WATCH_SIZE` (152, `bridge.h:499`) grows by about 72 bytes.
  Tests cover:
  - cross-object conjunction in both orders;
  - cancellation disjunction;
  - a captured operand (`busy_contexts.wf`, one bucket per waiter);
  - a helper guard;
  - read-only forwarding;
  - TSan.
- **Per release [inf]:** O(buckets) evaluations of at most four loads on the
  line just written, about 5–30 ns at one bucket. Distinct equality operands
  give O(N) buckets; a value index would make that O(1), and is deferred.

**What it can remove.** [data] At N = 50, oldest-only took 0.865/0.872 s
against the baseline's 0.538/0.540 s: about 17.4 against 10.8 µs per take.
Mean hold rose from 6.61 to 11.56/11.72 µs with the work unchanged. The
ordered queue kept 1.63× with 48.6 notifications per take against 25.4.

[inf] The hold window contains the holder's unlock after `out = 1`, whose
sweep A empties. If the hold growth is that sweep, about 5 of the 6.6 µs
excess (75%) goes, which predicts 1.10–1.15. Cutting the herd after
`out = 0` to one wake could go lower.

**Risk.** With few contexts ready, the other driver may sleep, and a
reserved hand-off there pays a cross-driver notification
(`bridge.c:1713-1724`), which no run has measured.

### 5. Registered experiment

- **Question.** Do wake tests with a single eligible notification meet the
  criteria?
- **Arms.** Unchanged `probe.wf` and same-source runtime modes: baseline, its
  byte-identical twin, candidate A (mode 3), and two non-voting references,
  A1 and mode 2.
- **Protocol.** Native 14900K, `WF_DRIVERS=2` on CPUs 2 and 4, K = 1,000,
  N = 2, 8 and 50, two passes with N and arm orders reversed, pilots first.
  Five broken variants must fail, including "skip an eligible watch" and
  "frozen foreign atoms".
- **Diagnostics.** Existing counters, test evaluations per write, and the
  hand-off latency from installing a reservation to its owner's acquisition,
  sampled at the first and every 64th.
- **Acceptance (unchanged).** Twin/base in [0.95, 1.05] in every cell;
  N = 50 maximum wait at most 3.3 ms in both passes; candidate/base at most
  1.10 at every N in both passes.
- **Prediction.** At N = 50: candidate notifications (wakes plus handed
  reservations) at most 3 per take, against 48.6; white hand-offs at most 0.1
  per take; mean hold within 10% of baseline; acceptance met. A1 keeps at
  least 15 wakes per take.
- **Rejection.**
  - Any failing cell rejects the candidate.
  - More than 3 notifications per take at N = 50 refutes the mechanism,
    whatever the runtime.
  - At most 3 notifications with an N = 50 ratio of at least 1.25 in both
    orders refutes wakeups as the main remaining cost. The hand-off samples
    then point to reservation transfer, and the trade between fairness and
    throughput returns to the owner.
- **Time.** About 5 minutes of 14900K time (step D estimated 3 minutes for
  four arms), excluding the hosted build and queueing. Firn's engine is
  measured only after a probe pass.

### 6. Recommendation

**Design A** with soft single notification below T and the per-target
write flag, keeping the reservation, its threshold and the ordered queue
(per bucket). It removes both remaining sources on the probe at O(buckets)
per write. B is exact but costs O(N) opaque calls under the lock, and C
cannot see values.

**Confidence 3/5.** The counts follow from the code. The runtime prediction
rests on attributing the hold growth to the sweep after `out = 1`, which no
measurement has separated. It would be overturned by a large hand-off
latency, or by consumers whose guards are mostly helper calls or
cross-object disjunctions.

**Design tree.** Adoption replaces three decisions in
`design/compiler/waiting-contexts/state-locks.md`:

- the guard-watch decision (line 9): object writes would wake only eligible
  watches, with tables unchanged;
- proposed step B (line 11): "below the threshold writes retain the
  baseline wake-all behavior" no longer holds;
- the proposed ordered queue (line 13): one list with an unchanged watch
  frame becomes buckets and a larger watch.

Nothing approved in `design/language/waiting` is contradicted.
