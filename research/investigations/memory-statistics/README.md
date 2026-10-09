# Memory statistics

## The question

A Redis-compatible server bounds its memory with `maxmemory`. Before each
command that may grow the dataset, it compares the memory it uses with the
limit, and evicts keys or refuses the write while it is over. Its `INFO`
reports that memory as `used_memory`. Redis 7.0.15 reads the number from
its allocator wrapper, `zmalloc_used_memory`: every allocation and free adds
or subtracts the block's usable size (`zmalloc_size`) to one atomic counter
(`src/zmalloc.c`, `update_zmalloc_stat_alloc` and `_free`).
It does not read it from the operating system's resident set, because pages
a free returns to the allocator stay resident, so an eviction loop driven
by the resident set would overshoot and oscillate.

firn, the Redis-compatible server written in Whitefoot, needs that number:
its deployment milestone requires memory-bounded operation. A Whitefoot
program on the base of this branch cannot read it; this branch adds the
standard-library observation of what the program's heap holds.

The question is what a program should be able to read about its own
memory, and how the implementation counts it, cheaply enough to be read
before every write command.

## Where a Whitefoot program's memory comes from

- **Emitted code.** A `Box` is allocated with `wf__heap_take` of its content's
  size and released with `wf__heap_give` (`compiler/src/backend/emitter/boxes.rs`),
  and runtime-capacity storage, runs and segments do the same
  (`emitter/buffer.rs`, `runs.rs`, `segments.rs`, `cleanup.rs`).
  `Paged<T>` page and directory allocations also use these wrappers
  (`emitter/paged.rs` and `cleanup.rs`). Each release site knows the size it frees:
  the type for a `Box`, the stored capacity for the others.
- **The runtime's pool** (`completion/bridge.c`, `wf_pool_take`/`give`).
  Under one spin lock it grants size classes and maps larger blocks. It
  holds context frames, timers, concurrent-map metadata, small cell arrays
  and large nodes, key sets,
  and shared objects. The granted size is known at both ends.
- **The map's direct host mappings.** Large cell arrays and small entry
  nodes use the accounting described in the
  [concurrent-map correction](#concurrent-map-accounting-correction).
- **The host heap,** for one descriptor registry on Windows
  (`windows_runtime.c`).

Only the optional `heap.c` wrappers may call libc's allocator
(`compiler/src/backend/runtime.rs`, the allocator assertions). Counter
storage and observations in the always-linked bridge call no allocator.

## Candidates

- **A. Count at every allocation and release.**
  - Emitted code calls runtime functions `wf__heap_take(bytes)` and
    `wf__heap_give(block, bytes)`, which call `malloc` and `free` and add
    the size to a counter of the calling driver; the pool adds its granted
    sizes under its existing lock.
  - A read sums the drivers' counters. Redis 7.0.15 keeps one atomic
    counter instead; per-driver counters avoid a shared write on every
    allocation, at the cost of a sum over drivers on each read.
  - Cost: one call layer and one add to a driver-local counter per
    allocation and free, with no shared write.
  - The count is exact when allocations and releases are quiescent; emitted
    storage contributes requested bytes and pool storage contributes granted
    bytes. Concurrent reads have the bounded error described below.
- **B. Count usable sizes at release.** Wrap `free` and ask the allocator
  for the block's size (`malloc_usable_size`, `malloc_size`, `_msize`),
  as Redis 7.0.15's `zmalloc_size` does where its allocator offers it. This needs no size at the
  release site, but each platform's allocator answers differently, and
  every free pays the lookup.
- **C. Ask the allocator when read:** glibc's `mallinfo2`, macOS's zone
  statistics. Allocations pay nothing, but a read walks the allocator's
  arenas under its locks, too slow before every write command, and each
  platform's answer means something different.
- **D. The resident set from the operating system.** Cheap to read, but
  not the memory in use, for the reason Redis does not use it: it lags
  every free. `INFO` reports it beside `used_memory` (`used_memory_rss`),
  so it may be offered too, but it cannot drive eviction.

## The interface

A read of a quantity other contexts change concurrently follows the
existing host pattern of `now(clock: &Clock)`: a capability handed to the
entry, whose reads are ordered through it. The selected interface is:

```
public opaque nocopy struct MemoryMeter { }
public fn heap_in_use(meter: &MemoryMeter) -> bytes: u64 writes(meter)
public fn resident_bytes(meter: &MemoryMeter) -> bytes: Option<u64> writes(meter)
```

`Inputs` gains the meter, and `meter_share` gives one to another
context, as `clock_share` does.

A reading depends on how other contexts' allocations interleave, so it is
an input of the execution [WAIT-2], as a clock reading is: a program may
branch on it, as firn's eviction must, without making its acceptance depend
on the host.

A counts emitted storage's requested bytes and the pool's granted bytes, where Redis counts the
allocator's usable sizes, which round each request up to a size class. For
the same emitted allocation requests, the difference is libc's rounding;
pool grants include their own size-class rounding. `maxmemory` bounds the
counted storage, while a resident-set reading includes retained resident
pages and other process mappings and is not a measurement of that rounding.

## Proposal

A, with D's resident set as a second reading for `INFO`.

The validation is stated before implementing:
- a program case whose reading grows by at least the size of a box it
  allocates and returns to within a bound after release;
- the count is exact after contexts allocating on several drivers join,
  with separate evidence that at least two drivers contributed and that a
  concurrent reading obeys the selected error bound;
- the cost of the counting layer is measured on firn's redis-bench at
  depth 1 and 16, the rule being that the counted build loses no more than
  1% against the uncounted one on the 14900K.

## Status

Adopted by the owner: proposal A, including the resident-set reading from D, specified in PRE-2 ([`spec/log.md`](../../../spec/log.md)). It counts per driver modulo 2^64 with a clamped non-instantaneous sum, keeps heap-free programs free of the allocator, and returns the resident set as an optional value.

The cost was measured with firn on the i9-14900K (Firn-wf runs 37811516445 and 37812975250, branch `exp/memstats-cost`: the same firn built with this branch's experiment release `wf-exp-d89b6a051f2e` and with its base `wf-exp-2c28a4ecdd38`). With redis-benchmark `set` and `mset` at depths 1 and 16 on one and two CPUs, two passes and a twin of each image, the counted build ran at 0.971 to 1.025 of the uncounted one, inside the twins' 3 to 7% spread, so the stated 1% bound could not be resolved that way. A profile of the counted build under `set` and `mset` at depth 16 lists neither the counting functions nor `malloc` above 0.01% of samples: those commands' hot path makes no counted allocation of emitted storage. That attributes no visible cost to the emitted counting there, but it does not bound the comparative slowdown, since the pool counts inside its own operations, which the profile attributes to them; the 1% criterion remains unresolved. Scripts, whose engine allocates through the emitted heap (the C allocator took 11 to 13% of firn's CPU under rate-limiter-flexible's script), are not measured; `docs/todo.md` records that.

## Implementation findings

The capability is `std::process::MemoryMeter`: process memory is its resource,
while `std::time::Clock` supplies the capability, sharing and effect pattern.
`Inputs.memory_meter` follows `wall_clock`. The host interface is specified in
[PRE-2](../../../spec/kernel-spec.md), and `meter_share`, `heap_in_use` and
`resident_bytes` are ordinary declarations of `std::process`.

The pool counts its granted size classes, not the caller's smaller request
or the whole mapped reserve. Thus the count has no libc usable-size rounding,
but it does include pool grant rounding. The earlier claim that every byte
is the program's unrounded request was too broad. Windows's descriptor
registry also contributes its requested capacity while that table stays live.

Emitted code can allocate on compute workers as well as context drivers.
The counter inventory therefore includes both, with one registration per
thread and a separate cache line per single-writer counter. Publication uses
atomic stores to avoid a C data race with reads; it needs no shared atomic
read-modify-write for each allocation. Counter slots persist through driver
shutdown because allocation and release may occur on different drivers.

Summing independently sampled counters is exact when nothing allocates or
releases during the reading, neither another context nor a statement of
the reading's own context that overlaps it [PAR-1], since allocation and
release never prevent overlap. Otherwise the reading may differ
from the holding at every single instant by at most the bytes those
concurrent operations moved; either reading is an execution input [WAIT-2].
Every counter, including the pool's and the Windows registry's, is a 64-bit
value taken modulo 2^64, and a read adds them modulo 2^64. A driver can
allocate blocks that another driver releases, so the two drivers' lifetime
deltas grow in opposite directions without bound; modulo 2^64 their sum is
still exact, because the true total is the live heap, which is below 2^63.
A read is not a snapshot: when a block moves between drivers while the read
adds their counters, the sum can be below zero, which shows as a total above
2^63 and is clamped to zero. For example, sample A at zero, allocate eight
bytes on A, transfer and free them on B, then sample B at minus eight: the
result is zero rather than a value near 2^64.

The initial implementation covered boxes, runtime-capacity windows, buffers
and segments, before Paged was implemented. The current allocation inventory
is listed above.

The resident-set paths use `/proc/self/statm` on Linux, `task_info` on macOS
and `GetProcessMemoryInfo` on Windows. The selected result is `Option<u64>`:
host failure returns `None`, including an unavailable proc mount, failed
task-info query or failed Windows process query. A successful query returns
`Some` of the byte count. No failure to obtain this reading terminates the
program. The ordinary C representation and LLVM register-return wrapper use
the existing tag-and-payload ABI.

The formal allocation observers use the two-argument release ABI and retain
allocation-request sizes to detect an incorrect size at release. Retained
research adapters still matching `malloc`/`free` need migration before reuse;
their scope and validation are recorded in `docs/todo.md`. No local build,
test, format check or performance measurement was run for this implementation;
the primary session owns CI validation.

The counted wrappers live in `heap.c`; only emitted heap references select
that unit in the compiler's fresh and cached link paths. Native Makefile
callers receive it as an archive member, extracted only when referenced.
The completion probe inventories and Windows link list include the unit,
and the shared test-link builders select it for emitted heap dependencies.
The existing allocator-source assertion again checks the whole completion
bridge; only `heap.c` has a narrowly delimited exception. The driver inventory
test checks that a memory reading alone selects no heap unit and either heap
entry point does. All linkage execution remains pending CI.

The context program follows `compiler/tests/programs/contexts.rs` and
`parallel.rs`: it requests one and four drivers with `WF_DRIVERS`, checks the
independent arithmetic total and exact balance after joining a warmed wave.
Those existing cases do not assert that work ran on several drivers. The
compute tests' `WF_SCHED_REPORT` reports compute workers, not context drivers
or allocation-counter contributors. Context drivers expose no corresponding
report or source identity, and no usable native ring means only one driver.
Consequently this program does not establish two-driver contribution; that
qualification remains explicit in `docs/todo.md` rather than being inferred
from the requested count.

Focused native regression cases exercise real allocation on a peer thread
and release on the reader, retained deltas after thread exit, a scripted
negative sampled total and its clamp, and `None`/`Some` through the ordinary
resident-reading C body and LLVM result ABI. They do not establish the
concurrent sampling error bound, and have
not been executed here. The negative total is injected through the runtime's
accounting entry point; it is arithmetic evidence, not a scheduled context
interleaving.

Context completion previously published its join before returning the context
record to the pool. The memory reading makes that ordering visible: a joined
wave could still count its final record. The draft returns that record before
publishing the join; the group resides in the starter's frame and survives the
released record. This is covered by the program's exact post-join balance,
with execution still pending CI.

## Concurrent-map accounting correction

Source inspection at base `f887e82c46119dedf20e364c73461cb14fb2dfb3`
found that the map's direct host mappings bypassed both counted allocation
paths. This is an implementation defect against the existing
[PRE-2 memory holding](../../../spec/kernel-spec.md), not a specification
amendment. The question for the regression is whether a presized shared map
reports its live entry storage separately from its table and allocator
reserves. The unfixed prediction is zero growth during insertion; a corrected
reading must grow by at least the entries' requested bytes, then lose those
bytes on removal even though chunks remain mapped. Failure of either
observation rejects the correction.

### Lifetimes and counter path

`concurrent_map.c` reports direct cell requests and small node requests through
the host-supplied `WF_CMAP_HEAP_CHANGE`. `keyed_table.c` connects it to
`wf__heap_change`, the same function `heap.c` calls: after first registration,
one thread-local addition and a relaxed atomic publication, without a shared
read-modify-write or pool lock. The standalone C test supplies an independent
counter; the two standalone throughput adapters explicitly supply no observer
because they do not implement MemoryMeter. Their timings therefore do not
qualify the production accounting cost.

- Cell arrays at least 2 MiB count `count * sizeof(cell)` from successful
  allocation until `free_cells` unmaps them. Alignment overmapping and its
  immediate prefix/tail unmaps count nothing. Current tables, successors
  during moves and retired tables still reachable by users remain live.
  Reclamation releases old tables or retains one cell array as the map's
  spare. That spare remains a live map-owned allocation until replacement or
  destruction, like a small spare whose pool block has not been returned;
  it is not an allocator free-list block or unused capacity inside an entry
  chunk. Reuse has no accounting delta. Every whole-array unmap goes through
  `free_cells`, including pending successors, retired tables and the final
  spare at destruction.
- Nodes through 512 bytes count their `new_node` request when carved or
  reused, and lose it on `free_node`. Chunk headers, untouched chunk capacity
  and free-list nodes are reserves, not live requests. Nodes over 512 bytes
  continue through `take`/`WF_CMAP_GIVE`: `wf__runtime_take`/`give` already
  count the pool's granted sizes, so no second delta is added.
- A drain keeps its returned node live until the next call, after the caller
  releases its value; then a small node becomes uncounted chunk storage and
  a large one returns to the pool. Swapping maps transfers ownership without
  changing process totals. Clear uses swap, drain and destruction. Destruction
  also drains any remaining native slots whose payload needs no release;
  previously a direct native destroy leaked undrained large pool nodes.
  That adjacent defect is fixed in the same path. Emitted drops already drain
  their values before destruction.

### Host-allocation audit

The audit searched the C sources in `compiler/src/backend/` and its
`completion/` directory for host mapping and allocation calls, then traced
their callers and release paths. No additional omitted program-storage
allocation was found:

| Source | Disposition under PRE-2 |
| --- | --- |
| `heap.c` | Already counts successful malloc/realloc request deltas and matching frees through `wf__heap_change`. |
| `completion/bridge.c` | Host reservations feed the runtime pool. `wf_pool_take`/`give` already count granted live blocks under the pool lock; region tails and returned blocks remain excluded. |
| `windows_runtime.c` | Descriptor-registry HeapAlloc/HeapReAlloc requests already publish their byte count, which `wf__heap_in_use` sums separately. |
| `completion/linux_io_uring.c` | Submission/completion ring and submission-entry mappings are kernel-interface bookkeeping, not emitted program storage or pool blocks; excluded. |
| `wf_floor.c` | Alternate signal stacks are excluded stacks. Windows floor and IOCP code introduce no corresponding host storage allocation. |
| `floor_probe.c`, `concurrent_map_test.c` | Probe reservations and native test/reference/history allocations are test machinery, not emitted program storage. |
| Other C sources in scope | No additional direct host mapping/allocator call for program storage. No executable mapping allocation site was found in this scope. |

### Regression observations and limits

The existing `memory_statistics.wf` program, registered in
`compiler/tests/programs/memory.rs` through `compiler/tests/corpus.rs`, now
presizes a shared map for 200,000 keys. It inserts distinct three-byte keys
with inline u64 values twice, with individual removal between waves. A node
needs at least 8 bytes of length, 3 of key and 8 of value, rounded up to the
16-byte request grain: at least 32 bytes, so each wave must add at least
6,400,000 bytes over the empty map. The current Option<u64> slot layout also
fits that request. The presized table is 524,288 cells of 16 bytes, or
8,388,608 bytes, checked separately against the pre-map reading. Presizing
keeps moves from hiding an absent node delta. On the unfixed base the expected
insertion delta is exactly zero and the program exits with status 8; this is
a source-derived prediction, not an executed measurement.

After individual removal, the allowed difference from the empty-map reading
is zero bytes: the table remains live but the chunk nodes do not. The second
wave exercises free-list reuse, then clears the map to exercise bulk drain;
a final live entry is dropped with the map, after which the reading must
equal the pre-map reading exactly. Map counts independently check distinct
insertion and complete removal. No helper key storage is heap allocated.

The existing `concurrent-map-test` target also checks exact direct-request
deltas at the 2 MiB mapping threshold; fresh, freed and reused 32-byte nodes;
absence of a second count for a large pool node; pinned retirement, spare
retention/replacement/reuse; delayed drain release; and complete destruction,
including native undrained slots. Suite-end counter balance extends that
observation across the existing swap, clear and interleaving cases. The
scripted competing-removal hooks now release the displaced node and live
count, and the node-free synthetic claim is returned before destruction;
these fixture repairs preserve their race observations while making their
allocation lifetimes match real operations. The
existing cross-thread heap-counter test in backend `tests/completion.rs`
remains the evidence path for the shared counter mechanism.

No build, test, static gate, formatter or performance experiment was run
locally for this correction, as requested. Compilation, execution of the new
observations, their runtime budget and platform results remain for CI; no
passing result or measured hot-path cost is claimed. The specification and
design choices are unchanged; the retained-spare classification above is the
allocation-lifetime interpretation used by this fix.
