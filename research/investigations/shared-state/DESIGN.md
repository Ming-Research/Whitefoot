# Shared state: one handle, locked by parts

This record keeps the working names of the discussion, `Table` and `Keys`;
specification v0.86 names them `KeyedTable` and `KeySet`, with `KeyedEntries`
for the entries a binding names over a set (`design/language/waiting/shared-objects/keyed-tables`).

## The question

A shared object is state that several contexts reach through `Shared<T>`
handles, only inside an atomic statement whose block has exclusive access to
the state [SHARE-1, SHARE-2, SHARE-3]. firn's keyspace needed more than one
lock. PR #202 added a second sharing primitive, `SharedMap<V>`, with keyed
and whole-map statements and rules for nesting them, and PR #208 added shared
reads of one key, holding only the entries a whole-map statement reaches, and
a condition over the whole program that kept the second sound.

On 2026-10-02 the owner judged that direction a workaround that had grown
without a design ("`Shared<store>` is a hack, like interior mutability plus a
reference count; SharedMap enlarged the workaround without thinking it
through, so the trouble keeps growing", written in Chinese), and asked for a
design that

1. makes sharing between spawned contexts convenient,
2. keeps concurrency safe, including freedom from deadlock, and
3. handles locking a part of a shared thing, such as a shared hash map,
   elegantly, which the first design had not considered.

This record holds the design reached in that discussion, its grounds, and the
alternatives refused on the way. The owner's rulings are quoted as given,
translated from Chinese.

## Why the first design grew special forms

- A handle is an owned value, and two handles can name one object. The
  checker takes two roots as two storages, so one statement holding two
  objects could form two writable references to one state. The first design
  deferred several objects in one statement for exactly this reason
  ([shared objects](../io-model/SHARED.md#why-a-statement-and-not-a-function)).
- Without several objects in one statement, freedom from deadlock had to come
  from holding one object at a time: object statements do not nest, and state
  that changes together must live in one object. firn's `Meta` holds its
  expiry queue, its append-only buffer, its client count, its stop flag and
  its random seed under one lock.
- A map needs blocks that hold several parts at once: an entry and the log,
  or several entries. Each need became a statement form (keyed, whole-map, an
  entry under a held map state), a table of what may nest in what, an
  exception by which one form counts as no waiting call, and, once statements
  on different entries ran beside each other, a rule about blocks that run
  two object statements and a condition over the whole program
  ([holding only the entries](../concurrent-map/DESIGN.md#holding-only-the-entries-a-statement-reaches)).
- In `atomic s = &h { }` the target and its binding repeat what the compiler
  already knows, but the block's extent does not: which statements form one
  step cannot be inferred, which is why the first design refused an implicit
  scope. With one object the target and the extent coincide; with a map they
  separate, and the special forms were one target per statement standing in
  for one extent over several targets.

## The design

### Sharing: owned handles and a count

Sharing is unchanged: `shared_new` makes a state, `shared_share` makes
another handle to it, each context holds its own handle, and the state is
released with the last one [SHARE-1]. A spawn still takes values [WAIT-3].

A spawn taking references to a state its starter owns was considered and
refused. The owner's objection settled it: passing references removes only
the reference count, while the content is still reached only under an
exclusion established at run time. An effect row cannot describe that content
as written: a row entry excludes every other access for the whole call, a
spawned call lasts its context's life, so `writes(store)` would forbid the
second spawn ("once it is written there is no parallelism"). And because the
same state can be reached through two names either way, its identity is
checked at run time, which owned handles serve as well.

### Meaning: a block owns the whole state at one point

    atomic s = &h when g { body }

with the guard optional. The body executes with exclusive access to the whole
state the handle `h` names and takes effect at one point, and the statements
on one state take effect in one order [SHARE-3]. This is today's object
statement unchanged. An implementation may hold less than the whole state
wherever the program's outcomes are those it has under exclusive access (the
sentence [SHARE-3] gained in v0.86). Everything below about parts is that
liberty: the owner's words were "it is still an optimization, with the same
effect as locking the whole object".

### Parts

- A *keyed table* is a field type of a shared state (`Table<V>`, name open).
  It holds an `Option<V>` entry for every byte string, its key. Its entries
  can be locked one by one, and an operation on the whole table, such as
  counting its `Some` entries, locks it whole.
- The table is the only type whose parts are identified by values known at
  run time. It appears only as a field of a shared state, is built empty with
  a capacity inside the argument of `shared_new`, and is never moved into or
  out of a state whole, so it has no second, unshared representation to
  convert to and from. The owner later withdrew this confinement (Q18): a
  table is a value `keyed_table_new` builds, with the one representation
  wherever it is, and a statement may write over a state's table or swap it,
  which exchanges the tables' entries so the state's table never moves
  (compiler/waiting-contexts/state-locks).
- `SharedMap<V>` disappears: `Shared<Table<V>>` is the old map, and
  `SharedMapState<V>` is the table.
- The table's interior stays concurrent in the runtime. Entries under
  different keys are inserted, removed and moved to a larger table by
  different contexts at the same time, so the table's structure must allow it;
  a single-threaded hash map inside a state could only be locked whole, which
  is firn before PR #202 ([the cell design](../concurrent-map/DESIGN.md#the-cell-design)).

### Entries are named in the header

The header of an atomic statement is evaluated before its block. A table's
entries are reached only through bindings in that header:

    atomic s = &store^.state, slot = &s^.map[key] { ... slot ... }
    atomic s = &store^.state, slots = &s^.map[keys] { ... slots^[i] ... }

- Every key a statement uses on one table must be known before any of that
  table's entries is locked, because they are locked together in one order
  (below). The header is the place the language already evaluates first;
  today's keyed statement `atomic e = &m[k]` had no such problem because its
  key was in its target.
- The table has no index operation inside a block. A key computed inside a
  block cannot be used on the table, since there is nowhere to write it; an
  attempt is a type error at the index, not a rule about where keys come
  from.
- A key set named in the header cannot be changed in the block: the binding
  refers to it, and a write to it invalidates the binding [REF-2].
- Object fields need no header binding, since their identity is fixed by the
  declaration: the body writes `s^.meta.log`.
- Operations on the whole table are written in the body and lock the table
  whole. Nothing else ever locks a whole table, so no slow path appears
  unwritten.

Half of `hold` came back, as the owner agreed: parts whose identity the
declaration fixes are found by the compiler, and parts whose identity is a
value are named before the block.

### Keys: an ordered set

`Keys` (name open) is an ordinary value, built before the statement by
ordinary code:

- it owns copies of its keys, since an aggregate holds no reference [REF-3];
- its keys are kept in byte order, and a key added twice is one element;
- each element may carry a payload, which a later addition of the same key
  replaces, or which counts the additions;
- iterating it visits the keys in that order, which is the order their
  entries are locked in, so the loop a reader sees is the locking order and
  nothing is sorted out of sight;
- two different positions are two different keys, so `slots^[i]` and
  `slots^[j]` for different `i` and `j` are different entries.

Building the set and then using it is two loops, and the owner noted why that
is not a cost: until every key is known their order is not, and the entries
cannot be locked without it.

The multi-key commands keep their Redis results: `MSET` carries each key's
value position, a later pair replacing an earlier one so the last value wins;
`DEL` with a key named twice removes it once; `EXISTS` carries a count so that
a key named twice counts twice.

### Locking: order, taking and release

- **Units (first layout).** Each table is a unit whose entries are locked by
  key, or whole. All other fields of the state form one unit together, as
  today's object lock does. Units are ordered by the declaration order of
  their first fields, and one table's entries by the byte order of their keys.
- **Taking.** The implementation takes each unit before the block first uses
  it, and takes an earlier unit before any later one. A table's entries named
  in the header are taken in one step, sorted, before the guard and the block,
  since the take reads the keys the statement reads when it begins; a later
  unit, such as the log after the entries, is taken only on the path that
  reaches it: `INCR` takes the log only when it logs.
- **Release.** After the statement's last taking on a path, a unit may be
  released once the block no longer uses it or any reference derived from it.
  The first implementation releases every unit when the block ends.
- **One point.** Every taking precedes every release, which is two-phase
  locking, so every execution equals one in which the statements on a state
  run one after another, each with the whole state to itself. A block's
  behaviour depends only on what it reads, and it reads what it would read in
  that serial execution.
- **No deadlock.** Every wait is for a lock that comes after every lock the
  waiter holds: a later unit is later in the order, and one table's entries
  are taken together, sorted. A cycle of waits would need a lock to come after
  itself. A block contains no waiting call and no atomic statement [SHARE-2],
  so it waits for nothing else.
- **No order rule in the source.** A statement holds parts of one state only,
  without a rule saying so: a state's content is reached only through its own
  binding, and a block contains no atomic statement. The order is the
  implementation's, applied the same way to every statement.

### Type invariants

A type invariant is owed at a construction and at a function's entry, not
over a value's whole life, and [TYPE-11] already makes the edges of an atomic
block on a state of such a struct points of the same kind: established at the
block's entry and owed at every edge leaving it, the monitor invariant of the
concurrency model
([section 5](../io-model/CONCURRENCY-MODEL.md#5-monitor-invariants)). Locking
by parts does not weaken it: by the two-phase argument above, each block runs
as it would with the whole state, where the invariant held at its entry, and a
block's behaviour depends only on what it reads. (In the discussion I first
called this a next step; the specification has carried it since v0.82.)

### Guards

A guard reads parts and writes nothing. When it is false the statement
releases everything it holds and waits until another statement writes a part
the guard read, then takes its parts again and re-evaluates. Waking more often
than needed changes nothing, since how often a guard is evaluated is not
observable [SHARE-3]: the first implementation wakes a guard that reads an
entry at any write to that entry's table. A guard on an entry is what a
blocking pop needs, which the first design could not express.

The promise that a begun statement whose guard stays true takes effect
[WAIT-2] is restated for a state of several units: each lock is taken in
finitely many steps, a table's entries through the wait bound that ends in the
table's line of whole holds, and other units through the object lock's
handoff.

### The layout is chosen in lowering

No rule a program sees depends on which fields share a lock: the order and the
taking points are the implementation's. The first layout above follows from
the declaration alone. A finer one, such as a lock per field that blocks touch
separately, may use the whole program, because an entry's composition lowers
its whole closure (the incremental-compilation decisions in
`design/compiler/incremental-compilation.md`). If composition ever stops
lowering the whole closure, the layout must again follow from the declaration
alone; the design tree records that dependency where the layout is chosen.

### What leaves the language

- the handle type `SharedMap<V>` and the functions `shared_map_new`,
  `shared_map_share` and `shared_map_count` as they stand;
- the four target forms of [SHARE-2] and the table of what each block may
  contain;
- the statement on an entry under a held map state, `atomic e = &s^[k]`, and
  the exception that counts it as no waiting call;
- the rule about blocks that run two object statements and the condition over
  the whole program that PR #208 added;
- the pre-run of a block's key computations (the twin of PR #208) and its
  five conditions.

## The runtime

- **The table** is today's concurrent index unchanged in role: cells with a
  lock bit and a claim bit, claims of empty cells for new keys, cooperative
  moves to a larger table, probes that take no lock, reader counts, the gate
  and tickets of whole holds, and sets of entries locked in one order
  ([the cell design](../concurrent-map/DESIGN.md#the-cell-design),
  [holding only the entries](../concurrent-map/DESIGN.md#holding-only-the-entries-a-statement-reaches)).
- **The other unit** is today's object lock and its queue of parked contexts.
- **A state hold** takes a statement's units in order. A table that falls back
  to its whole hold, after a wait past its bound, for two keys of one hash, or
  when its new keys do not fit its cells, does so at its own place in the
  order: it holds more than asked, with the same outcomes.
- **Reader counts.** A statement that only reads an entry adds one to the
  entry's reader count and subtracts it after, two locked read-modify-writes
  where an exclusive hold takes one. `GET` answers 2.6% lower on 4 server
  CPUs and 4.0% lower on 1, while readers of one list double
  ([shared reads](../concurrent-map/DESIGN.md#shared-reads-of-one-key)). The
  owner accepted the cost (Q7) and asked for other remedies later.

## firn under the design

Every atomic statement of firn at `cea9188d4` was mapped onto the design: 56
statements in nine files.

- **19 disappear into the bodies around them.** 16 object statements nested
  in entry or whole-map blocks become uses of `s^.meta` in the one body, and
  the 3 entry statements under a whole-map state become `slots^[i]`. Each
  nested object statement is the last action of its body, so folding it in
  moves no read or write; only `run_expire` holds two, in the two branches of
  one `if`.
- **12 statements on one key** become
  `atomic s = &store^.state, slot = &s^.map[key] { ... }` with their bodies
  unchanged. Seven only read; five write only to remove an expired key.
- **13 statements on one key also reach `meta`**, always on a conditional tail
  of the body: to log the change, and for `EXPIRE` and `SET` with an expiry to
  queue the expiry. In every one but `SET`, whether `meta` is reached depends
  on what the block finds in the entry, so only the block can decide it. With
  `meta` declared after `map`, the lowering takes it at its first use, after
  the entry, and holds it as briefly as today's nested statement did.
- **3 statements on several keys** move their key computation into a `Keys`
  built before the statement: `DEL` with no payload, since a key named twice
  is removed and counted once today; `EXISTS` with an occurrence count, since
  a live key named twice counts twice and an expired one counts zero today;
  and `MSET` with the position of each key's last value. Their loops then
  visit distinct keys in byte order instead of argument order, which nothing
  observes: each answers with a count or `OK` after the statement, and the
  logged record is the command as sent. The one-key branches of `DEL` and
  `EXISTS` exist only because their many-key statements held the whole map,
  and can fold into them.
- **1 statement counts the table** (`DBSIZE`) and holds it whole, as today.
- **8 statements reach only `meta`**: the client count, the random seed, the
  stop flag, the expiry worker's batch and the append-only writer's exchange of
  its buffer every 10 ms. One of them carries the only guard,
  `when s^.meta.clients == 0_u64`. They leave the table's entries free because
  the table is a unit of its own, and the guard is woken by writes to the unit
  holding `meta`, not by every write to an entry.
- **None fails to fit.** No key is computed in a block from what the block
  writes or from shared content. The one key that comes from shared content,
  `expire_key`'s, is taken out of the expiry queue by an earlier statement of
  its own, and `expire_key`'s check that the entry still expires at the queued
  instant covers the gap. No two keys of the map become known at different
  points, and no guard reads an entry.
- **Handles.** `Keyspace` becomes a `Shared<KeyspaceState>` handle beside the
  `logging` flag, and `KeyspaceState` holds `map: Table<Entry>` and then
  `meta: Meta`. `keyspace_new` makes one state and `keyspace_logging` shares
  one handle. `logging` stays outside the state: replay runs through a handle
  whose flag is false while the serving handles carry true, and the bodies
  read it as an ordinary field of the handle's holder.

## Measured

`redis-bench.sh quick` on the owner's i9-14900K under WSL2, firn at
`f8ca277a9`, and then with the two fixes below, against firn at `cea9188d4`,
the branch before this design, with the runs of each build interleaved
([raw lines](../../experiments/io-completion-bench/shared-state-14900k-samples.csv)).
Ratios are the head's rate to `cea9188d4`'s.

The first comparison, at `f8ca277a9`, found two costs the design did not
intend:

| Test | 4 CPUs | 8 CPUs | 16 CPUs |
|---|---:|---:|---:|
| `LPUSH` | 0.89 | 0.86 | 0.78 |
| `RPOP` | 0.88 | 0.83 | 0.82 |
| `SADD` | 0.84 | 0.86 | 0.78 |
| `HSET` | 0.89 | 0.88 | 0.83 |
| `MSET` | 0.58 | 0.19 | 0.06 |

- **`MSET`.** Every statement that names keys builds a key set, and its
  store and arena came from the context pool, whose free lists sit behind
  one lock every driver takes: about eight takes and gives for ten keys.
  The cost grew with the drivers. Each thread now keeps the last set it
  freed for its next one.
- **One hot key.** firn's client grew from 80 to 280 bytes with the
  connection commands, and a read of one of its fields through a reference
  copied the whole client first; at 280 bytes LLVM no longer removed the
  copies, and `SADD`'s compiled statement made seven copy calls where
  `cea9188d4`'s and the migration's, `0e992aa3d`, before the connection
  commands, made one
  ([profiles and copies](../../experiments/io-completion-bench/shared-state-14900k-profiles.txt)).
  A field read through a reference now loads that field alone.

After both, on 4, 8 and 16 server CPUs, `SET`, `GET`, `INCR`, `ZADD` and
`LRANGE_100` answered 0.97 to 1.02, the four tests on one hot key 0.89 to
1.00, and `MSET` 0.80, 0.84 and 0.90; with the append-only file on, on 4,
0.88 to 1.00. Two costs remain:

- `MSET` builds an ordered set that owns its keys' bytes, a binary search
  and a copy per key, where `cea9188d4`'s statement collected the keys'
  places in the request and sorted them. Profiles of `MSET` on four drivers
  show the head's time in `run_mset`, where the set's insertion is
  inlined, and in `insert_key`, where `cea9188d4`'s went to sorting its
  held keys (`sift_held`). It is the price of keys that are values when a
  statement begins.
- On one hot key the head stays 4% to 11% below on some counts. Profiles of
  `SADD` on four drivers spend 60% of the CPU waiting for the cell in both
  builds and show no further step of the head's, so the cause is unknown.

The append-only file's records and every reply are unchanged (firn under
the design, above).

## Prepared keys: source order and lock order

The owner selected one explicit preparation call after application deduplication.
The application chooses which value a duplicate key keeps or how its count is
combined; the locking interface carries neither policy nor business payload.
`key_prepare` consumes a `KeySource` containing owned bytes and a list of
`KeySpan` ranges, and returns a `PreparedKeys` with a transitively readonly
source and a private permutation. Entry index i refers to source span i.
Only acquisition follows the private order. Moving out the source consumes
the wrapper and returns both buffers, allowing Firn to retain its request
buffer, including a pipelined tail. Errors return that same source.

Bounds validation precedes duplicate detection. The draft selects the first
invalid source index or, with valid bounds, the smallest original duplicate
index pair. This makes errors independent of the private sorting algorithm.
The concrete boxed source and these error details remain part of the final
specification review. The existing KeySet path remains during development:
replacing it must not silently remove its fixed-source-storage/no_heap use.
A boxed source requires source allocation when constructed; that fact does
not establish that preparation of an already-owned source allocates from the
program's heap. Runtime allocation and the allocation-effect closure must be
checked separately.

The true dependencies are source construction and application deduplication,
then validation and ordering, then acquisition in that order, then protected
operations. Independent input work remains ordinary source computation.
There is no required dependency between value construction and sorting, but
the chosen interface does not add a new asynchronous protocol to overlap
them. Firn initially keeps value construction in the protected operation,
so the experiment changes preparation without claiming that overlap.

The runtime candidate retains source bytes and sorts compact original indices.
It provisionally stores up to 16 indices inline and uses runtime storage above
that count. A moved wrapper contains no pointer to its former inline storage.
The implementation must release only the private order on source extraction
and release both owners and the order on full destruction. The layout ceiling
is provisionally 160 bytes with alignment 8; measured layout pressure or a
better inline threshold reopens this representation choice.

### Performance question and criteria, before measurement

Does separating application order from acquisition order remove the ordered
set's repeated copying and insertion work without losing MSET throughput?
The historical 14900K/WSL2 measurements above are a motivation, not a result
for the current candidate. That machine is presently unavailable. Measure
each comparison on one host; the available local host is an Apple M1 Pro
running Darwin arm64. A Linux CI comparison is a separate environment.

1. Run the same native source and build with both algorithms: ordered insert
   and prepared permutation. Include 0, 1, 4, 10, 64 and 1024 distinct keys,
   short and long keys, sorted/reverse/shuffled order, and both early and late
   distinguishing bytes. Validate against generated byte identities and
   expected original indices before timing. Report prepare-and-release time,
   warm allocation calls and bytes, and explicit key-copy/metadata-move bytes.
   This fixture substitutes its allocator and is single-threaded; it does
   not establish runtime-pool contention or full application performance.
2. Compare complete Firn images built with the same optimization and client:
   the candidate, the published PR208 head, and the pre-regression revision.
   Resolve and save full revisions and binary hashes at execution. Include
   MSET, with SET and GET controls, pipeline depths 1 and 16, and available
   driver counts. Explicitly use three-byte values, the Redis 7.0.15 default
   used by the historical MSET suite. GET uses a completely populated
   100,000-key domain and therefore measures hits; this is a controlled
   comparison across images, not a replay of historical random-population
   occupancy. Rotate run order, preserve every raw sample, and include an
   identical candidate image under a second label as a noise control. State
   whether CPU affinity is supported; do not claim pinned cores on Darwin.
3. Use six paired rounds. A candidate/control throughput median outside
   0.97–1.03 or a paired throughput-ratio interquartile range above 0.05 makes
   that cell inconclusive. Compute quartiles by linear interpolation at
   positions `(sample_count - 1) * 0.25` and `(sample_count - 1) * 0.75` in
   the sorted paired ratios. Repeat the entire matrix at most once after an
   identified environmental correction; retain both runs. Do not keep the
   best rounds or reinterpret missing cells as passing.
4. The provisional recovery target is MSET median throughput at least 0.97
   of the pre-regression image in every stable measured cell, with SET/GET
   at least 0.97 of the published PR208 image. Report p50/p95/p99, memory and
   allocations as well. A stable p99 increase over 10 percent, increased
   per-request key copying, a correctness failure, or failure of a throughput
   target rejects a claim of completed recovery and requires investigation.
   If latency control itself differs by more than 10 percent, latency is
   inconclusive. These tolerances are experimental criteria, not a change to
   the user's performance goal or proof about the unavailable original host.

The falsifier for the attribution is that removing copies and insertion moves
fails to reduce preparation time, or preparation improves while full MSET
does not recover. Report either outcome and profile the remaining work.
An end-to-end improvement alone does not attribute the change to sorting.

### M1 Pro ten-key preliminary result

The [raw samples](../../experiments/io-completion-bench/prepared-keys-m1pro-n10.csv)
contain all 432 observations: four key lengths (8, 16, 32 and 256 bytes),
two distinguishing-byte positions, three input orders, six paired rounds,
and ordered insertion, bulk preparation and an identical bulk control.
The host was an eight-core Apple M1 Pro with 32 GiB RAM, Darwin 25.6.0
arm64, unpinned, using Apple Clang 21.0.0 with `-std=c11 -O2`. These are
preparation-and-release timings, not complete Firn measurements.

| Ten-key source | Ordered insert, median ns | Bulk, median ns | Paired median speedup |
| --- | ---: | ---: | ---: |
| 16 bytes, shuffled, distinction at head | 281.29 | 193.59 | 1.462 |
| 16 bytes, shuffled, distinction at tail | 277.98 | 191.99 | 1.448 |
| 256 bytes, shuffled, distinction at tail | 467.71 | 539.71 | 0.868 |

All 24 cells met the identical-control median and interquartile criteria.
The long-prefix shuffled cell regressed about 15 percent in time, so this
result rejects a claim that the current preparation algorithm improves
every tested key shape. The other cells and every round remain in the raw
file, including the eight-byte shuffled cases whose median speedups were
only 1.005 and 1.020. Application deduplication, source-buffer construction,
the compiler's owned-result transfer and concurrent allocator contention
are outside this fixture.

The source was the uncommitted prepared implementation over
`d04c684bc6930dbb95b3a4adfc82eae1a54603ff`, not that commit alone. The fixture
was `key_prepare_bench.c`, with only its count array restricted to `{10}`
and its runtime include resolved to the current source. Restricting that
constant array can permit compiler specialization; the complete matrix and
the Firn program remain necessary evidence. SHA-256 identities are:

- fixture: `3ec236fc1cce9b625c39145b22d2ce6a9616980f2a405b56dff0735f3d57d9f7`;
- `concurrent_map.c`: `44ceb962db6e9c1071816b7fa41a0856b2d28fddc6a71effc31d5c1046f102ca`;
- `concurrent_map.h`: `e48498b1e87ae3169e75278f64fe616c3952834b2c7e94555c3fbfb6c088aff7`;
- pilot binary: `34a38080c599655d44989ca2fe6671e7ab156d7eccb458bce7a54ec295677c52`;
- raw CSV: `4f56cd39321611dc667b119667b4641892744c363df958e87138227ad3e6383a`.

Both fixture modes passed the full correctness matrix before measurement;
fourteen scratch mutants (seven independent failures in each mode) failed
at their intended oracle. Instrumentation fields in the timing CSV are
unobserved zeros, not measured zero allocations or copies.

### Small-batch sorting experiment

The first M1 Pro preparation-only comparison, restricted to ten keys, found
that the heap sort improved shuffled sixteen-byte keys but regressed
shuffled 256-byte keys distinguished at their tail. All six paired rounds
and the identical-image controls are retained with the measurement record.
This result does not include application deduplication or the owned wrapper.

Before selecting a change, compare the current heap sort with binary
insertion of indices for at most sixteen keys, retaining the existing
already-ordered path and heap sort above that threshold. Neither algorithm
changes the dependency between validation, ordering and lock acquisition;
both small-batch sorts are serial, and comparisons of long common prefixes
make their comparison counts relevant. Only indices move, never source
bytes or application payloads. Keep the production path unchanged during
the scratch comparison. Validate both against generated key identities,
including duplicates and original-index error selection, before timing.

The trial must include 4, 10, 16, 17 and 64 keys, all existing key lengths,
orders and distinguishing-byte positions. Use the same paired noise
criteria above. Select the candidate only if it removes the observed
ten-key long-prefix regression without a stable preparation-time regression
above three percent in another trial cell; any inconclusive cell remains
unverified. A microbenchmark win still requires the full Firn comparison.

Rejected forms are a public new/sort/finish protocol, because it exposes
intermediate states solely to schedule an overlap not required by the
consumer, and a copied-key getter with a separate business payload, because
the existing source already owns the bytes and source indices identify the
application metadata. Neither extra surface is needed to establish lock order.

## Refused along the way

- **A block with no target whose locks, keys included, the compiler derives
  from every access.** The keys of a loop such as `MSET`'s are computed inside
  the block, so the compiler had to run the part of the block that computes
  them before taking any lock: the generated code differs from the written
  code, that part runs twice and must answer the same both times, and five
  conditions guard it (PR #208 met a clock read, a key formed out of bounds
  after a dropped guard, and others). Naming entries in the header replaces
  it.
- **The whole map written in the header while only the reached entries are
  locked**: the header misstates what is locked ("the header is misleading;
  what is locked is just those few objects").
- **Locking the whole table when the keys come too late**: it lets slow code
  through unseen, against making the default the best.
- **A rule that every key used on a table must exist before the statement**:
  a rule about one container written into the language ("I want the error to
  be the language's own, from the type system or ownership").
- **Taking some locks later than others by a fixed order of kinds**, maps
  before objects: an order prescribed for one optimization ("a hack on a
  hack").
- **An order of store types inferred from every atomic statement**: it needs
  every function body.
- **Lock levels declared in store types**: unnecessary once a statement holds
  one state, whose fields already give the order.
- **An order of fields over separate handles placed in a struct**: the same
  two handles can sit in two structs in opposite orders. Sharing the struct
  whole removes that.
- **A plan before the block in which every lock is declared with `hold`**: it
  stated parts whose identity the declaration already fixes; only the part
  naming entries survives, as the header.
- **Taking each entry at its first use in code order**: two statements meeting
  two keys in opposite orders wait for each other.
- **Sharing by reference**: above.
- **Shards owned by contexts, reached by messages**: every command pays two
  hand-offs, a command over keys of two shards needs a protocol between their
  owners, and the queue between contexts is the same run-time exclusion moved,
  not removed.
- **Transactions that read optimistically and retry**: a block runs once, moves
  values and writes its context's locals, so it cannot be repeated.

## Open

- finer layouts chosen at composition;
- releasing a unit once its block no longer uses it, after the last taking;
- wait lists per entry instead of per table;
- a cheaper read of one entry than the reader count;
- the names, the payload operations and the spelling of header bindings,
  which v0.86 settles as `KeyedTable`, `KeySet` and `KeyedEntries`,
  `key_set_put` and `key_set_add`, and `e = &s^.t[k]`, for the owner to
  confirm.
