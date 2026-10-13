# Shrinking the standard-library hash table

Status: research proposal, 2026-10-13. No implementation, compilation or
measurement has been performed for this investigation. The questions below
need the owner's decisions before implementation; this document does not
amend the language or the approved design tree.

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

Source inspection uses Whitefoot commit
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

The standard-library [interface](../../../lib/std/collections/hash_map/module.wfm),
[implementation](../../../lib/std/collections/hash_map/hash-map.wf) and
[library documentation](../../../lib/std/README.md) define the first table.
It is ordinary Whitefoot over `Box<Slots<HashMapSlot<K,V>>>`, with linear
open addressing. Every materialized bucket is an initialized enum: vacant,
deleted (a tombstone), or filled with one owning key/value pair. `length`
counts live pairs; `vacated` counts tombstones; public capacity is
`cells.inner.len`, not logical length. The fields are public readonly.

Removal finds a key, swaps its bucket with Deleted, decreases length,
increases vacated, and returns the complete owned pair. It neither releases
backing nor makes that bucket Vacant: a vacant bucket would prematurely end
a probe for colliding keys farther along the run. Tombstones are reusable
by insertion but are still traversed by lookup and Firn's random scan.

Insertion below the ceiling rebuilds when live plus deleted buckets reach
`C - floor(C/4)`. It cleans at the same capacity when live count is below
half that threshold, rounded down; otherwise it doubles capacity, with
zero-to-one growth and a ceiling clamp. At the ceiling it does not rebuild.
`hash_map_try_put` never changes capacity, `hash_map_reserve` only increases
it, and `hash_map_rehash` cleans tombstones without reducing capacity.
The private rebuild rejects a requested capacity smaller than the old bucket
count. It migrates by occupancy, never equality, through a one-pair pending
window, retaining every owner even with inconsistent callbacks.

The approved [HashMap storage decisions](../../../design/language/data-model/hash-map-storage.md)
and their ancestors [data model](../../../design/language/data-model.md) and
[language](../../../design/language.md) therefore support enum storage and
owning transfer, but contain no shrink policy. The ceiling decision expressly
says a map created at its ceiling never allocates again. Automatic removal
shrink would change that promise, including later regrowth, and needs an
explicit owner ruling rather than an undocumented exception.

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

Let `n` be live pairs after a successful removal, `d` tombstones, and `C`
materialized buckets. Define a proposed sparse condition `8*n < C`, tested
without multiplying as `C > 0` followed by `n <= (C-1)/8`. Equality at 1/8
does not shrink. Define the shrink target as zero for an empty map and
`max(8, 4*n)` otherwise. On this sparse branch the target is strictly smaller
than C and within the existing ceiling, and `4*n` fits in u64. The minimum
of eight is a proposed small-table floor, not a minimum construction size;
existing capacities zero through seven remain legal and empty maps release
to zero. No power-of-two rounding is needed: today's library supports
arbitrary capacities.

| Option | Program change and specification | Grow-only cost | Costs and risks |
| --- | --- | --- | --- |
| A. Automatic shrink after successful removal | Same call, result and effect row; changed library capacity/allocation behavior. No language-rule change expected. | No added insertion test, field or migration state; preserve current growth path. Generated code/layout can still regress and must be compared. | Default sparse bound; extra removal branch and synchronous migration; changes the ceiling allocation promise. |
| B. Explicit `hash_map_shrink(map, env)` only | New ordinary library function returning `unit`, with `reads(env), writes(map)`. Caller selects when to call it; example below. No OP-1 prelude addition. | No work for callers that never call it. | A caller that omits it remains sparse; Firn must invoke it inside a safe hold and choose frequency. Full compaction at every call would destroy amortization. |
| C. Automatic policy plus explicit shrink | A plus B, sharing one ownership-preserving migration mechanism. | Same intended grow-only cost as A. | Useful explicit release after reservation or bulk work, but adds an interface and two policy entry points without a demonstrated need in SPOP. |

For B/C, the proposed explicit operation chooses
`T = min(C, max(8, min(C, 4*n)))` for nonempty maps (compute the capped product
without overflow), or zero when empty. It rebuilds when T is smaller than C;
at equal capacity it cleans only if tombstones exist. No live owner is
discarded. Its internally chosen target is always admissible for a map made
and maintained by the library, so the proposed public result is unit; there
is no caller-requested capacity to refuse or allocation-failure outcome.
Repeated explicit compaction
has only per-call cost bounds; callers choosing it own the invocation policy.

Recommendation: A first, with one-shot migration. Existing explicit
`hash_map_rehash` already covers same-size tombstone cleanup; add B later only
for a concrete caller needing earlier capacity release. Do not force Firn
to encode a missing container policy or duplicate its storage. The proposal
is conditional on the safety/progress review and the performance falsifiers
below, not on accepting worse grow-only performance.

### Hysteresis and amortized work

After a non-floor shrink, occupancy is 1/4; the next shrink requires live
count to fall below half of that value. Growth still starts near 3/4 used
buckets. At the floor, shrink waits for empty. For example, C=64,n=8 does
not shrink; removing one more member produces C=28,n=7. Reinserting the
removed member leaves C=28, far from its grow threshold of 21 used buckets.
One insertion/removal cannot alternate these two capacities.

With well-distributed hashes and bounded hash/equality cost, rebuilding costs
expected O(C+n): inspect the old buckets, initialize the target and transfer
the live pairs. Since the preceding automatic resize, a constant fraction
of C successful deletions, or growth-producing insertions, pays for the next
resize. The geometrically decreasing old capacities give O(N) total expected
rebuild work while deleting a table built from N members. An explicit large
initial reservation charges O(C) to its construction/reservation, not to a
claim that its first single deletion is O(1). Existing tombstone cleanup on
insertion remains in the accounting; non-sparse churn can still accumulate
tombstones, especially at the ceiling, and is not solved by shrink alone.

This is not worst-case constant-time hashing. Colliding keys can require
quadratic migration probes and callback cost is additional. The unconditional
requirements are complete owning transfer, finite capacity-bounded individual
probes, and progress for returning callbacks; the expected amortized argument
does not authorize a partial operation or excuse an infinite retry.

For a map following A after successful deletions, `n>0` implies `C<=8*n`
(including the eight-bucket floor). At n=0 the proposed post-removal capacity
is zero. An explicitly reserved but never successfully deleted map need not
satisfy this bound. The acceptance condition concerns **available physical
bucket positions**, not occupied positions: occupied positions already equal
n in the current implementation. Tombstones reset to zero on every shrink;
between shrinks they may remain, but the same bound limits the scan extent.

## One-shot versus incremental migration

One-shot migration preserves the single public `cells` backing and ordinary
lookup/put paths. Allocate the target, transfer every owning pair by occupancy,
publish the completed representation and free the old backing before returning.
The existing helper swaps before draining; that internal ordering is also
possible under its exclusive effect, provided every intermediate owner remains
accounted for. Empty shrink must drain vacant/deleted enums and release the old
box; the existing zero-target early return cannot simply be reused.

Peak backing is old C plus target T buckets, plus constant pending storage;
moving an owning key does not deep-copy its heap allocation. At the first
sparse crossing T is about C/2, so the transient increase can still matter
near Firn's memory limit. Allocation failure retains STOR-8's existing
trusted-base exhaustion behavior; there is no new recoverable failure or trap.
The rebuild holds Firn's entry/Meta for O(C+n) expected work, possibly longer
under collisions. It adds no lock to the HashMap itself.

Incremental migration could spread old-bucket scanning over removals, but
needs two persistent backings, migration position/owner state, and a rule for
lookups, insertions, replacements, deletion, each/scan and freeing during a
move. It must preserve probe chains in the old table, avoid duplicates, handle
regrowth during migration and make progress even when deletions stop. Budget
old empty buckets as well as moved entries; a fixed entry budget alone does
not bound sparse-table work. Allocation/initialization and a colliding
destination probe can still be long. Thus incremental does not automatically
give a hard lock-time bound or the immediate `C<=8*n` retained-space bound.

Persistent dual backing also breaks Firn's direct `cells` picker unless its
interface is replaced with a migration-aware library operation and all direct
readers migrate. It adds representation/branch costs even to a grow-only
instance unless independently eliminated; a separate grow-only specialization
would need its own design justification. No concrete incremental interface is
selected here, so no claim is made that the explicit-shrink example below
is sufficient to implement it.

[Redis 7.0.15's dictionary](https://github.com/redis/redis/blob/7.0.15/src/dict.c#L115)
uses chained buckets, prepares a second table and advances migration in
steps, limiting empty-bucket visits per step; whole chains and allocation
remain costs. Its [set removal](https://github.com/redis/redis/blob/7.0.15/src/t_set.c#L85)
requests resizing when sparse. That is useful prior art for latency, not a
drop-in algorithm for owning open-addressed buckets.

Choose one-shot for the first library implementation because it preserves
the one-backing consumer interface and grow path. Reject it if whole-drain
or threshold-crossing contention violates the registered latency comparison;
then return to the owner with an incremental design, not a hidden Firn rebuild
or a relaxed acceptance condition.

## Safety, proof and cursor obligations

The [active specification](../../../spec/kernel-spec.md) supplies the rules;
library behavior is not a new primitive in OP-1. MOD-10 checks these standard
library records as ordinary modules. Relevant obligations are:

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

The key new algorithm argument is that live count equals Filled buckets,
so T holds all live owners even though T<C. Establish it inductively across
new, put/replacement/refusal, tombstone reuse, remove, reserve and rehash.
For the j-th transfer, j<n<=T implies a vacant destination exists; occupancy-only
migration does not merge owners even when equality/hash laws are inconsistent.
Keep the existing pending-owner discipline so local machine proofs cover
every operation independently of that global counting argument.

This is a correctness/progress argument, not a presently machine-proved
cardinality invariant. EFF-3 does not promise general termination; TYPE-11
does not admit a type invariant on this generic struct, and the existing
range facts supply no count-of-Filled aggregate. The current rebuild's
capacity-at-least-old-count argument avoids depending on metadata for progress;
the smaller rebuild must justify its stronger library invariant explicitly.
Do not merely delete its capacity check and retain a potentially endless
pending-owner loop without that argument. Do not add a recount/fallback to
hide an expressibility gap. If ordinary safe transfer cannot be expressed
and checked under the existing rules, preserve a minimal semantic witness
and bring the needed rule/compiler change to the owner before proceeding.

There is no stable bucket index, address, or iteration-order promise in the
library. Bounds safety is distinct from cursor coverage: a saved numeric
index can be checked in range yet skip a surviving member moved behind it.
Automatic shrink is compatible with today's per-call SPOP picker and borrowed
each; it does not create a mutation-stable scan API. If Firn adds collection
cursors, their protocol needs a separate design. Redis's
[SCAN guarantee](https://redis.io/docs/latest/commands/scan/#scan-guarantees)
requires returning members present throughout an iteration (duplicates may
occur); restarting or clamping an old bucket index does not establish that.
The outer map_scan guarantee in SHARE-1 remains unchanged.

No specification rule is proposed to change: OP-1's inventory, PRE-1's grow
domain, REF-2/3, ownership, effect separation and SHARE-1/2/3 retain their
before behavior. Policy and any new helper belong in the library interface,
implementation, documentation and the HashMap design node. A new built-in
table operation or stronger normative cursor guarantee would instead require
a specification proposal, conformance changes and an owner approval card for
the exact revision. No approval log is written by this investigation.

## Complete program comparison

These are complete source-bundle proposals, not compiler-validated examples.
They use the existing module/binding/entry-point form. They deliberately use
copy payloads; owning-payload validation is listed separately below.

Before (also unchanged source under automatic option A):

```wf
alias HashMapKey = std::collections::hash_map::HashMapKey;
alias ExitStatus = std::process::ExitStatus;

fn key_hash(env: &unit, key: &u64) -> result: u64 reads(key) {
  return key^;
}
fn key_equal(env: &unit, left: &u64, right: &u64) -> result: Bool reads(left), reads(right) {
  return left^ == right^;
}
binding Keys : HashMapKey<u64, unit> {
  hash = key_hash;
  equal = key_equal;
}
fn main() -> status: ExitStatus pure {
  let env = unit;
  let map = std::collections::hash_map::hash_map_new::<u64, unit, 128>(capacity: 64_u64);
  for (key in 0_u64..16_u64) {
    let put = std::collections::hash_map::hash_map_put::<Keys, unit, 128>(map: &map, key: key, value: unit, env: &env);
  }
  for (key in 0_u64..15_u64) {
    let removed = std::collections::hash_map::hash_map_remove::<Keys, unit, 128>(map: &map, key: &key, env: &env);
  }
  let count = std::collections::hash_map::hash_map_len::<u64, unit, 128>(map: &map);
  if count != 1_u64 {
    return std::process::exit_status(code: 1_u8);
  }
  return std::process::exit_status(code: 0_u8);
}
```

After, if explicit option B or C is selected (new function is proposed only):

```wf
alias HashMapKey = std::collections::hash_map::HashMapKey;
alias ExitStatus = std::process::ExitStatus;

fn key_hash(env: &unit, key: &u64) -> result: u64 reads(key) {
  return key^;
}
fn key_equal(env: &unit, left: &u64, right: &u64) -> result: Bool reads(left), reads(right) {
  return left^ == right^;
}
binding Keys : HashMapKey<u64, unit> {
  hash = key_hash;
  equal = key_equal;
}
fn main() -> status: ExitStatus pure {
  let env = unit;
  let map = std::collections::hash_map::hash_map_new::<u64, unit, 128>(capacity: 64_u64);
  for (key in 0_u64..16_u64) {
    let put = std::collections::hash_map::hash_map_put::<Keys, unit, 128>(map: &map, key: key, value: unit, env: &env);
  }
  for (key in 0_u64..15_u64) {
    let removed = std::collections::hash_map::hash_map_remove::<Keys, unit, 128>(map: &map, key: &key, env: &env);
  }
  std::collections::hash_map::hash_map_shrink::<Keys, unit, 128>(map: &map, env: &env);
  let count = std::collections::hash_map::hash_map_len::<u64, unit, 128>(map: &map);
  let buckets = std::collections::hash_map::hash_map_capacity::<u64, unit, 128>(map: &map);
  if count != 1_u64 { return std::process::exit_status(code: 1_u8); }
  if buckets > 8_u64 { return std::process::exit_status(code: 3_u8); }
  return std::process::exit_status(code: 0_u8);
}
```

The affine map is released at scope exit under STOR-3. The before program
retains 64 buckets today; A should leave eight without a caller change, while
B reaches eight only after the explicit call. Calling B after each threshold
crossing within counted SPOP would be necessary to bound scans inside that
command; calling it only after the command does not fix the current long hold.

## Acceptance fixed before measurement

All builds, checks, tests and measurements run through CI. None run on this
Mac. Start with one small timed CI sample, inspect its spread and resource
cost, then select scale before the million-entry case or performance panel.

### Correctness and storage

Extend the maintained [HashMap program](../../../tests/programs/containers/hash-map-program.wf)
and [allocation observer](../../../tests/programs/containers/container-allocation-observer.c)
where they already exercise the property. Use an independent expected key
set/ledger, not the new implementation's output as oracle.

1. Insert the distinct integers 0 through 999,999, remove 0 through 998,999,
   and check exactly the 1,000 survivors, `length==1000` and capacity at most
   **8,000** immediately after the last remove. Count Filled/Deleted/Vacant
   independently and verify they sum to capacity. Observe released backing
   allocations; reducing reported length alone is a failure. Repeat with
   selected random removal order and byte-string keys. The base fails the
   capacity condition; record that failure before relying on the new case.
2. Check the 64/8 boundary (no shrink), 64/7 transition (28 buckets, no
   tombstones), alternating insert/remove around that point, odd capacity 65,
   and initial capacities 0,1,7,8,9. Count rebuilds/allocations to detect
   oscillation. Missing-key removal changes neither contents nor capacity.
3. Remove to empty: capacity zero, no backing allocation retained, no owner
   leak; repeat an absent removal, then regrow from zero and verify every
   entry. Repeat at a small ceiling, including a table originally reserved
   exactly to that ceiling, replacement/full outcomes and refusal above it.
4. Exercise collisions that require tombstones, wraparound probe runs and
   repeated delete/reinsert. Check lookup after shrink and count `vacated==0`
   at each completed rebuild. Keep a non-sparse churn case demonstrating
   existing same-size rehash still cleans tombstones.
5. Use owning/nodrop keys and values and an independent consumption ledger:
   each removed/resident/replaced/refused owner is returned or consumed exactly
   once. Include inconsistent returning hash/equality callbacks: contents
   need their documented consistency laws, but ownership and finite migration
   must not. Small all-collision cases suffice; do not start a quadratic
   million-key hostile test.
6. Preserve existing negative conformance checks for stale references,
   overlapping callback effects, out-of-bounds indexing and invalid ownership;
   add a focused case only for an uncovered observation. A reference into old
   cells used after remove/shrink must be rejected for invalidation, not an
   unrelated parse error. A fresh reference under a current bound is accepted.
   Unchanged language expectations are not regenerated to match the compiler.
7. Firn's session verifies SPOP/SREM/SMOVE, repeated counted/full drain,
   SRANDMEMBER, hashes and sorted scores, AOF replay and existing outer SCAN.
   Membership/replay is the oracle, not exact random draws or unordered order.
   Verify concurrent same-set and unrelated-command correctness and holds.

### Same-source performance comparison

Primary benchmark: a new, explicitly invoked **HashMap grow-only** trace in
this investigation's later experiment bundle, using the current standard
library, inserting 1,000,000 unique keys from capacity zero with no removals,
plus presized insertion and grow-then-hit-lookup variants. Include u64/unit,
Firn-like owning byte-string keys/unit, and a wide owning value. Check contents
outside the timed region. Measure insertion and lookup separately from
construction/destruction and compiler build time. The old
[map-library comparison](../../experiments/container-representation/map-library/RESULTS.md)
is methodology evidence, not a current runnable benchmark: its Makefile still
names the former library path. Do not silently import or modernize that
historical harness into the gate.

Build base B, a second identical-source base twin T, and shrink candidate S
with identical compiler/backend settings apart from the library change.
Because the compiler embeds the library, record both compiler revisions and
embedded module bytes. Keep the benchmark source, inputs, allocator, flags,
thread counts and machine fixed; inspect emitted insertion/growth paths for
unexpected added work. Run on the idle 14900K through CI, coordinate use,
and interleave B/T/S in rotated orders with independent fresh processes and
identical seeds. Grow-only single-table tests use one thread; disjoint-table
tests use one and two workers under the same allocation conditions.

Pre-register this decision rule: target S/B elapsed-time ratio is **<=1.00**
for each grow-only cell; no slowdown allowance is part of the requirement.
Use paired log ratios and 95% confidence intervals across independent blocks.
Start a pilot; a T/B median outside [0.99,1.01] or interval width over 0.02
means the comparison is unresolved and needs a quieter or longer run. Those
are precision criteria, not permission for a 1% slowdown. A candidate whose
95% interval is wholly above 1.00 in two independent cohorts fails. A result
whose interval overlaps 1.00 establishes only no detected regression at the
reported resolution, not proof of zero slowdown; do not mark the strict
requirement proved. Retain the intervals and code-path evidence for the owner.

In Firn, use identical server/workload source for B/T/S and change only the
Whitefoot library build. Measure SADD on fresh sets (grow-only) and depth-1
SPOP after a million-to-thousand preparation, at one and two cores. Include
threshold-crossing individual pops, one counted full drain, and concurrent
requests waiting on the same set or Meta; record maximum hold time and per-pop
latency as well as throughput/p99. Register sparse SPOP acceptance as positive
throughput improvement and lower p99 against B beyond twin variation in two
cohorts; reject a repeatable p99 increase in threshold/full-drain cells.
Measure LRANGE_100 unchanged as the previously regressed unrelated control,
using the same no-slowdown criterion as grow-only. An unexplained repeat of
its supplied 9% loss blocks attribution and delivery.

The falsifiers are lost/duplicated owners or contents, failure of the 8,000
bucket bound, shrink/grow oscillation, a repeatable grow-only slowdown, or
one-shot hold-tail regressions. A sparse-case win cannot cancel any of these.
Performance tests belong in explicitly requested experiment CI; useful
correctness cases belong in the ordinary gate, never depending on research.

## Ordered plan and owner decisions

1. Owner selects automatic versus explicit policy and rules on the ceiling
   allocation promise; select one-shot first only with its latency falsifier.
2. Audit all library mutation paths for the live-count invariant and all
   direct downstream bucket consumers. Verify the smaller migration's local
   proof obligations in focused CI when implementation is authorized. Any
   compiler/language gap stops that path with a minimal witness, not a fallback.
3. Implement the selected library policy and shared owning transfer; keep
   insertion/growth unchanged. Update its interface documentation and proposed
   design node together. Firn changes, pin adoption and verification belong to
   Firn's session. No concurrent-runtime change is scheduled.
4. Demonstrate base failures/new passes for the named correctness cases,
   then run the pre-registered performance panel after its small pilot.
   These runs are the acceptance evidence: cite their actual revisions and
   results, and repeat only when changed code could affect them or unresolved
   variance requires it, never merely to tick a separate acceptance row.
5. Apply an independent completion review, fix findings, run the canonical
   gate on the delivery revision, and bring exact design changes/evidence to
   the owner. Only after rulings write approval records and prepare delivery;
   the owner merges. This research task performs none of those implementation
   or measurement steps.

The following are proposed decision-card texts, not recorded owner rulings.
The session has no callable wf-board tools, so they could not be filed there.

### 删除后默认缩容吗？

**背景：** Firn 的集合使用标准库 HashMap。删除只留下墓碑，SPOP 在持锁期间扫描旧桶。

- A（推荐）：成功删除后，低于 1/8 占用时缩到每个成员四桶，非空最少八桶，空表归零。调用方不改代码；删除可能分配和搬迁。
- B：只提供显式缩容。调用方控制时机，但必须接入每条删除路径，否则问题仍在。
- C：自动和显式都提供。覆盖更多用途，但首轮多一个没有具体需求的接口。

**把握 4/5：** 已核实调用链与表结构；尚未验证迁移证明和性能。

### 允许删除改变“预留到上限后不再分配”的承诺吗？

**背景：** 现有设计承诺预留到上限的表不再分配。默认缩容需要临时新表，随后重新插入也可能增长。

- A（推荐）：允许删除触发缩容；保留插入在当前容量等于上限时不重建的规则，并明确更新原承诺。
- B：保留原承诺，采用显式缩容作为调用方主动放弃容量保留的入口；自动方案需重选，不能暗中豁免这类表。

**把握 4/5：** 冲突可由源码与设计直接确认；依赖该承诺的全部调用方尚待审计。

### 首轮采用一次性搬迁吗？

**背景：** 一次性搬迁保留现有桶接口和增长路径，但会延长触发删除的持锁时间。增量搬迁需要跨调用保存两张表，并改造直接读桶的调用方。

- A（推荐）：先做一次性搬迁；阈值跨越或整表弹出的 p99 若可重复变差，就返回此决策，不降低标准。
- B：先设计增量搬迁及遍历接口；覆盖更多状态，且需单独证明增长路径没有额外成本。

**把握 3/5：** 接口成本明确，最长持锁时间还没有测量。

## Unverified and found along the way

- Independent source review used `gpt-6.1-sol` at high effort, from the
  inspected Whitefoot HEAD to this new research file. It covered repository,
  documentation and reporting checks plus applicable design/correspondence
  checks, with no material findings. No implementation or test-wiring checks
  applied; algorithm/proof completion remained unverified. The review read
  the Firn snapshots and Redis sources and ran no build, test or measurement.
- This is a source-based proposal. The illustrative programs, smaller rebuild
  proof acceptance, all correctness cases and all performance outcomes remain
  unverified. No new specification behavior is claimed or approved.
- Live-count preservation must replace the old bucket-count progress argument;
  local safety checking alone is not a termination proof. This is a required
  implementation/review obligation, not permission to weaken ownership.
- The existing ceiling allocation promise needs an owner decision before A.
  The displayed automatic bound is conditional on that policy being selected.
- First-filled random selection bias is outside the shrink task. Record it
  with Firn for a separate compatibility decision; no change is made here.
- The historical map-library harness uses an old library path. It is retained
  evidence, not selected as runnable guidance; the future benchmark must use
  current module imports. No historical artifact was modified.
- Full downstream memory-accounting, persistence, scripts and direct-bucket
  consumer audits remain with implementation and Firn verification. Existing
  outer-map cursor semantics must not be confused with member iteration.
- Status-board filing was unavailable in this session. These decisions and
  dispositions are preserved here for the responsible session to file; no
  board update, CI result, measurement or owner approval is implied.
