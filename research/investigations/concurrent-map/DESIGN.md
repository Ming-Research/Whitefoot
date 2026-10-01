# A concurrent hash index under keyed atomic statements

## The question

Every client of `apps/firn` reaches its keyspace through one shared object, so
every command takes one lock ([firn](../firn/DESIGN.md#the-owners-rulings)).
A production server has more than two cores, and the
[shared-object design](../io-model/SHARED.md#remaining-questions) leaves
reader concurrency and several objects per statement open. Can the trusted
runtime provide a concurrent hash index, reached through atomic statements
on one key, that leads the established concurrent hash maps of other
languages on a measurement of its own, and is much faster than what firn
uses today, an `std::collections::hash_map::HashMap` behind one `Shared`
lock?

The writer keeps the language's meaning: a statement on one key runs with
exclusive access to that key's entry and takes effect at one point, as an
atomic statement on a whole object does [SHARE-1]. The mechanism behind it,
lock-free reads, entry locks, incremental resizing and deferred reclamation,
belongs to the runtime.

## The owner's rulings

On 2026-10-01, after the [firn suite](../firn/DESIGN.md#the-full-suite-results):

- **The core is a concurrent hash map.** Scaling past two cores comes down to
  one keyed concurrent index; the index is a hash table, since the keyspace
  needs lookup by key and no command the benchmark sends needs key order.
  Ordered structures stay inside values: a sorted set's tree belongs to one
  key's entry, which that key's statement holds exclusively. The surface,
  keyed atomic statements and a keyed prelude type, is designed in stage (b).
- **The index lives in the trusted runtime, not in Whitefoot source.** A
  lock-free index needs compare-and-swap and deferred reclamation, which
  Whitefoot source could express only through the atomic fields and
  lock-free cells the shared-object design refuses; writing it in Whitefoot
  would deny the language's premise. The goal is a strong trusted base that
  leaves no such problem to the writer. Its verification is model checking of
  the algorithm and of its C11 memory orderings, and linearizability checking
  of the implementation under stress.
- **A standalone stress measurement, fast and outside CI, with control
  groups.** `redis-benchmark` is slow and measures the index only indirectly,
  through parsing, replies and the network, which obscures attribution.
- **The comparators** are the ones listed under [Comparators](#comparators).
- **Win on the index first, then return to firn.** The new index must lead
  the comparators; being much faster than the map firn uses now follows and
  makes the return to firn easy to attribute, and is reported rather than
  made a criterion. The criteria's numbers are fixed before this
  implementation's first measurement.
- **The host for scaling** is the owner's i9-14900K, reached through a
  session running on that machine; this 4-CPU host measures one to four
  threads.

Still open, for stage (b) with stage (a)'s evidence: statements over several
keys, acquired in a canonical order inside the runtime (MSET is their
instance), and statements that only read running at the same time (the four
`LRANGE` tests are their instance).

## Why the Redis benchmark cannot measure this

Run against a listener that logs every command it receives and answers
`+OK`, `redis-benchmark` 7.0.15 with `-r 100000 -n 3 -c 1` sends, per test:

- no key: `PING_INLINE`, `PING_MBULK`;
- a key drawn from 100,000: `SET key:<12 digits>`, `GET`, `INCR counter:<12
  digits>`, and `MSET` with ten independently drawn keys;
- one key for the whole test: `mylist` (`LPUSH`, `RPUSH`, `LPOP`, `RPOP`, the
  four `LRANGE` tests), `myset` (`SADD`, `SPOP`), `myhash` (`HSET`),
  `myzset` (`ZADD`, `ZPOPMIN`); `-r` draws only the members and fields
  inside that one value.

So 4 of the 19 measured tests spread over keys, and 13 serialize on one key in
every server. Of the ten tests below the 1.4 criterion on two CPUs at depth
16, a concurrent index reaches `SET` 1.25, `GET` 1.28 and `MSET` 1.12; reader
concurrency reaches the four `LRANGE` tests; `ZADD` 1.19 depends on the
length of one key's critical section; and the two `PING` tests touch no key.
A hundred thousand small keys also fit in this host's caches, and almost no
test grows the table. The benchmark is a fair test of firn and a poor one of
a concurrent index.

## Stages

1. **(a) The index alone,** in C, against the comparators and the controls on
   integer keys, until it meets the criteria; then the same on byte-string
   keys shaped like firn's, where hashing and comparison cost lives.
2. **(b) The language,** keyed atomic statements and the keyed type, with the
   open questions above; measured through Whitefoot code on the same
   workloads.
3. **(c) firn on the new keyspace,** against the suite's criteria, then on the
   14900K.

## The measurement

The bundle is `research/experiments/concurrent-map-bench/`. These rules are
fixed before any measurement.

**Operations.** `get` copies a value out; `insert` inserts or replaces;
`remove` deletes; `update` adds one to a present key's value under exclusive
access to that key, the shape of an atomic statement on one key. A
comparator whose update may run its function more than once, an optimistic
compare-and-swap loop, or that offers only an atomic addition, is flagged in
every table: a Whitefoot block runs once and may do anything to the entry, so
a lead over a flagged update compares different guarantees.

**Mixes,** over N live keys inserted before timing:

- `read`: 100% `get`;
- `mostly-read`: 95% `get`, 5% `update`;
- `balanced`: 50% `get`, 50% `update`;
- `update`: 100% `update`;
- `churn`: 50% `get`, 25% `insert`, 25% `remove`, over 2N possible keys
  with N present at the start;
- `grow`: N distinct `insert`s into an empty map, each thread a disjoint
  share, timed to the last thread's end.

**Key choice.** Uniform; Zipf with θ = 0.99, the constant of the Yahoo!
Cloud Serving Benchmark; and one key, for `update` only, the Redis shape of a
single counter. `churn` and `grow` draw uniformly. Sizes N are 2^10, 2^20
and 2^24. Uniform choice is generated
inside the timed loop by every driver from SplitMix64, integers only, so
every language draws the same sequence. Zipf ranks come from a buffer of
2^20 per thread that one generator writes to a file before timing and every
driver reads; the buffer repeats, so Zipf cells measure hot-key contention
and not the cold tail, which the uniform 2^24 cells measure.

**Keys and values** are 64-bit. A key is one plus a bijection of its index
onto 62-bit integers, the SplitMix64 finalizer's steps taken modulo 2^62, so
keys are uniformly random, no table gains from their order, and none is zero
or has a top bit set, values some comparators reserve. Native implementations
all hash a key by one multiplication with the 64-bit golden ratio, which
spreads these keys over every bit any of them reads at the cost of one
instruction, so integer cells compare structure; byte-string keys add real
hashing. Managed implementations keep their languages' own hashing.

**Drivers.** One C driver serves every C, C++ and Rust implementation,
including this one: each is a separate binary in which the driver calls the
implementation's functions directly, without inlining across the boundary,
the cost a Whitefoot program pays calling the runtime. Java, Go and .NET
implementations have drivers in their own languages with the same streams,
mixes and timing, and are reported as a separate reference group, since
their drivers differ.

**Controls.** `empty` answers every operation without storage: the floor of
the driver itself. `mutex-flat` is Boost's `unordered_flat_map` behind one
mutex: a fast table and one lock, so table speed and lock cost separate.
`wf-current` is a Whitefoot program that runs the same workload through
`atomic` statements on one `Shared` object holding a standard `HashMap`, on
as many drivers as threads: the map firn uses now.

**Timing.** A process runs one implementation at one size and one key
choice; it prefills untimed, then runs every mix at every thread count,
`churn` last since it changes the key set, each cell warmed for 0.2 seconds
and timed for a fixed duration that a stop flag ends. Threads start at a
barrier and count their own operations. Three repetitions are interleaved
across implementations, and a cell's rate is their median, in millions of
operations per second.

**Placement.** Thread i runs on the i-th CPU of a placement list: one thread
per physical core, performance cores first, then efficiency cores, then
second hardware threads, and each row names its mix of cores.

**Checks.** Every cell checks its own result: `read` finds every key;
`update` leaves the sum of all values equal to the prefill's sum plus the
updates counted, so a lost update fails the cell; `churn` leaves a count of
live keys equal to the prefill plus successful inserts less successful
removes; `grow` finds every key. A cell that fails is reported and not
timed. This implementation adds a linearizability mode: recorded histories
of a few threads over few keys, checked key by key against the sequential
map, which suffices because linearizability is local to each object.

**Profiles.** `quick`, for iteration, runs N = 2^20 at uniform and Zipf
choice, `mostly-read`, `balanced` and single-key `update`, at one thread and
at every CPU, for this implementation, the controls and the fastest
comparators, in a few minutes. `full` runs the whole matrix on request. Both
write only to the scratch root; neither is part of `make check`.

## Criteria, stated before measuring

The owner ruled the form when the work began and the numbers on 2026-10-01,
before this implementation's first measurement. A cell is one size (2^10,
2^20, 2^24), one key choice and mix (uniform `read`, `mostly-read`,
`balanced`, `update`, `churn` and `grow`; Zipf `read`, `mostly-read`,
`balanced` and `update`; one key `update`) and one thread count (one, two
and four on this host): 99 cells. The 14900K judges them again with its own
thread counts.

- **It leads.** In every cell its median reaches at least the median of the
  fastest comparator, native or managed, flagged or not, with the flags
  reported beside it. A margin smaller than the cell's spread, the larger
  of the two implementations' differences between their fastest and slowest
  repetitions as a fraction of their medians, counts as a tie and is listed
  apart, not as a lead.
- **No criterion against firn's map.** The owner ruled that leading the
  comparators suffices; `wf-current` is measured beside the index and its
  ratio reported, not judged.
- **Its single thread is not paid for concurrency.** At one thread, in every
  mix, it reaches at least `mutex-flat`'s rate.
- **The read path judged is the copy-out read.** Every comparator's `get`
  copies a value out, and so does this index's lock-free read. A variant
  whose reads hold the bucket's lock, the read a Whitefoot block needs
  unless it may run again without effect, is measured and reported beside
  it, not judged: it is evidence for stage (b)'s question on statements
  that only read.
- **It passes every check**, including the linearizability mode.

## Comparators

Native, through the C driver, at pinned versions:

- Rust: `papaya` (lock-free, read-optimized), `DashMap` (sharded
  reader-writer locks), `scc::HashMap`;
- C++: Boost 1.92 `concurrent_flat_map`, Intel TBB `concurrent_hash_map`,
  `libcuckoo`, `growt` and the parallel-hashmap library's
  `parallel_flat_hash_map` with a mutex per submap;
- C: liburcu's `cds_lfht`, a lock-free resizable table under
  read-copy-update;
- floors at one thread: Rust's standard `HashMap` and Boost's
  `unordered_flat_map`, with no lock.

Managed, through their own drivers: Java 21 `ConcurrentHashMap`, .NET 8
`ConcurrentDictionary`, Go 1.24 `sync.Map` and `xsync.MapOf`.

Garnet's Tsavorite index is not separated from Garnet; firn meets it again in
stage (c). folly's `ConcurrentHashMap` is left out: its build needs most of
folly.

On 2026-10-01, after a first quick profile in which Boost's map led every
multi-threaded mix but one, the owner added `growt`, which the first list
named only if obtainable, and `parallel_flat_hash_map`, so that a lead
counts against the strongest designs for integer keys; the baseline was
restarted with them so that every comparator is measured in the same
interleaved run.

## The index: a first design, stated before measuring

The first candidate follows the cache-line hash table of David, Guerraoui
and Trigonakis ("Asynchronized concurrency", ASPLOS 2015), whose aim
matches this one: one cache line touched per operation.

- **A bucket is one 64-byte line:** a state word, three key and value
  slots and a pointer to an overflow bucket. A lookup touches the home
  line, and an overflow line only when the bucket holds more than three
  keys.
- **The state word is a ticket lock and a version at once:** its high half
  counts tickets taken, its low half counts tickets served, and the bucket
  is free when the two are equal. A writer takes a ticket and runs once,
  with the bucket to itself; tickets serve writers in arrival order, which
  bounds overtaking as the shared-object design requires of every begun
  statement.
- **A read takes no lock and writes nothing:** it reads the state, the
  slots and the state again, and starts over if a writer came between. So
  readers of a hot key do not pass its line between cores, which is where
  a reader-writer lock on the line (Boost's group lock, TBB's accessor)
  pays. This is a copy-out read: the comparators' `get`, and the form a
  Whitefoot statement that only reads can use only if its block may run
  again without effect, which is stage (b)'s question.
- **Growth is cooperative and incremental:** a writer that finds the table
  three quarters full allocates one twice as large, and every writer that
  arrives during the move first moves a run of buckets, locking each,
  copying it into the two buckets it splits into and marking it moved;
  readers and writers that meet a moved bucket go on to the new table, so
  no operation waits for the whole table to move. Old tables are kept until
  the map is destroyed, which bounds their memory by the live table's and
  needs no reclamation; deferred reclamation is a later step.
- **The count of keys is kept per thread** and summed only when an
  insertion needs an overflow bucket, so no insertion contends on a shared
  counter.

What would refute it: a lead lost at one thread to the single-thread floors
would show the version check costing more than it saves; a lead lost on
`update` with one key would show the ticket lock's handoff costing more
than a test-and-set's; a lead lost in `grow` would show the cooperative
move slower than the comparators' rebuilds.
