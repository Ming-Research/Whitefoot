# Consistent snapshots while service continues

## Question and status

What should Whitefoot provide so a program can retain its dataset at one
instant, keep serving mutations, and export or inspect that old state without
eagerly allocating a second dataset? Does that require supporting `fork`?
This answers the owner's fresh-look request of 2026-10-10, including fork,
rather than treating memory accounting as the starting design.

Research only, against main `87fa2524f4da93bded8d06e1bfbac2a16c08c68c`,
specification v0.119. No proposal below is an approved rule or implemented
capability. No build, test or measurement was run. Repository citations are
`file:line` at that revision; cross-branch records are identified separately.

**Provisional recommendation:** specify an opt-in frozen dataset view and its
resource policy before selecting its mechanism. Compare software versioning
with a restricted fork backend; do not expose unrestricted fork merely to
separate memory counters. Neither fork nor versioning guarantees both
uninterrupted writes and substantially less than twice the memory under
arbitrary mutation. Small embedded systems remain a design constraint, not
a claim that the current compiler already targets them
(`docs/constitution.md:16`, `:28`, `:48`, `:56`).

## Need, evidence and a limit no mechanism removes

A successful capture selects one point **t** between request and capture
completion. Every key, payload, deletion and relevant metadata in the view
must equal the selected dataset at t. A transaction changing two keys must
appear wholly before or wholly after t. A dataset spanning several maps
needs one cut, not one independently chosen time per map. Nested mutable
objects must be included in the cut or explicitly outside the dataset;
retaining a top-level pointer is insufficient.

For persistence, associate t with an exact command-log boundary: restoring
the base and the suffix applies each committed change once. Expiration needs
the dataset's captured expiry metadata and a stated restore-time policy;
reading a fresh clock for each exported key does not itself define a snapshot.
File publication and durability are separate obligations: PRE-2's sync
hands data to the host without promising which bytes survive host failure
(`spec/kernel-spec.md:2561`). A failed export must leave the previous usable
base/log combination authoritative.

Client reads, writes, accepts, replies, timers and cancellation must keep
making progress during traversal, encoding and storage I/O. “Nonblocking”
here means no dataset-long service stop, not zero capture pause or zero
contention. Capture pause, command p99/p99.9 and longest service gap, steady
throughput, and total snapshot duration are separate requirements. A
single-CPU server must share its CPU; a second process provides scheduling
isolation, not extra compute capacity. Application checkpoints, backups and
analytics over live state need the same frozen view; restarting arbitrary
threads, sockets or a distributed computation requires a broader contract.

These Firn-wf observations come from
[PR #35, maxmemory](https://github.com/Ming-Research/Firn-wf/pull/35),
i9-14900K runs [rewrite probe](https://github.com/Ming-Research/Firn-wf/actions/runs/38020613686),
[rewrite comparison](https://github.com/Ming-Research/Firn-wf/actions/runs/38021251447)
and [follow-up](https://github.com/Ming-Research/Firn-wf/actions/runs/38021863317):
with 1,000,000 keys and a 117 MB limit, in-process replay uses 208–240 MB,
takes over 10 s, and one-server-CPU throughput falls to about 2% of normal;
Redis 7.0.15 takes 0.4–0.8 s without the reported service loss and reports
about 77 MB `used_memory`. These are supplied results, not rerun or raw-log
verified here. The checked-in Firn-wf
[memory-limit record](https://github.com/Ming-Research/Firn-wf/blob/68b57bb705615c1ebc0ce779b8aefbc24c73cabf/research/investigations/memory-limit/README.md#results-eviction-quality-and-at-limit-behaviour),
lines 336–383, independently records the earlier private-keyspace eviction
problem, not those later runs. The reported pure-computation driver starvation
is a separate scheduling gap; none of these comparisons isolates its cost
from replay, representation or accounting. Redis's parent `used_memory` is
not parent-plus-child physical memory, so 77 MB is no proof of a physical
snapshot-memory bound.

**Deduction.** Suppose the exporter has not read an arbitrary S-byte dataset
when clients replace all of it with unrelated S-byte data. Recovering the old
state while retaining the new requires preserving S bytes' worth of old
information somewhere. Compression cannot guarantee a saving for arbitrary
data. RAM, disk or another machine must hold it, or writers must wait, the
snapshot must fail, or its consistency must weaken. Thus “without doubling”
can mean no eager second copy and a bounded change allowance under a stated
workload. It cannot mean an unconditional sub-2× RAM guarantee with unlimited
writes, no spill, a stalled exporter and guaranteed snapshot completion.

Use **D** for current live dataset storage, **H** for other process storage,
**V** for retained old data, **B** for buffers and metadata. The planning model
is `H + D + V + B`, plus OS overhead where relevant. These are analytical
estimates, not measured peaks. A usable policy must bound V/B or refuse/abort
the snapshot before exhausting its reserve; merely sampling RSS cannot prove
a hard bound. Current heap exhaustion may terminate outside the source model
(SCOPE-3 and STOR-8, `spec/kernel-spec.md:17`, `:844`). A service-preserving
budget guarantee would be new behavior, not something those rules establish.

## Comparison fixed before future measurements

Compare the current closed-log replay with (1) a frozen versioned dataset,
(2) restricted fork over the same logical dataset, and (3) a correctly
reconciled batched export. Redis is an external reference, not the oracle for
Whitefoot safety. Compare no snapshot, snapshot support present but unused,
capture, active export and reclamation. Include read-mostly traffic both with
and without access-time/LRU/eviction-sampling updates, repeated hot-key writes,
uniform replacement of every key, deletes/reinsertions, large mutable values,
multi-key transactions, and an exporter stalled on I/O.

Prospective rejection criteria:

- Any view inconsistent with its recorded cut, double-applied log operation,
  missed deletion, premature reclamation or externally duplicated effect
  rejects correctness, regardless of speed. A serial operation-history model
  independent of the implementation supplies expected states and log suffixes.
- Any source path reaching an inherited lock/handle without a valid contract
  rejects fork safety. An unproved platform assumption remains unverified.
- Reject a claimed bounded mode if its reserve is exceeded without its stated
  failure/backpressure outcome, including during cancellation and cleanup.
  The all-keys-overwritten/stalled-reader case must exercise that boundary.
- As a proposed screening bound, reject a resolved **over 1%** cost in programs
  that do not use snapshots. For the supplied firn workload, a candidate must
  improve replay's memory peak and rewrite time beyond control noise, without
  worse service latency or throughput. This rejects an unhelpful mechanism;
  it is not yet an absolute service-level promise. Fix the consumer's numeric
  pause, tail-latency and reserve limits before performance acceptance.

Future execution belongs in CI, precise timings on the idle 14900K. Start
with a timed small sample, then interleave same-source before/after runs and
a base twin; record revisions, allocator, affinity, driver/worker counts,
the exact kernel release and its page-table/COW source, page/huge-page settings,
dataset and update rate. Measure fork time separately from reaching quiescence,
and record the target kernel's huge-page write-fault behavior. Measure aggregate
physical memory without double-counting shared pages, process heap counters,
reserved address space and commit separately. Compare mechanisms on identical
logical traces; do not attribute a representation change to the scheduler. Qualify
one named MMU-less target and its RAM/latency budget separately. Nothing in
this protocol authorizes execution in this edit-only task.

## Routes and their costs

The following costs are deductions or unmeasured estimates. No row promises
a bounded pause merely because its capture is called “constant time.”

| Route | Memory peak above live D + H | Latency and blocking | Complexity and cost when unused |
| --- | --- | --- | --- |
| **Process fork with OS copy-on-write** | Page-granular V can greatly exceed changed bytes and approach a second resident image; child scratch, page tables, allocator retention and growth add more. Read-side metadata writes also cause divergence. | Quiescence plus page-table work pauses capture. Each first parent write to a still-shared private page pays a fault and copy, normally 4 KiB. Export competes for CPU, memory bandwidth and disk. | Broad execution/host integration burden below. No required per-entry version field, but runtime fork coordination may tax all drivers/allocators unless isolated. |
| **Persistent/functional map** | Old nodes retained by a root plus paths copied on update and changed payloads; approximately V + B, with amplification from tree paths. | Root retention can be short; every writer pays path copying. Large payload cloning and final reclamation can create spikes. | Immutable sharing and lifetime discipline; updates/refcounts cost users even between snapshots. A separate opt-in representation can leave ordinary maps untouched. |
| **ConcurrentHashMap epochs/generations** | Snapshot-era entries, old payloads and old index generations; V + B. A retained table alone does not retain mutable values. | Short cut protocol; copy before mutation, and defer reclaim. Resizes, clear/swap and large values need bounded handling. | Substantial runtime and ownership design. Global epoch publication must respect multi-object atomicity. Adding fields/barriers to every existing map charges nonusers; compare an opt-in representation. |
| **MVCC-like entry versions** | Version chains, timestamps and tombstones; a naive implementation grows with writes during the oldest snapshot. One retained cut can discard intermediate versions only with a correct reader/reclamation protocol. | Snapshot epoch is cheap; writers publish versions and readers search/select them. Long readers retain history. | Transaction publication and garbage collection; generic in-place payload writes need interception or replacement discipline. Metadata/indirection remain even without an active reader. |
| **Batched/fuzzy export plus change log** | Bounded scan buffers plus retained redo/undo information; an in-RAM log grows with update rate × duration. Disk spill trades RAM for I/O and storage. | Short per-batch holds, write-path logging, and final reconciliation; catch-up may never finish if writes outrun it. | Application protocol is substantial. A library opt-in can avoid costs elsewhere; every relevant mutation must participate. A log alone is not consistency. |
| **Snapshot region allocator / software page COW** | Retained region pages/blocks plus allocation metadata and scratch; page-size amplification may make V much larger than changed values. | Publish a region generation, copy on writes; exclude outside aliases and coordinate all writers. Hardware faults or software barriers delay a first write. | General storage-domain and reference/alias design, not an allocator switch. A sealed opt-in region confines ordinary-program cost; universal write barriers do not. |
| **Full copy or stop-and-stream** | Full copy adds approximately D; streaming while mutations are stopped adds only buffers. | Copy pauses mutations for O(D); streaming pauses for the export's duration. | Simplest controls, no inactive tax; each fails one central requirement. |

**Deduction: page amplification.** For k independent uniformly random writes,
each touching one of P pages still retained by the child, the expected copied
fraction is `1 - (1 - 1/P)^k`, approximately `1 - e^(-k/P)`. A roughly 117 MB
dataset occupies about 29,000 4 KiB pages; about 29,000 such writes copy about
63% even if each changes only a few bytes. Allocator metadata and free-list
writes, map tombstones and resizes, and access-time/LRU/eviction-sampling
updates on reads dirty pages too. A read-mostly workload with that metadata
is not read-only to the kernel. Whitefoot uses the system allocator
(`compiler/src/backend/heap.c:8`, `:18`, `:29`), and even map read-lock release
writes bookkeeping (`compiler/src/backend/concurrent_map.c:1356`).

Huge-page amplification depends on the kernel: older Linux could copy a whole
2 MiB transparent huge page on a write fault
([v4.9, `mm/huge_memory.c:969`, `:1008`](https://github.com/torvalds/linux/blob/v4.9/mm/huge_memory.c#L969));
Linux v6.6 instead splits the mapping and falls back to a base-page copy
([v6.6, `mm/huge_memory.c:1279`](https://github.com/torvalds/linux/blob/v6.6/mm/huge_memory.c#L1279),
[`mm/memory.c:2902`](https://github.com/torvalds/linux/blob/v6.6/mm/memory.c#L2902)).
These versions establish the difference, not the runner's behavior: the
comparison must identify and cite the exact kernel measured.

[Redis's latency documentation](https://redis.io/docs/latest/operate/oss_and_stack/management/optimization/latency/)
models Linux/AMD64 page tables at about 8 bytes per 4 KiB page and reports
roughly 10–13 ms per GB of RSS on several bare-metal and modern VM systems
(other entries are slower). **Deduction:** scaling those entries to about
200–240 MB RSS gives a few milliseconds, making quiescence the likely capture
pause limit rather than a large page-table copy. This is not a Whitefoot
measurement. The 14900K runner is a Hyper-V guest
(`.github/workflows/io-bench.yml:32`), absent from Redis's table; measure its
fork time. The later fault and page-copy cost remains on the parent's write
path, as the versioned kernel source above shows.

**Versioning must cover the transitive data.** An epoch for table reclamation
prevents freeing an old table; it does not stop a `Box` below an entry from
changing. Arbitrary V is not automatically cloneable, and `SharedRead<T>`
retains the same live object rather than freezing it (SHARE-1/2,
`spec/kernel-spec.md:2270`, `:2295`). Persistent values need an ordinary
ownership/representation contract for shared immutable nodes and safe updates.
A new frozen-map representation must cover nested objects, deletes, growth,
clear and swap and publish all of a multi-target atomic statement together.
Changing the current map's interior mutation rules or making shared versions
of unique owners is a language decision; this record assumes neither exists.

**The prior fuzzy-scan objection still holds.** Firn's
[AOF rewrite investigation](https://github.com/Ming-Research/Firn-wf/blob/68b57bb705615c1ebc0ce779b8aefbc24c73cabf/research/investigations/aof-rewrite/README.md#why-firn-cannot-copy-this-directly),
lines 48–80, rejects plain scan-plus-command-replay and records scan-position
routing's missing position interface and duplicate log streams. Minimal
witness: start with `x=0`; log `INCR x`; scan `x=1`; replay the log over that
base; obtain `x=2`. Reopen this route only with new machinery: per-record
sequence numbers and idempotent after-images/tombstones can reconstruct an
**end** cut; before-images or versions are needed for a chosen **start** cut.
Multi-key transactions, create/delete/reinsert and expiry must share the
protocol. Scanning alone offers neither guarantee: each map_scan step has
its own atomic observation (SHARE-1/3, `spec/kernel-spec.md:2276`, `:2311`).
Replaying an immutable closed log remains correct but retains its second
keyspace and replay work; putting it in another process only relocates them.

**Other placements.** A disk-backed versioned store can retain an immutable
root and stream old pages with bounded RAM; it trades memory for storage,
write amplification and a changed dataset representation. A replica can
take the pause while the primary serves, but adds a dataset elsewhere and
requires a log/cut protocol. A filesystem snapshot captures durable files,
not RAM that has never been made durable. Process checkpoint packages solve
a larger restart problem: [CRIU freezes a process tree](https://criu.org/Checkpoint/Restore),
and [DMTCP documents external-world limitations](https://dmtcp.sourceforge.io/supportedApps.html).
Neither establishes this dataset's cut, low pause, bounded extra RAM or
Whitefoot's ownership contract without integration. Their wrappers, metadata
and coordination would cost participating applications; they are not a
required dependency for ordinary Whitefoot programs.

## Fork is an execution-model question

[POSIX fork](https://pubs.opengroup.org/onlinepubs/9799919799/functions/fork.html)
keeps only the calling thread in a multithreaded child; the child must restrict
itself to async-signal-safe operations until exec. Descriptor copies refer
to the same open file descriptions. Thus a copied mutex can name an absent
owner, and a copied descriptor is not an independent file/socket resource.
`pthread_atfork` does not make arbitrary child code safe. Fork followed by
exec starts a clean program but discards the inherited heap: exporting the
dataset still needs an explicit transport or frozen mapping.

No current POSIX Whitefoot program is single-threaded at a point where its
source could request fork: the launcher runs entry on a second thread while
the original receives stop signals (`compiler/src/backend/wf_floor.c:348`,
`:353`, `compiler/src/backend/completion/stop_signals.c:168`). Context starts
can add Linux drivers with their own rings
(`compiler/src/backend/completion/bridge.c:3372`, `:3402`, `:3423`); without a
ring, file helpers grow on demand, normally up to eight (`:70`, `:167`), and
compute lanes start on first attachment (`compiler/src/backend/sched/core.c:1254`,
`:1280`). The runtime has no fork handling today: under `compiler/` there is
no `pthread_atfork`, `MADV_DONTFORK` or `MADV_WIPEONFORK` integration.

The following are **new obligations inferred from current Whitefoot rules**,
not a claim that the existing runtime can discharge them.

| Boundary and current rule | What a safe snapshot child requires |
| --- | --- |
| **Atomic cut and locks.** SHARE-3 gives whole statements one point (`spec/kernel-spec.md:2311`); the runtime locks by type/object/key and releases after the block (`design/compiler/waiting-contexts/state-locks.md:3`, `:7`). | Holding whole-map locks for every captured map across fork suffices for the dataset cut if they cover all transitive mutations and the child reads only frozen data without entering copied locks (`compiler/src/backend/concurrent_map.c:1363`). Unrelated drivers need not quiesce; their copied state must be unreachable. Resetting a copied lock cannot repair half-written data. Shared mappings and external writers need exclusion or coordination; allocator consistency is a separate platform assumption below. |
| **Contexts/drivers.** WAIT-2/3 define calls, starts and joins (`spec/kernel-spec.md:2246`, `:2256`); contexts reside in resumable frames and driver-owned queues (`design/compiler/waiting-contexts.md:1`, `:3`, `:13`). | Create a fresh child root running only the designated job. Do not resume copied ready/parked frames or join missing sibling contexts. Parent contexts retain their existing joins. A rendezvous must not wait for clients to finish pending network operations. |
| **Compute workers and --par.** PAR-1/2 hide worker identity and allow sequential execution (`spec/kernel-spec.md:2159`, `:2167`, `:2234`); workers steal/help (`design/compiler/parallel-lowering/parallel-runtime.md:1`, `:5`). | Drain computation writing the captured dataset and its publications. A copied started pool (`wf__par_started` and lane count, `compiler/src/backend/sched/core.c:1261`, `:1280`) has no lane threads. Publishing can call `wf__par_signal`, which deadlocks if its lane wait lock was held at fork (`:765`, `:791`, `:1328`, `compiler/src/backend/sched/prim_host.c:286`). Child --par code needs a runtime reset to “no pool”; sequential lowering is permitted, but --par on/off cannot select source safety. |
| **Completion, timers, cancellation.** One pending record per context, per-driver deadlines and route-owned cancellation (`design/compiler/waiting-contexts.md:7`, `design/compiler/waiting-contexts/bounded-waits.md:1`, `:3`). PRE-2 defines watches and transfer races (`spec/kernel-spec.md:2558`). | Parent owns pending operations and completion buffers. Linux rings are live `MAP_SHARED` mappings, not a COW snapshot (`compiler/src/backend/completion/linux_io_uring.c:258`, `:276`, `:291`, `:311`). The child retains the calling driver's adapter (`compiler/src/backend/completion/bridge.c:348`, `:3365`); submissions or completion consumption corrupt the parent's ring (`compiler/src/backend/completion/linux_io_uring.c:625`, `:1095`). Prove no child completion path is reachable, or add `MADV_DONTFORK` to the rings: stray ring access then faults instead of corrupting the parent, which still cannot count as a safe export. On macOS the stop-signal kqueue is not inherited (`compiler/src/backend/completion/stop_signals.c:214`; [kqueue contract](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kqueue.2.html)); file helper threads are absent, so helper-dependent requests can wait forever (`compiler/src/backend/completion/file_adapter.c:279`, `:592`). Child timers/watches need fresh state or IPC; copying CancelWatch cannot convey later parent firing. |
| **Linear handles and host order.** PRE-2 opaque handles, shared factory budget and Inputs (`spec/kernel-spec.md:2554`, `:2564`, `:2962`); HOST-1 orders through overlapping state (`:2241`). | No duplicated source owners for sockets, files, listeners, factories or invocation capabilities. The Linux epoll descriptor shares the parent's open file description (`compiler/src/backend/completion/linux_io_uring.c:121`; POSIX fork above). Transfer designated output authority, or create a fresh private output; close unwanted copies through an audited child path. Shared offsets, socket shutdown and factory credits make blind duplication wrong. No copied close/finalizer pass may operate on all parent owners. |
| **Memory and release.** Process meter (`spec/kernel-spec.md:2560`); single heap and ordinary releases (`:844`, `:855`). | Define child logical heap and snapshot accounting separately. The copied meter retains every thread's slots, including vanished threads (`compiler/src/backend/completion/bridge.c:1357`, `:1362`, `:1383`); COW physical copies are not source allocations. Do not decrement parent credits or counters on child exit. Count aggregate physical/commit pressure separately. |
| **Lifetime and exit.** ExitStatus is reported when entry returns (`spec/kernel-spec.md:2975`); source cleanup has defined edges (`:855`). | Parent owns a job that reports creation failure, completion, I/O error or cancellation and is always reaped. A child consumes its new output/scratch owners, reports status, then uses a minimal exit path without running parent cleanup. Failed/cancelled output is unpublished; parent publishes only after successful completion. No orphan child may indefinitely retain a snapshot. |

**Hard reserve enforcement remains open.** Software versions can reserve
space before publishing a mutation. Ordinary OS COW faults do not pass
through Whitefoot's allocation counter; sampling and then killing a child
can overshoot before it releases pages. Inference: a hard service-preserving
fork bound needs worst-case reservation, a bound covering mutation and abort
latency, or write interception. A child-only limit or an OS action that kills
the serving parent is not that guarantee. This may disqualify fork for
decision 1's strict bounded mode even when its typical peak is excellent.

Quiescence is a real latency issue. WAIT-2 promises eventual progress only
under its finite-steps-to-wait/completion premise, with no wall-clock bound
(`spec/kernel-spec.md:2252`). Drivers are not preemptive
(`design/compiler/waiting-contexts.md:29`). Waiting for a long atomic block or
computation writing the captured dataset can delay capture indefinitely in
elapsed time; unrelated compute need not finish if its copied state is
unreachable from the child.
Adding safe points or restricting snapshot writers would itself need a
design and inactive-cost comparison; it cannot be hidden in “fork is fast.”

**Allocator consistency is an unproved platform assumption.** Whitefoot's
heap calls `malloc`/`realloc`/`free` (`compiler/src/backend/heap.c:8`, `:18`,
`:29`). [glibc 2.39's fork wrapper](https://github.com/bminor/glibc/blob/glibc-2.39/posix/fork.c#L61)
takes malloc locks and releases them in the child;
[macOS libmalloc's fork hooks](https://github.com/apple-oss-distributions/libmalloc/blob/c49dafa25f1efe8607701ae6014a663ad2ee437f/src/malloc.c#L3723)
lock allocator zones and reinitialize child locks. These support platform
qualification, not a POSIX promise that arbitrary child allocation is safe.
Holding dataset locks alone does not prove the allocator or native call closure.

**Possible surface, not valid Whitefoot syntax:**

```text
capture(dataset-domain, reserve) -> FrozenView + cut | refusal
start_export(FrozenView, moved-output, bounded-scratch, static-function)
    -> owned SnapshotJob | refusal returning the moved resources
finish/cancel(SnapshotJob) -> completion status and resource disposition
```

Capture rendezvous and job waits belong to waiting functions. SHARE-2 forbids
a waiting call inside an atomic block (`spec/kernel-spec.md:2306`). A native
fork host call is not itself a WAIT-1 wait: that boundary is the callable's
`waits` declaration (`spec/kernel-spec.md:1533`). Holding the dataset locks
across that call is therefore a capture/publication protocol question, not a
contradiction of SHARE-2. Define how the cut and log rotation share one point;
main supplies neither a fork callable nor that protocol today.

The function reads only the frozen view, mutates only its own scratch and
writes only its designated output. It cannot reach live Shared handles,
spawn, fork again or use copied invocation handles. Ordinary references
remain call-local (REF-3, `spec/kernel-spec.md:694`). Its source capture and
call closure require machine checking; an effect row reading a Shared handle
does **not** prove its state immutable. A restricted child also needs a
separate, preinitialized output/allocation path: ordinary `waits`, `pure` or
`no_heap` alone does not prove the underlying native calls async-signal-safe.
Linux/macOS qualification must inspect every emitted helper and linked body;
merely replacing application locks is insufficient.

This restriction should describe ordinary data/resource contracts equally
for source and linked bodies, not a list of privileged host operation names
(SCOPE-1/3 and `docs/constitution.md:70`; historical refusal at
`design/language/system-interface.md:20`). The scoped-meter record also
identifies the generic runner body-provider choice: PRE-2 host
modules have no source implementation records and FN-5 binds direct calls
(`spec/kernel-spec.md:2552`, `:2133`). Its branch uses a checked Whitefoot
runner; that proposed host-module extension is absent from main. A native
fork wrapper calling arbitrary waiting functions still needs an account of
who supplies its instantiated body. A general frozen view usable
by ordinary in-process code may avoid the restricted child altogether.

**Specification impact if selected.** Preserve SCOPE-3's conditional no-UB
promise, rather than exempting fork. Add capture/cut, frozen-view reachability,
lifetime, resource-refusal and export-job rules; extend SHARE-1/2/3 and
PRE-1 if views of shared datasets become language-supported. PRE-2 must
define process/job authority, handle transfer, child inputs, cancellation,
exit and meter meaning. WAIT-1 remains the waiting boundary; WAIT-2 needs
the child execution and progress relation. WAIT-3 can remain structured
in-process spawn if a separate job API owns the child; a fork-spelling spawn
would instead require changing its argument/result/join rules. PAR-1/2 need
no permission relaxation: runtime capture must honor existing joins and may
run child computation sequentially. CAP-1's one-concurrency-construct account
(`spec/kernel-spec.md:2146`) must be reconciled if child execution is exposed.
STOR-8/REF/OWN/PROV need examination for regions or shared immutable owners;
an ordinary opaque snapshot value is not permission to duplicate arbitrary
linear values. These are proposed rule deltas, not amendments made here.

**Can no undefined behavior still hold?** In principle yes, with statically
enforced capture restrictions and a runtime/OS implementation satisfying all
those contracts. General `fork() -> parent-or-child` over today's arbitrary
Whitefoot continuation cannot inherit that conclusion. Restricted fork is
still unverified, particularly its post-fork native call closure and bounded
quiescence. Reject a backend whose safety depends on writers avoiding a
reachable operation by convention.

## Platforms, including small embedded CPUs

Current target layouts are x86-64/AArch64 Linux and macOS, and x86-64 Windows
(`compiler/src/target.rs:57`). Architecture alone does not supply an OS or
MMU. The portable contract must allow an explicit unavailable/budget outcome
or a separately selected implementation with the same consistency; silently
substituting an eager copy would conceal the failed memory requirement.

| Route | Linux / macOS with MMU | Windows x86-64 | MMU-less embedded |
| --- | --- | --- | --- |
| Process fork | Native private-memory COW; qualify the restricted child independently on each OS, including libraries, allocator and signals. | No equivalent CreateProcess heap continuation; starting an exporter requires serialized data or explicitly shared immutable storage. | No hardware page COW; unavailable, or software versioning/full copying with its stated costs. `vfork` is no concurrent snapshot substitute. |
| Persistent maps, epochs, MVCC | Software algorithms work without fork; thread synchronization and reclamation remain necessary. | Same logical route; no dependency on Unix processes. | Use bounded pools and software indirection/copying; single-core execution can simplify synchronization, not old-version storage. Interrupt/DMA writers must participate or be outside the captured domain. |
| Batches plus log | Ordinary files and bounded buffers; spill/catch-up costs apply. | Ordinary file/worker process or context, same cut protocol. | Fixed buffers and flash/external storage if available; account for write latency/endurance. With neither storage nor RAM reserve, fail or pause explicitly. |
| Snapshot region / OS faults | Linux userfaultfd write protection or mprotect/fault handling can preserve a page before a write. macOS needs its own VM/fault adapter; userfaultfd is not portable. | File mappings, protection faults or process snapshots are candidates, not a VirtualAlloc switch for cloning any heap. | Software barriers and region handles replace page faults; no transparent alias remapping. Full-copy or paused export is an explicitly weaker resource/latency mode. |
| Checkpoint packages / disk-backed store | CRIU/DMTCP are Linux-oriented process mechanisms; qualify macOS separately. A logical disk-backed root needs no fork. | Native process snapshot facilities differ from a runnable child; a logical store remains viable. | Logical application checkpoints can be designed for fixed storage. Desktop process-image tooling is not an embedded baseline. |

Platform grounds: [CreateProcess starts a new executable process](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessw);
[Linux no-MMU mapping documentation](https://docs.kernel.org/admin-guide/mm/nommu-mmap.html)
states that uClinux lacks fork. [Linux userfaultfd](https://docs.kernel.org/admin-guide/mm/userfaultfd.html)
offers feature-negotiated write protection: a faulting writer waits for its
handler. **Inference:** protecting pages one by one is not an atomic dataset
cut; a barrier or epoch protocol is still needed. Handler buffers and locks
must not fault recursively, and kernel/device writes need their own account.

[Windows MapViewOfFile](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-mapviewoffile)
offers FILE_MAP_COPY over a file mapping and charges commit for the whole
view; it does not capture another view's already-private dirty pages.
[PAGE_WRITECOPY is unsupported by VirtualAlloc](https://learn.microsoft.com/en-us/windows/win32/memory/memory-protection-constants).
**Inference:** mapping tricks require an intentionally designed, stable
backing generation, offset/handle references and a cut protocol, not merely
a second mapping of a live mutable heap. Windows also exposes
[PSS_CAPTURE_VA_CLONE](https://learn.microsoft.com/en-us/windows/win32/api/processsnapshot/ne-processsnapshot-pss_capture_flags)
for cloneable memory. This is worth evaluating for export, but it does not
document a general fork continuation or Whitefoot-level atomicity. None of
these Windows routes has been implemented or measured here.

## Maxmemory and the paused scoped-metering work

The owner ruled on the board card **firn-maxmem-scope** on 2026-10-10:
firn's rewrite must stop building a complete in-memory copy, so that
maxmemory stays close to what the server actually occupies; the ruling names
a chunked scan of the live keyspace completed by an incremental log, or a
replay that writes as it builds with a bounded working set. That ruling
fixes this investigation's acceptance target for firn: a snapshot's extra
memory V + B must be small relative to D under the stated workload, and it
counts against the server's real footprint rather than being excluded from
it. The ruling chooses the outcome, not the mechanism; the routes above are
the candidates for reaching it.

Two consequences follow. First, “actual footprint” still needs a precise
meter: PRE-2's heap count omits stacks, allocator reserves and executable
mappings, and RSS is optional (`spec/kernel-spec.md:2560`), so neither is
already a hard total-footprint limit. Second, fork does not satisfy the
ruling by moving bytes out of the parent's counter: pages the parent dirties
during export are copied, and that page-amplified divergence is part of the
deployment's footprint. The copies never pass through `wf__heap_take` or
`wf__heap_change` (`compiler/src/backend/heap.c:7`,
`compiler/src/backend/completion/bridge.c:1362`): no in-process allocation
meter, scoped or otherwise, sees them. A fork route meets the ruling only under
a stated dirty-page budget with the failure policy of decision 1.

Only the kernel's physical-memory view can account for fork-route V. On Linux,
collect parent and child [`Private_Dirty` in `/proc/<pid>/smaps`](https://docs.kernel.org/filesystems/proc.html)
alongside aggregate physical memory. Redis 7.0.15 uses the same proxy
([`src/zmalloc.c:655`, `:716`](https://github.com/redis/redis/blob/7.0.15/src/zmalloc.c#L655),
[`src/childinfo.c:91`, `:126`, `:133`](https://github.com/redis/redis/blob/7.0.15/src/childinfo.c#L91))
for [`current_cow_size` / `rdb_last_cow_size`](https://redis.io/docs/latest/commands/info/),
giving the comparison a reference instrument. Private dirtiness also includes
scratch and other private writes; sampling it is evidence, not hard reserve
enforcement.

[Whitefoot #322, scoped metering](https://github.com/Ming-Research/Whitefoot/pull/322)
is paused for acceptance per the owner's request. Its
[cross-branch record](https://github.com/Ming-Research/Whitefoot/blob/7f2743c3914aba23991e416d28499d6918e3a23d/research/investigations/scoped-metering/README.md)
was read from `origin/claude/scoped-meter`: lines 263–308 explain why charges
must follow allocation origin across transfers; 399–402 and 519–520 mark
implementation evidence unvalidated; 536–557 distinguish origin from privacy
and separate samples from an atomic count. Its prefix/arena costs remain
open. Those observations survive this broader investigation.

| Chosen route | What becomes unnecessary, and what remains |
| --- | --- |
| Restricted fork export | Replay scopes disappear, and allocation meters cannot see V at all; scoped metering has no consumer here. Kernel physical-memory accounting, a dataset-vs-total policy and reserve enforcement remain necessary. |
| Versioned map or snapshot region | Replay disappears; old versions remain in the same process. Container/region live-versus-retained accounting can replace general context attribution for this task. A block originally allocated by a client can later become snapshot-only: origin scopes alone do not classify that transition. |
| Batches/log or disk-backed snapshots | No full private replay heap if the protocol streams correctly; explicit buffer/log/version accounting may suffice. Spill/storage and retained-history bounds still need policy. |
| Retain current private replay | Dataset-style admission still needs accurate exclusion, and scopes remain one candidate. A whole-deployment limit should include rewrite storage instead; excluding it defeats that policy. Neither choice fixes replay's CPU/memory costs. |

Under the ruling, the current private replay is no longer a destination,
so #322 loses the consumer it was built for: no route above needs the
rewrite's allocations excluded from admission. Its possible value for
independent accounting consumers (per-tenant or per-request metering)
remains, but no such consumer exists in the maintained programs today.
Fork requires kernel accounting; in-process routes favor container-level
live-versus-retained accounting because allocation origin cannot classify a
block that later becomes snapshot-only. This strengthens decision 4's
recommendation to keep #322 paused, rather than selecting it before a consumer.

## Decisions for the owner

These four proposals are in dependency order; none is assumed decided.

### 1. What must yield when snapshot retention exhausts its budget?

**Background.** The overwrite witness requires old information somewhere;
an unconditional low-RAM, always-completing snapshot beside unlimited writes
is impossible. Capture itself also needs a latency budget.

**Options.** **A, recommended:** a point-in-time dataset view with an explicit
reserve and service-first refusal/abort, bounded cleanup, and consumer-set
pause/latency targets. Continuous heavy writes may prevent a successful
snapshot. **B:** guarantee completion by throttling or stopping writers;
trades service availability for persistence progress. **C:** spill retained
versions/logs to storage or a replica; buys progress with I/O, space and a
larger failure model. A can admit C as an explicit deployment policy later.

**Confidence 5/5** in the impossibility boundary; **3/5** in recommending A.
Mandatory checkpoint deadlines or a firn durability requirement could favor B/C.

### 2. Is the public abstraction a frozen dataset or a general fork?

**Background.** The named consumers need old data, not duplicated sockets,
drivers or continuations. SharedRead and batched scan do not provide it today.

**Options.** **A, recommended:** an opt-in frozen dataset domain and owned
export job; define transitive capture and one multi-object cut. It needs a
new ownership/representation contract but confines obligations to participants.
**B:** restricted fork running one read-only function with designated outputs;
avoids a full per-entry version design on Unix but requires the child safety
closure above and another route elsewhere. **C:** general fork returning into
both continuations; largest expressiveness, but broad changes to ownership,
contexts, handles and platform guarantees, with no independent consumer here.
A permits B as a backend without making fork the source abstraction.

**Confidence 4/5.** The current contracts establish the missing snapshot and
fork hazards; a real consumer needing process continuation could reopen C.

### 3. Which mechanisms deserve the first comparative implementation?

**Background.** No Whitefoot measurement ranks software versions, a safe
fork and a reconciled scan-plus-log export. Redis's result establishes
motivation, not a portable winner. The firn-maxmem-scope ruling names the
scan-plus-log route explicitly; it is correct only with the per-record
sequence numbers and after-images described under routes.

**Options.** **A, recommended:** implement the three candidates of the fixed
comparison, an opt-in versioned dataset, restricted Unix fork and a
reconciled batched export, against one cut/resource contract and one trace
set; the first and third are the Windows/MMU-less paths. Cost: three bounded
research implementations, no commitment yet to change every
ConcurrentHashMap. **B:** start with the reconciled batched export alone,
since it needs no new language representation and the ruling names it; the
risk is a write-heavy workload whose log outruns the scan, which only the
comparison would expose. **C:** fork first and defer other hosts; quickest
access to OS COW, but leaves platform coverage and post-fork safety
unresolved and does not by itself meet the footprint ruling. Full copy and
replay remain controls, not hidden fallbacks.

**Confidence 3/5.** Workload dirtiness, value representation, capture pause,
native-call safety and measured inactive cost can change the ranking.

### 4. What happens to scoped metering (#322)?

**Background.** #322 makes allocations made inside a scope countable and
excludable; firn needed it to keep its private replay keyspace out of
admission. The firn-maxmem-scope ruling removes that replay, so #322 has no
consumer in maintained programs. Its experiment release is green, its
completion review's one finding is fixed, and its acceptance is paused.
Fork's COW pages bypass every allocation scope; in-process snapshot retention
changes a block's role without changing its origin. Neither route supplies
the missing scoped-meter consumer, strengthening recommendation A.

**Options.** **A, recommended:** keep #322 paused as a draft, unmerged, and
reopen it when a real consumer (per-tenant or per-request metering, or a
snapshot route that needs live-versus-retained attribution) appears; cost:
the branch drifts from main and needs a rebase when reopened. **B:** finish
acceptance and merge it now as a general capability; cost: a heap prefix and
per-structure tags charged to every program, plus specification surface,
with no consumer to validate it. **C:** close it and keep its research
record as evidence; cheapest, but a future consumer restarts from the
record instead of the branch.

**Confidence 3/5.** A rests on judgment that unused capability should not
enter the main line; a concrete consumer would favor B.
