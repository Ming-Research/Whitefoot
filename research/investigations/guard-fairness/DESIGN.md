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
