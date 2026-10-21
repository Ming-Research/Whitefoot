# Resumable scans of the ordinary HashMap

Status: design study, 2026-10-13; the original source review was at
`2d0b6adf6762fb283773584a2f94d07d6aab201d`, and this extension reads
`176740a6286ee44eab11ecf4f7d28ec209fa26d9`. No scan implementation, checker run,
test, benchmark or CI polling was performed. This is a sibling of the
shrinking investigation on the branch of Ming-Research/Whitefoot#436:
scanning requires a separate traversal representation decision, rather than
another shrink threshold. No specification is changed by this record. The
[outcome](#outcome-rulings-implementation-and-criteria) records the owner's
rulings, the implementation and the criteria set before measurement; the
sections after it are the study as written before those rulings and describe
the linear-probing map of the source revisions named above.

The board item is “the standard library hash map can resume from where it
stopped, visiting a bounded number of entries each time”. Firn needs SSCAN,
HSCAN and ZSCAN now. Its witness is one million members, visited over separate
atomic holds, while insertions, removals, growth, same-capacity rehash and
shrinking happen between calls. The original study interpreted 64 as a hard
pair cap. The clarified requirement is to bound work per hold, with Redis
7.2.16's scan tests and deterministic replies as Firn's compatibility oracle.
Whether the work bound must hold for arbitrary collision patterns, or permits
a complete home-chain overrun with COUNT as a hint, is an owner decision below.
Every member present throughout a completed pass must be reported at least
once. New or removed members have no coverage promise. The design must also
state the amount of internal work, rather than equating 64 replies with a
short hold.

The [constitution](../../../docs/constitution.md) makes safety a constraint,
requires performance useful to real projects, and weighs actual compatibility
needs. The representation alternatives below are proposals on those grounds,
not choices selected by implementation convenience or measured winners.

## Outcome: rulings, implementation and criteria

The owner ruled the first three questions below on the status board, in
Chinese, on 2026-10-14 and 2026-10-19:

- COUNT is a hint (first question, option A). A call visits at most `limit`
  consecutive home buckets and lends each bucket's whole chain, so the last
  chain may exceed the limit, as Redis 7.2.16's COUNT does. A hold criterion
  is written down before measuring; if the measurements fail it, the design
  returns to a strict cap through the stable-identifier directory.
- Home chains first (second question, option A). The stable-identifier
  directory stays the fallback if any measured cell regresses.
- Capacity counts slots for pairs (third question, as drafted on
  Ming-Research/Whitefoot#487). The bucket count is the least power of two
  that is at least the capacity and at least two. A pair stays in its slot
  while it stays in the map. An insertion into a map whose slots are all
  taken doubles the capacity, from none to one, up to the ceiling. The
  three-quarter mark and the vacated count of linear probing are gone. The
  shrink rule approved for linear probing is re-derived on this structure by
  the shrink work, not here.

The implementation on Ming-Research/Whitefoot#487 differs from Valkey 8 in
two ways the third ruling accepted:

- No seven-pair cache-line blocks: the pairs live in one slot array, a
  parallel array of link words chains the pairs of one bucket and threads the
  free slots, and a power-of-two array holds each bucket's first link. Growth
  appends slots and links and rebuilds only the head array; no owner moves.
- The home bucket is the top bits of the hash after one multiplication by an
  odd constant, and the cursor is a position in that mixed hash space,
  instead of the low bits with a reverse-binary cursor. Both split every
  bucket into two contiguous children when the count doubles, which is what
  the coverage argument needs.

Coverage argument. With 2^k buckets, bucket b holds exactly the pairs whose
mixed hash lies in [b * 2^(64-k), (b+1) * 2^(64-k)), whatever was inserted or
removed before. A call starts at the bucket containing the cursor, which
begins at or before it, visits whole buckets, and returns the start of the
first bucket it did not visit, or zero after the last. By induction every
mixed hash below the returned cursor lay in a bucket some call of the pass
visited whole. When the bucket count changes between calls, the cursor is
reinterpreted under the new count: growth splits intervals, so the call
resumes exactly at the cursor; shrinking merges them, so it resumes at or
before the cursor and may lend some pairs again. A pair present from the
start to the end of a pass therefore lies in a visited bucket at the moment
its interval is reached, and is lent at least once. Pairs inserted or
removed during the pass carry no promise.

### Hold criterion, set before measurement

Measured on the 14900K, pinned to one P core, `u64` keys and values,
uniformly distributed hashes, a callback that only counts:

- H1, the hold does not grow with the map: the p99 duration of one
  `hash_map_scan` call with limit 64 over 1,000,000 pairs is at most four
  times its p99 over 1,000 pairs. Both calls read 64 heads and about the
  same number of pairs; the factor allows the cache misses a million-pair
  map takes on its slots and links, which a thousand-pair map keeps in the
  first two cache levels. A larger factor means the work per call depends on
  the size of the map, which this item exists to remove.
- H2, the hold is short in absolute terms: that p99 over 1,000,000 pairs is
  at most 20 microseconds. A call touches about 64 pairs, each costing up to
  two dependent memory misses (slot and link) of roughly 80 nanoseconds on
  this machine, about 10 microseconds; the bound allows twice that. The
  status quo it replaces is one `hash_map_each` over the whole map per hold.
- H3, a collision overrun costs what its chain costs: a call whose first
  bucket holds m pairs of identical hash, for m of 64, 1,024 and 65,536,
  takes at most 2 * m times the median per-pair time of the uniform
  limit-64 call. The hint semantics accepts the overrun; H3 checks that it
  is linear in the chain, with no extra cost per collision.

Failing H1 or H2 sends the design back to the strict cap (first question,
option B). Failing H3 is a defect of the chain walk to fix before
measuring again. Firn's own hold measurements under its workloads belong to
Firn's board items.

### Comparison resolution and predictions, set before measurement

The before-and-after comparison follows the engineering standards' paired
rule: base, candidate and a twin of the base interleaved, each cell judged
from the 95% interval of the candidate-to-base ratio once the twin's
interval contains one and its half-width is below the cell's resolution.
The resolution is the smallest difference each cell must be able to
resolve; it is not an acceptance threshold, and a detected regression of any
size still counts. Timing cells: 3% at 1,000 pairs, 2% at 10,000 and
1,000,000. Peak memory: 1%.

Predictions, exploratory and not acceptance:

- Growth-only insertion: no slower, likely faster at 1,000,000, because
  growth no longer rehashes owners into a second backing and no probe walk
  lengthens near three quarters full.
- Lookup hit and miss: possibly slower at 1,000,000, because a lookup reads
  a head word and then a slot, a dependent miss that linear probing's
  contiguous run avoids.
- Removal and alternating insert and remove: no slower; removal unlinks one
  slot and no vacated marks accumulate.
- Full iteration: no slower; both walk one dense slot array.
- Peak memory: likely higher for small pairs. For `u64` pairs a slot takes
  24 bytes; linear probing holds between 4/3 and 8/3 slots per pair, the
  chains between one and two slots per pair plus one link word per slot and
  one head word per bucket. If the comparison detects this, accepting it
  needs an owner card stating its size.

### First comparison and the link word in the slot

Measured on the 14900K on 2026-10-21 with `hash-map-bench.yml`, candidate
Ming-Research/Whitefoot@11ea902e4c6e, base and instrument
Ming-Research/Whitefoot@48b990f05ba3, clang 22.1.8, timed processes pinned
to CPU 2. The floor came from run 39560615391 and the judged run is
39561373318; no cell's spread exceeded its floor. Candidate-to-base time
ratios with their 95% intervals:

| cell | 1,000 | 10,000 | 1,000,000 |
| --- | --- | --- | --- |
| insert, presized | 0.67 [0.66, 0.69] | 0.43 [0.43, 0.44] | 0.46 [0.45, 0.46] |
| insert, growing | unresolved | 0.98 [0.97, 1.00] | 0.71 [0.71, 0.72] |
| lookup hit | 0.79 [0.78, 0.80] | 0.64 [0.63, 0.65] | 1.30 [1.29, 1.30] |
| lookup miss | 0.66 [0.65, 0.66] | 0.43 [0.42, 0.43] | 0.81 [0.80, 0.81] |
| remove | 1.19 [1.14, 1.21] | 0.73 [0.72, 0.74] | 1.20 [1.20, 1.22] |
| remove then insert | 0.82 [0.82, 0.83] | 0.69 [0.68, 0.69] | 1.00 [1.00, 1.00] |
| iterate | 0.23 [0.23, 0.24] | 0.14 [0.14, 0.15] | 0.10 [0.10, 0.10] |

Retained bytes after building from capacity zero, candidate against base:
41,008 against 49,168 at 1,000 pairs, 655,408 against 393,232 at 10,000 and
41,943,088 against 50,331,664 at 1,000,000; peaks 45,120 against 73,760,
720,960 against 589,856 and 46,137,408 against 75,497,504. The byte counts
follow from the layouts: linear probing holds 24-byte slots and doubles at
three quarters full, so 2,048, 16,384 and 2,097,152 slots; the chains held
1,024, 16,384 and 1,048,576 slots of 24 bytes plus one link word and one
head word each, plus one 16-byte header per backing. At 10,000 pairs both
hold 16,384 slots, so the two words per slot show as a 1.67 ratio; at the
other sizes the chains need half the slots.

Hold: H1 failed (p99 1,185 ns at 1,000,000 pairs against 94 ns at 1,000,
four times being 376 ns), H2 passed (at most 20 microseconds) and the three
H3 chains passed.

Diagnosis, inferred from the layout and not measured apart: the chain heads,
the link words and the pairs lay in three separate arrays, so at a million
pairs, beyond the 36 MB last-level cache, a hit read a head word, a slot
and, past the first pair of its chain, a link word on a third cache line,
where linear probing reads one run of slots. A removal read the same and
wrote a link word. H1 compared a map that fits the cache with one that does
not, so its ratio measured the cache, not per-call work.

The next round, recommended on the board's design card for this outcome and
followed before its ruling, keeps every ruled rule:

- Each slot becomes a node holding the link word beside the pair, so a hit
  reads a head word and the nodes of its chain, and a removal rewrites words
  in nodes it has already read. No backing for links remains. Storing each
  pair's hash in its node as well was considered and not taken: for `u64`
  pairs it adds a word to every slot, which would make the million-pair map
  16 bytes larger than linear probing's and the 10,000-pair map twice as
  large, while equality of a `u64` key is one comparison.
- The stable-identifier directory is not taken: it keeps the linear-probing
  table and adds a directory, so it is larger than the old map at every
  size and adds a directory write to every insertion and removal.
- H1 is replaced, set here before the remeasurement: over uniform hashes,
  `u64` pairs and limit 64, the p99 of one call over 8,000,000 pairs is at
  most twice its p99 over 1,000,000. Both maps exceed the 14900K's 36 MB
  last-level cache (about 42 MB and 335 MB), so nearly every pair is a
  memory read in both; the factor allows the extra translation misses of the
  larger map. A larger ratio means per-call work grows with the map. H2 and
  H3 stay as written, judged at 1,000,000 pairs.

Predictions for the remeasurement, exploratory and not acceptance: lookup
hit and removal at 1,000,000 pairs faster than in the first comparison but
possibly still slower than linear probing, which reads one cache line where
a chain reads two; retained bytes 16 fewer at every size, the ratios
otherwise unchanged, so the 10,000-pair memory cell still regresses and goes
to the owner with its size; iteration slower than in the first comparison,
32-byte nodes against 24-byte slots, and still faster than linear probing.

## What the two maps already provide

The ordinary [HashMap interface](../../../lib/std/collections/hash_map/module.wfm)
offers callback-based lookup/edit and whole-map `hash_map_each`, but no scan.
Its [implementation](../../../lib/std/collections/hash_map/hash-map.wf) stores
owning pairs in Vacant/Deleted/Filled enum buckets over `Box<Slots<...>>`.
Homes are `hash % capacity`, followed by circular linear probing. Deleted
buckets do not terminate a probe. Migration takes old buckets from the end
and inserts each owning pair into a fresh backing by occupancy.

Its capacities are **not powers of two**. Construction and reserve accept
arbitrary capacities within the ceiling, growth doubles or saturates at an
arbitrary ceiling, and shrink uses `max(8,4*live)`. For example, 32 buckets
can shrink to 12 after a removal leaves three live pairs. The current
[storage node](../../../design/language/data-model/hash-map-storage.md),
[data-model ancestor](../../../design/language/data-model.md) and
[language ancestor](../../../design/language.md) record the enum-owning
representation and no stable bucket/address promise.

The existing concurrent scan is a **prelude function**, not a function in
`lib/std/collections/hash_map` or a separate standard-library collection
module. Its source declaration is embedded in
[prelude.rs](../../../compiler/src/prelude.rs), and PRE-1/SHARE-1 in the
[active specification](../../../spec/kernel-spec.md) define:

```wf
fn map_scan<V: drop>(map: &ConcurrentHashMap<V>, cursor: u64,
  count: u64, keys: &KeySet) -> next: u64 reads(map), writes(keys);
```

This excerpt omits the declaration's KeySet extent postconditions; those
remain authoritative in PRE-1. Each byte key has an execution-fixed `u64`
position. A call selects a mathematical endpoint `cursor < e <= 2^64`,
reports **all** present keys with positions in `[cursor,e)`, in position then
byte order, and returns `e mod 2^64`. Zero starts a pass and a returned zero
completes it. **Count is only a hint**, with no pair-count or work bound.
Several byte keys can have the same position; a call includes the whole group.
`map_scan_within` limits peak heap growth, also keeps whole position groups,
and can return Needs without advancing. It is not a strict entry-limit API.

The [runtime scan](../../../compiler/src/backend/concurrent_map.c) maps high
position bits to homes, selects a home interval, then reads through its probe
runs to an empty bucket beyond the interval or through one complete table
revolution. It filters by the stable position, sorts, and collects keys.
Tombstones never terminate that coverage walk. The preliminary cell budget
therefore does not truncate a displaced key. A whole-map hold and completed
moves make one current backing sufficient. See the
[enumeration investigation](../map-enumeration/DESIGN.md#the-runtime-step-under-a-whole-hold)
and [runtime decision](../../../design/compiler/waiting-contexts/concurrent-map.md).
Neither its count nor its preliminary budget establishes 64 returned keys or
64 examined buckets, and worst-case work can reach the whole table.

Use the same scan name and zero start/completion convention where meanings
match. The stable-ID option's **limit** would be a hard callback cap and its cursor would
be map-local; it must not be advertised as the concurrent function's count
hint, execution-wide hash position, lexicographic ordering or KeySet collector.
No concurrent-runtime change is needed for the ordinary library proposal.

## Why a physical bucket cursor is insufficient for today's linear probing

These are deductions from the current operations, not executed tests.

1. **Same-capacity rehash alone loses a member.** At capacity two, A and B
   both hash to zero. They occupy buckets zero and one respectively. Scan
   bucket zero and save physical cursor one. Rehash drains B first, then A,
   producing B at zero and A at one. The remaining step reports A again and
   finishes; B was present throughout and never reported.
2. **Shrink loses an unvisited member.** At capacity 32, distinct hashes
   0,1,2,28 occupy their homes. Scan `[0,8)` and save eight. Remove hash zero;
   three pairs trigger shrinking to 12. The surviving hash-28 pair moves
   from bucket 28 to bucket four. Continuing from eight misses it.
3. **Power-of-two physical buckets still fail under probing.** At capacity
   four, A and B both hash to one and occupy one and two. Reverse-binary
   traversal visits buckets `0,2,1,3`. After the first two buckets reported
   B, same-capacity rehash puts B at one and A at two. The remainder reports
   B again and misses A. Rounding capacities does not fix displacement.
4. **A non-power-of-two mask does not visit every bucket.** At capacity 12,
   substituting mask 11 into reverse-binary bucket arithmetic visits only
   `0,8,2,10,1,9,3,11`. Buckets four through seven are omitted. Modulo homes
   under arbitrary capacities do not form the required nested bit partitions.

[Redis's dictionary scan](https://github.com/redis/redis/blob/7.0.15/src/dict.c#L769)
uses powers-of-two capacities and chaining. Its reverse-binary progression
completes home-bucket chains and their expansions across rehashing, allowing
duplicates. Entries displaced by open addressing require a coverage walk
for their **home**, not merely a visit to one physical cell. That walk can
be a full probe cluster, including wraparound and tombstones.

Clamping the physical cursor to a new capacity, checking each subscript, or
restarting after a resize does not prove coverage of continuously present
pairs. Restarting can also prevent completion under repeated rehashing.

## Hash-position cursors: correct coverage, different bounds

A stable hash-position interval is independent of capacity. It can cover
the current ordinary layout by checking every Filled bucket's stable hash
and selecting all pairs in `[cursor,e)`. No sorting is needed if the ordinary
callback promises no order. A key present throughout the pass with a stable
hash falls in exactly one completed interval, so movement cannot hide it.

However, one call can examine all C buckets even when it reports one pair;
doing this for a million members in batches of 64 is not a short-hold
solution. A high-bit home layout could narrow expected work, but must still
cover displaced entries through complete probe runs. Full clusters and
tombstones can make that walk O(C); arbitrary hash callbacks add their own
unbounded cost. No property of the current hash/equality interface promises
a well-distributed hash.

There is also an unavoidable collision boundary: **65 distinct keys with one
hash position**. Advancing beyond that position after reporting 64 skips a
remaining continuously present key. Keeping the same position gives a scalar
hash-boundary cursor no way to identify the unreported suffix. An ordinal
offset is insufficient: if the already reported prefix is deleted or the
group is reordered, an unreported key moves below that offset.

Thus complete hash-position intervals, mandatory progress and a strict 64
cap cannot all be supplied by that cursor. Redis and the existing concurrent
scan choose complete groups and a hint. This excludes the cursor only under
the hard-cap option; it does not settle the clarified Firn requirement.

## Redis 7.2.16 compatibility and the meaning of bounded work

The fixed-tag [scan tests](https://github.com/redis/redis/blob/7.2.16/tests/unit/scan.tcl)
collect a full pass and deduplicate SSCAN members, HSCAN fields and ZSCAN
members, checking coverage and field/value or member/score pairing. The COUNT
test checks final coverage, not batch size. The deletion regression removes
most members mid-SSCAN, then requires all 100 protected survivors. These are
semantic checks, not a hold-work bound or an exact unordered reply/cursor trace.

The [command implementation](https://github.com/redis/redis/blob/7.2.16/src/db.c#L781)
accepts positive COUNT, default ten. Its literal hashtable loop permits up to
10*COUNT+1 dictionary steps because of the post-decrement test, stopping sooner
after enough sampled pairs; MATCH is applied after sampling. Compact
sets, hashes and sorted sets return all matching entries with cursor zero in
one call, independent of COUNT. The [dictionary step](https://github.com/redis/redis/blob/7.2.16/src/dict.c#L1219)
completes home chains; during migration it also completes their expansions.
This is hint semantics, not a strict pair or slot cap.

Firn should compare coverage and allowed duplicates without requiring random
hashtable ordering or identical cursor numbers. Where replies are deterministic,
including compact full results, empty/missing results, option errors and score
serialization, it must retain byte-for-byte oracle checks. An alternative
cursor alone does not establish those command behaviors. The test suite does
not prove every mutation interleaving; retain the million-member witness too.

For a one-table home-chain scanner, let q be a positive pair hint and b a
positive home-step budget. Stop after a complete chain when sampled pairs
reach q, after b roots, or at cursor zero. If the last chain contains L pairs,
the callback total is at most q-1+L; HSCAN/ZSCAN produce two reply items per
pair. Empty roots count against b, and MATCH must not reset the work budget.
The work includes b roots, every child block and occupied pair in those chains,
plus callbacks. It is bounded by the selected chains' sizes, **not by q or b
alone**. All million keys may hash to one home; even b=1 then visits a million
pairs. A keyed hash can mitigate adversarial input in a caller, but HashMapKey
does not promise a maximum chain size and safety must not assume dispersion.

The proposed library callback signature can retain the same K/V/env effects
and u64 start/completion cursor as the strict option below, but replace limit
with count and add home_budget, requiring both positive. It counts sampled
pairs before a caller's filter. This makes the independent work budget explicit;
the command layer maps Redis COUNT to that policy. A budgeted empty-root call
may return no pairs and a nonzero cursor. The concurrent prelude scan keeps
its own position-group semantics; neither library candidate changes it.

Therefore hint semantics satisfies Redis's coverage/count semantics and the
listed coverage/count assertions, subject to exact deterministic command behavior.
Encoding-report assertions, MATCH and score behavior are additional downstream
full-suite obligations, not consequences of a cursor algorithm. It
can meet Firn's practical short-hold goal only if the owner permits collision
overruns and measurements meet an explicitly selected hold-latency criterion.
It cannot meet a distribution-independent constant bound on pair/node work.
The revised recommendation is to investigate home chains with a hint first
for Redis compatibility, explicitly asking for this qualification; retain
stable IDs if a uniform worst-case work cap is required. No performance
acceptance is claimed, and no already-approved shrink policy is changed here.

## Alternative: home buckets with local slots and owned overflow chains

The fixed reference is [Valkey 8.1.0's hashtable.c](https://github.com/valkey-io/valkey/blob/8.1.0/src/hashtable.c).
Its 64-bit layout uses 64-byte blocks containing seven entry pointers,
presence bits and short hash fingerprints. A chained block uses its last
pointer slot for the child, leaving six entry slots there and seven in the
tail. Insert and lookup follow only `hash & (B-1)` and that root's children;
there is no displacement into another home. Root count B is a power of two.
The source implements incremental two-table rehash and reverse-binary scanning.
These are source facts, not Whitefoot layout or performance guarantees.

### Coverage and migration

For a proposed ordinary-map version, every key belongs to exactly one root
chain at its current hash mask. The scalar cursor visits **the whole chain**,
including all child blocks; a cache-line block is not the coverage unit.
Slot exchange, deletion compaction and same-size rehash can reorder a chain
without hiding an entry behind a saved intra-chain offset, because none is
saved. Stable hashing/equality remains the existing logical-key requirement.
Valkey's resize helper declines a same-exponent resize; the proposed complete-
chain traversal would also tolerate an explicit same-size owning rebuild.

Growth refines each low-bit hash partition into nested subpartitions. Reversing
bits makes those expansions consecutive in traversal order. Shrink merges
partitions; rescanning a merged chain may duplicate visited members but includes
its still-unvisited members. Thus a continuously present entry is covered under
arbitrary completed power-of-two rebuilds between calls. Rehash must preserve
the hash protocol; changing its seed would change the partitions and invalidate
this argument. Zero begins/ends a pass; cursor is a traversal position, not a
stable entry address. A finite ceiling bounds available partition refinements.

With one-shot migration there is one current root array per scan hold, so
one elementary step completes one root chain. An incremental version must
scan one smaller-table root plus all corresponding larger-table expansions,
pausing moves within the hold. That can be many physical roots in one step,
and the overrun is one complete **partition group**, not one bucket's members.
Splitting that group with a scalar cursor requires another coverage proof.
Start the comparison with one-shot migration, consistent with the approved
shrink direction; incremental migration is a separate state/latency decision.

### Capacity, ceiling and the approved shrink rule

Today's capacity C counts individual enum positions, accepts arbitrary reserve
requests within an arbitrary ceiling, and shrinks exactly to max(8,4*n).
Do not equate C with either a Valkey root count B or seven pointer slots per
block: overflow also allocates positions. Adopting roots with a power-of-two
count requires an owner-approved capacity model, not just a new mask.

If public C becomes the power-of-two root count, the existing exact rule
cannot hold: n=3 requires C=12, which is not a power of two. Similarly reserve
13 under ceiling 13 cannot both materialize at least 13 roots and stay under
that ceiling. Growth cannot saturate at an arbitrary ceiling such as 13. The
largest positive u64 power of two is 2^63, another endpoint to define.
Choices include power-of-two ceilings and rounded reserve, rejecting requests
above the largest permitted power of two, or redefining ceiling/capacity as
logical member quotas independent of physical roots. Each changes an existing
interface promise; a quota cannot be described as materialized bucket count.
Overflow insertion also allocates at an unchanged root count; define separately
whether try-put permits that, and enforce any logical member ceiling.

For a root-count interpretation, a candidate is to retain the strict 1/8
trigger and empty-to-zero, but use max(8,floor_power_of_two(4*n)). For n>=3,
2*n<C'<=4*n, so occupancy after shrink is at least 1/4 and below 1/2;
the small-n floor is handled separately. This fits all retained members but
replaces the **approved exact target**, and root occupancy is no longer the
old slot occupancy. Upward rounding gives C'<8*n, yet n=17 can produce 128
roots and just two deletions trigger another shrink. The old per-rebuild
hysteresis argument cannot be reused; any aggregate geometric-cost argument
must be written separately. Neither rounding is adopted by this study.

A separate logical quota could retain exact max(8,4*n) as a quota while choosing
power-of-two physical roots, but that does not preserve the current physical
capacity contract or its memory acceptance test. Count root storage, overflow
storage and entry owners in sparse-release acceptance. Retain today's approved
capacity semantics with stable IDs if these changes are unacceptable.

### Work, memory and costs compared on the same terms

| Concern | Home-chain blocks | Sparse stable-ID directory | Today's linear probing |
| --- | --- | --- | --- |
| Scan work | b complete roots plus all their children/pairs; hint can overshoot by final chain. Worst case O(n) in one call. | Hard pair cap with bounded successor/payload lookup; proposed 16,640 node-inspection target still unproved. | Movement loses physical-cursor coverage; stable-hash filtering can inspect all C positions. |
| Owning memory | One owner per pair; root and child metadata. Inline generic K/V make block width type-dependent; pointer-sized slots require boxed owners and indirection. Empty child blocks must be reclaimed. | One owner per pair, hash index plus sparse prefix nodes; uncompressed baseline up to 64*n prefixes. | Contiguous tagged inline owners and tombstones, no per-entry Box required. |
| Grow-only | Adds chain allocation/fingerprints and possibly entry Boxes; root rebuild can reuse boxed addresses. No second ordered directory. | Adds ID allocation, directory updates and lookup indirection as well as the hash index. | Contiguous allocation/reinsert path is the comparison control. |
| Lookup | Fingerprints can skip equality checks; local slots improve locality, children require pointer traversal. Collision chains remain linear. | Hash lookup followed by ID-to-owner directory access. | Probe clusters and tombstones add inspections; inline owners can avoid pointer loads. |
| Churn | Clears actual slots; no tombstone needed across independent homes. Hole filling and child pruning add owning transfers. | Maintains both hash and ID metadata and prunes directory storage. | Deletion is cheap until shrink/cleanup; tombstones lengthen later probes. |

With fixed slot width and compact child chains, root/child metadata is O(B+n);
boxed payload storage is another n owners plus allocation overhead. Peak
one-shot rehash includes both root arrays and all retained owners/child blocks
until transferred; it is not just the new root count. Sparse pruning must
prevent empty children retaining a historical high-water chain.

Neither structure is asserted faster or smaller. Measure the same grow-only,
lookup, replacement, collision-heavy churn, shrink/rehash, full-pass cost and
hold tails across small/scalar/wide/nodrop owners. Use identical source with
interleaved base/head and a twin; measure allocations, equality/hash calls,
root/child/node inspections and retained/peak bytes separately. Keep the
existing grow-only nonregression criterion. A 64-byte C pointer bucket is not
evidence that Whitefoot's generic owning enum bucket occupies a cache line.

### Natural proof obligations and consumers

Use ordinary owning enums for occupied/empty slots and an owned optional child
Box. Pruning requires structurally empty children, not discarding nodrop pairs
from a numeric presence summary. Prove every take/place/exchange result is
consumed once, slot indices and root subscripts are bounded, and a migration
has exactly one owning location for each pair. One-shot migration keeps those
obligations local; incremental migration adds two-table lookup/removal and
partition-frontier correspondence. No writer-accessible unsafe C idiom,
untyped pointer slot or trusted presence bit is an allowed port.

Natural source witnesses must prove `hash & mask < B` from B=2^k and mask=B-1,
bounded bit reversal and shift counts below 64, and cursor progression with
intentional modular arithmetic. Power-of-two rounding must discharge exact
arithmetic/overflow and ceiling bounds, including zero and maximum u64. Check
these against OP-2/OP-4, INV-1/PRF-1 and TYPE-11; do not assume the checker
automatically derives bit-mask range or recursive cardinality theorems.
REF-2/REF-3, TYPE-8 and EFF-5 still prevent callback references escaping or an
overlapping env mutating the scanned owners. No new source witness was run;
checker acceptance or a language gap remains unknown.

Both candidates change HashMap's public readonly `cells` representation.
Firn's random picker must be migrated to a library operation that includes
all children/owners and defines its sampling behavior. Uniformly choosing a
root then its first pair would change bias and is not a substitute for that
audit. Capacity readers, memory accounting, owning iteration/free, persistence
and logical rollback need review too. Home-chain cursors need retained hash
partitions rather than preserved IDs; script restore may reorder chains at the
same mask safely but must preserve the hash protocol and logical map lifetime.
Representation/interface design approval is required for either candidate;
an ordinary library implementation alone does not change the language spec.

## Stable logical scan positions: option for a strict work cap

If a distribution-independent work cap is selected, investigate an integrated stable-ID iteration directory for the ordinary
library map. A live logical entry has one scan ID `s` in `[0,ceiling)` that
stays fixed until removal. Deletion frees the ID; a later insertion may reuse
it. Replacement of an equivalent key and editing its value preserve the ID.
Hash-index rebuilds do not change IDs. IDs are positions, not addresses or
escaping references, and are not advertised as application handles.

The map would own **one copy of every pair** in a sparse ordered directory;
its hash lookup index contains numeric IDs referring to those owners. This
is an integrated library representation change, not a second table of pairs
kept by Firn. A sparse binary radix directory over fixed-width IDs gives a
concrete baseline: each present subtree is nonempty, leaf payloads own pairs,
and empty subtrees are removed. Its depth H is at most 64. An ordered sparse
page directory is an alternative layout of this same idea, but needs a
separate page-occupancy and reclamation argument before choosing it.

The full-width radix baseline can require up to `64*n` occupied prefix
nodes, besides the hash buckets and pair payloads; common dense prefixes
share nodes. This is a correctness candidate, not an acceptable measured
layout yet. Compression/pages could reduce allocation and cache costs; their
stored bit positions, summaries and owner-pruning obligations must be checked.
There is no claim that those optimizations or zero grow-only regression are
already achieved.

### Cursor and exact operation

Under the strict-cap option, refine the proposed interface without adding a hash/equality parameter:

```wf
public fn hash_map_scan<K, V, E,
  fn visit(env: &E, key: &K, value: &V) -> result: unit
    reads(key), reads(value), writes(env), const ceiling: u64>(
  map: &HashMap<K, V, ceiling>, cursor: u64, limit: u64, env: &E
) -> next: u64 reads(map), writes(env) contract {
  requires limit > 0_u64;
}
```

This is a proposed declaration, not an installed module interface. It reuses
the existing each callback shape. The directory, rather than rehashing keys,
finds entries, so `HashMapKey` and a hash environment are unnecessary on scan.

Cursor means the lower bound of the remaining **map-local logical ID** range.
Zero starts a pass; a returned zero ends it. Starting at c, repeat at most
limit times: find the least current live ID s>=c, lend that pair to visit,
then set c=s+1. If no such ID exists, finish with zero. At the fixed domain
endpoint `c==ceiling`, finish with zero. Otherwise return c when the callback
budget is exhausted. An empty final call is permitted. Cursor values at or
above ceiling safely finish; the coverage promise applies only to a pass
starting at zero and continuing with its returned cursors on the same map.

IDs satisfy `s<ceiling<=u64::MAX`, so s+1 fits without wrap, even at the
largest legal ceiling. Ceiling zero has no IDs and ends immediately. This
uses the existing finite ceiling as the domain; it neither requires a
permanent insertion serial nor silently wraps a generation counter. A pass
can advance at most ceiling times, even under continuing insertion/removal.
Returning c unchanged is not permitted for a valid unfinished positive-limit
step. The potentially huge ceiling bound is not a latency or practical pass
duration guarantee.

An optional owned cursor `{next,end_at_start}` would exclude IDs above the
initial extent and give a smaller termination bound for a sparsely reserved
map. It is not required for coverage; it changes the scalar interface and
client cursor transport. Do not introduce it merely to store a bucket index.

### Coverage argument

Choose any entry present from the beginning to the end of a pass. Its ID s
does not change. The cursor's lower bounds increase, and a step cannot cross
s without finding and reporting that entry: successor search either reports
every chosen live ID before advancing past it or stops earlier at its budget.
It cannot report completion while a live ID remains at or after the cursor.
Therefore that entry is reported. Reusing earlier IDs for newly inserted
entries does not affect the argument; those entries have no pass-wide promise.
Directory addresses may move and hash capacities may grow, shrink or rehash
without changing s. The hash table need not use powers-of-two capacities.

Clarify “pair”: recommend coverage of a continuously present **key/entry**,
reporting its value when visited. HSCAN/ZSCAN do not receive a snapshot of
values or scores across holds. If a key is removed and reinserted, it has no
continuous-presence promise. Even when hashing/equality is inconsistent,
memory safety cannot depend on their laws; directory coverage concerns each
continuously resident entry, and ordinary logical-key semantics keep their
existing consistency requirement.

### What limit bounds

Limit counts callback invocations/pairs, not buckets, reply bytes, radix
nodes, distinct hash groups or time. Firn passes 64; HSCAN may emit two RESP
items per pair. At most limit payloads are lent, including equal hashes.

A radix successor query can descend and backtrack at most H levels, using
child presence/prefix information to skip empty subtrees, and then descend
to its successor. It need not inspect every leaf or bucket. For the explicit
uncompressed binary candidate, register a verification target of at most
`3*H+3` directory-node inspections per successor query, including resumed
ancestor branch checks, and at most `H+1`
additional inspections to reborrow the selected payload without returning a
reference. At most limit queries are issued, giving target framework work
`(4*H+4)*limit`, H<=64: at limit 64, at most **16,640 node inspections**.
This operation-count target must be proved against the actual traversal;
it is not a measurement or a claim about a not-yet-written helper. A compressed
or paged layout must state its own bound, including occupancy checks.

The scan allocates no snapshot or collision buffer; traversal can use
bounded recursive frames retaining only call-local references, or an owned
fixed-size path of at most 64 numeric directions/prefixes. The numeric path
stores no references; an iterative reborrow-from-root at every backtrack
could cost O(H squared) and would need a different bound. Callback cost is additional and may allocate, perform
I/O preparation or traverse a large value. The scan cannot promise a fixed
wall-clock hold duration for an arbitrary returning callback. Between calls,
a writer's one-shot hash-index migration remains a separate long-hold cost.

### Effects on the existing map

This changes the recorded enum-owning representation and mutation protocol.
Each insertion/deletion updates the directory; each lookup follows an ID to
a payload; replacement preserves membership identity; freeing consumes each
owner once. Hash capacity/shrink policy can retain its numbers, while sparse
directory pruning releases empty storage. A flat never-compacted slot array
would retain its historical high-water mark and defeat the shrink goal, so
it is not the recommended layout.

Direct readers of `cells`, including Firn's current random picker, must be
audited and given an ordinary library operation appropriate to their actual
need; they can no longer interpret index cells as owning pairs. No consumer
migration is implemented here. An alternative keeping owners in buckets and
maintaining a reverse ID-to-bucket directory must update every moved pair,
keep both directions consistent and establish cross-backing bounds. It is
another representation, not a cost-free compatibility shortcut.

The grow-only requirement from shrinking remains a real decision constraint.
Directory allocation, metadata updates and extra lookup indirection can make
default growth slower. No evidence yet establishes otherwise. Consider an
explicit scan-capable library type if a general replacement fails that
criterion; that would require an owner choice and a real Firn type migration,
not a private caller-side reconstruction. Do not add a hidden lazy first-scan
million-entry rebuild and claim a bounded first hold.

## Natural proof obligations and checker boundary

Use [the active specification](../../../spec/kernel-spec.md) as authority,
not a runtime algorithm or a passing test. No checker run is authorized here,
so **none of the new source forms is claimed accepted**.

- OP-2/OP-4: exact s+1 follows s<ceiling<=MAX; counted traversal depth proves
  exact shift amounts below 64 and each fixed path-array subscript in range.
  A shift by 64 is not admitted; a wrapping shift would change that request's
  meaning rather than prove it. Branch to completion before any overflowing
  domain-end representation. Positive limit prevents zero being returned for
  an unfinished zero-budget call.
- OP-10/OP-11/OP-14, OWN-1 and PROV-6: initialized path storage, page/node
  construction, subtree replacement and pruning retain all owned children
  and all K/V outcomes. Prune by actual structural absence, not by using a
  numeric empty summary to discard an unproved nodrop payload. Generic keys
  and values impose no clone/drop/order requirement on scan.
- REF-2/REF-3 and TYPE-8: each call reborrows from current storage. The cursor
  and any owned path array contain numbers only; recursive frames may retain
  call-local references. Key/value references lent to the caller exist only
  inside visit; none return, enter aggregates or survive an atomic boundary.
- EFF-5 and FN-2/FN-5: callback reads keys/values and writes a disjoint env.
  It cannot resize/delete the scanned map through an overlapping env. Borrowed
  payload paths and whole-map readers must remain valid for the callback.
  The function-kind parameter is static, not a stored closure.
- SHARE-2/SHARE-3: the caller holds one entry/ordinary shared state for each
  scan call, releasing it between calls. The scan itself neither waits for
  another hold nor suspends inside its callback traversal. Each callback sees
  the value within that hold, not a cross-hold snapshot.
- Structural index invariants: hash entries refer to live directory owners,
  every live ID is unique and below ceiling, occupied subtree summaries match
  children, replacement retains an ID and rebuild preserves the directory.
  Generic type invariants are restricted by TYPE-11. The implementation must
  expose natural local bounds/ownership contracts and explicitly verify the
  inductive correspondence; it must not pretend a generic cardinality or
  recursive summary theorem is already a checker assumption. Sparse page
  occupancy and pruning need particular owning-source witnesses.

Ordinary integer/range/ownership rules are the intended implementation path.
Natural witness families before selecting a layout are: bounded radix
successor and recursive reborrow of a generic payload; pruning an empty
subtree with linear child owners; bounded-ID allocation/reuse; and the
callback's disjoint env across independent atomic calls. If CI rejects a
specified natural form, retain the smallest witness and identify whether
the missing capability is normative or an implementation gap. No extra
runtime validation, unsafe escape, hash-consistency precondition or alternative
spelling may hide that gap. No gap is established by this source study alone.

An ordinary library function/representation does not amend OP-1 or SHARE-1.
Its interface, library documentation, owning tests and proposed design node
would change together after the direction is selected. A new normative proof
rule or an amendment of the concurrent scan's contract would instead require
a separate specification decision, conformance evidence and owner approval;
this study proposes neither as already needed.

## Alternatives and rejection grounds

| Alternative | Coverage and cost | Disposition |
| --- | --- | --- |
| Physical index, clamp or restart | Same-capacity counterexample; repeated restarts can fail to finish. | Reject: fails the stated witness. |
| Powers-of-two plus Redis bucket cursor over today's linear probing | Changes reserve/ceiling/shrink capacities; probing still displaces owners. | Reject alone: not sufficient even at capacity four; this does not reject home chains. |
| Complete stable hash intervals over today's linear probing, count hint | Correct movement-independent coverage; collision groups and clusters can require whole-table filtering. | Semantically valid hint option, but no structural short-hold advantage. |
| Valkey-style home chains plus reverse-binary cursor | Complete homes preserve coverage across nested power-of-two changes; final chain can exceed count and has no collision-independent bound. Changes capacity and owning layout. | Recommended first comparison if the owner selects hint semantics and authorizes capacity-policy research; not implemented or approved. |
| Full-table top-64 by stable `(hash,key-order token)` | Needs a strict total order and an owned continuation token/cloning capability absent from HashMapKey; O(C) work per hold. Infinite ordered token domains can postpone a retained key indefinitely under bounded-size churn. | Reject for short holds and arbitrary owning K; not a memory-safety defect. |
| Stable flat directory | Finite numeric position and bounded physical-slot scans; sparse high-water storage stays. | Reject as final representation: conflicts with sparse backing release. |
| Registered scan/snapshot, scalar cursor as session handle | Can track collision suffix/owners, but start can cost O(n); storage, abandonment, expiry, simultaneous scans and effects become part of the API. | Do not select without explicit lifecycle/resource needs; a u64 handle is not a stateless position. |
| Integrated sparse stable-ID directory plus hash index | Hard pair cap and finite position domain, no clone/comparison callbacks; additional mutation/lookup/allocation cost. | Recommended if a distribution-independent cap is required; conditional on owning proof and performance evidence. |

## Acceptance to settle the design

No acceptance is marked complete. A future CI investigation should use a
small sample before a million-entry run and an oracle independent of the
implementation:

1. Protect a known subset of the million keys throughout a pass; interleave
   deletion/insertion, explicit same-size rehash, growth and shrink. Record
   each hold's callback count, framework work and union of reported keys.
   Under the strict option require callbacks<=64 and the proved node bound;
   under the hint option require roots<=b and callbacks<=q-1+last-chain size,
   counting every child/slot and explicitly recording collision overruns.
   Every protected key must appear in either option.
2. Include 65 and 129 distinct keys with identical hashes, deletion of a
   reported prefix, wraparound clusters and arbitrary capacities 3,12,28.
   The minimal physical/reverse-binary counterexamples must refute those
   alternatives over linear probing before relying on the positive oracle.
   Home-chain tests instead use power-of-two masks, complete chain reorder,
   repeated growth/merge and the selected reserve/ceiling/shrink boundaries.
3. For stable IDs, preserve IDs through replacement/value editing, verify current values at
   visit, exercise recycled IDs above/below cursor and continuing insertion
   within fixed ceiling, and check empty, ceiling-zero and final empty calls.
   A forged cursor must never permit an invalid subscript or reference.
4. Use nodrop owning K/V with an independent consumption ledger; prune empty
   prefixes/pages, regrow and free. Test the intended callback overlap and
   reference-escape rejection, not an earlier grammar failure.
5. For stable IDs, verify sparse directory memory scales with live owners, including a single
   survivor at the highest ID; no high-water array remains pinned. Count all
   retained metadata, not only reported hash capacity. For home chains, include
   root arrays, child pruning, boxed owners and allocator overhead; confirm
   sparse release and empty-to-zero under the future owner-selected capacity model.
6. Compare lookup, grow-only insertion, churn, resize/shrink and scan holds
   under identical source, machine and settings, interleaved before/after with
   a base twin. Keep reply count, framework inspections, allocations, total
   pass cost and latency separate. The existing no-grow-only-slowdown promise
   is not relaxed by a scan speedup. No performance number is predicted here.
7. Run the pinned Redis 7.2.16 scan tests and deterministic reply oracles in
   Firn, including compact full replies independent of COUNT and shrink
   regression #4906. Do not replace those checks with a callback-count test.
   Define the actual hold-latency acceptance number before measuring the hint
   option; compatibility alone cannot accept its work bound.

Firn must also review script rollback/logical restore: its earlier rationale
allowed collection layout to be rebuilt because no collection cursor existed.
Adding SSCAN/HSCAN/ZSCAN removes that premise. Stable-ID entry identities, or
home-chain hash partitions, must survive such internal restore for entries kept
logically present, or restore/whole-map replacement
must have a precise lifecycle contract. This is a downstream design dependency,
not a reason to preserve an invalid bucket cursor. The outer ConcurrentHashMap
cursor remains its existing position-space scan; the member/field/score cursor
belongs to the ordinary map inside the held entry.

## Questions for the owner

These are the questions as the study proposed them. The owner's rulings are
recorded under [the outcome](#outcome-rulings-implementation-and-criteria).

### Should Firn use COUNT as a hint, allowing a complete home-chain overrun?

**Background:** Redis 7.2.16 tests coverage rather than a per-call COUNT cap;
compact encodings return whole results. A bounded number of roots cannot bound
all member work when one root owns a million colliding keys. This is the premise
that the original hard-64 interpretation failed to expose.

- A (recommended for Redis compatibility): Use a positive pair hint and a
  separate complete-home step budget, allowing the final chain to exceed count.
  Require the selected hold-tail criterion on Firn workloads and collision
  trials. This accepts collision overruns; it gives no uniform member-work cap.
- B: Require a strict callback and internal-node cap for arbitrary hashes,
  using stable IDs or equivalent continuation. This costs an ordered directory
  and still needs a separate Firn policy for deterministic compact Redis replies.
- C: Require strict work bounds and home buckets together. Add stable intra-home
  continuation or owned scan state; whole-chain cursor steps alone cannot do it.

**Confidence 4/5** for the hint's coverage/count compatibility, **2/5** for its
latency sufficiency until the owner defines the hold criterion and CI measures it.

### Which representation should the next proof and cost comparison examine?

**Background:** Both candidates replace the owning cells and require consumer
migration. Home chains remove cross-home displacement without a second ordered
index; stable IDs supply stronger worst-case scan bounds at additional cost.

- A (recommended if hint semantics is selected): Compare home chains first,
  with one-shot migration and complete chain steps. Prove mask/ownership/cursor
  obligations and measure growth, lookup, churn, memory and hold tails; do not
  treat Valkey's pointer layout as a generic Whitefoot performance result.
- B (recommended if strict work is selected): Compare sparse stable IDs first,
  preserving current arbitrary capacities and exact shrink target. Prove the
  successor bound and pruning; measure the extra index's costs.
- C: Retain linear probing and scan by whole-table filtering. It avoids this
  representation migration but can inspect a million positions per hold.

**Confidence 3/5:** Conditional coverage arguments discriminate the options;
natural checker admission and relative cost are unverified for both.

### May a home-chain experiment revisit the approved capacity and shrink policy?

**Background:** Exact max(8,4*n), arbitrary reserve and arbitrary ceiling cannot
all denote a power-of-two physical root array. Seven-slot blocks and overflow
also change the meaning of capacity. The owner already approved the exact
shrink policy; no reinterpretation or rounding is authorized by this study.

- A (recommended only for home-chain research): Compare an explicitly revised
  physical-root capacity model, power-of-two ceilings/rounded reserve, and
  downward-rounded shrink with the retained 1/8 trigger and empty-to-zero.
  Re-prove hysteresis and set member/try-put limits. This breaks existing
  arbitrary capacity examples and requires updated tests and consumer docs.
- B: Keep an exact logical member quota while independently selecting roots.
  This can keep numeric shrink targets but changes capacity's physical meaning,
  memory guarantees and allocation behavior; requires separate contracts.
- C (recommended if approved physical semantics must stay): Keep the existing
  capacity policy and pursue stable IDs. Pay directory costs without changing
  the approved rule. Do not partially adopt a non-power-of-two bucket mask.

**Confidence 5/5** that a policy change is necessary for the root-count model;
**3/5** on which replacement to study. No policy change is approved here.

### What independent work and latency criteria should acceptance use?

**Background:** Neither count nor compatibility establishes hold duration;
callback costs are additional under both representations.

- A (recommended with hints): Prove roots<=b and total pair/child work relative
  to selected chains, explicitly accepting collision-dependent work. Set the
  numeric hold-p99 and total-pass criteria on Firn's board before measurement;
  record callback/child/slot counts and collision tails without hiding overruns.
- B (recommended with strict caps): Prove the exact directory bound, targeting
  16,640 node inspections for limit=64 and H<=64, and separately measure holds.
  The target is unverified and does not bound arbitrary callback time.
- C: Bound all metadata reads to 64 as well. Neither current candidate supplies
  this; redesign a stronger work cursor before implementation.

**Confidence 3/5:** These distinguish structural work from latency, but no
numeric latency criterion or measurement yet selects an acceptable option.

### Should a pass cover continuous keys and report values at visit time?

**Background:** Field values and scores can change between holds. Without a pass-wide snapshot, returning all initial values cannot be promised.

- A (recommended): Cover undeleted keys and report current values at visit;
  preserve stable IDs or home partitions as the chosen representation requires.
  Removal and reinsertion carry no coverage promise. This avoids snapshot cost.
- B: Require an initial snapshot. Introduce an explicit snapshot with an owning lifecycle, extra memory and startup cost.

**Confidence 4/5:** This matches Redis-style key coverage; Firn's commands, rollback and replacement lifecycle still need its session's confirmation.

### Should the selected representation replace the default HashMap?

**Background:** Either new representation can change non-scanning growth,
lookup and churn costs; the existing growth nonregression requirement remains.

- A: Replace the default representation while retaining the nonregression criterion; reopen this decision if it fails, without lowering the criterion.
- B (recommended as a choice after cost comparison): If the default proposal
  misses the criterion, provide an explicit scan-capable library type so callers
  choose the capability and cost. Firn migrates its type; audit cells readers
  and random selection in either case. Avoid a private second table in Firn.

**Confidence 2/5:** No comparison establishes which default or specialized layout meets the cost goal. Only a research direction is justified now.

## Unverified and dispositions

- No natural source witness has been run. Checker acceptance, coverage tests,
  metadata bounds, sparse memory release, mutation cost, actual hold latency
  and Firn consumer/restore compatibility remain unverified.
- Home chains are now an equal candidate, conditional on the count/work and
  capacity decisions. Redis/Valkey source reading establishes algorithms and
  test expectations, not Firn test-suite acceptance, checker admission or a
  measured speedup. Exact deterministic replies remain a downstream obligation.
- The fixed-ID directory is not implemented. The node-inspection target is
  conditional on an explicit successor algorithm and counts must be checked
  before any completion claim. Compressed/page variants need their own proof.
- Runtime wording about distinct “keys” having distinct positions describes
  distinct numeric tags; it must not be read as injective over arbitrary byte
  strings. SHARE-1 and the collision-group code govern the scan contract.
  Clarifying that comment is deferred; no runtime behavior defect is claimed.
- The ordinary-map proof study identifies required invariant transport, not
  an observed language/compiler gap. A real rejection would need its minimal
  witness and disposition under the owning Whitefoot board item.
