# Resumable scans of the ordinary HashMap

Status: design study, 2026-10-13, at
`2d0b6adf6762fb283773584a2f94d07d6aab201d`. No implementation, checker run,
test, benchmark or CI polling was performed. This is a sibling of the
[shrinking investigation](../hash-shrink/DESIGN.md): scanning requires a
separate traversal representation decision, rather than another shrink
threshold. No approved design or specification is changed by this record.

The board item is “the standard library hash map can resume from where it
stopped, visiting a bounded number of entries each time”. Firn needs SSCAN,
HSCAN and ZSCAN now. Its witness is one million members, visited over separate
atomic holds with at most 64 pairs reported in each hold, while insertions,
removals, growth, same-capacity rehash and shrinking happen between calls.
Every member present throughout a completed pass must be reported at least
once. New or removed members have no coverage promise. The design must also
state the amount of internal work, rather than equating 64 replies with a
short hold.

The [constitution](../../../docs/constitution.md) makes safety a constraint,
requires performance useful to real projects, and weighs actual compatibility
needs. The representation alternatives below are proposals on those grounds,
not choices selected by implementation convenience or measured winners.

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
match. A library **limit** would be a hard callback cap and its cursor would
be map-local; it must not be advertised as the concurrent function's count
hint, execution-wide hash position, lexicographic ordering or KeySet collector.
No concurrent-runtime change is needed for the ordinary library proposal.

## Why a bucket cursor is insufficient

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
scan choose complete groups and a hint. The Firn witness requires a stronger
cap; this study does not silently relax it.

## Recommended direction: stable logical scan positions

Investigate an integrated stable-ID iteration directory for the ordinary
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

Refine the proposed interface without adding a hash/equality parameter:

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
| Powers-of-two plus Redis bucket cursor alone | Changes reserve/ceiling/shrink capacities; probing still displaces owners. | Reject alone: not sufficient even at capacity four. |
| Complete stable hash intervals, count hint | Correct movement-independent coverage; collision groups and clusters are unbounded relative to limit. | Correct weaker option only if the owner changes the hard-64 requirement. |
| Full-table top-64 by stable `(hash,key-order token)` | Needs a strict total order and an owned continuation token/cloning capability absent from HashMapKey; O(C) work per hold. Infinite ordered token domains can postpone a retained key indefinitely under bounded-size churn. | Reject for short holds and arbitrary owning K; not a memory-safety defect. |
| Stable flat directory | Finite numeric position and bounded physical-slot scans; sparse high-water storage stays. | Reject as final representation: conflicts with sparse backing release. |
| Registered scan/snapshot, scalar cursor as session handle | Can track collision suffix/owners, but start can cost O(n); storage, abandonment, expiry, simultaneous scans and effects become part of the API. | Do not select without explicit lifecycle/resource needs; a u64 handle is not a stateless position. |
| Integrated sparse stable-ID directory plus hash index | Hard pair cap and finite position domain, no clone/comparison callbacks; additional mutation/lookup/allocation cost. | Recommended research direction, conditional on owner selection, owning proof and performance evidence. |

## Acceptance to settle the design

No acceptance is marked complete. A future CI investigation should use a
small sample before a million-entry run and an oracle independent of the
implementation:

1. Protect a known subset of the million keys throughout a pass; interleave
   deletion/insertion, explicit same-size rehash, growth and shrink. Record
   each hold's callback count (<=64), node-inspection count (against the chosen
   bound), and union of reported keys. Every protected key must appear.
2. Include 65 and 129 distinct keys with identical hashes, deletion of a
   reported prefix, wraparound clusters and arbitrary capacities 3,12,28.
   The minimal physical/reverse-binary counterexamples must refute those
   alternatives before relying on the proposed positive oracle.
3. Preserve IDs through replacement/value editing, verify current values at
   visit, exercise recycled IDs above/below cursor and continuing insertion
   within fixed ceiling, and check empty, ceiling-zero and final empty calls.
   A forged cursor must never permit an invalid subscript or reference.
4. Use nodrop owning K/V with an independent consumption ledger; prune empty
   prefixes/pages, regrow and free. Test the intended callback overlap and
   reference-escape rejection, not an earlier grammar failure.
5. Verify sparse directory memory scales with live owners, including a single
   survivor at the highest ID; no high-water array remains pinned. Count all
   retained metadata, not only reported hash capacity.
6. Compare lookup, grow-only insertion, churn, resize/shrink and scan holds
   under identical source, machine and settings, interleaved before/after with
   a base twin. Keep reply count, framework inspections, allocations, total
   pass cost and latency separate. The existing no-grow-only-slowdown promise
   is not relaxed by a scan speedup. No performance number is predicted here.

Firn must also review script rollback/logical restore: its earlier rationale
allowed collection layout to be rebuilt because no collection cursor existed.
Adding SSCAN/HSCAN/ZSCAN removes that premise. IDs for entries kept logically
present must survive such internal restore, or restore/whole-map replacement
must have a precise lifecycle contract. This is a downstream design dependency,
not a reason to preserve an invalid bucket cursor. The outer ConcurrentHashMap
cursor remains its existing position-space scan; the member/field/score cursor
belongs to the ordinary map inside the held entry.

## Questions for the owner

These are proposed questions for the named board item, not recorded rulings.
The session has no callable board tools; no card state has been changed.

The task already fixes the hard cap of 64 pairs; no reconfirmation is requested. Recommend counting callbacks with limit and excluding zero through a precondition. The concurrent scan's and Redis's count hints cannot meet this requirement; reconsider only if the owner changes direction.

### Should scanning use a library-owned stable-position directory?

**Background:** Current buckets can exchange owners during same-capacity rehash. A stable directory preserves member positions but adds indirection, metadata and allocation costs; sparse storage must actually release empty storage.

- A (recommended research direction): Design an integrated sparse stable directory and hash index with one owning copy of each pair. Retain arbitrary capacities, without rounding to powers of two or adding a second Firn table. Prove and compare costs before approving the final layout.
- B: Use an owned cursor or scan-session interface, separately resolving collision continuation, release and startup cost.
- C: Retain the representation and filter the whole table. Callback count can be bounded, but each call can inspect a million buckets, missing the short-hold goal.

**Confidence 3/5:** The conditional coverage argument holds; layout proof acceptance, sparse release and growth performance remain unverified.

### What independent bound should internal work have?

**Background:** Sixty-four callbacks do not mean sixty-four reads or a fixed duration. The proposed directory has a candidate bound of 16,640 node inspections per call; callback cost is additional.

- A (recommended): Adopt bounded per-pair directory work as described, targeting 16,640 node inspections with H<=64 and limit=64. The implementation must prove its exact bound; measure hold duration separately.
- B: Require at most sixty-four inspections including metadata. Redesign with a stronger work cursor or index layout first; callback count cannot establish this bound.

**Confidence 3/5:** The framework permits a finite bound; no layout is implemented, so the candidate is neither measured nor a verified contract.

### Should a pass cover continuous keys and report values at visit time?

**Background:** Field values and scores can change between holds. Without a pass-wide snapshot, returning all initial values cannot be promised.

- A (recommended): Preserve positions for undeleted keys and report current values at visit; removal and reinsertion carry no coverage promise.
- B: Require an initial snapshot. Introduce an explicit snapshot with an owning lifecycle, extra memory and startup cost.

**Confidence 4/5:** This matches Redis-style key coverage; Firn's commands, rollback and replacement lifecycle still need its session's confirmation.

### Should every default HashMap pay for a scan directory?

**Background:** A stable directory could slow non-scanning growth workloads, which the preceding task requires not to regress.

- A: Replace the default representation while retaining the nonregression criterion; reopen this decision if it fails, without lowering the criterion.
- B (recommended as a choice after cost comparison): If the default proposal misses the criterion, provide an explicit scan-capable library type so callers choose the capability and cost. Firn migrates its type; the library shares applicable common mechanisms.

**Confidence 2/5:** No comparison establishes which default or specialized layout meets the cost goal. Only a research direction is justified now.

## Unverified and dispositions

- No natural source witness has been run. Checker acceptance, coverage tests,
  metadata bounds, sparse memory release, mutation cost, actual hold latency
  and Firn consumer/restore compatibility remain unverified.
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
