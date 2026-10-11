# Enumerating and clearing a concurrent hash map

## The question

A program can reach a `ConcurrentHashMap<V>` entry by key, reach the entries
under a `KeySet`, and count the map's `Some` entries (active specification
v0.93, [SHARE-1] and [SHARE-2]). It cannot learn which keys a map holds, and a
statement holding a map cannot empty it.

Minimal witnesses:

- Given `m: Shared<ConcurrentHashMap<u64>>` after `m["a"] = Some(1)` and
  `m["b"] = Some(2)`, no program computes `["a", "b"]` without already
  knowing those keys.
- A statement `atomic t = &m { ... }`, or a callee receiving
  `keys: &ConcurrentHashMap<V>` from one, cannot leave every entry `None`:
  `swap` needs a second map, reached only through a second target of the
  same statement, and a callee that does not wait cannot take one [SHARE-2].
- Given a `KeySet` `ks`, no program computes the bytes of the key at index
  `i`: the set exposes only `len`.

Firn-wf needs these for Redis's `SCAN`, `KEYS`, `RANDOMKEY` and for
`FLUSHALL`/`FLUSHDB` inside `EXEC` and scripts (Firn-wf `docs/todo.md`, "A
program cannot enumerate the keys of a `ConcurrentHashMap`" and "A held
concurrent map cannot be emptied without another held map"). Firn-wf
confirmed on 2026-10-06, at firn `c60db650d`, that every one of these
commands runs under a whole hold of its one keyspace map, through a whole
target's binding or through a `keys: &ConcurrentHashMap<Entry>` parameter a
script's executor receives; that it filters `MATCH` and `TYPE` itself after a
step; that at-least-once suffices; that every step must advance or end the
scan; that a stale cursor must stay safe; and that a cleared map's storage
should be given back within the statement or right after it.

## What a resumable cursor must survive

`SCAN` runs one bounded step per client request, each in its own statement,
so between two steps other contexts insert, remove, grow and shrink the map.
Redis promises that a key present from the first step to the last is
returned at least once and allows repeats; its cursor is a bucket index
incremented in reverse binary, because Redis homes a key by the low bits of
its hash and its table doubles and halves.

The runtime index (`compiler/src/backend/concurrent_map.c`) homes a key by
the top bits of a multiplicative hash: a byte key's tag is a 62-bit hash of
its bytes (`tag_of`), and its home cell in a table of 2^b cells is
`(tag * 0x9E3779B97F4A7C15) >> (64 - b)` (`start_of`). Multiplication by an
odd constant is a bijection on 64-bit words, so the product, called here the
key's *position*, is distinct for distinct tags, and the home index is the
position's top b bits in every table size. Increasing home indices therefore
visit increasing position ranges whatever size the table has when a step
runs.

## Proposal

### The cursor

Every byte-string key has a position, a `u64`, fixed for the whole execution
and the same in every map; which position a key has is an input of the
execution, as the order of atomic statements is [WAIT-2]. A step

```
fn map_scan<V: drop>(map: &ConcurrentHashMap<V>, cursor: u64, count: u64, keys: &KeySet) -> next: u64 reads(map), writes(keys) contract {
  ensures keys^.len >= entry(keys)^.len;
};
```

takes an extent `e`, an integer with `cursor < e <= 2^64` that is an input
of the execution, inserts into `keys`, as `key_set_insert` does, exactly the
keys whose entries are `Some` and whose positions lie in `[cursor, e)`, in
increasing order of position and, among keys of one position, in
lexicographic order of their bytes with a proper prefix first, and returns
`e` modulo 2^64, so `0` exactly when the step reaches the last position. An
implementation may choose the extent by `count`, which states nothing else:
one that ignores it conforms, and no result size or work bound follows from
it, since keys of one position cannot be split between steps.

Consequences:

- A scan from `0` to `0` partitions the position space into disjoint
  intervals, so a key whose entry is `Some` at every step of the scan, and
  whose position is fixed, is inserted exactly once; a key inserted or
  removed during the scan is inserted at most once.
- Every `u64` is a cursor with a meaning, so a stale or forged cursor is
  safe, and any connection may continue another's scan.
- `next > cursor` or `0` bounds a scan by 2^64 steps and lets a loop within
  one statement (`KEYS`) end.
- A program learns what the rule entails: a key returned by a step lies in
  that step's range, so scans of a one-key map from chosen cursors can find
  the key's position by bisection. The rule states that order, so nothing
  the program infers rests on an unstated property of the index.
- Sorting a step's keys is a choice apart from the coverage guarantee: each
  step's order could instead be an input of the execution. Sorting gives a
  complete scan of the same keys one sequence in every map; its cost beside
  reading the step's cells is assumed small and has not been measured, and
  `count` does not bound a step's keys.
- `RANDOMKEY` is a run of steps from a random cursor, wrapping once at `0`,
  all under one hold, until one inserts a key; a step may insert none. The
  key it finds is weighted by the gap of positions before it, as Redis's
  choice is not uniform either; Firn-wf needs no distribution (2026-10-06).

Alternatives to this proposal, A:

- **B. An opaque cursor with Redis's at-least-once promise.** Every cursor
  can be given a safe meaning, but the coverage promise is a property of a
  chain of calls rather than of one call, which makes the rule longer and
  weaker, and a cursor encodes the table's index structure. Its runtime
  saves one multiplication per cell read and the sort of a step's keys.
- **C. A specified hash.** Positions become computable from the rule, which
  fixes the runtime's hash in that specification version and lets an input
  choose colliding keys. The present runtime's hash is unseeded, so its
  positions are computable from its source today; A leaves seeding to the
  implementation.

What would reject A: a runtime step that cannot filter by position under a
whole hold without reading cells outside the probe runs of its home range,
or a measured step cost dominated by the position sort.

### Bounded work and `count`

The specification states no cost. A step first reads the map's exact count
of `Some` entries as the statement sees it (`wf_cmap_count_held`), which
writes nothing, and ends the scan at once when it is zero. When `count`
reaches that count, the step's homes run to the table's end, so a map of at
most `count` keys is covered in one step. Otherwise the runtime chooses the
homes from `cursor`'s upward until about `count` keys have been passed, or
`count` times ten times the table's cells per live entry, at most the
table's capacity, as Redis bounds a step by ten empty buckets per requested
key in a table it keeps sized for its keys; `count` of zero means ten. The
extent is the first position of the first home not chosen. The step then
reads every probe run of the chosen homes to its end, past that budget,
since a key displaced beyond the budget would otherwise be skipped when the
next step starts past its home; the worst case reads the table's capacity.

The first budget, ten cells per requested key whatever the table's size,
was refuted by Firn-wf's trial of the first experiment release on
2026-10-06: Firn-wf presizes its keyspace for 262,144 keys, a 2^19-cell
table, and a step over five keys at count 1,000 returned a nonzero cursor
and no key, while an empty map took capacity / (10 * count) steps, 5,243
at Redis's default count of 10, where Redis answers cursor 0 at once.
`scans_sparse` in the runtime test reproduces both.

### Clearing

```
fn map_clear<V: drop>(map: &ConcurrentHashMap<V>) -> result: unit writes(map);
```

leaves every entry of the map `None`, releasing every value its entries held.

The design tree refused `map_clear`
(`design/language/waiting/shared-objects/keyed-tables.md`, Rejected):
"swapping with a fresh map releases old entries outside every lock, while
clearing releases them under the whole hold". What changed: a statement
reaches a second map only through a second target, and a nonwaiting callee
such as a script's command executor cannot take one, so a map held through a
reference cannot be emptied at all; and releasing values is not observable,
so the implementation may move the entries out under the hold and release
them after the statement, which answers the refusal's cost. The runtime
already exchanges two maps' entries under a whole hold (`wf_cmap_swap`); a
clear exchanges the held map's entries with a new empty map's, settling the
hold's own entries first, and keeps that map, with the address of the map
type's drop helper, on a list of the cleared map. The hold's release
(`wf__table_hold_release`) takes the list while it still holds the map, so
no other clear can add to it, gives the hold up, and then runs each drop
helper, which drains the map, releases each `Option<V>` and frees it, as a
map's last handle release does. A prelude row is a callee of its own, so
its locals cannot carry the map past the statement; the list on the map is
the statement's.

Alternative: no clear; a program scans every key into a `KeySet` and writes
`None` through `&t^[ks]`. That is O(n) work and a copy of every key under the
hold, the cost the refusal names.

### Reading a key set's key

```
fn key_set_read_key(keys: &KeySet, index: u64, out: &[u8]) -> length: u64 reads(keys), writes(out) contract {
  requires index < keys^.len;
};
```

copies the first `min(out^.len, length)` bytes of the key at `index` into
`out`, leaves the rest of `out` as it was, and returns the key's length; an
empty `out` asks for the length alone.
A reference result (`-> &[u8]`) is refused by [REF-3] today, and appending to a
`Box<Slots<u8>>` would fix a growth policy in the prelude. Firn-wf copies
each key into its reply buffer anyway.

## Retained bytes and the contract-language gap

The owner's option B for firn's reservation accounting selects
`key_set_bytes(keys: &KeySet) -> u64`: the heap holding attributable to the
set's storage, including unused capacity, and exactly the holding its release
returns. The v0.128 amendment states the operation in [SHARE-1] and [PRE-1]
of the [active specification](../../../spec/kernel-spec.md), and states
that a successful bounded scan increases this quantity by at most its limit.
`ScanStep` keeps its existing fields. The owner's subsequent option A ruling
closes the contract-language gap through the ordinary callable boundary.

The minimal semantic witness is one map containing a key and one destination
set. Save its retained holding B, call `map_scan_within` with limit L, and
on `Next` observe holding A. The needed mathematical proposition is
`A - B <= L`. A caller needs to carry that fact into reservation accounting
without testing the inequality again. Inspection of v0.127 found four
independent restrictions:

- [FN-9] admits routed postconditions only for `Result.Ok` and
  `Option.Some`, each with the field `value`. Neither `when Next(next: r)`
  nor a `Next` route with a renamed field is admitted for `ScanStep`.
- [FN-8] excludes ordinary function calls from clauses. Calling
  `key_set_bytes` in a clause is therefore unavailable even though the
  operation only reads its parameter. [MSR-1] gives `KeySet` only a `len`
  measure, which cannot name retained bytes or establish that a byte query
  observes the same state datum.
- [MSR-3] already supplies `entry(parameter)` for a written reference's
  entry-state measure or fragment-integer place. No `old` former is needed,
  but the existing former has no byte measure to project and cannot name
  the value of a call.
- [FN-9] publishes only difference-bound relations: one datum with
  coefficient one plus a constant on each side. `A - B <= L` has three
  varying datums. A contract `define` expands inline, so it neither makes
  the difference one datum nor captures B. The affine range-clause route
  [RANGE-1] also excludes calls and entry images, and requires a bound
  variable; it does not supply this missing state relation.

The selected option A keeps `ScanStep` and adds no general enum route solely
for this operation: a `Needs` outcome leaves the set unchanged, so the bound
holds unconditionally. [MSR-1] adds the readonly KeySet `bytes` measure,
and `key_set_bytes` declares its result equal to that measure. [MSR-3]'s
entry former names its call-entry value, with the projected spelling
`entry(keys^.bytes)` and the existing `entry(keys)^.bytes` denoting the same
quantity. [FN-9] admits one additional own integer parameter term with
coefficient one on one side of a relation. The bounded scan declares:

```whitefoot
ensures entry(keys^.bytes) <= keys^.bytes;
ensures keys^.bytes <= entry(keys^.bytes) + limit;
```

Formation, selected-return proof, caller publication, effect invalidation and
the existing fixed affine residual families apply generally to user functions
as well as prelude declarations. An internal mathematical sum retains the
ordinary relation and evidence path; it does not execute an addition and need
not fit u64. The query result preserves the byte measure's immutable image,
so a prior observation survives a scan while a subsequent observation reads
the new state. An intervening KeySet write kills the scan's exit-byte fact.
Conformance cases require both reservation subtractions to be admitted and
reject a missing entry observation, an unrelated reservation value and a
post-scan write. Their verdicts remain unverified until CI runs them.

Option B, the runtime keeping the reservation account, was rejected because
it only solves this one interface and reverses the shape just chosen. Option C,
a runtime check in firn, was rejected because it routes around a language gap
with a runtime check. Calls remain excluded from contract expressions; the
measure and ordinary result equality supply the needed state observation.

A caller writes the account as two subtractions bound by `let`
(`let used = after - before;` then `left - used`), the composition
[GRAM-6] prescribes for every expression; the conformance witness uses
that form.

The comparison that decides the proposal is the same witness through an
ordinary wrapper: the caller must prove the reservation relation from the
wrapper's declared boundary alone. It must lose the current-state relation
after a write to the set while preserving the saved entry observation.
Reject a route that needs a runtime inequality check, changes `ScanStep`,
publishes a fact only for the prelude's symbol, or admits a stale observation
after a write. No experiment or proof-checking run has been performed for
this proposal.

The runtime byte query sums the host-granted item/index store and byte arena
capacities; live key lengths alone omit unused capacity. Public KeySet
release must return both allocations to the runtime pool. Its previous
thread-local spare path kept those allocations in the metered heap after
release, so it could not meet the selected meaning. No production caller
would populate that cache once public release bypassed it, so the cache is
removed and every KeySet release returns its storage. The performance effect
of losing this reuse is unmeasured.

## The runtime step under a whole hold

A step runs in a statement holding or reading the map whole, which has
waited out every keyed statement. Every statement that starts a move
finishes it before it leaves the map: a keyed release, a hold's release and
a write under a whole hold (`wf_cmap_held_entry`) each call `finish_move`,
which waits until the next table is current. So no move is under way when a
step begins, and the step reads the current table only; reading the old
table beside the next would insert keys a move copied twice. The step writes
nothing of the map, its cells or any hold, keeping its keys in its own frame
or host memory, because statements that read the map whole may run at once
(compiler/waiting-contexts/state-locks). The statement's own entries are cells its hold has
locked: their key words carry the lock bit, and a step reads their nodes and
treats a slot whose tag is `None` as absent, as `wf_cmap_count_held` does.

A step strips the lock and pending bits from each key word, skips empty and
removed cells, and reads each node's slot tag, skipping `None`, so the
statement's own locked entries count as they stand, as
`wf_cmap_count_held` counts them. For home indices `i0 = cursor >> shift`
through `i1 - 1`, it reads cells
from `i0` forward to the first empty cell at or after `i1 - 1`, wrapping at
the table's end; keys sit at or after their home, before the first empty
cell after it, and the read stops after the table's capacity in cells,
since racing claims can leave a small table full (the index's cell design).
It keeps each key whose position lies in `[cursor, i1 << shift)`, sorts
the kept keys by position and bytes, and inserts them into the set.

## Review before implementation

On 2026-10-06 a separate read-only agent (GPT-6 Astra, effort max) reviewed
this proposal against the specification v0.93, the design tree and the
runtime. Its verdicts were sound with changes on each question; the changes
taken are above: the opacity claim withdrawn, alternatives B and C restated,
`count` stated as advice, the work budget kept from truncating a probe run,
the cleared maps' release given an owner, the scan kept free of writes, and
the conformance plan below. It also found that a hold kept the caller's
bytes of each key after its take, while a move under the whole hold finds
the entry again by them: a statement that changes the bytes it named a key
by and then grows the map aborted. The hold now keeps the node's bytes from
its take on (`fill_slots`), and `holds_keep_their_bytes` in the runtime
test reproduces the abort without the change.

## Conformance and runtime tests

The conformance cases state the language's guarantees with one exit code in
every execution, asserting membership, uniqueness, cursor progress and
equality between scans, never a position assignment or a step's size:

- `share-pos-map-scan-every-key-once`: a scan in steps inside one statement,
  over the statement's own insertions and removals, merged step by step
  into a set that refuses repeats, and checked against the removed and the
  present keys.
- `share-pos-map-scan-across-statements`: steps in separate statements with
  growth, removal, and removal and reinsertion between them.
- `share-pos-map-scan-beside-other-contexts`: a spawned context writing
  other keys during the scan.
- `share-pos-map-scan-order-across-maps`: complete scans of two maps holding
  the same keys, filled in opposite orders and one after other keys came and
  went, give one sequence; forged cursors.
- `share-pos-map-scan-into-a-used-set`: a scan through a callee's reference
  into a set already holding keys.
- `share-pos-map-scan-short-and-prefix-keys`: the empty key, prefixes, and
  bytes 0 and 255.
- `share-pos-key-set-read-key`: lengths, truncation, the untouched rest.
- `share-pos-map-clear` and `share-pos-map-clear-wakes-a-guard`.
- Rejections: a clear or a scan invalidating an entries reference [REF-2],
  a clear in a guard [SHARE-2], an unproved key index [FN-8].

Conformance cannot require a resize, a step of a given size or a collision,
so the runtime test (`compiler/src/backend/concurrent_map_test.c`) covers
them: `scans_resume` scans in steps against a plain reference while the
table grows and shrinks, and in the build that narrows hashes, where many
keys share a position and runs wrap at the table's end, checking order,
ranges, uniqueness and coverage; `scans_write_nothing` compares the map, its
cells and the hold before and after a scan; `maps_clear` checks that
cleared entries are released once, after the hold, without a leak.
