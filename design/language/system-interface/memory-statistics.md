Decision: A process memory reading requires the opaque nocopy MemoryMeter in std::process, appended to Inputs and shared through meter_share, and heap_in_use and resident_bytes write their meter, because the observed resource is the process while the Clock pattern provides an ordinary capability and footprint that orders observations, instead of a time capability, an ambient observation without a parameter or a handle-factory credit.

Decision: heap_in_use counts allocation requests and matching releases, with emitted storage counted by requested bytes and runtime-pool storage by granted bytes, because each allocation path and release knows its extent and eviction needs a reading that falls when storage is released, instead of querying allocator usable sizes on release, walking allocator arenas when reading or using the resident set as the heap count ([investigation](../../../research/investigations/memory-statistics/README.md)).

Decision: Emitted allocation accounting uses counters local to the calling driver or compute worker, published with single-writer atomic stores and summed on reading, while the runtime pool counts grants under its existing lock, because a global atomic read-modify-write would make every allocation contend on one cache line, instead of one shared atomic counter for all allocations.

Decision: resident_bytes supplies the operating system resident set separately from heap_in_use, and each memory reading is an execution input, because retained allocator pages and concurrent allocations make the resident set and live requested storage different observations that applications need for different purposes, instead of conflating an eviction budget with resident pages or deriving source acceptance from a host reading.

Rejected:
- Allocator usable-size queries on release: rejected because their meaning and availability depend on the platform allocator and every release would pay that lookup.
- Allocator-wide queries on reading: rejected because arena traversal and allocator locking make their cost unsuitable for the intended per-command observation, and the platforms expose different quantities.
- Resident set as heap_in_use: rejected because pages can remain resident after their live allocations are freed, so eviction would not observe its own progress.
- One atomic counter updated by every allocation: rejected because all drivers would write one cache line on the allocation path.
- Placing MemoryMeter in std::time: rejected because it observes process memory, and following the clock's capability discipline does not make memory a clock.
