# Shrinking the standard-library hash table

Status: implemented on the high-bit-home representation (board item
wf-need-firn-hash-shrink, Whitefoot#436). The owner ruled the three design
questions as recommended: automatic shrinking after a successful removal,
removal allowed to shrink a map reserved to its ceiling, and one-shot
migration. Correctness evidence comes from the maintained HashMap program;
the grow-only and removal-tail comparison on the 14900K is pending.

## Question and evidence boundary

Can the ordinary owning HashMap release sparse backing after removals, so
Firn's SPOP stops walking a large mostly empty table, without weakening any
safety guarantee or slowing workloads that only grow?

The supplied Firn report says a caller-side rebuild below 1/8 occupancy into
four buckets per member improved depth-1 SPOP throughput by 14% on two cores
and 4% on one core, with p99 ratios of 0.32 and 0.58 against its baseline,
while LRANGE_100 became 9% slower. Those are supplied observations, not results
reproduced here; the experiment's exact revisions, raw data and attribution
were not supplied. Its second Firn table was rejected as a workaround. These
numbers motivate a standard-library change, not a predicted result or a
relaxation of the grow-only requirement.

The first study inspected Whitefoot commit
`50773fdcd3007685e4cdd1903b2dcb0cc1052600`, active specification v0.146,
and [Firn's inspected tree](https://github.com/Ming-Research/Firn-wf/tree/c6f6058fb17277d4d44d364a45efad1eae01a22b).
Firn's [compiler pin](https://github.com/Ming-Research/Firn-wf/blob/c6f6058fb17277d4d44d364a45efad1eae01a22b/whitefoot.pin)
is `wf-d75f318d5631`, resolving locally to
`d75f318d56318834785322bde3a3656fb93116a6`. The hash-map module's interface
and implementation have no diff between that pin and the inspected Whitefoot
revision. Firn was read through the GitHub connector after a shallow clone
failed DNS resolution; no Firn files were changed.

The [constitution](../../../docs/constitution.md) makes safety a constraint
and performance a project requirement; implementation or migration effort
alone does not select a design. Real compatibility effects still need an
account. The [research method](../../README.md) requires attribution and a
falsifier before measuring. This record serves that purpose and remains the
home of this investigation's rationale and later results.

## Which table actually matters

There are three unrelated hash-table concerns here:

| Table | Role and source | Relevance |
| --- | --- | --- |
| `std::collections::hash_map::HashMap<Key, unit, ceiling>` | Firn's `Value::Members` in [store interface, lines 42–61](https://github.com/Ming-Research/Firn-wf/blob/c6f6058fb17277d4d44d364a45efad1eae01a22b/firn/store/module.wfm#L42) | This is the set's member table and the proposed change's home. |
| `Shared<ConcurrentHashMap<Entry>>` | Firn's outer keyspace, [lines 236–243](https://github.com/Ming-Research/Firn-wf/blob/c6f6058fb17277d4d44d364a45efad1eae01a22b/firn/store/module.wfm#L236) | It protects the set value under atomic entry access; its buckets are not what SPOP scans. |
| Compiler fact maps | [fact-map hashing decision](../../../design/compiler/fact-map-hashing.md) | Compiler proof-state hashing, unrelated to either runtime table. The requested `design/language/fact-map-hashing.md` does not exist; the compiler node is its actual location. |

The current standard-library [interface](../../../lib/std/collections/hash_map/module.wfm),
[implementation](../../../lib/std/collections/hash_map/hash-map.wf) and
[library documentation](../../../lib/std/README.md) define the first table.
It is ordinary Whitefoot over `Box<Slots<HashMapSlot<K,V>>>` with linear
probing. Every bucket is an initialized enum: vacant, deleted (a tombstone),
or filled with one owning key/value pair. The bucket count `B` is zero or a
power of two of at least two; a key's home is the top `log2(B)` bits of its
hash after one odd multiplication, so each home is an interval of the hash
space ([hash-scan study](../hash-scan/DESIGN.md#next-representation-linear-probing-with-high-bit-homes)).
`length` counts live pairs and `vacated` tombstones. Capacity, the pairs a
map holds before an insertion rebuilds it, is `B - floor(B/4)` capped at the
ceiling, which bounds the pairs a map holds.

Removal finds a key, swaps its bucket with Deleted, decreases length,
increases vacated, and returns the complete owned pair. It never makes that
bucket Vacant: a vacant bucket would end a probe for colliding keys farther
along the run. Before this change removal released nothing, so a map that
grew to `B` buckets kept them however few pairs remained. Insertion below
the ceiling rebuilds when live plus deleted buckets reach the capacity, at
the same bucket count when fewer than half the capacity are live, else at
twice the buckets; it never rebuilds a map whose capacity has reached its
ceiling. `hash_map_reserve` only grows and `hash_map_rehash` keeps the bucket
count; the private rebuild refuses a smaller one. Migration moves pairs by
occupancy, never equality, through a one-pair pending window, so every owner
is kept even under inconsistent callbacks.

The separate [C concurrent map](../../../compiler/src/backend/concurrent_map.c)
uses atomic cell words, entry nodes, cooperative moves and retired-table
reclamation. Its [decision node](../../../design/compiler/waiting-contexts/concurrent-map.md),
[waiting-context ancestor](../../../design/compiler/waiting-contexts.md) and
[compiler ancestor](../../../design/compiler.md) govern publication, locks,
stable entry-node addresses during holds, and scanning. None of that
machinery migrates the ordinary HashMap stored inside one entry. Changing
it to fix member sparsity would target the wrong storage. Its sanitizer
workflow is required if its runtime or test changes; this proposal needs
neither change and cannot use that sanitizer result as proof of library
ownership correctness.

### SPOP and other consumers

[SPOP's network entry](https://github.com/Ming-Research/Firn-wf/blob/c6f6058fb17277d4d44d364a45efad1eae01a22b/firn/commands/sets.wf#L352)
parses, takes the set's outer entry and Meta in one atomic statement, calls
the body, records propagation, and releases the hold. Its
[picker and removal loop](https://github.com/Ming-Research/Firn-wf/blob/c6f6058fb17277d4d44d364a45efad1eae01a22b/firn/commands/sets.wf#L272)
read the member table's current bucket count, start at RNG modulo that count,
and scan forward, wrapping, to the first Filled bucket. The picker copies
the key into an owned result. Only then does the loop call `hash_map_remove`,
write the reply, and keep the selected owned key for propagation. Every next
pop reloads the current backing. No bucket reference or saved bucket index
survives removal in this path.

Sparse backing thus extends the entry/Meta hold, delaying competing commands.
Shrinking also executes inside that hold unless the protocol changes. A
counted pop that empties the set currently removes members individually,
so it can pay several shrinking steps before deleting the outer entry.
This must be measured, not excused by better average sparse scans.

SREM and SMOVE also remove through the library using separately owned keys.
SRANDMEMBER and set combinations inspect buckets, but the inspected traversals
do not delete from their scanned source. Hash fields and sorted-set score
indexes also use this HashMap type, so automatic policy is not set-specific.
Different bucket placement can change random selections and unordered reply
order; SPOP propagation records actual selected keys, not a replayed RNG draw.
First-filled selection is biased by preceding empty-run length; shrinking
does not establish uniform sampling. That is a separate Firn concern, not
grounds for a second member table here.

Firn's [SCAN](https://github.com/Ming-Research/Firn-wf/blob/c6f6058fb17277d4d44d364a45efad1eae01a22b/firn/commands/scan.wf#L206)
uses `map_scan` on the outer ConcurrentHashMap. Its cursor contract is not
a member-bucket cursor. Firn's [script design](https://github.com/Ming-Research/Firn-wf/blob/c6f6058fb17277d4d44d364a45efad1eae01a22b/design/firn/scripts.md)
records that there is no collection cursor or layout-dependent result promise
and calls for reopening that choice when SSCAN/HSCAN/ZSCAN is added. The
inspected set and scan files agree; a complete downstream consumer audit is
still an implementation prerequisite.

## Policy choices

Let `n` be the live pairs after a successful removal and `B` the bucket
count. The rule: when `n = 0`, release every bucket (`B = 0`); otherwise,
when `8*n < B`, rebuild into `T`, the least power of two that is at least 8
and at least `4*n`. The test is `n < B/8` with integer division, exact
for the power-of-two counts the map uses and needing no multiplication; `T`
is found by doubling from 8 while `T/4 < n`, again without multiplication.

| Option | Program change | Grow-only cost | Costs and risks |
| --- | --- | --- | --- |
| A. Automatic shrink after a successful removal (chosen) | Same call, result and effects; changed capacity and allocation behavior. No language-rule change. | No added insertion test, field or migration state. | Removal sometimes allocates and migrates; changes the ceiling allocation promise. |
| B. An explicit shrink operation only | A new library function the caller invokes. | None for callers that never call it. | Every caller that omits it stays sparse; Firn must call it inside its hold and choose when. |
| C. Both | A plus B over one migration. | As A. | A second entry point without a demonstrated need. |

**Re-derivation on power-of-two buckets.** The first implementation, on the
previous representation of arbitrary bucket counts, rebuilt into exactly
`max(8, 4*n)` buckets. Power-of-two bucket counts cannot hold that target in
general, so it is rounded up. The ruled numbers keep their meaning:

- The threshold, one eighth, is unchanged. For a power of two `B >= 16`,
  `n < B/8` is exactly `8*n < B`. For `B <= 8` and `n >= 1` it never holds,
  so small maps shrink only to zero.
- The target keeps at least four buckets per pair and the eight-bucket floor.
  Rounding up puts the load right after a shrink in `(1/8, 1/4]`. Since `T`
  is the least power of two of at least `4*n`, `T < 8*n` whenever `4*n > 8`,
  so a map right after a shrink is not sparse and the next removal does not
  shrink again; at the floor, `n <= 2` and `T = 8`, which never shrinks for
  `n >= 1`.
- A shrink always reduces the bucket count: `n < B/8` gives `4*n < B/2`, and
  `B >= 16` gives `8 <= B/2`, so `T <= B/2`.
- The bound the acceptance asks for, at most eight buckets per pair, holds
  between removals: a nonempty map that a removal left unshrunk has
  `B <= 8*n`, and a shrink leaves `T < 8*n` or the floor of eight.

Rounding down instead, to the greatest power of two at most `4*n`, would put
the load in `[1/4, 1/2)`, closer to growth and with less room before the next
shrink; computing `T` from capacity, for example `4*n` pairs of capacity,
would leave about five buckets per pair and change the ruled number. Neither
improves on rounding up, which keeps the ruled ratio as a lower bound.

### Hysteresis and amortized work

After a shrink from a map that crossed the threshold by one removal,
`n = B/8 - 1`, so `T = B/2` and the load is just under 1/4: the next shrink
needs the live count to halve again. Growth starts when live plus deleted
buckets reach three quarters of the buckets, so right after a shrink at
least `T/2` insertions come before growth. Right after a doubling the load
is at least 3/16, so at least `B/16` removals of the doubled map come before
a shrink. For example, 64 buckets holding eight pairs do not shrink;
removing one more leaves seven pairs in 32 buckets, capacity 24, and
reinserting and removing that pair alternates eight and seven pairs in 32
buckets with no rebuild.

With well-distributed hashes and bounded callback cost, a rebuild costs
expected `O(B + n)`. Between automatic resizes a constant fraction of `B`
removals or insertions occurs, so deleting a map built from `N` pairs costs
expected `O(N)` rebuild work in total. One exception: a map reserved far
above its contents is first shrunk by its first removal, which may land just
above a power-of-two boundary and shrink again a few removals later; the
reservation paid for that size. This is not worst-case constant time:
colliding keys make migration probes quadratic, and callbacks cost extra.
The unconditional requirements are a complete owning transfer, finite
capacity-bounded probes, and progress for returning callbacks.

The acceptance condition concerns available buckets, not occupied ones:
occupied buckets already equal `n`. Tombstones reset to zero at every
shrink; between shrinks they may remain, within the same bound.

## One-shot versus incremental migration

One-shot migration preserves the single public `cells` backing and ordinary
lookup/put paths. Allocate the target, transfer every owning pair by occupancy,
publish the completed representation and free the old backing before returning.
The existing helper swaps before draining; that internal ordering is also
possible under its exclusive effect, provided every intermediate owner remains
accounted for. Empty shrink must drain vacant/deleted enums and release the old
box; the rebuild's zero-target early return cannot simply be reused.

Peak backing is old B plus target T buckets, plus constant pending storage;
moving an owning key does not deep-copy its heap allocation. At the first
sparse crossing T is B/2, so the transient increase can still matter
near Firn's memory limit. Allocation failure retains STOR-8's existing
trusted-base exhaustion behavior; there is no new recoverable failure or trap.
The rebuild holds Firn's entry/Meta for O(B+n) expected work, possibly longer
under collisions. It adds no lock to the HashMap itself.

Incremental migration could spread old-bucket scanning over removals, but
needs two persistent backings, migration position/owner state, and a rule for
lookups, insertions, replacements, deletion, each/scan and freeing during a
move. It must preserve probe chains in the old table, avoid duplicates, handle
regrowth during migration and make progress even when deletions stop. Budget
old empty buckets as well as moved entries; a fixed entry budget alone does
not bound sparse-table work. Allocation/initialization and a colliding
destination probe can still be long. Thus incremental does not automatically
give a hard lock-time bound or the immediate `B<=8*n` retained-space bound.

Persistent dual backing also breaks Firn's direct `cells` picker, and the
resumable scan, unless both read the two backings and all direct readers
migrate. It adds representation/branch costs even to a grow-only
instance unless independently eliminated; a separate grow-only specialization
would need its own design justification. No concrete incremental interface is
selected here.

[Redis 7.0.15's dictionary](https://github.com/redis/redis/blob/7.0.15/src/dict.c#L115)
uses chained buckets, prepares a second table and advances migration in
steps, limiting empty-bucket visits per step; whole chains and allocation
remain costs. Its [set removal](https://github.com/redis/redis/blob/7.0.15/src/t_set.c#L85)
requests resizing when sparse. That is useful prior art for latency, not a
drop-in algorithm for owning open-addressed buckets.

Choose one-shot for the first library implementation because it preserves
the one-backing consumer interface and grow path. Reject it if the hold
fails the reopen criterion under [Measurement](#measurement), Firn's `SPOP`
drain p99 against the fastest competitor; then return to the owner with an incremental design, not a hidden Firn rebuild
or a relaxed acceptance condition.

## Safety, proof and cursor obligations

The [active specification](../../../spec/kernel-spec.md) supplies the rules;
library behavior is not a new primitive. These standard-library modules are
checked as ordinary modules, and the change adds no proof step: the
threshold and target use division and doubling with `+wrap` on a value that
stays at most `2^63`, and migration reuses the growth path's operations.

| Rules | Obligation for shrinking |
| --- | --- |
| OP-1, OP-2, OP-4 | Prove arithmetic domains, nonzero divisors and every current subscript bound. Threshold/target arithmetic must not silently wrap. |
| OP-10, OP-11, OP-13, OP-14; PRE-1; STOR-7 | Use ordinary construction, initialized-slot transfer, swap and relocation. Prove room before place, nonempty before take, empty before free. `grow` is monotone capacity growth; shrink allocates another box rather than changing that rule. |
| OWN-1, PROV-6, STOR-3 | Preserve affine and nodrop keys/values exactly once, including the removed pair, each resident pair and every exchange result. Never drop a returned owner to make a smaller target fit. |
| TYPE-2, MOD-5 | Readonly fields prevent clients from mutating the metadata or constructing a forged HashMap. Consuming public destructuring kills the original owner; it does not allow rebuilding it with forged fields. |
| REF-2, REF-3; EFF-5; ENT-5 | A backing replacement invalidates descendant references and supporting facts. References cannot escape callbacks. `writes(map)` already invalidates caller facts conservatively; reacquire current bounds and references after a mutating call. |
| SHARE-2, SHARE-3 | Firn's enclosing atomic hold keeps the set exclusive and takes effect at one point. The library must not release that hold, publish half-migrated state or introduce a wait inside the block. |

The map-reference itself remains valid when its fields change; an element
reference does not survive replacement of its containing backing. The remove
key and hash environment must be disjoint from the written map under EFF-5.
Firn's copied picked key satisfies that requirement. `hash_map_each` borrows
the map for its whole callback traversal; a callback cannot use an overlapping
environment to structurally mutate it. `hash_map_edit` and `hash_map_try_put`
keep their existing extent-preservation postconditions. Automatic shrink is
on remove, not a new side effect of those operations.

The progress argument: `length` equals the number of filled buckets, which
every library operation keeps (insertion, replacement, refusal, tombstone
reuse, removal, reserve, rehash). A shrink targets `T >= 4*n` buckets, or
zero when `n = 0`. While a pair is pending, fewer than `n` pairs fill the
new backing, so a probe bounded by its bucket count finds a vacant bucket,
without assuming hash or equality consistency. This is a reviewed library
argument, not a machine-checked cardinality invariant; the growth path's
argument, at least the old bucket count of destinations, rests on the same
count. At `n = 0` the zero target drains only vacant and deleted buckets.

**Scan coverage.** The resumable scan's cursor is a position in the mixed
hash space, and a call resumes at the bucket whose home interval holds it
([hash-scan study](../hash-scan/DESIGN.md#next-representation-linear-probing-with-high-bit-homes)).
A shrink halves the bucket count one or more times, which merges adjacent
home intervals; the interval holding the cursor then starts at or before
it. Every pair whose hash position is at or after the cursor therefore has
a home at or after the resumed bucket and is lent later in the pass, and a
pair before it was lent by an earlier call over the old interval. Pairs of
the merged interval before the cursor may be lent again, which the scan
allows. A shrink to zero buckets leaves no pair. The maintained program's
scan-shrink test resumes a pass across four shrinks.

No specification rule changes. Policy and the new private helpers belong to
the library interface, implementation, documentation and the HashMap design
node.

## Implementation

`hash_map_remove`, after a successful removal, calls a private
`hash_map_shrink`, which applies the rule above through `hash_map_migrate`,
the growth path's migration split out of `hash_map_rebuild`. The rebuild
keeps its refusal of a smaller bucket count and its zero-bucket no-op for
reserve, rehash and insertion; the shrink calls the migration directly.
A missing-key removal returns before any of it. No public signature or
representation changes.

## Tests

The maintained [HashMap program](../../../tests/programs/containers/hash-map-program.wf)
checks, through public operations and the vacated count:

| Test | Observation | Fails on the base library (no shrink) because |
| --- | --- | --- |
| growth | 16 buckets with two pairs, exactly at the boundary, still rebuild at the same count; a map at its ceiling of 48 shrinks to 32 buckets at seven pairs and grows back, refusing a 49th pair | the ceiling map keeps 64 buckets |
| shrink million | 1,000,000 pairs removed to 1,000 leave 4,096 buckets, capacity 3,072, within 8,000 | 2^21 buckets remain |
| shrink oscillation | 128 insert/remove cycles between eight and seven pairs change capacity once | capacity never changes |
| shrink empty reuse | removing the last pair releases all buckets; an absent removal changes nothing; the next insertion grows from zero | the buckets remain |
| shrink regrow | 64 buckets shrink to eight at one pair and grow back to 64 with 48 pairs, every pair found | the buckets remain |
| owned shrink | nine colliding must-consume pairs migrate through 32, 16 and eight buckets, every owner released once | capacity stays 48 |
| scan shrink | a pass resumed across four shrinks lends every kept pair, five of them after the cursor | capacity stays 48; a scan reading its cursor as a bucket index also fails it |

The native allocation ledger counts every backing, so a shrink that left a
backing unreleased fails the container test; its expected counts derive from
the map's bucket sequence in each case, written beside them.

## Measurement

The comparison uses [hash-map-compare](../../experiments/hash-map-compare/README.md)
on the 14900K, dispatched on this branch: base is the merge base's library,
candidate this branch's, twin a byte copy of the base. Its grow-only cells
(presized and growing insertion, lookup hit and miss, iteration) carry the
no-slowdown criterion: a candidate interval wholly on the slower side is a
regression of any size. Its removal cells change by design (removal to empty
now shrinks) and are reported, not judged against that criterion. The
removal tail is the instrument's `tail.wf`: it drains 10,000 and 1,000,000
pairs, timing every removal alone, so each drain crosses the threshold at
every halving down to empty. A drain migrates once per halving, a dozen to
twenty of its removals, far fewer than one in a hundred, so its 99th
percentile measures the ordinary removal path and cannot see a migration;
it carries the paired criterion for that path only: a candidate interval
wholly on the slower side, against a twin that contains 1, is a regression.
The maximum and each removal that changed the capacity, with its
nanoseconds, are reported as numbers, not judged: they are expected to be
far above the base's, since the base never migrates on removal.

Whether a one-shot migration holds a request too long is judged where a
request sees it: Firn's `SPOP` measurement (board item firn-bl-01-19) fills a
1,000,000-member set and pops it empty at pipeline depth 1 and 16, and a 99th
percentile repeatably above the fastest competitor's, with the time spent
migrating, reopens one-shot migration. The originally ruled criterion, a
library p99 at the threshold crossing or over a full drain, cannot decide:
over a drain the migrations fall outside the 99th percentile, and the
crossing removal alone is always slower than the base's, which never
migrates.

The ordinary removal path is measured slower, and the owner accepted that
slowdown for this change. On the 14900K sample run (ten rounds, no floor
given, so no verdict), remove-insert at a constant size, which never
shrinks, was 1.6% to 8.5% slower than the base from 1,000 to 1,000,000 pairs,
and the 10,000-pair drain p99 3.9% slower. The cause, read from optimized
IR on a GitHub-hosted runner: the base's `hash_map_remove` has 75
instructions and LLVM inlines it into the caller's loop; the shrink check
and its call push it past LLVM's inline threshold, so every removal in the
loop becomes a real call. A refuting check holds the attribution: with
`-inline-threshold=2000` the candidate's remove-insert ratio is 0.998 at
1,000 pairs and 0.995 at 10,000 (40 samples per arm, base 1.000, candidate
1.040 and 1.045). Annotations on the source do not recover it: a 1:2000
branch weight on the shrink branch, as an unlikely marker would lower,
keeps the shrink out of line but leaves the removal at 81 instructions,
still above the threshold (1.037, 1.044), because LLVM's inline cost counts
instructions on rarely taken branches too; an inline hint on the removal
moves the boundary one call out (1.020, 1.026); Whitefoot#420's automatic
branch weights stop `hash_map_find` from being inlined into the removal
(1.050, 1.056). The gap is how the compiler decides inlining, not this
library, so the slowdown is recorded rather than worked around here; its
real effect is judged by Firn's `SREM` and `SPOP` measurements, and the
remove-insert cell is the witness for the compiler's inlining work.

## Owner decisions

- Automatic shrinking after a successful removal, below one eighth, into
  four buckets per pair with a minimum of eight, empty maps to zero (A).
- Removal may shrink a map reserved to its ceiling; insertion at the
  ceiling still does not rebuild (A).
- One-shot migration first (A). The reopen measurement was ruled as a
  threshold-crossing or full-drain p99; a replacing card
  (hash-shrink-reopen-measure, A) moved it to Firn's `SPOP` drain p99
  against the fastest competitor, for the reason under
  [Measurement](#measurement).
- The 14900K measurement runs this comparison (A).
- The ordinary removal's slowdown from crossing LLVM's inline threshold is
  accepted for this change and recorded; inlining is studied as compiler
  work (hash-shrink-cold-path, A).

## Found along the way

- First-filled random selection in Firn's `SPOP` is biased by the preceding
  empty-run length; shrinking does not make it uniform. That is a Firn
  concern, not grounds for a second member table here.
- The historical map-library harness uses an old library path; it remains
  evidence, not runnable guidance.
