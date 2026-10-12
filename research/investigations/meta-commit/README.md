# Meta commit: statements on different keys overlap their computation

## The question

Firn runs each data-modifying command as one atomic statement that names
entries of a shared map (the key table) and a shared object, Meta, holding the
global sequence number, the AOF pending bytes and the expiry queue. Do two such
statements on different keys compute at the same time, meeting only for the
short moment each takes the next sequence number from Meta at its end?

The specification fixes a statement's targets when it starts and does not
require them to be taken together ([SHARE-2], [SHARE-3]). The compiler takes
map entries before the block and any other shared object at its first use
inside the block
([state locks](../../../design/compiler/waiting-contexts/state-locks.md)).
A statement that computes first and reaches Meta last should therefore hold
Meta only briefly. Until this investigation that was a conclusion from reading
the code; the witness makes it a checked result, so that a later change in lock
timing is caught.

## The witness and its control

- [meta_commit_overlap.wf](../../../tests/programs/meta_commit_overlap.wf):
  two contexts run on two drivers. Each runs one atomic statement that names
  its own key of a shared map (`a`, then `b`) and Meta, computes for 200
  milliseconds on its own entry, and then, in the same statement, takes the
  next sequence number from Meta and appends a log record (key, sequence,
  value).
- [meta_commit_held.wf](../../../tests/programs/meta_commit_held.wf): the same
  program, except that each statement takes its sequence number from Meta
  before it computes, so it holds Meta through the computation.

Both are run by
[the program cases](../../../compiler/tests/programs/parallel.rs) with
`WF_DRIVERS=2`.

The computation is a loop of integer mixing that runs until the monotonic
clock reaches a deadline 200 milliseconds after the statement began, so its
length does not depend on the speed of the host and no calibration is needed.
Its result becomes the value stored under the key.

## Criterion

The first measurement, on the hosted macOS runner (three shared cores, the
test running beside other compiler tests), used a ten-millisecond computation
and a fixed bound of 15 ms for the overlap program. Its three runs took 23.4,
25.9 and 31.5 ms, so a bound fixed in advance did not hold: the start of the
second context was delayed by 13 to 21 ms, a delay the load decides. The
criterion therefore compares the overlap program with the control measured in
the same test and uses a computation C of 200 ms, long against that delay.

With C and a start delay d, an overlapping run takes C + d and a run that
excludes takes at least 2 C + d. The delay moves both runs alike, so the test
runs the overlap program and the control in turn, up to five rounds, and
passes once the best overlapping run so far

1. is shorter than 1.8 C (360 ms), which no serialized run reaches because it
   takes at least 2 C, and
2. is shorter than the best control run by more than C / 2 (100 ms), half of
   the difference C that an overlap leaves.

Both conditions tolerate a start delay of 100 ms or more in the best run (160
ms for the first), about five times the worst delay measured. The control must
take at least 1.8 C (360 ms) in each run, which the control program checks
itself (status 4); the overlap program must take at least C (status 5).

The overlap program also exits nonzero when the log does not hold exactly two
records, when the sequence numbers are not 0 and 1 in log order, or when
replaying the log in sequence order does not give the final table. Status 0
needs all of these. The best of several rounds is read because a loaded
hosted runner can delay the start of the second context and this costs the
result only when it happens in every round. A repeat cannot hide a lock held
through the computation: every run of it takes at least 2 C and fails the
first condition.

## What a failure would mean

- No round satisfies both conditions while the state checks pass: the two
  statements exclude one another for most of their computation. The compiler
  or runtime takes Meta earlier than the first use, for example at the start
  of the statement, or holds the map entry in a way that excludes another key.
  The lock timing the Firn command design relies on has changed, and the
  finding goes to the owner as a decision.
- The overlap program takes less than C (status 5): the computation did not
  run, and the duration says nothing about locks.
- The control takes less than 1.8 C (status 4): Meta is no longer held through
  the computation although the statement used it first, or the clock-bounded
  computation no longer runs inside the statement. The criterion then does not
  separate the two forms and the overlap result proves nothing.
- A state check fails (log length, sequence numbers, replay): a lost or
  duplicated update, which is a correctness defect whatever the timing.

## Limits of the result

It shows two statements on two drivers with a 200-millisecond computation, one
shared Meta and two keys. It does not measure the cost of the Meta commit
itself, scaling past two drivers, or statements that reach Meta in the middle
of their block. It needs two drivers and no more, so it holds on a hosted
runner with three cores. A host with one core runs both drivers on that core:
the overlap program can still pass there, because each computation is bounded
by the clock and not by the work it does, while the control cannot.
