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

The global roster's spawn and finish paths take one shared spin lock. A
single spawner cannot expose cache-line contention among drivers that spawn
and finish children simultaneously. The comparison therefore includes both a
single-spawner input and independent concurrent spawners, each retaining its
fixed work across panels through eight physical performance cores and their
sixteen hyperthreads.

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

The cached printing model retains source descriptions as literal parts and
position parts identified by qualified module name, logical file, line and
column. Graph row ordinals are not persistent source identity. A cache
hit binds those positions to the current invocation's display paths and
regenerates only the tagged wait constants and developer lock comments;
emitted functions and their cache key remain independent of path spelling.
The resulting descriptors are still ordinary read-only static strings, so
this binding introduces no runtime lookup, allocation or ABI change.
Canonicalizing display labels is not an alternative source identity: an
in-memory invocation may name virtual files that do not exist on the host.
Typed template binding never searches or substitutes arbitrary LLVM text.
Atomic target-binder descriptions live outside checked function bodies so
their display bindings cannot enter proof-receipt identity.
Ledger ordering and deduplication use this logical identity and numeric
coordinates, so a display-path change cannot reorder a cached comment.
A valid library module can also contain a refused executable-caller diagnostic
with source paths in its JSON comment. That recorded diagnostic retains an
explicit source-reading constraint: reuse requires its original writer paths
to remain unchanged. Otherwise the normal fresh build renders the diagnostic
again. No arbitrary JSON or LLVM text is rebound.

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

1. Base: the merge-base of the candidate revision and the fetched main head.
2. Twin: a second independent build of that same base revision.
3. Branch: this implementation's exact candidate revision.

Record the resolved revisions and all compiler/runtime settings with the
results. Build time is recorded separately and excluded from runtime time.
The comparison has three maintained program inputs, all compiled with `--par`:

- `tests/programs/context_starts.wf`: 10,000 tiny named contexts, each joined
  at its result's first use, followed by 100 batches of 100 unnamed contexts
  joined at each batch's activation exit. The expected sum of 49,995,000 and
  total batch count of 10,000 detect wrong results.
  This fixture isolates frequent spawn/finish and includes overlapping live
  contexts; no measured side gets a source variant.
- `tests/programs/context_starts_concurrent.wf`: 16 top-level contexts each
  perform 10,000 tiny named spawn/result-use joins. Each registers at a
  startup barrier and waits until all 16 have arrived, allowing the drivers
  to take ready spawners together. Each spawner keeps its sum locally and
  publishes once at completion; no shared-state access serializes its inner
  loop. The expected aggregate sum of 799,920,000 and completion count of 16
  detect wrong results. The same 16 spawners and work run in every panel,
  exposing concurrent roster add/remove traffic that one spawner cannot.
- `tests/programs/tcp_contexts.wf`: one context per accepted TCP connection,
  each echoing through its own buffer. Use a loopback client on the same
  runner, the same connection count and byte stream on all sides, and verify
  every echoed byte and the server's zero exit. Start with 64 connections,
  then include 1,024 connections if the pilot is practical. Run both fixed
  numbers of exchanges per connection and connection churn, so a long-lived
  connection does not hide spawn/finish contention.

Take all three fixtures from the candidate revision and preserve their exact
bytes in the comparison bundle. Compile those same files with all three
compiler builds, including the spawn fixtures absent from the base tree.
All three fixtures use unchanged language forms; this is a same-source
comparison, not a comparison of different test directories in the checkouts.

Use one, four, eight and sixteen drivers in separate panels. Discover the
runner's performance-core CPUs through `/sys/devices/cpu_core/cpus` and group
them by physical package and core ID, checking reciprocal
`thread_siblings_list` entries against online CPUs and the allowed affinity.
This reuses io-bench's sysfs core grouping and `taskset` placement mechanism;
unlike its client/server placement, the eight-core panel needs all eight
performance cores and does not reserve one for the OS. The one/four/eight
panels use one logical CPU from each of one/four/eight distinct performance
cores. The sixteen-driver panel adds the partner hyperthread of each of those
same eight cores. Every measured side in a panel gets the same mask. Record
the exposed topology and all selected masks, rather than assuming CPU numbers
0 through 15 are performance cores. An explicit ordered CPU list is still
checked against that topology. Missing physical topology, guest-only topology
or an affinity that cannot supply all eight performance cores and their
partners stops the comparison before construction; no logical-only panel is
reported as a physical-core measurement. TCP clients share their panel's mask
as in the temporary harness, so their competing CPU use is an explicit part
of that workload and is identical across arms.

The [io-bench workflow](../../../.github/workflows/io-bench.yml) describes
the runner as a Hyper-V virtual machine. That description does not establish
the requested physical-core placement. The runner must expose qualified
physical topology for this protocol; guest CPU numbers alone cannot supply
the eight-P-core or sixteen-sibling measurement. Current host qualification
remains unverified until dispatch, and a qualification failure is retained as
such rather than producing cost ratios.

First run the smallest useful sample of every side and workload, time it and
inspect its spread. Select a
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

**Rejection criterion:** apply the same criterion separately to both
spawn-heavy fixtures at each of one, four, eight and sixteen drivers. Reject
this branch's mechanism if any such full panel's spawn-heavy wall
time rises by more than 1% over base beyond the twin's spread. For each
matched round let the overhead be `branch / base - 1`; define the twin spread
as the largest absolute `twin / base - 1` in the valid panel. The criterion is
`median(overhead) > 0.01 + twin_spread`; a noisy panel that cannot resolve the
threshold is inconclusive, not a pass. TCP results separately state the effect
on equal-work wall time and throughput; they do not replace the spawn-heavy
acceptance criterion.

If any spawn-heavy panel fails, the contingency is per-driver rosters and a
repeat of every panel with the same sources, work and criterion. A context is
assigned its owning roster at creation; add and remove touch only that
driver's roster lock. Context migration changes its executor, not its roster
ownership, so it adds no roster operation. After establishing all-stuck, the
reporter locks each roster in turn to protect membership and lifetime while
reading its slots, retaining the scheduler-counter publication argument.
Profiles and roster-lock contention evidence distinguish a shared-cache-line
loss from another cause before attributing the failure. This is a contingency,
not an implemented runtime change or a measured result.

## Temporary measurement harness

The dispatch-only `.github/workflows/deadlock-sites-cost.yml` owns the
temporary `.github/deadlock-sites-cost.py` helper. Delete both before the
branch is ready; retain the uploaded measurement bundle as the evidence for
this investigation. The helper reuses the candidate revision's unchanged
`research/experiments/io-completion-bench/netload.c` loopback client, which
checks every echoed byte. It observes listener readiness without connecting,
so readiness does not consume one of the server's counted connections.
TCP wall time includes server startup, client execution and successful server
shutdown on every side. Short connections use one exchange per connection;
the fixed-exchange panel uses the recorded exchange count.

An untimed preflight compiles the candidate's unchanged
`tests/programs/driver_count.wf` with each compiler and requires its reported
count before and after spawning to match each panel. A host that falls back
to one driver because its root io_uring is unavailable cannot supply the
four/eight/sixteen-driver comparisons; the harness stops rather than
mislabeling that run.

Pilot mode uses three rounds to exercise each rotating order with minimal
work: one execution per side and panel, 64 TCP connections by default, two
exchanges for the fixed-exchange workload and one for short connections. It
reports duration and spread without an acceptance verdict. Full mode uses
explicitly selected rounds and work after inspecting that pilot. The
bundle keeps all observations, including failed or partial samples, build
times, exact source bytes, resolved revisions, toolchain and affinity settings,
per-round ratios and the recorded rejection criterion. One outer
`run-check.pl` invocation holds the shared host lock through construction and
measurement; an occupied lock stops the run and is never automatically
removed. This manually invoked research harness is not a gate dependency.

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

## Cache and fixture corrections from CI

The initial implementation keyed the emitted entry module by display paths.
The [unit CI run](https://github.com/Ming-Research/Whitefoot/actions/runs/38407483612/job/115330922786)
rejected this design: the existing same-file path-spelling test missed
the cache, and relocation of the atomic fixture recorded 67 proofs instead
of retaining the previous 66. Inspection traced the extra proof to an
atomic target's lock-ledger display string inside the analyzed body's debug
rendering. Both issues are addressed by the structured printing-model
binding above and by keeping target-binder presentation outside proof bodies.
The relocation check now requires cached-code reuse, unchanged proof counters,
and complete equality with a fresh emission using the new paths. A separate
forced frontend check protects proof reuse independently of emitted-module
reuse. These are correctness checks, not a performance measurement.

The early-watch fixture also initially notified from the registering context,
whose false-guard unwind deliberately suppresses its own notifications. The
fixture now simulates a different writer and verifies the watch was actually
woken before checking the immediate park response and retained site. The
runtime guard behavior is unchanged. CI verification of these corrections is
pending; the earlier failed run is evidence of the original faults, not a
passing result for the corrections.
