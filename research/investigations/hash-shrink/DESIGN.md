# Shrinking the standard-library hash table

Status: proposed implementation in the worktree, 2026-10-13. The owner
instructed implementation on the three recommended options while the board
cards remain open: automatic shrinking, removal allowed to shrink a table
reserved to its ceiling, and one-shot migration. These are adjustable proposed
decisions, not approval records. Gate run 38396927995 on
`2d0b6adf6762fb283773584a2f94d07d6aab201d` failed; its supplied log is
discussed below. No passing acceptance or measurement result has been produced.
The owner commits, pushes and runs
CI; this session only reads and edits source.

## Implementation submitted for CI

The library now calls a private shrinking helper only after a successful
removal has returned its key/value owner from the bucket and updated the live
count. Empty maps migrate to zero capacity and release their previous backing.
Nonempty maps test `n <= (C-1)/8` after establishing `C>0`, then migrate once
to `max(8,4*n)`. Missing-key removal retains the existing early return.
No public signature or representation changes, and no explicit shrink
operation is added in this round.

The previous owning migration body is shared by growth and shrinking.
The growth/rehash wrapper retains its original ceiling refusal, refusal of
targets smaller than the current bucket count, and zero-capacity no-op.
The insertion and try-insertion function bodies retain their previous policy
and operations. Extracting a private helper can still affect emitted code;
equivalent growth cost remains a CI/performance question, not an established
result.

The private backing-initialization helper now states actual storage
length/capacity obligations without a logical map-ceiling parameter. It does
not select a map's capacity. Construction and growth retain their ceiling
checks; shrinking preserves the ceiling because its nonempty target is
strictly smaller than the previous capacity. This separates initialization
from capacity policy rather than adding a runtime clamp, recount or fallback.

The arithmetic certificate derives `8*n <= C-1` from `n <= floor((C-1)/8)`
and the specification's division fact. This bounds exact `4*n` without
overflow. Since n is nonzero in that branch, C is at least nine, so the
eight-bucket floor is also smaller than C. The submitted source asserted
`target<C` only after the mutable-target join, which CI refused with INV-1:
the branches had not established a common retained fact. The repaired source
proves that same conclusion inside each branch without a proof block. AUTO's
one-premise family can subtract the sparse fact: the floor branch leaves
`8-8*n<=0`, from `n>=1`; the scaled branch leaves `-4*n<=0`, from the unsigned
domain. Explicit blocks would be redundant under PRF-1. Both publish the same
difference bound before the join, so its intersection can retain it for the
final invariant. These are erased proof steps, not additional runtime checks.
Gate run 38401080942 accepts the repaired library; its Firn program tests
pass. The initial missing branch fact was an authoring error, not a demonstrated
language gap. The separate HashMap fixture repairs below still await CI.

Local take/place/index/swap/free obligations retain the existing pending-owner
protocol. The library's inductive live-count invariant supplies the progress
argument for a smaller destination: before moving the next owner, fewer than
the target's buckets are filled. This is a reviewed correctness argument,
not a new machine-checked cardinality invariant or a general termination
claim. The zero-target path drains only vacant/deleted enums when live count
is zero; it does not discard a generic owner as an impossible case.

The library interface and README describe shrinking removals and their
allocation, hashing and reference-invalidation implications. The proposed
HashMap design node retains insertion's no-rebuild rule at the current
ceiling and removes the lifetime no-more-allocations promise for maps that
undergo removal. No specification rule or approval log changes.

The existing normal-home HashMap fixture now covers the million-to-thousand
eight-buckets-per-member bound, exact threshold/hysteresis across 128 cycles,
empty-to-zero reuse, regrowth with all surviving lookups, reserved-ceiling
removal versus ceiling insertion, and smaller migration of must-consume
Box-owning keys/values. The old C=16,n=1 tombstone trace would now shrink,
so it instead retains two pairs at the strict equality boundary and keeps
testing same-capacity insertion cleanup and tombstone reuse. The existing
native allocation/release ledger remains exact; its new 118-event expectation
is derived from those sources, not observed in a passing run.

The [temporary hosted workflow](../../../.github/workflows/hash-shrink-hosted.yml)
uses the [explicit experiment bundle](../../experiments/hash-shrink/README.md).
It prepares identical-source base/head images and a separately emitted/linked
twin from a byte-identical base compiler. A small pilot precedes any selected
million-entry panel, and panel dispatch checks the previous pilot's source
identity while rebuilding images and rerunning a small pilot on its current
host. No native binary is reused across hosted machines. The bundle retains
raw results, identities, generated sources and LLVM for later inspection.

The library panel covers three owning representations, grow/presize/hit and
sparse/full pops, plus same-process disjoint maps with one and two configured
workers. Actual parallel offering still needs CI evidence. Fixed Firn source
builds cover SADD, sparse SPOP, threshold crossing, full counted drain,
LRANGE_100 and competing same-set requests on one/two server CPUs. Network
latency includes client overhead and cannot directly measure internal holds;
low-sample tails and hosted precision remain unresolved. The qualification
entry compiles the same maintained fixture with six separately selected test
entries under both compilers, requiring baseline assertion exit 1 and head
exit 0. A compiler rejection or crash cannot count as the pre-change failure.

Review identified missing measured budget rows for the new temporary labels
(the new-stage requirement in AGENTS.md). No 20-run CPU-model evidence exists
from which to derive them. The workflow therefore refuses execution by
default. Its `allow_unbudgeted_experiment` input exposes a proposed temporary
report-only mode for the owner to rule on and explicitly select at dispatch;
the choice is recorded, and it supplies no gate/readiness budget evidence.
This budget decision is additional to the three open design cards. No measured
budget or owner approval is fabricated. Remove the temporary workflow before
readiness and update its active documentation link at the same time.

The sections below preserve the pre-measurement question, alternatives and
acceptance criteria. They are not measured results or a record that the open
board cards have been ruled.

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

The following representation and removal description records the inspected
baseline, before this implementation. The current standard-library [interface](../../../lib/std/collections/hash_map/module.wfm),
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

The baseline [HashMap storage decisions](https://github.com/Ming-Research/Whitefoot/blob/50773fdcd3007685e4cdd1903b2dcb0cc1052600/design/language/data-model/hash-map-storage.md)
and their ancestors [data model](../../../design/language/data-model.md) and
[language](../../../design/language.md) therefore support enum storage and
owning transfer, but contain no shrink policy. The ceiling decision expressly
says a map created at its ceiling never allocates again. Automatic removal
shrink would change that promise, including later regrowth, and needs an
explicit owner ruling rather than an undocumented exception. The
[current draft node](../../../design/language/data-model/hash-map-storage.md)
now proposes the removal policy and revised ceiling promise; the board cards
remain open.

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
retains 64 buckets under the inspected baseline; A should leave eight without a caller change, while
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
   the owner merges. Implementation and test sources are now being prepared
   in this worktree on the recommended options; the owner performs commits,
   pushes and all CI execution. No acceptance run is complete yet.

The owner reports that the following three decisions are now cards on the
board. Their texts below remain proposals, not recorded owner rulings. The
instruction to implement their recommended options provisionally authorizes
this worktree change while the owner may still choose otherwise.

### Should removal shrink automatically?

**Background:** Firn's sets use the standard-library HashMap. Previously removal left tombstones, so SPOP scanned the old backing while holding the set.

- A (recommended): After successful removal, shrink below 1/8 occupancy to four buckets per member, with a minimum of eight for nonempty maps; empty maps return to zero. Callers need no code change; removal may allocate and migrate.
- B: Provide explicit shrinking only. Callers control timing but must cover every removal path or the problem remains.
- C: Provide both. This covers more uses but adds an interface without a concrete first-round need.

**Confidence 4/5:** The call chain and representation were verified; migration proof acceptance and performance remain unverified.

### May removal change the reserved-ceiling allocation promise?

**Background:** The previous design promised no further allocation for a table reserved to its ceiling. Automatic shrinking needs a temporary backing, and later insertion may grow again.

- A (recommended): Allow removal to shrink; retain insertion's no-rebuild rule when current capacity equals the ceiling, and explicitly update the promise.
- B: Retain the promise, with explicit shrinking as the caller's opt-out from reserved capacity. Reconsider the automatic policy rather than silently exempting these tables.

**Confidence 4/5:** Source and design establish the conflict; all consumers relying on the promise still need auditing.

### Should the first implementation use one-shot migration?

**Background:** One-shot migration retains the bucket interface and growth path but lengthens the triggering removal's hold. Incremental migration retains two backings between calls and requires changes to direct bucket readers.

- A (recommended): Start with one-shot migration. Reopen this decision if threshold-crossing or full-drain p99 reproducibly regresses; do not lower the criterion.
- B: Design incremental migration and traversal first. This adds states and requires separate evidence that the growth path incurs no extra cost.

**Confidence 3/5:** Interface costs are understood; worst hold duration has not been measured.

### May temporary measurement stages report budgets without enforcing them?

**Background:** The temporary hosted workflow exists, but its new stages lack at least twenty runs on the same CPU model from which the current rule derives budgets. It refuses execution by default and runs only with an explicit temporary-mode selection; compilation failures, incorrect results, missing inputs and timeouts still fail.

- A (recommended): Permit report-only budgets for this manual experiment. Run a pilot, choose scale from its results, retain all records, and remove the temporary workflow before readiness. This decides only temporary stage budgets; it neither approves the design nor treats hosted data as precise performance acceptance.
- B: Keep the workflow disabled until deciding how to establish new stage budgets. Library implementation and ordinary correctness CI can proceed; performance comparison waits.

**Confidence 3/5:** Source establishes missing data and default refusal; the temporary policy still needs an owner ruling, not an inference from prior experiments.

## Unverified and found along the way

- The supplied failure log for gate run 38396927995 records three repairable
  source defects: the post-join shrink proof described above, Han characters
  in the research decision questions, and an unsupported `//` comment in the
  ordinary HashMap fixture. Both the shrink and scan studies now use English
  throughout; the fixture's sentence is retained in its existing declaration
  `doc` field, with no test assertion or expected result changed.
  Canonical-corpus checking reported exactly one unexpectedly underived file,
  `hash-map-program.wf`, and zero unexpectedly noncanonical files. Its lexing
  refusal was FORM-4, independent of the library's semantic INV-1 refusal.
  The shared-receipt adapter reported 76 mismatches, all reaching INV-1;
  its graph checks the carried standard-library modules, so the failing
  HashMap module is the common displayed cause. No separate receipt-cache
  defect is evidenced by that log. The later gate outcome is recorded below.
- Gate run 38401080942 on `176740a6286ee44eab11ecf4f7d28ec209fa26d9`
  checks the library and passes the Firn program tests. The remaining HashMap
  fixture refusal is GRAM-9: a True constructor used directly as a call argument.
  That parse refusal also causes its unexpectedly-underived canonical result.
  The repair binds this constructor first, and likewise binds four False
  constructors used directly inside hash-environment fields. These are the
  specified flat source forms; assertions and workloads are unchanged.
  The indexed-membership fixture executes successfully and releases every
  allocation, but its observer reports 137 events against the old 129.
  Four payload/policy traces each keep C=8 until final expiry removes IDs
  16,17,10, reducing live count 3,2,1,0. No positive-live shrink is possible
  because 8*n<8 is false for n>=1. The last removal returns to C=0; final
  attachment of ID313 then allocates C=1 and retires that new shared empty
  header. Each trace adds one allocation plus one header event, so
  129+4*(1+1)=137. Auxiliary maps never lose their last ID through removal.
  The harness expectation now includes that derivation beside the number;
  its exact release checks remain. These test repairs await the owner's CI,
  and neither authoring error establishes a language/checker gap.
- The committed implementation, compared with its study-only parent
  `8f6a273ca53d6d1a2df56a35ab6093ff3a7165b1`, enlarged the existing fixture
  from 1,037 to 1,370 lines (386 inserted, 53 deleted; net 333). It added five
  invoked scenario functions and one borrowed-read callback, but no formal
  files, Rust test functions or conformance manifest cases. The existing
  harness still compiles two WF modes and builds/runs four native images.
  Across those four runs the new million-entry scenario schedules 4,000,000
  insertions, 3,996,000 removals and 4,000 survivor lookups, plus migrations
  and release work. The committed library body grew from 433 to 465 lines.
  This adds real gate work. In the failed run, fixture lexing stopped before
  its new native loops executed; added parsing and library proof work still
  can contribute. The corpus stage took 239.73 seconds against 235 seconds,
  but no paired or phase-specific timing attributes the delta. It cannot be
  called unrelated runner variance or assigned a precise added duration from
  this evidence. Budgets are unchanged; corrected-run timings are required.
- The research-only source review used `gpt-6.1-sol` at high effort, from the
  inspected Whitefoot HEAD to the original research file. It covered repository,
  documentation and reporting checks plus applicable design/correspondence
  checks, with no material findings. No implementation or test-wiring checks
  applied; algorithm/proof completion remained unverified. The review read
  the Firn snapshots and Redis sources and ran no build, test or measurement.
- The independent implementation review used `gpt-6.1-sol` at high effort,
  from study commit `8f6a273ca53d6d1a2df56a35ab6093ff3a7165b1` to the dirty
  worktree including the temporary workflow and experiment assets. It covered
  every applicable repository, documentation, code/case, check-wiring and
  design/correspondence group. Experiment source grammar and a historical
  decision citation were repaired and reread. The missing temporary stage
  budgets remain an owner decision, guarded by a default execution refusal.
  Machine proof, actual tests, gate and performance remain unverified; the
  review ran none of them locally.
- This contains a source-based proposed implementation. The illustrative programs, smaller rebuild
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
- Status-board tools remain unavailable in this session. The owner reports
  that the three decision cards have been filed; this session cannot verify
  or update their state. No CI result, measurement or owner approval is implied.
