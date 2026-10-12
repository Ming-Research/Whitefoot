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
  its own key of a shared map (`a`, then `b`) and Meta, computes for ten
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
clock reaches a deadline ten milliseconds after the statement began, so its
length does not depend on the speed of the host and no calibration is needed.
Its result becomes the value stored under the key.

## Criterion, registered before any measurement

All numbers are provisional until a first CI measurement of both programs has
been read; a change of a number is made on the evidence of that measurement and
recorded here.

| Program | Total duration of the run (spawn of the first context to the join of the second) | Why |
|---|---|---|
| overlap | at least 10 ms and at most 15 ms | one computation takes 10 ms, two one after the other take 20 ms |
| held | at least 18 ms | two computations that exclude one another take 20 ms |

The overlap program also exits nonzero when the log does not hold exactly two
records, when the sequence numbers are not 0 and 1 in log order, or when
replaying the log in sequence order does not give the final table. Status 0
needs all of these and the duration.

The overlap bound is read as the best of three runs, because a loaded hosted
runner can delay the start of the second context by milliseconds and this
costs the result only when it happens in all three. The held bound needs no
repeat: a lock held through the computation can only lengthen a run, and the
deadline-bounded computation makes 20 ms the least a serialized run takes.

## What a failure would mean

- The overlap program takes more than 15 ms in all three runs while its state
  checks pass: the two statements exclude one another for most of their
  computation. The compiler or runtime takes Meta earlier than the first use,
  for example at the start of the statement, or holds the map entry in a way
  that excludes another key. The lock timing the Firn command design relies on
  has changed, and the finding goes to the owner as a decision.
- The overlap program takes less than 10 ms: the computation did not run, and
  the duration says nothing about locks.
- The held program takes less than 18 ms: Meta is no longer held through the
  computation although the statement used it first, or the clock-bounded
  computation no longer runs inside the statement. The criterion then does not
  separate the two forms and the overlap result proves nothing.
- A state check fails (log length, sequence numbers, replay): a lost or
  duplicated update, which is a correctness defect whatever the timing.

## Limits of the result

It shows two statements on two drivers with a ten-millisecond computation, one
shared Meta and two keys. It does not measure the cost of the Meta commit
itself, scaling past two drivers, or statements that reach Meta in the middle
of their block. A host with one core runs both drivers on that core: the
overlap program can still pass there, because each computation is bounded by
the clock and not by the work it does, while the held program cannot.
