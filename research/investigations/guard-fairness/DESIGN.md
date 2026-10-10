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
reports no maximum):

| Line | Connections | Rate | p50 | p99 |
|---|---:|---|---|---|
| Redis 7.0.15 | 8 | 228,770 / 228,835 | 0.029 / 0.029 | 0.059 / 0.058 |
| Redis 7.0.15 | 50 | 231,140 / 231,900 | 0.202 / 0.201 | 0.410 / 0.408 |
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

