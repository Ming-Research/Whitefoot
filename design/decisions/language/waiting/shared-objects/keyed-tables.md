# Keyed tables

The concurrent hash map, the state many contexts reach by key: its keys, key sets, how a statement names its entries, and how a program scans and clears it.

### decision concurrent-hash-map-is-only-a-shared-state
`ConcurrentHashMap<V>` holds an `Option<V>` entry for every byte string and exists only as a shared object's state, made with its object by `shared_map_new`.
- replaced: A keyed table that could be a field of a shared state or an unshared local value was on main; the owner changed it, because a concurrent table is shared by nature and a type's behavior should not change with where it is put.
- replaced: A handle type of its own, `SharedMap<V>` with state `SharedMapState<V>`, was on main; the owner changed it, because a second handle type made a map's entries and its state's other fields two statements, nested by kind.
- rejected: an agent proposed a single-threaded hash map as a field of a state; the owner rejected it, because different contexts insert, remove and move entries under different keys at once, so such a map could be locked only whole.
- rejected: an agent proposed a map sharded into `Shared` objects, each a lock and a table; the owner rejected it, because the runtime's index led or tied every measured cell against the fastest rivals, while shards serialize unrelated keys (data: `research/investigations/concurrent-map/DESIGN.md`).
- rejected: an agent proposed a `shared struct` declaration; the owner rejected it, because once one statement holds several objects, maps and other state live in separate objects held together, so one guard construct and one placement table cover every case.
- rejected: an agent proposed a `map_reserve` operation; the owner rejected it, because construction fixes a map's initial capacity and cooperative index moves grow it.

### decision map-key-is-a-byte-string
A map's key is a `&[u8]` that the runtime hashes and compares.
- rejected: an agent proposed a key of any type with a `HashMapKey` binding; the owner rejected it, because the runtime would then call the writer's hash and equality under a cell's lock in the middle of a probe.

### decision statement-names-map-parts-by-target
A statement names a map's parts by targets: `e = &h[k]` for one entry, `es = &h[ks]` for the entries under a key set, `t = &h` for the whole map, alone on its handle place.
- replaced: Reaching a table through the state's binding, with header grants and their provenance, was on main; the owner changed it, because each target now names a map handle directly, and a callee receives a map only through a whole target.
- replaced: An implicit whole hold, when a block formed any other path to a table, was on main; the owner changed it, because the header then said less than the block held, and the slowest hold a statement can take was the one it did not show.
- replaced: A whole-map statement that held every entry was on main; the owner changed it, because a statement names the entries it reaches in its header, and counting is an operation on the table.
- replaced: Holding only the entries a whole-map statement's block reaches was on main; the owner changed it, because the header then said the whole map while only those entries were locked, and finding their keys meant running part of the block first.
- rejected: an agent proposed a hold per field that is no map, `m = &s^.meta`; the owner rejected it, because fields share one object lock, while fields that change apart can live in separate objects named by separate targets.
- rejected: an agent proposed admitting a path to a whole-bound table through `s` beside its binding; the owner rejected it, because a binding then had two spellings, and the check that every binding is used could not tell a forgotten one.
- rejected: an agent proposed a compiler that runs the part of a block that computes its keys before taking any lock; the owner rejected it, because the generated code would differ from the written code, key computations would run twice, and a key it could not compute fell back to the whole table.
- rejected: an agent proposed locking the whole table when a block's keys are known too late; the owner rejected it, because it lets slow code through unseen.
- rejected: an agent proposed taking each entry at its first use in code order; the owner rejected it, because two statements that meet two keys in opposite orders then wait for each other.

### decision whole-target-map-is-indexable
A whole target's map, and any `&ConcurrentHashMap<V>` a callee receives, is indexable: `t^[k]` is the entry under a key and `&t^[ks]` the entries under a key set.
- rejected: an agent proposed a lookup row that returns an index; the owner rejected it, because a map has no index a program could hold.

### decision key-set-keeps-insertion-order
A `KeySet` holds distinct keys in first-insertion order, `key_set_insert` answers a key's index, and a set target reaches the entry of the set's key at each index.
- replaced: Keys in byte order, each with a payload a put replaced or an add summed, was on main; the owner changed it, because that order was the runtime's lock order, which replies do not follow, and each insertion cost a search and a move (data: `research/investigations/shared-state/DESIGN.md`).
- rejected: an agent proposed an opaque key set whose length and order exist only inside the statement; the owner rejected it, because the block would still visit the keys in the lock order, so replies in argument order would still need a second ranking.

### decision read-only-statements-share-an-entry
A statement whose guard and block write nothing through an entry target holds that entry beside the other such statements on its key, and nothing in the source marks it.
- rejected: an agent proposed that every keyed statement hold its entry alone; the owner rejected it, because readers of one list then took turns: firn's `LRANGE_100` stayed at 1.24 to 1.55 million a second while Garnet reached 2.83 million (data: `research/investigations/concurrent-map/DESIGN.md`).
- rejected: an agent proposed a read form the writer marks; the owner rejected it, because the outcome is the same either way and the checker already knows which paths a block writes, so the writer would choose for a difference no program observes.
- rejected: an agent proposed reads that check a version afterwards; the owner rejected it, because a block that runs once cannot redo its work after a torn read.

### decision map-type-names
The map type is `ConcurrentHashMap<V>`, the key set `KeySet`, and the entries a target names over a set `Entries<V>`.
- replaced: Naming them `KeyedTable` and `KeyedEntries` was on main; the owner changed it, because a map now has only its concurrent shared meaning, and entries exist only as a reference's referent.

### decision map-scan-steps-a-position-cursor
`map_scan` steps from a `u64` cursor, inserting into a key set every `Some` key whose position lies from the cursor up to the step's extent, and returns the extent, `0` at the end.
- rejected: an agent proposed an opaque scan cursor with Redis's at-least-once promise; the owner rejected it, because that promise holds over a chain of calls rather than one call, which makes the rule longer and weaker, and the cursor would encode the table's index structure.
- rejected: an agent proposed a specified hash of a key's bytes as its position; the owner rejected it, because it fixes the runtime's hash in the specification and lets an input choose colliding keys.
- rejected: an agent proposed a scan step's keys in an order that is an input of the execution; the owner rejected it, because sorting makes a complete scan of the same keys one sequence in every map, at a cost assumed small beside reading the step's cells.

### decision map-clear-empties-through-a-reference
`map_clear` makes every entry of a map `None` through any `&ConcurrentHashMap<V>`, and the old values are released after the hold.
- replaced: Refusing `map_clear` and clearing by a swap with a fresh map was on main; the owner changed it, because a nonwaiting callee that holds the map through a reference, such as a script's command executor, cannot take the second map a swap needs.
- rejected: an agent proposed clearing by a scan that writes `None` under every key; the owner rejected it, because it copies every key and releases every value under the hold.

### decision key-set-read-key-copies-a-prefix
`key_set_read_key` copies as many first bytes of a key set's key at an index as a range holds, and returns the key's length.
- rejected: an agent proposed appending the key to a `Box<Slots<u8>>`; the owner rejected it, because that would fix a growth policy in the prelude.

### decision bounded-scan-is-a-separate-operation
A scan bounded by heap growth is a separate `map_scan_within` that returns `ScanStep::Next` or `ScanStep::Needs`, and `map_scan` is unchanged.
- rejected: an agent proposed giving `map_scan` itself the limit and the `ScanStep` result; the owner rejected it, because every caller would then have to handle `Needs`.
- rejected: an agent proposed returning an unchanged cursor when the first position does not fit; the owner rejected it, because a caller that reserves memory before it scans must learn the size it needs before anything is copied.
- rejected: an agent proposed a bound that counts only the keys' byte lengths; the owner rejected it, because the key set's own index and growth would not count, so firn could only estimate a factor and could not reserve strictly.

### decision key-set-reports-retained-bytes
`key_set_bytes` reports a key set's heap holding, unused capacity included, equal to its readonly `bytes` measure, and `map_scan_within` bounds that measure's growth by its limit.
- rejected: an agent proposed a `Next` variant that also carries the bytes the step retained; the owner rejected it, because it serves only this one operation, leaving what `key_set_insert` adds and what a release returns unknown.