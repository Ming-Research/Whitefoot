# Memory statistics

How a program reads its process's memory use: the meter it needs, what the heap reading counts, how the runtime keeps the counters, and the resident set.

### decision memory-reads-need-a-memory-meter
A process memory reading needs the opaque `nocopy` `MemoryMeter` of `std::process`, given in Inputs and shared by `meter_share`, and `heap_in_use` and `resident_bytes` write their meter.
- rejected: an agent proposed placing the meter in `std::time` as a time capability; the owner rejected it, because it observes process memory, and following the clock's discipline does not make memory a clock.
- rejected: an agent proposed an ambient memory reading with no parameter; the owner rejected it, because a capability parameter gives an ordinary footprint that orders the readings.
- rejected: an agent proposed paying a reading with a handle-factory credit; the owner rejected it, because the observed resource is the process, not a host handle.

### decision map-reserve-release-writes-the-meter
Releasing a map's reserve is the generic host function `std::process::release_map_reserve`, with `reads(map)` and `writes(meter)`.
- replaced: The prelude function `shared_map_release_reserve` writing the map's handle was on main; the owner changed it, because it forced callers to claim a write of an unchanged handle, while writing the meter orders the release with heap readings.

### decision heap-in-use-counts-requests
`heap_in_use` counts allocation requests and their releases, emitted storage by requested bytes and the runtime pool's storage by granted bytes.
- rejected: an agent proposed querying the allocator's usable size on each release; the owner rejected it, because its meaning and availability depend on the platform allocator, and every release would pay that lookup.
- rejected: an agent proposed allocator-wide queries on each reading; the owner rejected it, because walking arenas and locking the allocator cost too much for a reading per command, and platforms expose different quantities.
- rejected: an agent proposed the resident set as the heap count; the owner rejected it, because pages can stay resident after their allocations are freed, so eviction would not observe its own progress.

### decision allocation-counters-per-driver-modulo
Allocation counters are 64-bit, taken modulo 2^64, local to each driver or compute worker and published by single-writer atomic stores; a reading adds them and clamps a negative total to zero.
- rejected: an agent proposed signed counters that stop, saturate or rebase at a limit; the owner rejected it, because a block allocated on one driver and released on another drives two counters apart without bound while the live total stays small.
- rejected: an agent proposed one shared atomic counter, as Redis 7.0.15 uses; the owner rejected it, because every driver would write one cache line on every allocation and release.

### decision pool-counter-read-without-lock
A reading loads the runtime pool's counter without taking the pool lock; the counter is an atomic on its own cache line that only a lock holder changes.
- replaced: Reading the pool counter under the pool lock was on main; the owner changed it, because readings fell from 51.1 million per second on 2 cores to 16.3 million on 8, and firn's GET on 8 cores was 40% faster without the lock (data: `research/investigations/memory-statistics/README.md`).

### decision counted-allocation-in-own-unit
The counted allocation wrappers live in a separate native translation unit that only emitted heap code references, while counter storage and readings allocate nothing.
- rejected: an agent proposed putting the allocator references in the completion bridge that every program links; the owner rejected it, because a program built without heap use must link with no `malloc` or `free` reference.

### decision resident-bytes-may-be-unavailable
`resident_bytes` returns `Option<u64>`, `None` when the host cannot report a resident set, and every memory reading is an execution input.
- rejected: an agent proposed terminating the program when an observation fails; the owner rejected it, because a host that cannot report is a recoverable condition.
- rejected: an agent proposed deriving source acceptance from a host memory reading; the owner rejected it, because each reading is an execution input.