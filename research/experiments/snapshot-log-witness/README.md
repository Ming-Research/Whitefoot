# Reconciled batched export: pre-registered protocol witness

## Question and status

Can current Whitefoot express a batched export of a live
`ConcurrentHashMap<u64>` whose reconciliation equals the map at its recorded
**end sequence S1**, while another context performs sets, increments, deletes
and reinsertions? Does plain scan plus command replay fail on the same trace?

Registered before execution, on `claude/snap-log-witness`, based on
`a69d5afc7d37f65dd34e58366ba9ad5c01b15a2c`. No compilation or execution had
been performed at registration. [Results](#results) records the subsequent
CI evidence and source audit; completed execution of the protocol remains unverified.
There is no specification, compiler, conformance-verdict or gate change.

This implements the expressibility experiment selected by decisions 1–3 of
the [consistent-snapshots investigation](../../investigations/consistent-snapshots/README.md#owner-decisions),
especially [the fuzzy-scan objection](../../investigations/consistent-snapshots/README.md#routes-and-their-costs).
It does not implement a deployable persistence system or the later comparative
resource/latency experiment.

## Expected outcomes and rejection criteria

The CI harness first runs the [acquisition probes](#acquisition-probes-and-predictions).
It compiles each protocol variant once, then runs **N = 20** instances of
each, alternating correct and wrong. The first three protocol instances of each are
small cost probes included in N; their wall-time spread is printed before
the remaining instances. An unexpected probe outcome stops the extension.

- Correct reconciliation must have **zero mismatches**. Exit 70 is a mismatch
  in either membership or value and rejects correctness.
- The wrong control must mismatch at least once in N runs. In fact its
  initial rendezvous predicts a mismatch on every completed valid run:
  S0 precedes an increment of key zero, and scanning follows that increment.
  Key zero is never subsequently assigned or deleted. Replaying all its
  suffix increments therefore adds at least one twice. Failure to observe
  a control mismatch rejects the experiment's ability to separate the routes.
- At least one correct run must observe a larger journal sequence at its
  last scan batch than at its first. Otherwise consistency was checked but
  interleaved traversal remains untested: the harness reports an inconclusive
  failure, not support for concurrent export. `map_scan`'s count is a hint,
  so multiple batches and this coverage condition require observation.
- Exit 0 means equality with a commit between scan batches; exit 10 means
  equality without that observation. Exit 71 means an invalid experiment
  (log capacity, integer overflow, unexpected key width, or writer failure),
  never a successful comparison. Other exits, signals and timeouts are
  infrastructure/runtime failures. The harness prints all exit codes and
  separate correct/control mismatch counts, and fails on unexpected outcomes.
- A source rejection of a required operation is an expressibility finding
  to minimize before continuing. Preserve the compiler's complete diagnostic,
  exact rule and source extent; write the smallest complete rejected program
  and explain the needed rule or implementation change. In particular, inspect
  entry-plus-journal atomicity, scan-plus-metadata observation, shared-state
  proofs and spawned ownership. Do not replace any of them with another
  spelling, a runtime check, external locking, or changed expected outcome.
  A syntax mistake is an authorship defect; a specified valid operation that
  the compiler refuses is a compiler gap; an operation the specification
  actually excludes is a language gap. Timeout, crash or link failure alone
  establishes none of those source-language conclusions.

## Representation and trace

Integer keys are encoded as exactly one unsigned byte. The live map has
`u64` values; the journal's 256-element `last_write` array stores per-key
sequence metadata beside it, including the last deletion's sequence. This
uses the stated finite key domain, not an assumed scan-position interface.
All mutations participate in the same `mutate` operation.

The journal owns `Slots<Record, 2048>`. Its `len` **is the shared sequence
counter**, initially zero: each mutation takes `len + 1`, updates the map
entry and its stamp, and appends `(key, sequence, Option<u64>, command)` in
one atomic statement. `None` is a tombstone. No counter duplication, log
truncation, or wraparound is needed. The command field is used only by the
negative control. Append order and sequence order are identical.

The root initializes keys 0–31 to zero through `mutate`, starts the writer,
records S0 and releases the writer's start guard. The writer increments key
zero and signals readiness. The root then scans; after the first batch the
writer is free to run eight rounds. Each round assigns, increments, deletes
and reinserts each key 1–31, then increments key zero. The writer finally
deletes key 31. There are 32 initial writes and 1,002 writer mutations, for
1,034 records at completion, below the fixed reserve. S1 may precede writer
completion; the join happens after reconciliation.

The start/readiness/first-batch guards arrange the minimal double-count
witness and prevent all churn from finishing before the first batch. They
do not order subsequent batches against individual mutations. Runs use two
requested context drivers and one compute worker, with no `--par` or CPU
affinity restriction. No sleep or OS lock supplies protocol atomicity.

Capacity refusal is explicit and fails the experiment before changing an
entry. `+checked` defines a total increment operation with an overflow error;
that error also fails the experiment without changing an entry. Neither
changes the successful protocol. The prescribed small values and write
count do not reach either outcome. These are initial operation contracts,
not replacements introduced after a rejected proof.

## Protocol steps and current rules

Line numbers below refer to `spec/kernel-spec.md` at the registered base.
The specification is the authority; the listed cases show analogous source
forms and are not substitutes for this witness's CI result.

| Step in `protocol.wf` | Expression and rule |
| --- | --- |
| Retain shared state | `shared_map_new`, `shared_new` and `shared_share`; SHARE-1, lines 2267–2273. `Shared` and `ConcurrentHashMap` are PRE-1 types, lines 2354–2361; `map_scan` and sharing functions are PRE-1 declarations, lines 2509–2530, rather than source functions in `lib/std`. `lib/std/process/module.wfm` supplies `ExitStatus`. |
| Publish one mutation | `mutate` targets one map entry and the journal together. SHARE-2, lines 2285–2309, permits the different state types in one statement; SHARE-3, lines 2311–2313, gives all their changes one point and one order. OP-10, line 1189, and PRE-1 append contracts govern log growth. |
| Start and concurrent writer | `spawn writer` moves retained handles into a waiting function. WAIT-1, lines 1533–1538, and WAIT-3, lines 2256–2265, admit this and define the join. WAIT-2, lines 2246–2253, supplies context order and conditional progress, with no latency bound. |
| Record S0 | One journal statement reads `records.len` and opens the start guard. SHARE-2/3 prevents an initial writer mutation from straddling this observation. |
| Scan a batch | One whole-map-plus-journal statement calls `map_scan`, copies each returned key's `Option<u64>` and its `last_write` stamp, then releases both. SHARE-1, lines 2275–2277, defines advancing cursor extents and enumeration; SHARE-2, line 2301, permits `table^[key]` below a whole-map target. SHARE-3 makes enumeration and both reads one observation. No reference escapes the block (SHARE-2, line 2304). |
| Record S1 and oracle | One whole-map-plus-journal statement reads the counter, copies the bounded log prefix, and reads every possible key directly into `reference`. SHARE-3 puts these at the same cut, even if the writer continues afterward. |
| Reconcile | For each owned record with `S0 < sequence <= S1`, replace the exported `Option<u64>` only if its sequence exceeds the retained sequence for that key. Tombstones retain their sequence too. Older/equal records cannot roll back a scanned value or a newer replayed record. This is ordinary owned state and comparisons (OP-1, line 924; OP-4, line 1061). |
| Compare and report | Compare presence and value for all 256 possible keys, including absent keys. PRE-2's exit status, line 2975, reports 70 on any difference. Wrong mode replaces the sequence comparison and after-image assignment with command execution over the scanned value. |

Relevant existing examples read for this source are
`tests/conformance/cases/share-pos-map-entry-beside-an-object.wf`,
`share-pos-map-scan-beside-other-contexts.wf`,
`share-pos-map-scan-across-statements.wf`, and
`share-pos-map-whole-target-entries-over-set.wf` in that same directory.
The runner/workflow follow the source-bundle invocation and CI setup of
`research/experiments/context-starvation/run.sh` and
`.github/workflows/ctx-starvation.yml` on `origin/claude/ctx-handoff`.

## Why the end cut is reconstructible

A key with no write in `(S0, S1]` has unchanged membership and value throughout
the scan. Advancing scan extents cover its fixed position once, so a present
entry is copied with that value. A key changed in the interval has a latest
after-image or tombstone in the captured suffix. Every scan observation is
before S1; taking the highest sequence among that observation and the suffix
therefore selects exactly its S1 state. Deleted and reinserted keys obey the
same reasoning, including a key absent when its position was scanned. This
deduction relies on every mutation being logged at the same atomic point.

The reference is independent of that deduction's implementation: it reads
the actual map for all possible one-byte keys under the S1 whole-map hold.
It does not replay commands, choose log maxima, call `map_scan`, or inspect
the exported result or metadata to construct expected values. All writers
only create one-byte keys, so these 256 presence/value comparisons establish
exact equality over this program's entire key domain. This oracle checks the
export protocol, not independently the map runtime's own implementation.

## CI and limits

The temporary workflow runs on pushes only to `claude/snap-log-witness` on
`ubuntu-24.04`: fetch locked dependencies, `make -C compiler build`, then
`sh research/experiments/snapshot-log-witness/run.sh`. The compiler path is
`compiler/target/gate/whitefootc`. The workflow records the actual revision
and host, compiler diagnostics, every outcome and small-sample time spread
in an uploaded artifact. A compile failure stops the harness immediately;
no stale executable is run. Remove this workflow **before any pull request**.
It adds no dependency to the canonical gate or daily research checks.

No local build, compilation, test or commit is authorized for this task.

Pre-execution review: an independent Codex/GPT-6 agent inspected all six
added artifacts against the registered base, the relevant design ancestors,
specification and checklist groups A/D/T/V plus G/DC. No build, compiler,
test or validation command ran. The review found and rechecked two repaired
authoring defects: an early return would have forced WAIT-3's join before
scanning, and the constructor explicitly moved a copy array. Errors are now
recorded until the explicit join; the copy array is passed bare. The review
also reread canonical-syntax repairs and reported no unresolved findings
within its inspection scope. This is source review, not execution evidence.

The witness bounds its key domain and trace, retains the complete small log,
and takes a deliberately expensive full read only for the oracle. It makes
no latency, memory-efficiency, general-key, multi-writer, multi-map,
multi-key-transaction, expiry, nested-owner, durability, cancellation or
bounded-cleanup claim. Production reserve/abort, streaming and reclamation
remain the contracts of the parent investigation; a pass here establishes
only this finite end-cut protocol witness, not the route's comparative merit.

## Results

[CI run 38039919162](https://github.com/Ming-Research/Whitefoot/actions/runs/38039919162)
ran revision `c532b95e610873cab4dd56d62bb744f0606b9a67` on 2026-10-10,
GitHub-hosted Ubuntu 24.04 with four vCPUs, `WF_DRIVERS=2`, `WF_WORKERS=1`
and no `--par`. Both compilations exited 0. All three probes of each variant
exited 124 at the unchanged 30-second timeout; stdout and stderr were empty.
The harness recorded six unexpected outcomes and did not extend to the
remaining runs. The saved `.time` files record 29.77–29.82 seconds of user
CPU and 0.17–0.22 seconds of system CPU per probe, consistent with one busy
CPU throughout; this is evidence against a simple all-contexts-parked wait,
but does not identify the spinning code. These results establish source
acceptance at that revision, but establish no completed comparison,
interleaved traversal or control sensitivity.

### Source trace of the timeout revision

The locations below refer to `protocol.wf` at the CI revision, before phase
markers. They describe the source's waits and the inspected implementation,
**not observed program counters**: the old programs emitted no diagnostics.
Both entry files call this same function and differ only in `wrong`.

| Context and wait | Statement that lets it proceed |
| --- | --- |
| Writer: `started` guard, line 77 | Exporter records S0 and sets `started`, lines 161–164, before its readiness wait. |
| Exporter: `ready` guard, line 165 | Writer makes its first mutation at line 79, then sets `ready` at line 81, before its first-batch wait. Readiness is published even on the writer's failure path. |
| Writer: `first_batch` guard, line 86 | Exporter sets `first_batch` at line 178 in the first scan statement, before reconciliation or joining. |
| Exporter: join at `let writer_ok = job`, line 242 | After `first_batch`, the writer has only the finite loops at lines 88–113 and final mutation at line 114. There is no later dependency on the exporter. |

There is no loop polling for observed writes between batches. The scan loop
at lines 174–202 advances `cursor` to `next` or breaks at zero. The inspected
`wf_cmap_scan` advances its extent before returning the next bound or zero
(`compiler/src/backend/concurrent_map.c:2609–2674`). The writer attempts
1 + 8 × (31 × 4 + 1) + 1 = 1,002 mutations, not an unbounded loop.
The S1 capture and replay at lines 208–241 are bounded too. No post-spawn
statement before the explicit join names `job` or exits its binding's block;
the scan's `break` exits only the scan loop. The WAIT-3 planner handles that
inner-loop distinction (`compiler/src/semantic/permission.rs:975–1019`).

The runtime places the child on its starter's driver
(`compiler/src/backend/completion/bridge.c:2363–2391`); making one child
ready there does not itself wake an idle second driver (lines 1631–1641).
Nevertheless, the exporter already parks when `ready` is false, and the
writer already parks when `first_batch` is false: guard watches register
before unlocking and park until a write (lines 3095–3119). Once the exporter
opens `first_batch`, it may keep the driver during its finite scan and
reconciliation, but then its join suspends it (lines 2398–2435).
Mixed map/journal statements take the map before the journal and may spin
for the latter (`compiler/src/lowering/builder/atomic.rs:403–490` and
`bridge.c:2977–3008`); no concrete lock cycle was established by inspection.

**Diagnosis remains unresolved.** The non-waiting busy-loop starvation gap
`firn-gap-ctx-starve` would explain an exporter polling indefinitely while
its producer remains ready on the same driver. That spelling is absent
here, so this run is not evidence that this witness hit that gap. A guard
on the producer-written object is the correct form for such a wait, but
all three existing handshakes already use it. Inspection found neither a
closed source-level wait cycle nor a new compiler/runtime defect that can
honestly be given a minimal reproducer. Empty streams plus a timeout cannot
locate the contexts; the next CI run needs the phase evidence below before
a waiting change or runtime repair can be justified.

### Phase diagnostics prepared for CI

Both variants now pass their stderr stream and handle factory to `witness`.
Its shared `progress` helper writes one line at each boundary:

- `start`: before map and journal initialization;
- `writer started`: after the readiness guard observes the writer's first
  mutation and readiness publication;
- `scan done`: after the scan loop, before the S1 oracle capture;
- `reconcile done`: after replay, immediately before the explicit join;
- `join done`: after joining, before validity and equality checks.

Writes use `std::io::write_once`, retrying partial writes; an I/O refusal or
zero progress ends only the diagnostic helper. It cannot insert an early
return or `propagate` in the spawning function, which would move WAIT-3's
join ahead of the first-batch signal. Markers are best effort if stderr
fails and do not change the comparison's exit codes. Their waiting I/O can
change scheduling; completion with markers would not by itself identify or
prove a fix for the original timeout. A last marker identifies the interval
still to investigate, including a possible wait inside the next marker's
write, rather than an exact suspended instruction.

The oracle, mutation trace, pre-registered criteria, N = 20, three initial
probes, timeout, driver/worker settings, runner and workflow are unchanged.
No source waiting fix was claimed. At diagnostic preparation the instrumented
source had not been compiled or executed; the acquisition audit below records
the owner's subsequent CI report.

An independent Codex/GPT-6 agent reviewed the complete four-file diagnostic
diff against the CI revision, affected callers, I/O contracts, runtime and
design paths, and existing CI artifacts. No findings within that scope;
A4/D2 and G2/G3/DC1/DC2 passed, unchanged specification/check/compiler/tree
groups were not applicable, and DC4 execution evidence remained unverified.
The review ran no build, program, test or validation suite. Its specific
proof concern about the diagnostic write bound was locally addressed by
testing `sent >= end` before writing and rechecked by the reviewer. No
specification rule or design decision changed. The timeout diagnosis and
any fix remain open pending CI evidence; the worktree is left uncommitted.

### Multi-target acquisition audit (2026-10-10)

Inspected source: `abfc245cc21fb6af3a0a1608b8d5cb4012fa076e`, whose runtime
and lowering are unchanged from the registered base. The owner's report of
[phase-marker CI run 38040858769](https://github.com/Ming-Research/Whitefoot/actions/runs/38040858769)
places five of six last markers at `scan done` and one inside scanning.
That narrows the search to the intervals containing the whole-map statements
at `protocol.wf:211` and `protocol.wf:179`; it is not a sampled native stack.
No new CI result or local execution is claimed here.

**Finding:** the inspected paths do not establish the proposed map/journal
deadlock. Both statements acquire the map before `Journal`, and a context
holding a map entry **spins, rather than parks**, for the journal. The
following is a source trace, not a proof that the emitted executable or the
rest of the runtime is correct.

| Step | Source and consequence |
| --- | --- |
| Choose the order | `compiler/src/semantic/check/control/atomic.rs:246` assigns `atomic_type_order(state)`. `compiler/src/semantic/check/types.rs:1765–1777,1814–1822` ranks `ConcurrentHashMap` as `019` and source `Journal` as `100`. `compiler/src/lowering/builder/atomic.rs:90–120` groups by that key in a `BTreeMap`, so written target order does not choose lock order. These two different state types form singleton groups; the runtime identity sort for equal-type groups (`compiler/src/backend/keyed_table.c:241–278`) is not this path. |
| Take whole map, then journal | A whole target becomes `TableTake::Hold { whole: true }` (`atomic.rs:189–213`). On a binding's first use, `take_units_for` takes its group and every earlier group (`atomic.rs:353–365,403–408`), so even the journal-first body statements at `protocol.wf:180,212` acquire the map first. `prepare_hold` and `TableHoldTake` (`atomic.rs:367–400,479–484`) reach `wf__table_hold_take` (`keyed_table.c:194–212`) and `wf_cmap_hold_take` (`compiler/src/backend/concurrent_map.c:2158–2189`). Both its read and write whole paths call `wf_cmap_hold`. Only after that returns does the journal's later group call `SharedTake` (`atomic.rs:443–457`). |
| Wait for entries | `wf_cmap_hold` takes a ticket, waits its turn, waits for previously excluded keyed entrants, closes `map->gate`, then waits for every `users[i].active` to clear (`concurrent_map.c:1368–1389`). These are `back_off` loops, including the drain of in-flight entry holders at lines 1386–1387. They never park a WF context. `back_off` pauses and eventually calls `WF_CMAP_YIELD` (lines 257–265), mapped through `keyed_table.c:29` and `bridge.c:2778–2780` to `wf_prim_yield`, which calls host `sched_yield` on Linux (`compiler/src/backend/sched/prim_host.c:275`). Yielding the host thread does not run the next coroutine on that driver. |
| Take one entry, then journal | A singleton keyed target becomes `TableTake::Entry` (`atomic.rs:175–186`). Entry groups and every preceding group are eager (`atomic.rs:101–113,235–237,270–274`); `TableLockEntry` is emitted at lines 459–477. `wf__table_lock_entry` selects the calling driver's map user and, for a potentially inserting writer such as `mutate`, calls `wf_cmap_lock_entry` (`keyed_table.c:165–178`). `enter_keyed` marks that user active and rechecks the gate (`concurrent_map.c:1076–1092`); `wf_cmap_lock_entry` locks or claims the cell (`concurrent_map.c:1137–1172`, with `acquire_entry` at 966–1032). The entry remains held while the later journal group is taken. A patience upgrade clears the active mark before taking the whole map (lines 1149–1153). |
| Wait for journal with entry held | `SharedTake` emits a plain call to `wf__shared_take` (`compiler/src/backend/emitter/shared.rs:929–946`). The function loops until it takes the object or borrows a handed-off hold (`compiler/src/backend/completion/bridge.c:2965–3006`), with host yielding through `wf_shared_spin` at lines 2883–2889. It has no frame argument or park. The frame-parking `wf__shared_acquire` (lines 2893–2958) is selected only for a singleton ordinary object in group zero (`atomic.rs:448–455`), not for this journal in group one. |
| Release and guard waits | Groups release in reverse order (`atomic.rs:566–617`): journal, then entry/whole map. An ordinary entry release unlocks the cell and clears `active` (`concurrent_map.c:1175–1215`); a whole release opens the gate and advances the ticket (`concurrent_map.c:1392–1395`). A false guard registers its watch and releases its holds before parking (`atomic.rs:675–717`; for the witness's journal-only guards, `bridge.c:3095–3117`). A parked guard therefore retains no entry. |

Here `atomic.rs`, `keyed_table.c` and `bridge.c` in the table abbreviate the
full paths given in its preceding rows.

**The candidate cycles, and the missing edges.** Let E be the entry writer,
W the whole-map exporter, M the map and J the journal.

- A lock inversion would be `W holds J -> waits for E's entry/active mark;
  E holds that entry -> waits for J held by W`. W cannot hold J while still
  acquiring M: its map group must finish first. Conversely, when W holds M
  and J, E waits at M without holding J. The two paths do not form this cycle.
- A driver cycle would be `W occupies driver D spinning for an entry held
  by E; E is suspended and needs D to release the entry`. The entry holder
  does not suspend on its later journal acquisition; it calls `shared_take`.
  Another context on D cannot run until that hold is released. On another
  driver, the entry holder can continue and clear its active mark.
- A journal handed to a ready but not yet running context is the special
  case the runtime handles explicitly: `wf_shared_wake_locked` can grant
  it while waking the context (`bridge.c:2859–2877`), but `shared_take` may
  borrow that hold (`bridge.c:2965–3001`). When the grantee resumes, it sets
  `claimed` and waits out an existing borrower (`bridge.c:2907–2923`);
  unlock returns the borrowed hold (`bridge.c:3050–3078`). This avoids
  assuming that a granted journal necessarily has a running owner.

No acquisition cycle or endless retry schedule was established for this
two-context witness. Map acquisition and later `shared_take` do not call
`wf_context_pass` (`bridge.c:1657–1671`), so repeatedly successful mixed
statements can keep their driver. That fact alone does not explain this
finite workload's permanent stall: both loops are bounded and the eventual
join can suspend (`bridge.c:2398–2434`). It does mean a map-only passing
probe need not have executed the two loops simultaneously.

**Specification classification.** SHARE-1 permits retained map and journal
handles (`spec/kernel-spec.md:2267–2273`). SHARE-2 permits the mixed targets,
whole-map reads and nonwaiting bodies (lines 2285–2307); it imposes no
writer-chosen acquisition order. SHARE-3 requires one atomic point and
order (lines 2311–2317). WAIT-2 explicitly promises eventual effect for a
begun unguarded statement while all contexts keep reaching completion or
waits (line 2252). This program's finite bodies and loops satisfy that
condition; its guards have the producer transitions described above.
A genuine endless internal acquisition here would be a compiler/runtime
progress defect, not a rule violation by the program. The permitted
all-contexts-blocked execution at line 2253 requires false guards or joins
and no outstanding host operation, not an unguarded internal lock cycle.
A 30-second timeout and a last marker alone do not prove which internal
operation failed to progress.

### Acquisition probes and predictions

The owner-requested [repro.wf](repro.wf) creates one
`Shared<ConcurrentHashMap<u64>>` with one initialized key and one
`Shared<u64>` counter. A spawned context performs 1,000 keyed writes with
counter increments; the entry context performs 1,000 whole-map reads with
counter increments, then explicitly joins. Exit 0 checks completion, a
present in-range observed value and the final counter of 2,000. Exits 1/2
report incorrect results. The total `+wrap` operation follows the existing
mixed-target conformance example; the fixed 2,000 increments cannot wrap.
[repro-map-only.wf](repro-map-only.wf) keeps the same initialization,
map operations, iteration counts and explicit join, removing the counter
and its targets/check. It is expected to exit 0.

The fixed N = 1,000 is intended to take well under one second on a correct
runtime; that is an unmeasured sizing expectation, not a timing result or a
specification latency bound. Neither probe adds a start guard, sleep or host
I/O, and neither requires simultaneous execution for correctness.

**The counter changes the acquisition path.** `u64` ranks `007`, before the
map's `019` (`compiler/src/semantic/model.rs:380–388` and
`compiler/src/semantic/check/types.rs:1722,1769,1822`). Both repro statements
therefore acquire the **counter first**, using `SharedAcquire` (which may
park or cooperatively yield before taking any map hold), then the map
entry/whole hold. Releases run in reverse. This requested reduction does
not preserve the witness's map-then-`Journal`/`SharedTake` path, and its
success cannot clear that path. Because both statements take the counter
exclusively before the map and release it after the map, the counter also
serializes their map acquisitions: these two mixed statements cannot
contend with each other inside the map runtime. A further reduction of that exact path
would retain a source struct around the counter; no such alternate run or
runtime repair is claimed here.

`run.sh` compiles both probes separately and executes 20 alternating pairs
before the protocol runs, with the existing `WF_DRIVERS=2`, `WF_WORKERS=1`,
no `--par`, `timeout 30`, captured stdout/stderr and `/usr/bin/time -p`.
Every exit and wall time is printed and appended to `results.tsv`. Probe
failures are counted separately, do not skip later pairs or the protocol
probes, and make the final harness result fail. The protocol's three-probe
extension criterion and mismatch criteria are unchanged. Compilation
failure still stops immediately with the complete diagnostic. Timings
serve only timeout diagnosis, not a performance comparison.

| Hypothesis or observation | Prediction and interpretation |
| --- | --- |
| Correct acquisition for these finite programs | Both probe modes exit 0 in all 20 runs, with subsecond runs expected. |
| Shared-object/map interaction also affects the counter-first path | `repro` may time out (124) while map-only completes; this isolates a mixed-target dependency but does not establish the journal-first inversion described above. Native stacks or emitted acquisition calls would still be needed. |
| Defect depends on the witness's map-before-source-struct path, guards, absent keys or churn | Both probes may pass while the witness still times out. A pass would not refute this hypothesis. The one present key does not exercise deletion, growth, absent-key upgrades, guard handoff or journal copying. |
| Map-only also times out | The second Shared target is not necessary to reproduce that failure; investigate map acquisition/release or context scheduling before attributing it to journal contention. |
| Both probes pass without observed overlap | This establishes finite completion for those schedules only, not contention coverage or a general progress proof. |

No source rule, compiler/runtime code, conformance expectation or design
decision changed. The found-along-the-way limitation is the counter type's
different lock order; it is recorded explicitly rather than reported as an
exact minimization. The timeout's root cause remains unresolved. This work
stops at the requested uncommitted probes and source report; compilation,
execution, subsecond sizing and native stall attribution remain unverified.

An independent Codex/GPT-6 read-only review covered the complete experiment
and workflow diff from `a69d5afc7` through `abfc245cc` plus this worktree,
including both new probes, affected runtime/lowering code, the specification,
design ancestors and applicable A/D/T/V and G/DC checklist items. It found
no unresolved findings within that scope. Its clarification that the
counter-first probe serializes map access is included above. T5 execution
and DC4 remain unverified; no build, compilation, test, shell syntax check
or program ran locally, and no new CI ran. Live status-board recording was
unavailable because this session has no ArtifactData tool; the report is
retained here for the owning session.
