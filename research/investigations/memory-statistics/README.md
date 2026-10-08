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
program cannot read it today. Nothing in the standard library reports what
the program's heap holds.

The question is what a program should be able to read about its own
memory, and how the implementation counts it, cheaply enough to be read
before every write command.

## Where a Whitefoot program's memory comes from

- **Emitted code.** A `Box` is allocated with `malloc` of its content's
  size and released with `free` (`compiler/src/backend/emitter/boxes.rs`),
  and runtime-capacity storage, runs and segments do the same
  (`emitter/buffer.rs`, `runs.rs`, `segments.rs`, `cleanup.rs`).
  `Paged<T>` (open PR #263) allocates its pages and page directory with
  direct `malloc` and `free`. Each release site knows the size it frees:
  the type for a `Box`, the stored capacity for the others.
- **The runtime's pool** (`completion/bridge.c`, `wf_pool_take`/`give`).
  Under one spin lock it grants size classes and maps larger blocks. It
  holds context frames, timers, concurrent-map tables and nodes, key sets,
  and shared objects. The granted size is known at both ends.
- **The host heap,** for one descriptor registry on Windows
  (`windows_runtime.c`).

The runtime's sources may not call `malloc` or `free` themselves
(`compiler/src/backend/runtime.rs`, the allocator assertions), so only the
emitted code and Paged reach libc's heap.

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
  - The count is exact in requested bytes, and includes every source above.
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
entry, whose reads are ordered through it. A sketch, to be settled with the
candidate:

```
public opaque struct MemoryMeter { }
public fn heap_in_use(meter: &MemoryMeter) -> bytes: u64 writes(meter)
```

`Inputs` would gain the meter, and `meter_share` would give one to another
context, as `clock_share` does.

A reading depends on how other contexts' allocations interleave, so it is
an input of the execution [WAIT-2], as a clock reading is: a program may
branch on it, as firn's eviction must, without making its acceptance depend
on the host.

A counts the bytes the program requested, where Redis counts the
allocator's usable sizes, which round each request up to a size class. For
the same dataset firn's reading is therefore lower than an allocator-exact
one by the rounding, and `maxmemory` bounds the data rather than the
allocator's footprint; the resident set reported beside it shows the
difference.

## Proposal

A, with D's resident set as a second reading for `INFO`.

The validation is stated before implementing:
- a program case whose reading grows by at least the size of a box it
  allocates and returns to within a bound after release;
- the count stays exact under contexts allocating on several drivers;
- the cost of the counting layer is measured on firn's redis-bench at
  depth 1 and 16, the rule being that the counted build loses no more than
  1% against the uncounted one on the 14900K.

## Status

Proposal A, including the resident-set reading from D, is being implemented on this branch following owner approval. CI correctness validation and the stated firn performance comparison remain outstanding.

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

Summing independently sampled counters is exact after the allocating contexts
have joined; it is not an instantaneous snapshot while other contexts update
them. In particular, cross-driver allocation and release can be observed on
different sides of a read: sample A at zero, allocate eight bytes on A,
transfer and free them on B, then sample B at minus eight; the unsigned sum
is near its maximum although no block remains. The required semantics and acceptable error during
such a read remain an implementation question to settle before this can be
claimed ready for eviction. The branch has not substituted a snapshot claim
or a saturating fallback for that question.

This checkout has no Paged implementation. Its existing direct-allocation
sites are boxes, runtime-capacity windows, buffers and segments. Adoption of
Paged must use the same counted allocation and size-aware release ABI.

The resident-set paths use `/proc/self/statm` on Linux, `task_info` on macOS
and `GetProcessMemoryInfo` on Windows. The u64-only interface has no outcome
for failure to obtain that reading; the draft implementation reports a host
failure rather than fabricating zero. Whether that failure belongs outside
the execution boundary or needs an ordinary result is unresolved.

The existing allocation observers are being migrated to the two-argument
release ABI and retain allocation-request sizes to detect an incorrect size
at release. No local build, test, format check or performance measurement was
run for this implementation; the primary session owns CI validation.

The counted wrappers currently live in the always-linked completion bridge.
That introduces libc allocator references even for a no-heap entry, contrary
to the existing allocator-free runtime linkage commitment. An optional native
object for the two wrappers is the proposed integration repair, still to be
selected and wired by the primary session; the emitter and source no-heap
acceptance have not been weakened.

Context completion previously published its join before returning the context
record to the pool. The memory reading makes that ordering visible: a joined
wave could still count its final record. The draft returns that record before
publishing the join; the group resides in the starter's frame and survives the
released record. This is covered by the program's exact post-join balance,
with execution still pending CI.
