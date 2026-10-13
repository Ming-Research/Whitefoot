# Source positions in a deadlock report

## Question and scope

Can a deadlock report retain every waiting context and its source site without
measurable cost on ordinary spawn/finish? The roster exists to report every
waiting context, including a parent joining a producer before its consumer
has started. A named spawn inside a counted loop's body joins at the end of
that iteration when no earlier statement names its binding or leaves its
block [WAIT-3]. A producer filling a capacity-eight queue can therefore stop
at its ninth insertion while its parent waits at that implicit join.
[WAIT-2](../../../spec/kernel-spec.md) permits a report for that execution;
the report is not a program outcome and changes no source judgment.

The first implementation's spawn and finish paths took one shared spin lock. A
single spawner cannot expose cache-line contention among drivers that spawn
and finish children simultaneously. The comparison therefore includes both a
single-spawner input and independent concurrent spawners, each retaining its
fixed work across panels through eight physical performance cores and their
sixteen hyperthreads.

This record owns the synchronization argument and the comparison protocol.
It is kept with the runtime mechanism and updated if that mechanism changes.
The eager-roster pilot below is an observation, not acceptance evidence for
the revised mechanism. The cost criterion was recorded before measurement
and remains unchanged; the [full comparison result](#full-comparison-result)
applies it to the revised mechanism.

## Selected mechanism and alternatives

The root is registered once at startup as context 0. A child joins the global
roster only when its first guard or join actually parks, after the immediately
satisfied and early-wake exits have been excluded. Membership and identity
then persist through wake, repark and driver migration until finish removes
the child before any of its storage is reclaimed. Child diagnostic identities
therefore follow first-park order, not spawn order. They distinguish contexts
within a report and are not a source-language identity.

Diagnostic fields follow the existing scheduler fields in the context's cold
tail. Allocation clears the old fields and the owning-roster marker, leaving
links, identity and site untouched until registration initializes them. No
path reads an unregistered child's cold fields. A child that never parks does
no roster insertion, identity update or removal, and acquires no diagnostic
lock; finish tests only its null marker. The owning-roster pointer still
allows a later per-driver representation. The global lock protects membership,
identities and lifetime during traversal.

Generated wait descriptions are NUL-terminated constants in the emitted
module's read-only data. A guard description names its atomic statement's
source position. A named join description names its join position, its kind,
and the spawn's `let` position. A child's first actual park initializes its
atomic site before publishing its roster links. Later parks, and root parks,
write the site with a relaxed atomic store before the existing scheduler
publications; an immediately satisfied wait writes nothing and performs no
registration check. Wake does not clear the slot or remove membership.
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

At a genuine all-stuck snapshot every live context is registered: ready
contexts are excluded by ready counts, running contexts by active invocations,
unfinished adoption by ingress/moving publications, and host waits by their
counts. A shared-object acquisition has a running or runnable holder, handoff
owner, reserved guard retry or claimant under the existing lock-order and
admission rules, so it cannot strand an unregistered context at all-stuck. The
remaining guard/join waits have passed the registration point, including a false guard
whose watch contains no unit. No enumeration of shared objects is required.

The detector first uses the existing atomic all-stuck test, then takes the
roster lock and repeats that test and the root-completion check before
traversing. It loads slots with acquire ordering while the lock excludes
unlink and reclamation. Both decisions call the sole predicate
`wf_all_contexts_inactive(const wf_driver *self)`. Its caller owns the service
role and has announced idleness. Nonzero means a consistent all-stuck snapshot:
no active invocation, ready work, unfinished adoption, transfer or outstanding
host operation, with other published drivers idle. Nonzero also acquires the
completed parks' site publications; it is not merely a count observation.
Root completion is checked separately before reporting. False requires retry
or continued scheduling and grants no diagnostic snapshot.

The predicate owns the current counter-reading and consistency algorithm.
Report callers do not read the process-wide active/change counters directly.
The runtime line's proposed per-thread collect-scan-collect design is not
selected here; whichever representation is approved must meet this same
predicate and publication contract. Existing activity hints for driver sleep
and the separate teardown-quiescence check are not deadlock decisions.

The current implementation's publication argument is: the parking context's
relaxed slot store is sequenced before its invocation's existing sequentially
consistent decrement of the active-invocation count; the detector's acquire
read of that count at zero observes the completed invocations through the
count's read-modify-write release sequence. The ingress and idle publications
and the unchanged double collection additionally exclude unfinished adoption,
ready work and a concurrent scheduling transition. Thus the slot store happens
before the diagnostic read. First initialization is also published by the
roster lock; its cold atomic storage has no reader before that publication.
The acquire slot load alone does not turn a
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
- Eager registration at every spawn: the pilot exposes a one-driver cost while
  never-waiting leaves cannot appear in an all-stuck report. Their two roster
  lock acquisitions and link updates are unnecessary for that report.
- Per-driver locked rosters as the first cost repair: they reduce cross-driver
  contention but retain two uncontended lock acquisitions for every leaf, so
  they do not remove the one-driver work exposed by the pilot.
- An unlocked roster owned by a logical driver: a displaced executor can still
  spawn while a replacement executor serves that role, and a migrated child
  can finish on another driver. Driver identity alone does not provide one
  writer; a new transfer protocol would change scheduling responsibilities.
- Recovering the live set only from existing queues at report time: ready
  queues are empty at all-stuck, parked/polling lists index host waits, join
  groups have no global index, and a guard watch may contain no unit. Those
  structures do not provide an enumerable root for every waiter.

## Eager-roster pilot and source attribution

The owner supplied the summary of [run 38430959942](https://github.com/Ming-Research/Whitefoot/actions/runs/38430959942),
candidate `f4e847f80`. It records 122.987 seconds for the protocol, 114.491
seconds for construction and 5.229 seconds of measured native runtime. The
qualified masks were `0`, `0,2,4,6`, `0,2,4,6,8,10,12,14`, and those eight
CPUs plus `1,3,5,7,9,11,13,15`. Each panel retained three rotating rounds.

| One-driver workload | Branch/base rounds | Twin/base rounds | Median overhead | Twin spread | Recorded rejection threshold |
|---|---|---|---:|---:|---:|
| spawn | 1.026301880, 1.091742009, 1.109940790 | 0.969619379, 1.014130698, 1.001589027 | 9.174201% | 3.038062% | 4.038062% |
| concurrent spawn | 1.089759305, 1.082740699, 1.083772752 | 1.009460137, 1.002316610, 0.996752030 | 8.377275% | 0.946014% | 1.946014% |

The one-driver medians exceed the recorded numerical thresholds, but pilot
mode deliberately gives no acceptance verdict. Spawn-heavy samples ranged
from about 3 to 190 milliseconds. The wider panels are too noisy to resolve
the cost: their twin spreads were 3.191421%, 34.866915%, 6.234346% for spawn
at four/eight/sixteen drivers, and 20.433795%, 10.508141%, 3.152813% for
concurrent spawn. The observation is whole-process wall time including launch,
not a direct per-spawn CPU measurement. It supports prioritizing spawn/finish
work; it does not isolate the fraction caused by locks, layout or codegen.

The local source audit compares the candidate against `4cb31bafd`, its
merge-base with the local main tracking ref and the parent of the first
diagnostic commit. The supplied summary does not include the bundle's resolved
base manifest; this source base is not an independent verification of that
artifact. The diagnostic additions in `bridge.c` were:

| Path | Additional operations in the eager implementation |
|---|---|
| Prepare | The existing pool allocation requested a context 40 bytes larger, and the existing memset cleared 40 more bytes; no additional allocation call. |
| Spawn | One acquire atomic-flag TAS loop and release clear; ID-limit comparison; ordinary ID increment and context ID store; four mandatory pointer stores (owner, next, predecessor, head), plus the former head's predecessor store when nonempty. |
| Finish | One acquire atomic-flag TAS loop and release clear; predecessor-link and null-owner stores, plus the successor's predecessor store when present. |
| Root | One insertion at begin and one removal at completion, outside per-child cost. |
| Actual join/guard park | One relaxed atomic site-pointer store; an additional static-pointer call argument/address operand. Immediately answered joins and early-woken guards do not write a site. |
| Wake and scheduler publication | No new slot clear, scheduler counter, RMW or independent fence. Existing counters carry site publication. |

Thus each child added at least two successful diagnostic TAS operations and
two release clears, with unbounded retry attempts under contention. Both paths
write the shared roster's lock/head/identity storage and links in the context;
insertion/removal can also write a neighboring live context's diagnostic links.
The diagnostic prefix shared those contexts' first cache line with scheduling
fields, and moved ready-queue `next`/`previous` from offsets 16/24 to 56/64.

For the ordinary 64-bit layout with guard counters disabled, source layout
calculations give context sizes 936→976 bytes on Linux, 1192→1232 on Windows,
and 1704→1744 on macOS. Pool classes remain 1024, 2048 and 2048 bytes,
respectively; the frame-arena allocation formula is unchanged. On Linux,
pool-aligned context clearing spans sixteen 64-byte cache lines instead of
fifteen. These are source/ABI calculations, not a compiled size probe. Static
descriptor arguments add no emitter value or declared frame slot, but actual
coroutine frame sizes, register spills, machine instructions and coherence
traffic require emitted assembly or a CI probe; none was obtained here.

The repair removes roster work for never-parking leaves and restores the old
scheduler-field offsets. With the cold tail, clearing reaches only the null
roster marker: on Linux the calculated initialized prefix is 944 bytes rather
than 976, back within fifteen cache lines. Total context size and pool class
remain unchanged from the eager revision. First park initializes the cold
site and adds the context once; subsequent parks neither relink nor allocate
identities. Never-registered finish adds only one predictable marker check.
The reduced work is an attribution hypothesis with an inspectable source
change, not a claim that the entire 8–9% loss is explained or removed. Static
site arguments and actual-park pointer writes still remain.

Repeat the same-source base/twin/candidate comparison after correctness CI,
keeping the pre-registered criterion below. Use equal batches and more rounds
chosen from pilot duration/spread to resolve one percent; retain individual
executions and process-launch cost. That comparison ran afterwards; its
result is recorded in the [full comparison result](#full-comparison-result).

## Roster-disabled attribution arm

The fourth arm, `site-only`, is a separate build of the exact candidate
revision with only this prefix added to its detached checkout's `bridge.c`
before Cargo embeds that source:

```c
#define WF_DEADLOCK_SITES_ATTRIBUTION_NO_ROSTER 1
```

The helper checks the original bytes against the candidate, restricts the
changed path to that one source, and preserves the exact prefix, original and
derived C bytes, unified patch and their hashes in the bundle. It uses an
independent compiler target directory and compiles the same unmodified
fixtures as the other arms. The ordinary checkout, request settings, emitter
and source site parameters are unchanged. This avoids adding a writer-facing
compiler option or a native compiler wrapper.

The define defaults to zero for normal builds. With value one it omits roster
insertion, removal, identities, locks and the finish marker check, while
retaining context layout, allocation initialization range, static descriptor
arguments and one pointer write at every actual guard/join park. There is no
site reader in this arm. Exclusive context execution permits repeated
`atomic_init` through a volatile-qualified atomic pointer, which initializes
untouched storage and retains the write without a first-use branch. Its exact
generated-instruction equality with the normal relaxed store requires CI
inspection; it is not asserted from C source. Immediately answered and
early-woken waits still perform no site operation.

This arm is attribution only. A deadlock still invokes the same inactivity
predicate and prints the original stop summary, but no waiter inventory;
this deliberately disabled report is never the production candidate or a
correctness/acceptance result. The normal branch retains all waiter details.
The control also retains cold-layout and other branch compiler changes, so
`site-only / base` measures residual parameter/site/layout/codegen costs
together, not a pure parameter-passing estimate. `branch / site-only` is
intended to attribute incremental roster/check cost with those features
retained; until generated-store equivalence is checked, it also includes any
codegen difference between the volatile initialization and relaxed store. Both ratios
are reported per matched round and as panel medians with matched counts.

The unchanged rejection criterion uses only base, twin and normal branch.
Complete production triples remain reportable when an attribution sample is
missing, and attribution pairs remain reportable independently of a missing
third production sample. No samples are silently discarded. No attribution
build, generated-code comparison or measurement had been run when this
section was written; the arm's measured result is in the
[full comparison result](#full-comparison-result).

## Comparison and rejection criterion, before measurement

Run only through explicit CI on the organization's i9-14900K self-hosted
runner (`self-hosted`, `14900k`), after confirming it is idle and coordinating
the run. Use the same toolchain, release compilation settings, native host
route, driver count, CPU affinity and source bytes for the production builds
and their attribution control:

1. Base: the merge-base of the candidate revision and the fetched main head.
2. Twin: a second independent build of that same base revision.
3. Branch: this implementation's exact candidate revision.
4. Site-only: the same candidate with the recorded roster-disabled prefix,
   built independently and used only for attribution as described above.

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
compiler builds and the attribution build, including spawn fixtures absent
from the base tree.
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
the runner as a Hyper-V virtual machine. The supplied pilot summary records
qualified physical masks for that run; the workflow description alone does
not establish current placement. Each repeat must independently qualify the
exposed topology; guest CPU numbers alone cannot supply the eight-P-core or
sixteen-sibling measurement. A qualification failure is retained as such
rather than producing cost ratios.

First run the smallest useful sample of every side and workload, time it and
inspect its spread. Select a fixed amount of work and repetitions from that
pilot, sufficient to resolve
one percent, before the comparison. If a single spawn fixture execution is
too short, time equal batches of unchanged executions and retain their launch
cost in every side; report that limitation rather than attributing all elapsed
time to the roster. For the TCP panel, pin client concurrency, connections,
payload size and exchanges before recording the panel, and include startup
and shutdown identically on every side.

Interleave the four builds in cyclic orders (base/twin/branch/site-only,
twin/branch/site-only/base, branch/site-only/base/twin,
site-only/base/twin/branch) rather than completing one side first.
Compare matched wall times for equal work; retain all samples and report
the branch/base ratios alongside twin/base ratios. Repeat or lengthen only
when the twin's spread prevents a decision. Investigate a repeatable branch
loss with runtime profiles and roster-lock contention evidence; a fixture or
workload source change, or different work amount, cannot serve as attribution.

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

If any full spawn-heavy panel still fails after lazy registration, the
contention contingency is per-driver lazy rosters and a
repeat of every panel with the same sources, work and criterion. A context is
assigned its owning roster at first registration; add and remove touch only that
driver's roster lock. Context migration changes its executor, not its roster
ownership, so it adds no roster operation. After establishing all-stuck, the
reporter locks each roster in turn to protect membership and lifetime while
reading its slots, retaining the scheduler-counter publication argument.
Profiles and roster-lock contention evidence distinguish a shared-cache-line
loss from another cause before attributing the failure. This is a contingency,
not an implemented runtime change or a measured result; the full comparison
rejected no spawn panel, so it was not triggered.

## Full comparison result

The full comparison is [run 38471254657](https://github.com/Ming-Research/Whitefoot/actions/runs/38471254657),
job 115550595022, on the organization's i9-14900K self-hosted runner
(`self-hosted`, `14900k`), 21:31:30Z to 22:42:03Z on 2026-10-13. The measured
candidate is commit `cfb5d9ff8`; the temporary workflow was added there and
removed at `446524e29`, and the runtime and compiler code is identical to the
head apart from the later merge of main. The harness reports 4222.133 s
elapsed, 155.139 s of recorded build stages and 3990.791 s of measured native
runtime. Its settings were `DEADLOCK_MODE=full`, nine rounds, work 100,
client threads 4, 64 bytes, 100 exchanges per connection for the fixed-exchange
panel, 64 connections and no 1,024-connection panel. Nine rounds is not a
multiple of four, so the planned arm orders leave one unmatched remainder:
base took the first position three times, twin the second three times, branch
the third three times and site-only the fourth three times, and every other
arm-position pair twice. Each panel retained all nine matched rounds.

The qualified placement was identical for every arm and TCP client:

| Drivers | Physical P cores | Threads per core | CPU mask |
|---:|---:|---:|---|
| 1 | 1 | 1 | 0 |
| 4 | 4 | 1 | 0,2,4,6 |
| 8 | 8 | 1 | 0,2,4,6,8,10,12,14 |
| 16 | 8 | 2 | 0,2,4,6,8,10,12,14,1,3,5,7,9,11,13,15 |

Per-panel result, as printed by the harness (median of nine matched rounds of
`branch / base - 1`; twin spread is the largest absolute `twin / base - 1`;
threshold is `0.01 + twin spread`):

| Panel | Drivers | Min sample s | Median branch/base − 1 | Twin spread | Rejection threshold | Result |
|---|---:|---:|---:|---:|---:|---|
| spawn 0 | 1 | 0.347250 | -0.278709% | 1.375340% | 2.375340% | inconclusive |
| spawn 0 | 4 | 0.603585 | 1.305828% | 2.693018% | 3.693018% | inconclusive |
| spawn 0 | 8 | 0.764191 | -0.265599% | 1.587711% | 2.587711% | inconclusive |
| spawn 0 | 16 | 0.918045 | -0.706606% | 1.027516% | 2.027516% | within recorded noise bound |
| spawn-concurrent 0 | 1 | 2.369429 | 0.394366% | 0.495998% | 1.495998% | within recorded noise bound |
| spawn-concurrent 0 | 4 | 7.867055 | -0.306518% | 1.948417% | 2.948417% | inconclusive |
| spawn-concurrent 0 | 8 | 13.533753 | 0.165132% | 1.061466% | 2.061466% | inconclusive |
| spawn-concurrent 0 | 16 | 19.432615 | -1.190509% | 1.128155% | 2.128155% | within recorded noise bound |
| tcp-churn 64 | 1 | 7.623886 | 0.102390% | 0.998929% | outside veto | TCP evidence; outside spawn veto |
| tcp-churn 64 | 4 | 7.752322 | 0.095636% | 1.191932% | outside veto | TCP evidence; outside spawn veto |
| tcp-churn 64 | 8 | 7.794649 | 0.054796% | 0.589798% | outside veto | TCP evidence; outside spawn veto |
| tcp-churn 64 | 16 | 7.869319 | 0.040312% | 0.423339% | outside veto | TCP evidence; outside spawn veto |
| tcp-exchange 64 | 1 | 3.689409 | 0.119303% | 28.656793% | outside veto | TCP evidence; outside spawn veto |
| tcp-exchange 64 | 4 | 7.656015 | -0.093842% | 2.661951% | outside veto | TCP evidence; outside spawn veto |
| tcp-exchange 64 | 8 | 7.946598 | -0.133021% | 1.137056% | outside veto | TCP evidence; outside spawn veto |
| tcp-exchange 64 | 16 | 8.141232 | 0.117390% | 0.589366% | outside veto | TCP evidence; outside spawn veto |

Verdict against the criterion recorded above. No spawn-heavy panel rejects:
the largest median overhead is +1.305828% at four drivers on the
single-spawner fixture, below that panel's threshold of 3.693018%. Three panels
(single-spawner at sixteen drivers, concurrent spawners at one and sixteen
drivers) meet the finite within-bound condition
`median(overhead) + twin spread <= 0.01`: the branch's median wall time is
within the one-percent bound of base there, for this fixture, these nine
rounds and this placement. Five panels (single-spawner at one, four and eight
drivers; concurrent spawners at four and eight drivers) are inconclusive: the
twin's own spread exceeds the margin that would show a median overhead of at
most one percent, so these panels neither show the bound nor an overhead above
the threshold. For those five the record says only that the branch was not
rejected; it makes no claim of equality with base. The TCP panels are outside
the veto line. Their medians are within 0.14% of base in either direction,
with TCP startup and shutdown included in the wall time, but the one-driver
fixed-exchange panel has a twin spread of 28.656793% (its first three rounds
show branch/base 1.41, 0.81 and 1.14) and supports no conclusion beyond the
run completing correctly. The bundle also records client exchange rates; no
verdict uses them. Results are no wider than these two spawn fixtures, one
machine and one nine-round run.

Attribution-only arm (`site-only`, roster work disabled). Panel medians over
nine matched rounds; all counts were 9/9:

| Panel | Drivers | Median site-only/base | Median branch/site-only |
|---|---:|---:|---:|
| spawn 0 | 1 | 1.000984 | 0.998072 |
| spawn 0 | 4 | 1.028970 | 0.986006 |
| spawn 0 | 8 | 1.035801 | 0.959243 |
| spawn 0 | 16 | 1.056589 | 0.937297 |
| spawn-concurrent 0 | 1 | 1.004439 | 0.999390 |
| spawn-concurrent 0 | 4 | 0.992044 | 1.004745 |
| spawn-concurrent 0 | 8 | 1.000141 | 1.003376 |
| spawn-concurrent 0 | 16 | 0.993189 | 0.997318 |

The TCP panels' site-only medians were within 0.4% of base; the per-round
values are in the bundle. On the single-spawner fixture the site-only build was
2.9%, 3.6% and 5.7% slower than base at four, eight and sixteen drivers, while
the full branch matched base within the criterion's bound (medians +1.3%,
-0.3% and -0.7%); the full branch, which does strictly more diagnostic work
than site-only, was 1.4%, 4.1% and 6.3% faster than site-only at the same
drivers. The record already warned that site-only is not a parameter-only
control, since it differs from the branch in the compiled bridge source and
retains its own ABI, layout and site-store costs. This result shows that it
does not isolate the roster's cost, so it supports no attribution claim. The
cause of its slowdown on the single-spawner fixture is unexplained: no
profile, assembly comparison or repeat was run, and this record offers no
hypothesis.

Remaining limits. The eager-roster pilot was not rerun, so this result gives
the repaired revision's cost relative to base, not its improvement over the
eager revision. Resolving the five inconclusive panels would need lower twin
noise or more rounds, as the criterion's "repeat or lengthen only when the
twin's spread prevents a decision" allows. Windows and macOS cost are not
measured.

## Temporary measurement harness

The explicitly requested `.github/workflows/deadlock-sites-cost.yml` owns the
temporary `.github/deadlock-sites-cost.py` helper and request settings in
`.github/deadlock-sites-cost.env`. The owner changed the trigger to a push
that edits that request file because a workflow absent from main could not
be dispatched. This repair retains that trigger and leaves its settings
untouched. The workflow and request file were deleted after the
full run started and the helper before the branch was marked ready; all
three remain at commit 446524e29, the measured candidate's harness. The run
log and uploaded measurement bundle of run 38471254657 remain the evidence
for this investigation. The helper reuses the candidate revision's unchanged
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

Pilot mode uses four rounds to exercise each rotating order with minimal
work: one execution per side and panel, 64 TCP connections by default, two
exchanges for the fixed-exchange workload and one for short connections. It
reports duration and spread without an acceptance verdict. Full mode uses
explicitly selected rounds and work after inspecting that pilot. The
full round count must be at least four; multiples of four balance every arm's
position, and other counts preserve and report their remainder. The existing
request default of nine is not silently changed. The
bundle keeps all observations, including failed or partial samples, build
times, exact source bytes, resolved revisions, toolchain and affinity settings,
per-round ratios and the recorded rejection criterion. One outer
`run-check.pl` invocation holds the shared host lock through construction and
measurement; an occupied lock stops the run and is never automatically
removed. This manually invoked research harness is not a gate dependency.

The existing request-file push trigger has a static-boundary prerequisite:
`.github/check-research-inputs.py` recognizes only dispatch-only workflows as
manual research, so it will flag the workflow's literal research-client path.
This fourth-arm change does not hide that input or alter the checker's
boundary. The explicit request trigger needs a recognized boundary before
the static gate can accept it; no checker was executed here.

## Correctness evidence

Maintained compiler/program/runtime tests, independently of this record,
check the capacity-eight producer/consumer deadlock, source positions and
join kinds, immediate waits retaining registered sites, and successful
context batches. The lazy-roster changes add first-park/zero-unit coverage,
never-parking launch/finish checks, retained membership across wake/repark,
unlink-before-destroy and same-block reuse, and registered/unregistered
displaced WAIT/READY/COMPLETE schedules. Report expectations still require
every exact source description, distinct identities and root identity zero;
only child spawn-order numbering is retired. Wrong-report controls reject
missing entries, duplicated IDs, wrong sites and a wrong root identity.

Synthetic stack contexts in cancellation, timer-migration and early-wake
fixtures now remove their lazy registration before leaving scope. This fixes
a test lifetime obligation introduced by registration at the actual park
rather than only at the native launcher. The native reader fixture no longer
manually registers a context that waits only for a host result. Ordinary
tests do not import this directory. Their validation is the gate on the pushed head,
recorded in the pull request; the cost run above checked only fixture results
and driver counts and validates none of these tests. Linux io_uring and Windows IOCP must compile the same portable C.

The previous runtime correctness review covered eager membership. It must
redo live-set completeness at all-stuck, initialization and non-reading of
unregistered cold fields, first-site publication through the roster lock,
later-site publication through active/ingress counters, early-wake and
displaced-adoption races, reclamation and pool reuse, and the new
`watch_lock` to roster-lock nesting. Static descriptor storage and the
claim-tail/reservation low-bit representation are unchanged; the source
positions and report-kind regressions must still pass. A fresh gate and both
host builds establish correctness; the full comparison recorded below, with the
same sources and the unchanged veto line, establishes cost within its stated
limits. The pilot is not a cost result for the repaired revision.

The predicate-boundary cases exercise ready work, host waits, transfers and
a published driver not yet included in the driver count, restoring each
state and requiring the positive result so an always-false predicate fails.
Active and ingress exclusions retain the displaced-executor schedules. All
deadlock assertions use the shared predicate; teardown checks remain separate.

## Existing join-placement discrepancy found during implementation

Code inspection found a pre-existing mismatch: the checked plan used
`before: None` for a block-end join, but lowering's `await_contexts_before`
considered that plan due before any next statement in the block. In a loop
body with an unused named spawn followed by an unrelated statement, that
lowering placed the join before the statement rather than at the closing
brace [WAIT-3]. The diagnostic metadata described that earlier position as
"implicit join before the next statement". This inspection did not run a
compiler to confirm the earlier behavior.

[The block-end join correction](https://github.com/Ming-Research/Whitefoot/pull/451)
has merged into main and is present in this worktree. A `before: None` plan
now joins at the owning block's closing brace even when an unrelated sibling
follows the spawn. Diagnostic metadata follows the plan directly and names
the end of the loop iteration or ordinary block. The lowering regression
requires the unrelated sibling to finish before the await and checks the
exact closing-brace description. This description repair changes no join
schedule or language rule.

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
runtime guard behavior is unchanged. The earlier failed run is evidence of
the original faults. It is not evidence for the later lazy-roster repair or
its cost.
