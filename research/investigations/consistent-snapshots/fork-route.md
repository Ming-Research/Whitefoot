# Restricted fork behind frozen-dataset export

## Scope and grounds

**Proposal, 2026-10-10.** Can a restricted Linux child export one dataset cut
while the parent serves, without inheriting usable runtime machinery, and meet
the service-first reserve ruling? Compare it with the persistent library and
reconciled batched export under the [fixed comparison](README.md#comparison-fixed-before-future-measurements).
Unsafe child reachability, an inconsistent cut, or an unbounded reserve mode
rejects the candidate before performance can select it.

The [owner's rulings](README.md#owner-decisions) and
[frozen-datasets decision](../../../design/language/data-model/frozen-datasets.md)
stand: opt-in persistent library first; fork is only a possible backend behind
the dataset abstraction. This export-only candidate introduces no source fork
continuation or automatic freezing of arbitrary shared values. **Only Linux
x86-64 and AArch64 with an MMU are in scope.** macOS, Windows and MMU-less
targets are out of scope; no result here qualifies them.

**Citation convention.** Backend `file:line` references below were verified
against main as represented by `origin/main`,
`24dfcf4e98c6ce4127183b2c1462f1b1112307ba`; its cited backend/specification
bytes match this worktree. The overview's older line numbers are not reused
unchecked. The active
specification there is v0.121. **Deduction** marks a consequence of stated
premises; **proposal** marks unimplemented work. No build, execution,
measurement or specification amendment accompanies this investigation.

## 1. Child execution contract — B6/B7

**Verified runtime citations.** These are present hazards, not proposed fixes:

| Fact at main | Evidence under `compiler/src/backend/` |
| --- | --- |
| Entry already has another native thread receiving stops. | `wf_floor.c:327`, `:348`, `:353`; `completion/stop_signals.c:168`. |
| SQ/CQ mappings, including the single-mapping variant, and SQEs are `MAP_SHARED`; submissions and consumption write ring indices. | `completion/linux_io_uring.c:258`, `:276`, `:291`, `:311`, `:625`, `:1095`. |
| A copied driver's TLS still selects its adapter; additional drivers construct their own runtimes/rings and threads. Clearing only TLS would fall back to the global adapter. | `completion/bridge.c:348`, `:350`, `:3365`, `:3403`, `:3424`. |
| Compute startup records survive without their threads. Publication can wake an idle lane through its pthread mutex, which may have an absent owner in the child. | `sched/core.c:341`, `:714`, `:765`, `:791`, `:1254`, `:1280`, `:1328`; `sched/prim_host.c:286`. |
| Ordinary heap operations use the system allocator; runtime-pool allocations have another inherited implementation. | `heap.c:17`, `:35`, `:46`; `completion/bridge.c:1035`, `:2759`. |
| Helper-dependent file work has queues and separately started threads. | `completion/file_adapter.c:279`, `:300`, `:592`. |
| The Linux wait set owns epoll/eventfd descriptors; macOS's separate stop path creates a kqueue. The latter's noninheritance hazard in the overview is outside this candidate. | `completion/linux_io_uring.c:121`, `:125`; `completion/stop_signals.c:214`. |

Inspection of all backend C/headers found no `pthread_atfork`,
`MADV_DONTFORK`, `MADV_WIPEONFORK` or `close_range` integration. The native
`fork` in `floor_probe.c:239` immediately executes a fresh executable
(`:245`); it supplies no dataset-child implementation.

**Proposal.** The child enters a new native capsule, calls exactly one fixed
export function with a call-local `&FrozenDataset`, designated output owners
and bounded scratch, then exits. It may read the complete captured data,
encode it, mutate its scratch, and write those outputs. `FrozenDataset` here
names a contract, not a new storage domain: its transitive content is private
COW data, with no host resources, externally mutable mappings or outside
writers. A `SharedRead` handle alone does not establish that closure
(SHARE-1/2, `spec/kernel-spec.md:2272`, `:2297`).

The child must never resume the parent continuation or touch completion
rings, adapters, drivers, ready/parked contexts, compute queues/joins,
timers, cancellation state, invocation capabilities or the parent's handles.
It neither spawns, forks again, publishes files, nor runs inherited cleanup.
Even immutable-node traversal must avoid copied shared locks and refcount
updates; ordinary shared helpers change handle counts
(`completion/bridge.c:2804`, `:2809`). An audited native traversal can witness
this first; a general Whitefoot exporter still needs a machine-checked call,
value and release closure. `pure`, `waits` and `no_heap` do not supply that
proof, and linked bodies owe the same contracts as source bodies.

Enforce the capsule as follows:

- Apply checked `MADV_DONTFORK` at creation to **every** SQ/CQ/SQE mapping,
  deduplicating the single-mapping alias. Refuse this backend if any advice
  fails. `MADV_WIPEONFORK` is for private anonymous capsule metadata only;
  it cannot sanitize these `MAP_SHARED` rings. [Linux madvise contract](https://raw.githubusercontent.com/mkerrisk/man-pages/master/man2/madvise.2)
- Install fresh single-threaded capsule state: no driver/context roots,
  no global-adapter fallback, no timers, no compute pool. Clear copied TLS
  associations and prohibit lazy pool startup; merely setting `started=0`
  would invite another startup. The capsule calls none of the old queue,
  shutdown or release paths. Reset inherited signal dispositions/masks through
  audited primitives so handlers cannot reenter the parent runtime.
- Preallocate a child-only scratch arena before capture and initialize its
  private bookkeeping in the child. The first encoder needs no allocation;
  later scratch allocation uses only this arena, never inherited
  `malloc`/`realloc`/`free` or pool state. Reinitializing an allocator mutex
  cannot repair half-written allocator metadata. Qualify the parent fork
  wrapper too: [glibc 2.39](https://github.com/bminor/glibc/blob/glibc-2.39/posix/fork.c#L61)
  coordinates malloc locks and resets child locks, but that is not permission
  for arbitrary child library calls or a qualification of another libc.
- Close every inherited descriptor except the designated outputs and one
  bounded result channel. Use raw `close_range` calls over the gaps in a
  precomputed descriptor allowlist, including unwanted stdin/stdout/stderr;
  `CLOEXEC` alone is ineffective without exec. Never call socket shutdown or
  source-handle close/refund on copied owners. Designated files must be fresh
  unpublished outputs with no concurrent parent offset/flag mutation:
  inherited descriptors share open file descriptions. [Linux close_range](https://raw.githubusercontent.com/mkerrisk/man-pages/master/man2/close_range.2),
  [Linux fork contract](https://raw.githubusercontent.com/mkerrisk/man-pages/master/man2/fork.2)

**Deduction.** A forbidden ring access fault is useful containment evidence,
not an admissible source trap. The production closure must make it unreachable.
The whole post-fork native closure, including compiler-emitted helpers,
dynamic linking/TLS and architecture-specific syscall stubs, remains to be
qualified independently on x86-64 and AArch64.

## 2. Capture and the log boundary

**Proposal.** Prepare outputs, result channel, scratch, job storage and
admission outside dataset locks. A waiting rendezvous stops new participating
mutations and drains only work that can mutate the captured data or publish
its log record; it must not wait for unrelated clients' pending I/O.

At the cut, hold the dataset's publication object and transaction/log state
together. For an in-place fork-backed representation, hold **whole-map**
locks for every participating map plus all separately shared mutable objects
in the transitive dataset closure. All writers, including expiration and
read-side metadata changes, must use those holds. For already immutable
persistent nodes, publication/log holds suffice; do not lock every node.
Use the existing type/object/key acquisition order and hold-until-block-end
discipline (`design/compiler/waiting-contexts/state-locks.md:3`, `:7`).
Whole-map exclusion waits out active keyed statements
(`compiler/src/backend/concurrent_map.c:1363`, `:1383`).

While those holds remain acquired, select transaction sequence **N**, its
exact log-prefix boundary and captured expiry policy, complete the frozen
descriptor, and invoke the synchronous native fork leaf. No writer can
change that data between recording N and fork; any point in that interval is
the logical cut **t**. The child image contains N and the same descriptor.
The parent installs the suffix starting at N+1 before releasing the holds.
Sequence assignment, dataset mutation and log publication must already share
the transaction protocol; include committed-but-buffered records in the
boundary rather than equating it with the current file offset. No disk sync
or log copying belongs under these holds. Preserve the previous base and log
until the new export is successfully published.

SHARE-3 supplies the combined atomic point (`spec/kernel-spec.md:2313`);
SHARE-2 forbids a waiting call inside its block (`:2308`). Thus rendezvous is
outside, and only a genuinely nonsuspending leaf may run inside. Calling a
waiting launcher “nonwaiting” to fit there would violate its contract.
The interface/body-provider question in section 5 must settle this before a
Whitefoot witness. The child obtains a reference into its **own** native
image at entry; no source `Entries` reference or parent borrow escapes the
atomic block. It traverses data without acquiring/unlocking the copied holds.
Outstanding kernel/device writes into captured buffers, shared mappings and
uncoordinated nested writers disqualify capture; resetting locks cannot fix
a torn dataset.

**Deduction/estimate.** Pause is quiescence plus held bookkeeping, fork
page-table work and parent release, not O(1). The overview's external
[Redis fork timings](https://redis.io/docs/latest/operate/oss_and_stack/management/optimization/latency/#fork-time-in-different-systems)
of about 10–13 ms/GB scale to roughly 2–3 ms for 200–240 MB; this is
neither a Whitefoot measurement nor an AArch64 estimate. Reaching quiescence
can dominate and has no current wall-clock bound. Measure it separately;
fix consumer pause limits before acceptance. Record base-page size, VMA/PTE
counts and huge-page behavior; do not assume 4 KiB on every AArch64 kernel.

## 3. Retained V, reserve and maxmemory

**Proposal.** The only retained-V instrument for this route is the child's
kernel-reported private dirty memory: sum `Private_Dirty` in
`/proc/<pid>/smaps`, or use `smaps_rollup`. This is a conservative **proxy**,
not a count of logical dataset bytes: it includes scratch and divergence of
inherited non-dataset pages. Charge the whole reading; do not subtract an
initial baseline that could hide retained pages. Parent writes can make an
old child page private even when the exporter never writes it.
`/proc/<pid>/status` has RSS/anonymous/page-table/swap fields, **no
Private_Dirty field**; `RssAnon` cannot substitute. Qualify anonymous resident
dataset pages and page-accounting behavior; swap, hugetlb/KSM or private clean
backing require additional qualification before this proxy can bound V.
[Kernel proc accounting](https://docs.kernel.org/filesystems/proc.html)

Parent `heap_in_use` remains its process's logical allocation holding, not
parent-plus-child memory. It sums pool blocks and thread slots
(`compiler/src/backend/completion/bridge.c:1357`, `:1362`, `:1375`); the child
inherits those slots, including vanished threads, so must not use its copied
meter. Kernel COW does not call the heap allocation/change functions
(`compiler/src/backend/heap.c:16`, `:26`). Even `resident_bytes` reads only
`/proc/self/statm` (`compiler/src/backend/completion/bridge.c:1397`). These
facts match PRE-2's process-local meter (`spec/kernel-spec.md:2562`).

**Proposed policy.** Admit one active child against a single reserve **R**.
Private_Dirty is its sampled usage; charge fixed overhead outside that reading
(such as page tables and the parent job) separately within R. Scratch already
in Private_Dirty is not charged again. A parent supervisor polls
Private_Dirty on its own schedule; on threshold, lost accounting, cancellation
or export timeout, it kills the child and records **Aborted**. Writers keep
serving; no throttle or allocation-scope exclusion pays for export.
If J is the explicitly owned job storage already in the parent's admission
counter, maxmemory charges `parent_counter - J + R`: one reserved allowance,
with Private_Dirty reported as its usage, never an extra charge on top of R.
J comes from known preparation/job allocations, requiring no scoped meter.
Expose the parent counter, reserve and usage. Aggregate physical memory without
double-counting shared pages is separate comparison evidence, not a second V
counter. Parent allocator slack, stacks, child page tables and kernel/file
buffers still affect actual footprint. Fork inherits more than the dataset.

**Deduction: strict bound unresolved.** Poll-and-kill is an abort mechanism,
not a hard reserve proof. An early threshold `R - outside_overhead - G` needs
headroom G covering maximum divergence during polling/read/scheduling delay
and confirmed-stop latency. With a proved growth bound q and delay bounds
Δ/L, `G >= q(Δ + L)` plus accounting uncertainty is a possible condition;
there are no such bounds here. All-keys replacement and a stalled exporter
must try to falsify them. SIGKILL also does not promise a deadline for freeing
pages from uninterruptible kernel work. Keep the reserve charged until exit
is confirmed, forbid another child meanwhile, and leave the previous base/log
usable. Without qualified bounds or worst-case reservation, this candidate
**fails the ruling's strict bounded mode**, however good its typical peak.

## 4. Cancellation, failure and completion — B6/B8

**Proposal.** A linear parent-owned `SnapshotJob` owns the child identity,
cut, unpublished output owners, result channel, scratch/reserve and reaping
obligation. Start transfers the frozen image authority, outputs and scratch
together. A creation refusal with **no child** returns all prepared resources;
every child created, including one whose setup fails, belongs to a job until
confirmed exit/reap. Post-fork setup failure is a failed job, never an
immediate resource-returning refusal. After successful
fork the parent keeps no unnecessary old-root pin; inherited pages retain
the child's image. No parent source owner is duplicated or refunded twice.
Use a stable process handle where available and one designated reaper.

The lifecycle is `Prepared -> Running -> Completed | Failed | Aborted`, with
terminal resource return **only after confirmed exit and reap**. A bounded
child result record reports encoding/I/O status and cut N; exit status
distinguishes success, failure and signal death. Only exit zero plus a complete
validated result/output authorizes parent-side sync and atomic publication.
Short writes/errors, missing results, faults or abnormal exits leave partial
output unpublished. Parent cleanup closes/removes those temporary outputs
and retains the previous usable base and log; it never replays child cleanup.

A parent cancellation watch is observed by the supervisor, never copied as
the child's cancellation mechanism: COW cannot convey later firing. Request
abort, signal the child, and continue monitoring/reaping. A request is not
completion (PRE-2, `spec/kernel-spec.md:2560`). Bound the amount of user-space
cleanup per service turn; measured kill/reap/unlink latency must satisfy the
consumer target. Current filesystem waits have no cancel/deadline bound
(`:2561`), so a regular-file syscall can prevent a strict cleanup guarantee.
A bounded nonblocking output transport is an alternative to qualify, not
evidence that disk cleanup is bounded.

On orderly shutdown, refuse starts, abort outstanding jobs, finish reaping
before retiring their owning drivers, then release credits and outputs. For
abrupt parent death, the capsule sets `PR_SET_PDEATHSIG(SIGKILL)` and checks
the saved parent identity after installation to cover the setup race. This
signal is tied to the **fork-calling thread**, whose lifetime must cover the
job; it is not a process-wide shutdown guarantee. [Linux parent-death contract](https://raw.githubusercontent.com/mkerrisk/man-pages/master/man2/prctl.2)
An uninterruptible surviving child remains a cleanup failure, not a freed
reserve or a completed snapshot.

## 5. Minimal library/runtime surface

**Proposal: signature sketches, not specification text or checked syntax.**

```text
prepare_export(moved_outputs, scratch_bound, reserve)
    -> Prepared | Refused(returned_outputs)                         waits
capture_export_held(dataset_ref, cut, moved_prepared, export_body)
    -> SnapshotJob | RefusedNoChild(returned_prepared)              no context suspension
finish_export(moved_job, abort_request)
    -> Completed(artifact, cut) | Failed(resources) | Aborted(resources)  waits
```

`abort_request` is parent-side job state driven by cancellation/reserve policy;
it is not ordinary file-transfer `Cancelled` with a zero-byte promise.
Monitoring/signalling can stay private to the supervisor; no source PID,
descriptor, raw function pointer or `fork() -> parent-or-child` is exposed.
The library's export operation owns capture and lifecycle; its caller sees
the same dataset/result contract for each candidate. This export-only profile
does not yet implement a general parent-usable frozen lookup/enumeration API.

PRE-2 would need ordinary declarations for owned preparation/job/output
disposition, refusal and completion (`spec/kernel-spec.md:2554`, `:2556`).
WAIT-1 determines the waiting boundary (`:1535`), and SHARE-2/3 determine the
held leaf's placement; syscall duration alone does not select a `waits` atom.
WAIT-2/SCOPE-3 need the child execution, native closure and progress account;
WAIT-3's source spawn/join need not change. REF-3, OWN/STOR-3 and HOST-1 still
govern local borrows, exactly-once resource disposition and publication order.
No fork permission or new dataset storage domain is proposed.

**Open implementation boundary.** A host module has no source implementation
record, while FN-5 binds function-kind calls. Passing `export_body` does not
explain who supplies its concrete checked body or proves its capsule closure.
Recommend a fixed native C encoder for the first runtime witness; identify
the minimal missing general library operation before proposing a PRE-2/body
provider amendment. Do not accept arbitrary callbacks by convention or add
host-name privilege tables. These are future rule questions; this document
changes no rule.

## 6. Smallest prototype and comparison evidence

**Proposal; not executed here.** Add Linux-only ring advice at the mapping
sites and an isolated native capsule/start/reap unit beside the completion
backend. Link a C runtime test through the existing runtime-test ownership,
not from research into the gate. The first capsule uses fixed stack/scratch,
raw designated-output writes and a minimal exit; it never links an exporter
through the ordinary scheduler/allocator paths.

The C witness starts a real parent ring and compute pool, arranges a lane
wait mutex held by another thread, and captures two transaction-related
records plus sequence N under their common gate. A latch lets the parent
mutate/free its data after fork while the child exports the old cut.
Compare bytes and suffix replay with an independent serial-history model;
parent ring I/O and compute must continue, and the child must finish without
waiting for the held lane mutex. Test descriptor isolation, short writes,
output failure, refusal, cancellation and reaping/partial-file cleanup.

**Failing control:** in a separate child, deliberately read a known ring
mapping address. With DONTFORK and default fault handling, require SIGSEGV;
with the advice deliberately absent, the same read succeeds. The normal
export never touches that address and the parent ring remains usable. This
detects omitted advice without making a fault an allowed export path.
Also stall output while replacing all keys to exercise reserve abort and
record overshoot and confirmed-stop delay; a passing sample alone proves
neither a maximum delay nor general Whitefoot expressibility.

After the contract witnesses, feed the overview's identical traces into all
three candidates and the replay/no-snapshot controls. Measure unused cost,
quiescence and fork pause separately, command p99/p99.9/longest gap,
throughput, export duration, child Private_Dirty, aggregate physical/commit
pressure, parent heap/RSS, page faults, and abort/reclamation time. Record
architecture, kernel/libc, CPU affinity, driver/worker counts, page/huge-page
settings and representation. Start with a timed small CI sample; precise
paired timings belong on the idle 14900K, with interleaved same-source runs,
a base twin and the overview's prospective rejection criteria. AArch64 needs
its own safety qualification and measurements; x86-64 timing cannot rank it.

## 7. Questions for gran, the runtime owner

These are **open recommendations**, not new rulings:

1. **Which fork wrapper and native closure can be qualified?** Recommend a
   pinned Linux libc/kernel pair, an audit of its fork hooks and lock order,
   and a capsule that bypasses all inherited allocators. Do not infer safety
   from reinitialized locks or extend an x86-64 result to AArch64.
2. **How does a checked library exporter enter the capsule?** Recommend the
   fixed C witness first, then an ordinary callable/body-provider design with
   machine-checked transitive data, call and release contracts. A reachable
   shared helper or native lock without that account rejects the route.
3. **Can the cut leaf satisfy PRE-2 without suspension under SHARE-2 holds?**
   Recommend preparation/rendezvous outside and only synchronous capture
   inside; use the established lock order. If this boundary cannot be met,
   capture an immutable root and N together, then launch outside the atomic
   block; charge intervening retention and compare that distinct cost.
4. **What establishes the reserve and cleanup bounds?** Recommend treating
   poll-and-kill as exploratory until consumer R/Δ/L and worst-case divergence
   headroom are justified. Reject strict bounded mode if they cannot be
   established; no writer throttling, hidden full reservation or meter
   exclusion changes the approved service-first policy.
5. **Which outputs and parent lifetime are admissible?** Recommend fresh
   unpublished outputs, one reaper and a fork-calling thread retained through
   reap. Qualify nonblocking transport if file stalls defeat cleanup; test
   orderly shutdown and parent-death races before calling B8 complete.

## Runtime owner's answers, 2026-10-10

The runtime owner checked sections 1, 5, 6 and 7 against main and confirmed
the runtime facts, including the absence of atfork, `MADV_DONTFORK` and
`close_range` handling. Answers to section 7:

1. Use glibc's `fork()`, whose internal malloc atfork lock order applies, not
   `vfork` or `clone3`. Qualify first on x86-64 Ubuntu 24.04 with glibc 2.39
   (the hosted runners and the native i9-14900K), recording the kernel and
   libc pair in the witness's host record. The child takes no inherited
   allocator path. AArch64 needs its own qualification.
2. The first witness uses a fixed C encoder. A general Whitefoot exporter
   needs a body-provider with a machine-checked call, data and release
   closure; that is a specification question for the owner when reached.
   Callback-by-convention is not acceptable.
3. Capture inside a SHARE-2 hold is feasible on the runtime side: `fork()` is
   a synchronous system call with no coroutine suspension. Prepare output
   descriptors, scratch arena and result channel outside the hold, call only
   `fork()` inside it, and release the hold when `fork()` returns in the
   parent. The hold then includes the page-table copy, a separately measured
   pause. `MADV_DONTFORK` on the rings is applied where the rings are mapped
   (`compiler/src/backend/completion/linux_io_uring.c`), not at fork time.
4. Poll-and-kill stays exploratory; strict bounded mode is rejected if the
   bounds cannot be established.
5. Reap the child through a pidfd (`pidfd_open`, Linux 5.3 and later)
   registered as an ordinary readiness event in the waiting driver, with no
   SIGCHLD handler and no change to the stop-signal masks.

The runtime half is tracked on the status board as `firn-gap-fork-runtime`
and will be planned together with the driver handoff work, since both change
which thread owns a ring and driver; the child reset states that the child
holds no driver role.
