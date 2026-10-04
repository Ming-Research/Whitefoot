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
- `MSET`'s remaining cost (below).

## MSET after the redesign

The owner approved the names, the payload operations and the header spelling
as v0.86 wrote them (Q25), and the design landed in main as specification
v0.89. `MSET` stayed below `cea9188d4` on every measurement: 0.80, 0.84 and
0.90 on 4, 8 and 16 CPUs of the 14900K (Measured, above), and in a later
Linux runner comparison of the redesign's head 0.95 and 0.88 on one server
CPU at depths 1 and 16 and 0.84 on two at depth 16, where `SET` and `GET`
answered 0.97 to 1.00 (run 37178917372 of a withdrawn branch, whose
`published` line is this design's head). A later key-preparation design
measured worse, 0.69 in that last cell, and was withdrawn; it is not
reconsidered here.

**The question.** Which work makes `MSET` slower than `cea9188d4`, and does
keeping the key set in byte order at every `key_set_put`, the binary search,
the move of later items and the copy of the key's bytes, account for most of
it? The redesign's statement (`run_mset`, `apps/firn/commands/strings.wf`)
builds the set from the request's spans, takes the entries in the set's order,
and then copies each value from the request; `cea9188d4`'s collected the keys'
places in the request and sorted them when it took the hold. The candidates,
not exclusive: H1, the ordered insertion; H2, the copy of the keys' bytes into
the set's arena; H3, the hold over a key set and its release; H4, work in
`run_mset` around the set.

**Measurement 1, before any change.** On one Linux runner, firn images of
`cea9188d4` (`base`) and of main at `263a9c564` (`main`), each built by its own
compiler with `--full-lto`, and the `main` image again under a second name
(`main-twin`), measured interleaved by `redis-bench.sh compare` on two server
CPUs: `MSET`, `SET` and `GET` at depths 16 and 1, six passes of ten seconds,
the order reversed on even passes; then a flat `perf` profile of each image
under `MSET` at depth 16. The criteria, fixed before the run:

- a cell is valid when the median over passes of `main-twin`'s rate to
  `main`'s lies within 0.98 to 1.02, and inconclusive otherwise;
- the loss is the median of `main`'s rate to `base`'s per pass, in each
  valid `MSET` cell, with `SET` and `GET` as controls;
- the profiles attribute the loss when the symbols of one hypothesis, as a
  share of `main`'s samples per request, exceed their share in `base` by at
  least half of the loss; otherwise they do not separate the hypotheses, and
  the next measurement changes one hypothesis's work in a diagnostic image
  instead.

**Measurement 1's result: inconclusive.** Run 37186349691, an Intel Xeon
Platinum 8573C runner with four CPUs. Every image answered every check, and
`main-twin` matched `main` in every cell (medians 1.000). But the rates do
not resolve the question: redis-benchmark's own rate divides by a clock that
ticks every 250 ms, and the sized runs lasted about five seconds, so `MSET`
at depth 16 read 759,805 or 725,557 requests a second, one tick apart, and
nothing between; at depth 1 every image and test answered 200,000, the two
client threads' limit. The profiles took the same number of samples for the
same requests (52K for `base`, 51K for `main`), so at two server CPUs `main`
spent no more server CPU per `MSET` than `base`, but that run counts spinning
for a cell (`try_entry`, 13% in both) as work, and one run carries no spread.

**Measurement 2.** The same images on the same kind of runner, the rate
taken over the run's wall time and the server's CPU time per request read
from `/proc` before and after each run, at one and at two server CPUs, the
other settings as before. The criteria, fixed before the run:

- a cell is valid when the median over passes of `main-twin`'s CPU per
  request to `main`'s, and of its rate to `main`'s, lie within 0.98 to 1.02;
- the loss is the median per pass of `main`'s CPU per request to `base`'s,
  in each valid `MSET` cell, the rates beside it; `SET` and `GET` are the
  controls, and a control off by more than 0.03 makes the comparison suspect;
- the profiles at one server CPU, where no other driver contends for a cell,
  attribute a loss as Measurement 1 states.
