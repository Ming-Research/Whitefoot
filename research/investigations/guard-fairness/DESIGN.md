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
