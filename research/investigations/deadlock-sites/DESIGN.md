# Source positions in a deadlock report

## Question and scope

Does linking every live context into a roster at spawn and unlinking it at
finish cost measurable runtime time? The roster exists to report every
waiting context, including a parent joining a producer before its consumer
has started. A named spawn inside a counted loop's body joins at the end of
that iteration when no earlier statement names its binding or leaves its
block [WAIT-3]. A producer filling a capacity-eight queue can therefore stop
at its ninth insertion while its parent waits at that implicit join.
[WAIT-2](../../../spec/kernel-spec.md) permits a report for that execution;
the report is not a program outcome and changes no source judgment.

This record owns the synchronization argument and the comparison protocol.
It is kept with the runtime mechanism and updated if that mechanism changes.
No timing has been run. The cost criterion below is recorded before any
measurement; the implementation is not evidence that it meets the criterion.

## Selected mechanism and alternatives

Each context has intrusive roster links, its owning roster, a report identity,
and one atomic pointer to its current wait-site description. Initially one
global roster holds the root and every spawned context. Spawn links a context
before publishing it as runnable; finish removes it before reclaiming any of
its storage. The owning-roster pointer makes a later per-driver representation
possible without treating a context's current executor thread as its owner.
The roster lock protects membership, identities and lifetime during traversal.

Generated wait descriptions are NUL-terminated constants in the emitted
module's read-only data. A guard description names its atomic statement's
source position. A named join description names its join position, its kind,
and the spawn's `let` position. An actually parking wait writes that pointer
with a relaxed atomic store before the existing scheduler publications; an
immediately satisfied wait writes nothing. Wake does not clear the slot.
Consequently a slot is meaningful for the deadlock report after the detector
has established that every live context is waiting, rather than a general
query of what a running context is doing.

The detector first uses the existing atomic all-stuck test, then takes the
roster lock and repeats that test and the root-completion check before
traversing. It loads slots with acquire ordering while the lock excludes
unlink and reclamation. The publication argument is: the parking context's
relaxed slot store is sequenced before its invocation's existing sequentially
consistent decrement of the active-invocation count; the detector's acquire
read of that count at zero observes the completed invocations through the
count's read-modify-write release sequence. The ingress and idle publications
and the unchanged double collection additionally exclude unfinished adoption,
ready work and a concurrent scheduling transition. Thus the slot store happens
before the diagnostic read. The acquire slot load alone does not turn a
relaxed store into a release publication; ordering comes from the scheduler
counters. This argument must be reviewed against the implementation's actual
publication paths, including a displaced executor's ingress adoption.

Rejected alternatives:

- A list of only parked contexts, linked and unlinked on every park and wake:
  parks and wakes far outnumber spawns and finishes in an I/O server, and guard
  waits use per-object locks with no global entry point.
- Reporting only the detecting thread's own wait: it omits the other waiters
  needed to explain why the execution cannot proceed.
- A per-worker-thread slot: contexts usually outnumber worker threads, and a
  suspended context can later run on another executor.
- Keeping only a descriptor pointer without a live roster: static descriptor
  lifetime does not protect a reclaimed context or provide all the waiters.

## Comparison and rejection criterion, before measurement

Run only through explicit CI on the organization's i9-14900K self-hosted
runner (`self-hosted`, `14900k`), after confirming it is idle and coordinating
the run. Use the same toolchain, release compilation settings, native host
route, driver count, CPU affinity and source bytes for all three builds:

1. Base: the main revision on which this branch is based.
2. Twin: a second independent build of that same base revision.
3. Branch: this implementation's exact candidate revision.

Record the resolved revisions and all compiler/runtime settings with the
results. Build time is recorded separately and excluded from runtime time.
The comparison has two maintained program inputs, both compiled with `--par`:

- `tests/programs/context_starts.wf`: 10,000 tiny named contexts, each joined
  at its result's first use, followed by 100 batches of 100 unnamed contexts
  joined at each batch's activation exit. The expected sum of 49,995,000 and
  total batch count of 10,000 detect wrong results.
  This fixture isolates frequent spawn/finish and includes overlapping live
  contexts; no measured side gets a source variant.
- `tests/programs/tcp_contexts.wf`: one context per accepted TCP connection,
  each echoing through its own buffer. Use a loopback client on the same
  runner, the same connection count and byte stream on all sides, and verify
  every echoed byte and the server's zero exit. Start with 64 connections,
  then include 1,024 connections if the pilot is practical. Run both fixed
  numbers of exchanges per connection and connection churn, so a long-lived
  connection does not hide spawn/finish contention.

Take both fixtures from the candidate revision and preserve their exact bytes
in the comparison bundle. Compile those same files with all three compiler
builds, including the new spawn fixture that is absent from the base tree.
Both fixtures use unchanged language forms; this is a same-source comparison,
not a comparison of different test directories in the three checkouts.

Use one and four drivers in separate panels. First run the smallest useful
sample of every side and workload, time it and inspect its spread. Select a
fixed amount of work and repetitions from that pilot, sufficient to resolve
one percent, before the comparison. If a single spawn fixture execution is
too short, time equal batches of unchanged executions and retain their launch
cost in every side; report that limitation rather than attributing all elapsed
time to the roster. For the TCP panel, pin client concurrency, connections,
payload size and exchanges before recording the panel, and include startup
and shutdown identically on every side.

Interleave the three builds in rotating orders (base/twin/branch,
twin/branch/base, branch/base/twin) rather than completing one side first.
Compare matched wall times for equal work; retain all samples and report
the branch/base ratios alongside twin/base ratios. Repeat or lengthen only
when the twin's spread prevents a decision. Investigate a repeatable branch
loss with runtime profiles and roster-lock contention evidence; a source
change or different work amount cannot serve as attribution.

**Rejection criterion:** reject this branch's mechanism if spawn-heavy wall
time rises by more than 1% over base beyond the twin's spread. For each
matched round let the overhead be `branch / base - 1`; define the twin spread
as the largest absolute `twin / base - 1` in the valid panel. The criterion is
`median(overhead) > 0.01 + twin_spread`; a noisy panel that cannot resolve the
threshold is inconclusive, not a pass. If the global roster lock shows
contention, switch to per-driver rosters and repeat the same comparison and
criterion. TCP results separately state the effect on equal-work wall time
and throughput; they do not replace the spawn-heavy acceptance criterion.

## Correctness evidence and remaining work

Maintained compiler/program/runtime tests, independently of this record,
check the capacity-eight producer/consumer deadlock, source positions and
join kinds, immediate waits retaining their prior slots, roster removal before
reuse, and successful context batches. Ordinary tests do not import this
directory. Linux io_uring and Windows IOCP host builds must compile the same
portable bridge interface. CI validation and the runtime comparison remain
unperformed in this worktree; no test or performance result is claimed.

## Existing join-placement discrepancy found during implementation

Code inspection found a pre-existing mismatch, not confirmed by running a
compiler here: the checked plan uses `before: None` for a block-end join, but
lowering's `await_contexts_before` considers that plan due before any next
statement in the block. In a loop body with an unused named spawn followed
by an unrelated statement, the emitted join therefore precedes that
statement rather than the closing brace [WAIT-3]. The diagnostic metadata
reports the emitted wait position and says "implicit join before the next
statement" on this path; it does not change the plan or the emitted schedule.
When the spawn is the body's last statement, as in the capacity-eight queue
regression, the actual join is at the iteration's closing brace.

The discrepancy needs its own lowering correction and a witness whose
unrelated later statement enables the spawned context to finish. It is
deferred from this diagnostic-only change; the specification already fixes
the intended schedule, so no language amendment is proposed.
