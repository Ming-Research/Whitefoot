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
  the comparators and also be much faster than the map firn uses now, so that
  the return to firn is easy to attribute. The criteria's numbers are fixed
  after the comparators' baseline and before this implementation's first
  measurement.
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
compare-and-swap loop, is flagged in every table: a Whitefoot block runs once,
so a lead over a flagged update compares different guarantees.

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
single counter. Sizes N are 2^10, 2^20 and 2^24. Uniform choice is generated
inside the timed loop by every driver from SplitMix64, integers only, so
every language draws the same sequence. Zipf ranks come from a buffer of
2^20 per thread that one generator writes to a file before timing and every
driver reads; the buffer repeats, so Zipf cells measure hot-key contention
and not the cold tail, which the uniform 2^24 cells measure.

**Keys and values** are 64-bit. A key is the SplitMix64 finalizer of its
index, a bijection, so keys are uniformly random and no table gains from
their order. Native implementations all hash with the identity, which
costs nothing and is uniform on these keys, so integer cells compare
structure alone; byte-string keys add real hashing. Managed implementations
keep their languages' own hashing.

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

The form, ruled by the owner; the numbers are written here after the
comparators' baseline and before this implementation's first measurement.

- **It leads.** In every cell of the matrix, at every thread count, this
  index reaches at least the fastest native comparator's rate, with flagged
  updates reported beside it.
- **It is much faster than firn's map.** In every cell it reaches at least a
  stated multiple of `wf-current`.
- **Its single thread is not paid for concurrency.** At one thread it reaches
  a stated fraction of `mutex-flat`.
- **It passes every check**, including the linearizability mode.

## Comparators

Native, through the C driver, at pinned versions:

- Rust: `papaya` (lock-free, read-optimized), `DashMap` (sharded
  reader-writer locks), `scc::HashMap`;
- C++: Boost 1.83 `concurrent_flat_map`, Intel TBB `concurrent_hash_map`,
  `libcuckoo`, `growt`;
- C: liburcu's `cds_lfht`, a lock-free resizable table under
  read-copy-update;
- floors at one thread: Rust's standard `HashMap` and Boost's
  `unordered_flat_map`, with no lock.

Managed, through their own drivers: Java 21 `ConcurrentHashMap`, .NET 8
`ConcurrentDictionary`, Go 1.24 `sync.Map` and `xsync.MapOf`.

Garnet's Tsavorite index is not separated from Garnet; firn meets it again in
stage (c).
