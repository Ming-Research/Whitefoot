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
  `Paged<T>` is absent from this checkout; its page and directory allocations
  must adopt these wrappers when it lands. Each release site knows the size it frees:
  the type for a `Box`, the stored capacity for the others.
- **The runtime's pool** (`completion/bridge.c`, `wf_pool_take`/`give`).
  Under one spin lock it grants size classes and maps larger blocks. It
  holds context frames, timers, concurrent-map tables and nodes, key sets,
  and shared objects. The granted size is known at both ends.
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

Proposal A, including the resident-set reading from D, is being implemented on this branch following owner approval. It counts per driver modulo 2^64 with a clamped non-instantaneous sum, keeps heap-free programs free of the allocator, and returns the resident set as an optional value. CI correctness validation and the stated firn performance comparison remain outstanding.

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

Summing independently sampled counters is exact when no other context
allocates or releases during the reading. Otherwise the reading may differ
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

This checkout has no Paged implementation. Its existing direct-allocation
sites are boxes, runtime-capacity windows, buffers and segments. Adoption of
Paged must use the same counted allocation and size-aware release ABI.

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
