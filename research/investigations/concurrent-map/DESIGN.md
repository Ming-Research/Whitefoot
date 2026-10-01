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

Later on 2026-10-01:

- **The index lives in the runtime,** `compiler/src/backend/concurrent_map.c`,
  with its tests in the runtime's own test stage; the bundle measures that
  source and keeps no copy.
- **Two maps unless one is fastest everywhere.** The concurrent map does not
  replace the standard `HashMap`, which serves a single thread: if the
  concurrent map were faster for a single thread too, only it would remain;
  otherwise both stay, one concurrent and one single-threaded.
- **A `Shared` object holding a `HashMap` stops being the way to share a
  keyspace,** since it cannot be faster than the concurrent map; `Shared`
  objects and atomic statements remain for every other shared state. The
  bundle's control that measured it, `wf-current`, is removed.

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

## Stage (b): the language surface, stated before building

On 2026-10-01 the owner directed the work to continue through the language
and firn to a new run of the benchmark, ruling on the open questions (Q34
on the statement form, Q35 on statements over several keys, Q37 on
statements that only read) and on the choices below when the work is
handed back. Each choice is a proposal until then.

- **`SharedMap<V>`, a shared map from byte strings to values of type `V`,**
  whose keys the runtime hashes and compares. The runtime calls Whitefoot
  code today only to start a context; a key of any type would make it call
  the writer's hash and equality under a cell's lock in the middle of a
  probe, and the index's linearizability would then rest on a protocol that
  the standard `HashMap`'s own documentation says no source admission
  checks. Byte strings are what Redis keys are, and firn can name a key as a
  range of its input without building one. Rejected: a key of any type with
  a `HashMapKey` binding, for that reason; integer keys only, since firn's
  keys are bytes.
- **A keyed statement, `atomic e = &m[key] { ... }`,** whose binding is a
  `&Option<V>` naming the key's entry: the block reads it, replaces the
  value, inserts one by writing `Some` or removes the key by writing `None`,
  with exclusive access to that entry, and takes effect at one point.
  Statements on one key take effect in one order, and statements on
  different keys do not wait for each other. It counts as a waiting call,
  and its block contains no waiting call, as an atomic statement's does.
  One form serves every single-key command, with no library function and no
  callback.
- **A whole-map statement, `atomic s = &m { ... }`,** whose binding is a
  `&Keyed<V>` naming the whole map, with exclusive access to every entry,
  for commands over several keys (`MSET`, `DEL` and `EXISTS` of several)
  and for an exact count (`DBSIZE`). Inside its block, and only there,
  `atomic e = &s^[key] { ... }` reaches one entry and waits for nothing. To exclude it, every keyed statement publishes itself
  with one sequentially consistent store on its own thread's cache line, the
  price paid by every keyed statement and measured with firn. A statement
  over a list of keys, taken in one order the runtime fixes (Q35), would let
  `MSET` run beside statements on other keys; it is deferred, since it
  needs a type for a list of keys and a binding over several entries, and
  the whole-map statement gives the same meaning.
- **Every keyed statement holds its entry exclusively, reading or
  writing.** A Whitefoot block runs once and cannot be run again after a
  torn read of a value larger than a word, so the index's lock-free read
  serves no statement yet; statements that only read running at the same
  time (Q37, the four `LRANGE` tests) stay open.
- **A statement that finds its entry or its map held waits in the runtime
  without parking,** since a holder's block contains no waiting call and so
  runs to its end without yielding its driver; parking, with the key word's
  second bit marking waiters, comes with bounded overtaking.
- **A keyed or whole-map statement's block may contain an atomic statement
  on a `Shared` object, whose own block contains none,** so that an
  append-only file's record of a change is made in the same step as the
  change. Entries and maps are always taken before objects and an object's
  holder takes nothing, so no cycle of waits can form; the inner statement
  waits without parking for the same reason as above.
- **Entries live in nodes the runtime allocates from its own pool:** the
  key's bytes and a slot for the `Option<V>`. A cell holds the key's hash in
  its key word and the node's address in its value word; a probe compares
  hashes, and the full key only after it has locked the cell, so a node is
  freed under its cell's lock and no probe reads a freed node. A move copies
  the addresses, so an entry stays where its statement's binding names it.
- **The last handle's release drops every value** and frees the map.

Stage (b)'s own measurement, the same workloads through Whitefoot code
against the C index, is deferred: firn's suite measures the surface end to
end first, at the owner's direction.

## Stage (c): firn's keyspace on the shared map

firn's keyspace was one `Shared<Store>`, so every command of every client
took one lock. It is now a `Keyspace` of three fields, each client holding
its own handles (`apps/firn/store/module.wfm`):

- **`map: SharedMap<Entry>`**, created for 2^18 keys, so that the suite's
  2^17 keys (`key:` and `counter:` names drawn from 100,000) fit without a
  move into memory this host has not yet given the process (see this host's
  fresh memory, above). Every single-key command is one keyed statement on
  its key; `MSET`, and `DEL` and `EXISTS` of several keys, hold the whole
  map and reach each key through it; `DBSIZE` counts the held state.
- **`meta: Shared<Meta>`**: the queued expiries, the append-only file's
  pending bytes, the client count, the stop flag and the seed of each
  client's `SPOP` state. A command that logs a change, or queues an expiry,
  does so in a statement on `meta` inside its keyed statement, so the record
  and the change take effect together; the suite runs firn without a file,
  so no measured command takes `meta`.
- **`logging: Bool`**, fixed in each context's handles before any client is
  served, since it changes only once, after the file's replay.

A key is a range of the client's input whose bounds a contract proves
(`key_bounds` in `apps/firn/commands/keys.wf`), so no key is copied or hashed
in Whitefoot. The expiry context takes up to 256 reached expiries from `meta`
in one statement and then removes each key whose expiry still matches in a
statement on that key; two statements are Redis's own meaning, since a key
set between them keeps its new expiry. Each client keeps its `SPOP` state,
seeded from `meta` when it connects, instead of reading and writing one
shared seed in every `SPOP`.

**A quick comparison, before the suite.** On this host, servers on CPUs 0
and 1, `redis-benchmark` on 2 and 3 with two threads, 50 clients, two
interleaved rounds of about six seconds a cell, firn before the change (its
source at `e92a54ed7`, built with this branch's compiler) against firn after
it and Redis 7.0.15, requests a second:

| Test, depth 16 | Redis | firn before | firn after |
|---|---|---|---|
| `SET` | 730,579 to 733,915 | 1,291,434 to 1,332,938 | 1,634,877 to 1,713,633 |
| `GET` | 716,903 to 781,861 | 1,564,945 to 1,636,066 | 1,799,640 to 1,893,940 |
| `LPUSH` | 1,199,201 | 1,564,945 to 1,635,769 | 1,999,556 to 2,117,149 |
| `MSET` | 176,984 to 180,357 | 391,151 to 391,441 | 342,661 to 386,316 |

**What the suite's pilot found.** The first launch of the suite was stopped
at its pilot: firn answered `SPOP` and `ZPOPMIN` at depth 1 at about 29,500 a
second, against 114,000 to 177,000 for its other tests. Both pop one key until
it is empty and then go on missing it, and every statement on an absent key
claimed a cell and left it removed, so each miss walked one more removed cell
than the last: five rounds of 40,000 `SPOP`s on an emptied key answered
53,191, 19,714, 12,296, 9,401 and 7,990 a second. A claim now reuses the first
removed cell its probe passed and then looks on to the next empty cell for a
cell of the same key another writer claimed meanwhile; the same five rounds
answered 159,363 to 160,000. The runtime's test drives that interleaving step
by step and fails when the look ahead is left out. The quick comparison above
predates the change; none of its four tests misses a key.

**The handle each statement counted.** Every keyed statement retained and
released its map's handle, two atomic updates of one count every driver's
statements share, so that the map outlives a block that moves the handle it
was reached through. Criterion, stated before measuring: if firn built
without the pair answers depth-16 `SET` or `GET` on two server CPUs at least
5% faster, medians of two interleaved rounds beyond their spread, a statement
that cannot lose its handle omits the pair. Built so (a measurement build, not
safe in general), firn answered `SET` at 1,888,376 and 2,117,149 a second
against 1,713,633 and 1,799,280, `GET` at 1,893,940 twice against 1,713,633
and 1,799,640, and `INCR` at 1,799,280 and 1,999,556 against 1,634,877 and
1,636,066. A statement now omits the pair when it reaches the handle through
a reference parameter whose declared row writes nothing below it, since the
caller keeps that handle live for the call; every firn command reaches its
map and `meta` that way.

At depth 1 firn before and after both answered 171,298 to 179,928 a second
on `SET`, `GET` and `LPUSH`, the same steps of the benchmark's clock, so
those cells did not separate them. `MSET` holds the whole map, as the old
keyspace held its one lock, and adds a lock per key and the hold's wait for
keyed statements under way.

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
  comparators suffices.
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

## The baseline, and how the index is compared

The full profile ran for both of the comparator sets: the first, without
growt and parallel_flat_hash_map, was stopped to add them, and the second
was stopped on 2026-10-01 after its 2^10 and 2^20 sizes, three repetitions
each, when the owner ruled that the full matrix runs only when unavoidable:
it takes hours on this host. Its rows are kept with the bundle's results.

Among the comparators, growt is the fastest in nearly every cell of both
sizes, often by a wide margin over the next: at 2^20 keys, uniform
`mostly-read` on four threads, 94 million operations a second against
Boost's 46 in the quick profile. The exceptions are `update` on one key at
four threads (DashMap), `churn` (DashMap at one thread, scc at two and
four) and `grow` at one thread at 2^20 (DashMap).

So the index is compared, per the owner's ruling, in the `duel` profile:
N = 2^20, uniform `read`, `mostly-read`, `balanced`, `update`, `churn` and
`grow`, Zipf `mostly-read` and `balanced`, and one-key `update`, at one
thread and at every CPU, three interleaved repetitions, against growt,
DashMap and scc, the fastest comparators of the baseline, with `mutex-flat`
for the single-thread criterion. The criteria above apply to the cells it
runs.

## The index

### The first design, and why it was replaced

The first candidate followed the cache-line hash table of David, Guerraoui
and Trigonakis ("Asynchronized concurrency", ASPLOS 2015): 64-byte buckets
of three keys and values with a ticket lock that doubled as the read
version, overflow buckets, and a cooperative move bucket by bucket. In the
first duel it led one cell, tied eight and lost nine, most of them to growt
at between half and four fifths of its rate. Two costs were found: a branch
on which slot held the key mispredicted on most lookups and discarded the
loads of the lookups after it (a branch-free slot match took single-thread
reads at 2^10 keys from about 47 to about 101 million a second), and the
buckets held 2^20 keys in 41.9 bytes a key against growt's 32.4, so a
random probe missed the cache more often and a lookup that reached an
overflow bucket missed twice.

### The cell design

`compiler/src/backend/concurrent_map.c` now follows growt's layout (Maier,
Sanders and Dementiev, "Concurrent hash tables: fast and general?!", 2016),
with a lock in place of its compare-and-swap updates, since a Whitefoot
statement's block runs once with its entry held:

- **A cell is 16 bytes, a key word and a value,** four to a cache line, in
  one array probed linearly from the key's golden-ratio hash. A lookup at
  half load reads one line nearly always, and the cells cost 32 bytes a key
  at that load, as growt's do.
- **The key word carries the lock.** Its top bit locks the cell, and the
  next is kept for marking parked waiters; empty is zero and removed is all
  ones below them, so keys lie in [1, 2^62 - 2]. A writer locks its key's cell, or claims an empty
  one, by one compare-and-swap, runs once and stores the key back. A reader
  takes no lock: it waits while the cell is locked, since a claimed cell has
  no value yet, and then reads the value. A cell never holds another key and
  its value is written by one store, so the value read is one the key held
  during the read; entries larger than a word would need a version in their
  header, and stage (b)'s entries are read only under the cell's lock, since
  every keyed statement holds its entry (Q37). The runtime's test
  fails when a read does not wait, and cannot fail when a read checks the
  key word again after the value, so that check was removed.
- **A removed key stays a removed cell until the table moves,** so a probe
  never stops early. A probe that has seen every cell stops there: a full
  table, which racing claims can leave in a small one, answers absent to a
  read and sends a claim to help a move.
- **A table moves once half its cells are used,** to the fewest cells, a
  power of two, that hold the live keys in three eighths of them, at least
  half its size: growth doubles, and a churning map moves between tables
  of one size with at least an eighth of the cells left to claim. One
  writer makes the next table, and writers that meet the move copy blocks
  of 4,096 cells into it and retry there. A writer checks for a move after
  it locks its cell and gives the cell back unchanged when one has begun, a
  claimed cell as removed, since another writer may have probed past it; a
  mover publishes the move before it reads a cell and waits while the cell
  is locked. All four steps are sequentially consistent, so either the
  mover sees the lock or the writer sees the move, and the mover only reads
  the old table: marking each cell moved by compare-and-swap, as the first
  cell design did, wrote every line of it, and dropping the mark took
  single-thread `grow` from about 9.5 to about 12.6 million a second. A map
  created for N keys starts half full at N, as dense as a table gets before
  it moves, since reads cost less in a smaller table: four times N cells
  took single-thread reads at 2^20 keys from about 26 to about 20 million a
  second.
- **Each user publishes the table it works in,** as growt's handles do: an
  operation compares the map's current table with its user's, and only on a
  change publishes the new one and checks again, both sequentially
  consistent. A moved table is freed once no user is in it, and the cells of
  the newest one freed are kept for the next move to their size, so a map
  whose size holds steady moves between warm tables: on this host, at four
  threads, a move of 2^22 cells took 12 ms into reused cells and 111 ms
  into fresh ones. A
  user belongs to one thread and one map, so a thread can use several maps;
  a thread-local slot could not tell them apart.
- **A writer that finds its key locked waits 16 pauses, then twice as long
  each time up to 1,024,** about 0.2 to 12 microseconds here, as long as a
  sleep and wake by the system. The holder then makes many changes in a row
  instead of handing the cell's line to a waiter on each: one-key `update` at
  four threads went from about 6.5 to about 24 million a second, where
  waiting that started at one pause or yielded the processor stayed between
  6 and 16. A waiting writer can be overtaken without bound; parking
  waiting statements and handing the entry over after a bounded number of
  vain wakes, as the shared-object runtime does [SHARE-3], is recorded in
  `docs/todo.md`.
- **Cell arrays of 2 MiB or more are mapped and advised into huge pages,**
  since a random probe in a large table otherwise pays a page walk on most
  accesses: uniform `read` at four threads ran at about 127 million a second
  with huge pages and about 105 without.

### This host's fresh memory

This host returns every free 2 MiB block to its hypervisor
(`/sys/module/page_reporting/parameters/page_reporting_order` is 9), so a
huge page faulted in fresh arrives cold: fresh memory faulted in at about
140 MB/s in huge pages and 2 GB/s in 4 KiB pages, and memory the process
had just freed at about 6.6 GB/s. A move
into fresh cells pays this, so a map that grows while measured pays it once
per size; the comparators allocate 4 KiB pages and do not. The first
measurement of `churn` at one thread completed no operation in its window:
its writer spent 0.8 s clearing a fresh 128 MiB table, and at four threads
every writer that crossed the threshold allocated and cleared its own, 1.4
to 2.3 s each. One writer now makes the table, and reuse
leaves the cold cost to the first moves of each size. The 14900K, a host
without page reporting, measures the same cells without it.

### Measured

The fourth duel, on 2026-10-01, gave 3 leads, 15 ties and no loss
([results](../../experiments/concurrent-map-bench/RESULTS.md#the-index)):
uniform `balanced` and `mostly-read` at four threads and Zipf `balanced` at
one. Every cell's spread on this host is wide enough that a median a tenth
to a fifth either side of growt's counts as a tie. At one thread it reaches
`mutex-flat` in every mix but `churn`, whose first moves pay this host's
cold fresh memory. Uniform `read` at one thread is at the hardware's floor
for one miss a read: a flat table with no concurrency control ran at 25 to
27 million a second, growt at 19 to 26 and the index at 25 to 29, so that
cell can tie but not lead.

What would refute it: a lead lost at one thread to the single-thread floors
would show the lock bit costing more than it saves; a lead lost on `update` with one key at four threads once waits
park would show the batching depended on unbounded overtaking; a lead lost
in `grow` on the 14900K would show the cooperative move slower than the
comparators' rebuilds.
