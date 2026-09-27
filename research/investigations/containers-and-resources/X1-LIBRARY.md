# Containers over the x1 language

The reassessment and first Vector trial below use the merged PR #70 baseline,
`36be8784e84a26d34bc24668babd789e0f4c96fb`, kernel v0.60, and their stated
subsequent implementation revisions. The Box-placement and consumption
follow-up at the end starts from kernel v0.61. The initial study
used the earlier published snapshot `efd6ebc9`; its dated observations remain
in the [probe results](../../experiments/container-representation/x1/RESULTS.md).
The merged-baseline reassessment below supersedes outstanding-work claims
about that snapshot. Compiler implementation failures are distinguished from
specified language limits. The active
[specification](../../../spec/kernel-spec.md) remains the language authority.

The question is whether full system-container operations can be expressed at
competitive cost using the existing framework, and which implementation or
measurement should come next. One global heap, local nonescaping references,
structural copy/drop capabilities, ordinary owned values, and monomorphized
`interface`/`binding` behavior are premises, not alternatives reopened here.
Older provider/refusal and region contracts in [REASSESSMENT.md](REASSESSMENT.md)
and [FOUNDATION.md](FOUNDATION.md) are dated evidence, not requirements of this
library. A checked branch is acceptable when its cost meets the operation's
requirements; inability to erase it is not itself a language gap.

## Evidence and comparison criteria

The [external workload traces](EXTERNAL-WORKLOADS.md) supply application
contracts from pinned Rust, C++ and Go programs, including the later Linux,
Redis and SQLite extension. They do not measure prevalence. Application needs
must be separated from the host language's chosen representation: e.g. stable
logical identity does not by itself require pointer-linked nodes.

Use these labels throughout the assessment:

- **Rule deduction:** follows from the cited v0.60 rule, not a compiler run.
- **Source observation:** present in pinned source or generated lowering.
- **Checked experiment:** an identified program passed an identified binary.
- **Measured control:** an identified native model, not WF performance.
- **Candidate:** an interface/representation awaiting implementation evidence.

Before any new selection experiment, keep the operation contract and its
observable result fixed. For each representation record construction, lookup,
mutation, growth and cleanup; occupied and reserved bytes; allocations;
element transfers; and the facts required across helpers. Test copy, owning
droppable and must-consume elements separately where relevant. No failed
program may be called a language limit when it contradicts the specification.

The initial discriminators are:

1. Complexity: drain must move O(n) elements, ordinary probing must terminate
   after at most capacity probes even with hostile hash/equality, and rehash
   must account for its new allocation plus old live elements. Reject an
   implementation that meets only a small fixed-size timing while violating
   the promised complexity.
2. Sparse storage: compare one backing of valid enum slots with boxed payloads
   and with a dense payload store plus sparse indexes. Count all metadata and
   identity-repair work; do not charge a tag only to one side or assume niche
   encoding in a WF layout.
3. References: distinguish stable slot identity, stable physical addresses and
   retained membership. Compare the same removal/invalidation contract.
4. Construction: retain helper boundaries and separate the irreducible move
   between different storage from avoidable temporary/result transfers.
5. New proof or storage machinery needs a concrete consumer and evidence of
   the remaining cost after the best ordinary representation. No universal
   percentage threshold is invented for all workloads.

## Scope of the resulting comparison

The common families are growable vector, ring deque, generational slab,
owning hash map, priority queue, and an ordered tree. The ceiling challenges
are multiple indexes retaining one object, large-value construction and
compact variable-sized records. A composite in-memory index workload will
connect the families without being described as a production database.

This note is the current home for this comparison, with experimental evidence
linked at its source. It will retain its pinned conditions when subsequent
implementation replaces individual candidate conclusions.

## Findings and boundaries

The initial study identified ordinary-value representations for six families.
Its two narrow owned-element operations have checked/native evidence in the
[x1 probes](../../experiments/container-representation/x1/RESULTS.md): indexed
vacancy exchange with a must-consume element, and a generic ordered drain with
linear element movement. The later [completion matrix](#current-completion-boundary-at-v068)
now records all six complete libraries and their indexed composition in this
delivery; native parity remains a measured question for each operation.

Three different questions must not be collapsed:

| Kind | Example | Consequence |
| --- | --- | --- |
| Library algorithm choice | Repeated `remove_at(0)` versus reverse then `take_back` | A linear-time ordinary alternative exists; no new primitive follows from the quadratic source. |
| Specified interface limit | A range over a Ring, even an empty/unwrapped one, is refused by REF-4 | A generic two-span API is unavailable on that representation. A fully initialized copy-element Array is a different available representation. |
| Resolved snapshot defect | The earlier reserve helper supplied unrestricted u64 capacity to `grow` | OP-9 requires a size bound. The merged library supplies one through `ceiling`; the unbounded research negative remains correctly rejected. |

The family sketches below started as recommendations for implementation trials,
not adopted library interfaces. The current completion matrix and later trials
identify the executable libraries, their evidence and the selected boundaries. Existing temporary-reference,
global-heap, no-hole and no-stored-reference choices remain premises. Their grounds are in
the [data-model](../../../design/language/data-model.md),
[ownership](../../../design/language/ownership.md) and
[generics](../../../design/language/generics.md) decisions; rules, not those
records, determine acceptance.

## Common families: representations and complete operations

Independent source sketches below omit bodies and are not a single bundle:
their names illustrate interfaces, not final exported names. `T` is unbounded
when the operation can support must-consume elements. `T: drop` is an explicit
extra requirement only for operations that discard elements. A copy instance
of an unbounded generic body retains its template's move/swap spelling under
FN-2; separate copy algorithms are performance alternatives, not a requirement
to duplicate every library.

| Family | First ordinary candidate | Complete operation chain | Proof and ownership obligations | Cost obligations, not measured WF parity |
| --- | --- | --- | --- | --- |
| Vector | `Box<Slots<T>>`; `Slots<T,N>` for inline bounded use | Construct, reserve, append, insert, ordered remove, swap-remove, truncate, ordered consume/drain, final release | OP-9 on growth; index/room facts; exact count changes over references; a consume callback for nodrop T; `free_empty` after complete consumption | O(1) no-growth append; amortized O(1) under geometric growth; O(n-i) ordered insert/remove; O(1) swap-remove; O(n) drain. No owner round trip for mutation. |
| Deque | `Box<Ring<T>>`, fresh ring plus a counted take/place transfer for exact rebase contracts; fixed Ring for bounded queues | Push/pop both ends, wrap, indexed access, grow/rebase, consume, release | Bounds in logical coordinates; front operations invalidate old slot references; new backing invalidates every old path; source emptied before releasing it | O(1) endpoints and logical access; O(n) growth; one visit per drained element. Generic two-span consumption remains a separate unavailable interface, discussed below. |
| Slab | `Box<Slots<SlabCell<T>>>`, with `Slots<T,1>` occupancy, a free-list head and generation handles; compare a compact tagged native cell | Insert/handle, validate/get, remove, expiry, reuse, exhaustion/limit outcome, final consumption | Return every removed T; never wrap a generation into an old handle's generation; distinguish a slot ID from a physical address | O(1) free-list operations and validation; no payload movement on ordinary reuse. The bounded library does not grow its backing; its extra occupancy word is a measured cost. |
| Hash map | One window of valid `Vacant/Deleted/Occupied(K,V)` slots; compare a dense-entry/index-table alternative for large payloads | Construct, collision insertion, duplicate replacement, lookup after tombstone, remove, reuse, growth/rehash, iterate, consume | K and V need not be copy/drop if replacement returns the old pair and final cleanup explicitly consumes them; bounded probing; no assumed behavior laws; preserve every owner during rehash | Hash/equality cost plus probes; ordinary load-dependent expected constant access, capacity-bounded worst-case lookup. Rehash scans old capacity and reinserts live entries; adversarial collisions can make it quadratic. |
| Priority queue | Slots of T, with an explicit comparison behavior; indexed variant adds a reverse-position map | Push, peek via a local reference/callback, pop, replace top, change priority/remove by handle when selected, heapify, drain | Compare borrowed T; exchange slots then take a boundary value; every sift step progresses along an index; arithmetic domains and child bounds proved independently of comparator laws | O(log n) sifts and O(n) bottom-up heapify; no aggregate owner transfer per level. Count reverse-index repairs in an indexed queue. |
| Ordered map | Pool-indexed B-tree nodes with fixed-capacity key/value and child windows; compare owned Box-linked nodes | Find, insert, split/promotion, replace, remove, borrow/merge, root contraction, ordered iteration, cleanup | Node and child bounds; distinct node indexes at simultaneous writes; rotations return/move all owners; no key duplication assumed; index stack for paths | O(log n) node visits at balanced height, O(B) local shifts, O(n) full traversal. Bound retained path storage; charge pool validation and node fragmentation. |

The map witness in
[owning-behavior.wf](../../../tests/programs/containers/owning-behavior.wf)
is useful existing code but binds `K: drop` and fixes its payload to a Box.
It does not establish the unbounded K/V row above. The
[priority witness](../../../tests/programs/containers/priority.wf) is a fixed
u64 queue, and [ordered.wf](../../../tests/programs/containers/ordered.wf)
is explicitly one leaf split. Each has a narrower claim than a reusable library.

### Vector: same order without quadratic drain

An ordered drain can first swap symmetric elements, then repeatedly take the
last element and call a monomorphized consuming behavior. The probe consumes
four nodrop Items in the sequence 1,2,3,4 and empties the storage. The callback
receives its own environment and the current element, not access to the vector;
its effect boundary must exclude observing the reordered remaining contents.
No callback can store a reference under REF-3. This candidate does not promise
that partially processed contents stay in original order while a callback is
running, nor a pause/resume drain object.

At the abstract element-transfer level a conventional swap uses three moves.
For n elements the reverse-then-pop algorithm uses at most
`3*floor(n/2) + n` moves, against `n*(n-1)/2` shifts plus n extractions for
repeated front removal, and n extractions for a native queue-front drain.
For n=4096 these are 10,240, 8,390,656 and 4,096 respectively. These counts
exclude helper/result ABI transfers, callback work and optimizer elimination;
they are not timings. Slots can meet O(n) today, but this does not claim the
smallest possible constant or solve final-slot construction.

Reserve has a source size ceiling even though heap allocation has no refusal
arm. A capacity limit is a quantity/representation contract, not an OOM
protocol. For a concrete u64 backing, `requires count <= 1024_u64` makes the
count multiplication plainly representable. The merged generic library takes
an explicit const ceiling, with each T/ceiling instance checked against OP-9.
This filters supported instances; it is not a target-independent runtime
`sizeof<T>` query and not a blanket proof for every possible T. The public
choice between a caller-proved maximum and a genuine application Full outcome
must be explicit. Never add a fake OOM or impossible fallback to satisfy OP-9.

### Deque: endpoints and span consumers are different contracts

The ordinary growth candidate allocates a larger Ring, appends the old logical
window into it, then swaps the owning Box into the deque and consumes the now
empty old backing. For nodrop T the zero-length fact must reach the owned
backing that `free_empty` consumes; after a swap or an ordinary owned helper
return, check whether its declared contracts preserve that fact. An explicit
length reread is a possible validation cost; silently dropping the old owner
is not. The generic library instead uses the explicit consuming rebase in the
v0.63 trial below; this reference-based exchange is not its growth contract.

The merged append row now also publishes
`destination.len >= entry(source).len`. That is useful when the destination
starts empty, but it still neither publishes the two-entry sum nor transports
the old backing's zero-length fact across a later owner exchange. Reuse the
stronger existing row before proposing any additional contract mechanism.

The [linear-ring-publish probe](../../experiments/container-representation/x1/linear-ring-publish.wf)
tests another route: append first, then atomically call an ordinary helper
that requires the old backing empty, consumes it, and returns the new Box.
The compiler rejects its `set deref(values) = publish(...)` under WIN-3.
That agrees with the active text: OP-12 admits its atomic form only for affine
and copy targets, and WIN-3 refuses assignment to a linear target. Neither an
unbounded T nor a nodrop instance qualifies. Thus this route is unavailable
under the current rules even though no old element would be implicitly dropped.
This does not prove that every ordinary growth algorithm is impossible.

The candidate's atomic paragraph and the tree describe admission without that
class restriction; the candidate also generally refuses linear assignment.
X1-P3 below therefore asks the owner to clarify their intended scope, rather
than treating the omitted qualifier as proof that linear updates were selected.
No broadened atomic rule is assumed here. A by-value rebase consuming and
returning a genuinely new backing, or a different deque representation, is a
separate interface candidate if the restriction is intentional. The first
library trial must settle a complete nodrop growth route before claiming an
unbounded growable Deque.

REF-4 refuses *all* Ring range references. Therefore a function
that accepts two `&[T]` extents in queue order cannot obtain them directly
from Ring, even after testing that a subrange does not wrap.

For a byte queue, or another copy T, an ordinary fully initialized Array plus
head/count can implement the physical ring. It may pass the tail and beginning
as two local range arguments without relocating data. The copy-ring-spans
probe exercises the ordered physical ranges; it is not a complete queue.
Initialization of spare capacity is an extra cost, paid at construction/growth,
and must be compared with the actual native consumer contract. A native
uninitialized spare-byte implementation must not be charged artificial zeroing
merely to make the WF comparison agree.

For arbitrary owning T there is no universal filler. An Array of enums gives
ranges of enums, not ranges of T. The available alternatives are single-slot
visitation on Ring or O(n) transfer into contiguous Slots; those do not meet a
strict zero-copy two-span contract. This is the exact scoped interface limit,
not evidence that queues in general require a language amendment.

### Slab: reuse, addresses, and retained membership

The original enum representation sketch was:

```wf
struct Handle {
  index: u64;
  generation: u64;
}

enum Cell<T> {
  CellEmpty(generation: u64, next: Option<u64>);
  CellLive(generation: u64, value: T);
  CellRetired();
}

struct Slab<T> {
  cells: Box<Slots<Cell<T>>>;
  free_head: Option<u64>;
}
```

Remove swaps the indexed Cell with a valid local vacancy, consumes the returned
enum and hands its T to the caller. A slot whose generation reaches the maximum
is retired instead of wrapping; a reduced-width model should force this path
in the next implementation's tests. A handle is interpreted relative to the
supplied slab. Without an additional application identity field it does not
authenticate a different slab. These are ordinary data contracts under OP-13,
not new brands or memory-safety authority. The v0.63 insertion trial below
exposes the missing variant fact after the reverse exchange; the implemented
Slab therefore uses a one-element window in each cell. The sketch alone is
not a complete nodrop implementation.

`find_index` may return `Result<u64, unit>` with
`ensures when Ok(value: i): i < deref(slab).cells.inner.len;`.
Generation and variant are validated on the path before access. A callback
receives a locally formed `&T`; REF-3 precludes returning or storing it. FN-9
does not currently export arbitrary indexed-field or variant relations, so a
second helper may need another read/match. Price that cost on a lookup chain;
do not describe it as lost runtime state or a need for quantified occupancy.

Stable slots mean stable index/generation, not stable addresses. Growing a
window of inline records moves their storage. A window of Box records keeps
pointee addresses but spends one allocation per record; source references still
obey REF-2 and do not survive merely because machine addresses happen to agree.

Two memberships have two different valid application contracts:

- **Weak indexes:** the central slab owns the object; either index may outlive
  deletion and later report Expired. Generation checks give this cheaply.
- **Retained indexes:** deletion checks an ordinary member count and returns
  Busy while any membership remains. One composite owner updates indexes and
  counts together; explicit consuming tickets can prevent accidental ticket
  loss. Bounds and checked counters protect memory/overflow, but the current
  language does not prove that arbitrary separately authored functions maintain
  the global count-to-index relation. Tickets are not unforgeable capabilities
  merely because a source struct is nodrop. Ordinary constructors and direct
  mutations remain available.

Thus a checked WF protocol with defensive validation is a viable trial, but
an unrestricted library has no demonstrated static retained-membership theorem.
The existing [Rust control](../../experiments/container-representation/authority/RESULTS.md#two-indexes-and-object-lifetime)
distinguishes the contracts, not WF acceptance. The next trial must include
wrong index/generation, removal from one index, a remaining reader, final
membership retirement, and final store destruction.

### Hash map: representation before occupancy machinery

Start the complete generic trial from a valid slot enum:

```wf
enum MapSlot<K, V> {
  EmptySlot();
  DeletedSlot();
  FilledSlot(key: K, value: V);
}

interface Key<K, E> {
  fn hash(env: &E, key: &K) -> hash: own u64 reads(env), reads(key);
  fn equal(env: &E, left: &K, right: &K) -> equal: own Bool reads(env), reads(left), reads(right);
}
```

An insertion returns its previous slot or the replaced key/value pair, so a
duplicate need not drop an unbounded K. Removal returns an owned outcome.
Final cleanup takes every materialized slot and consumes each active payload,
then frees the empty window. The vacancy-exchange probe establishes one of
these operations, not the entire rehash protocol.

Hash/equality are not assumed reflexive, consistent, terminating or constant
time. Conditional on callbacks returning, public probe loops stop after
capacity inspections; comparator/equality answers cannot authorize an
out-of-bounds access or an ownership loss. Existing entries can be transferred
by occupancy rather than re-deduplicated with a newly inconsistent equality.
Rehash needs an ordinary owner-preserving insertion protocol. A supposedly
impossible Full arm that invents a value, leaks the pending entry, or silently
loses it is not an implementation. Complete checking of that protocol remains
part of the library trial; no per-slot proof is assumed here.

Let C be table capacity, n occupancy, P the entry stride, and I an index-slot
stride. These are comparison models, not promises of WF ABI layout:

| Representation | Storage to count | Rehash/mutation consequences |
| --- | --- | --- |
| Inline enum slots | C times the actual enum stride, including discriminant/padding; optional fingerprint storage separately | Rehash moves live K/V payloads. One backing, direct payload access. |
| Enum/optional Box slots | C times slot stride, plus n allocations of P and allocator overhead | Moves pointers during table rebuild; dependent payload load and allocation/free costs. No assumed Option niche. |
| Sparse indexes plus dense entries | C*I plus reserved dense capacity*P plus reverse-location metadata | Table-only rehash can leave payloads in place; deletion repairs the moved last entry's index. Dense growth still relocates payloads. |

Compare all three at the same occupancy and identity contract. The independent
[hash-slot study](https://github.com/mbbill/Whitefoot/blob/38c28403a2defd0b65b8a2ab2b5e4794315e9940/research/experiments/hash-slot-occupancy/RESULTS.md)
did not meet its registered criterion for a recurring tag-check tax and found
more consistent costs for boxing in its large-table cells. It is a native C
representation study with some noisy controls, not WF emitted-code evidence or
a comparison against SIMD-group probing. It argues against adding a projected
layout solely because one branch exists, not for a universal winning layout.

#### Generic owning-map trial after the Ring comparison

The reusable-map trial starts from `e6349b80`, kernel v0.67. Its contract is a
map over arbitrary owned K and V, including must-consume values, with hash and
equality supplied through the existing `interface`/`binding` mechanism. Neither
key nor value requires a separate Box. Collision insertion, replacement that
returns the previous pair, lookup past deletion, removal, slot reuse, growth
and rehash, visitation, and explicit final consumption belong to the same
operation chain. A caller-selected capacity ceiling may return the offered
pair; allocation itself has no source-visible refusal under STOR-8. Returned
references, persistent iterators and stable payload addresses are not part of
this contract.

Borrowed value editing belongs beside read-only lookup: a callback may read
the stored key, write the selected value and its disjoint environment, and
return an owned result. Without that form, a counter or wide record update
would require replacing or removing and reinserting the complete value merely
because references cannot escape. Ordinary effect rows already express this
boundary. Validate value-only edits, missing keys, owning children, and unchanged
map extents; no key mutation or retained reference is implied. Compare a small
scalar and wide-record edit trace with the same native callback contract before
claiming that it removes a measured cost.

Before using measurements to select a representation, hold the operation
trace, supplied behaviors, occupancy, capacity policy and ownership outcomes
fixed. Compare ordinary enum slots with dense entries plus sparse indexes;
the Slab's one-element window is an available occupancy encoding, not a
preselected map layout. Count complete backing and result layouts, sparse and
dense reserved capacity, allocations, probe work, relocation during growth,
and helper-boundary transfers. Use both ordinary optimization and retained
helpers, scalar and wide inline values, and a native C control for the same
contract. A layout improvement must survive inclusion of reverse-index repair
and dense-growth costs; reducing table bytes alone does not select it.

The first matched trace uses exact runtime capacities, no cached hash, and
rehashes each live key through the supplied behavior. Reserve names its target
capacity. An insertion that exhausts its bounded probe grows geometrically,
saturating at the caller's ceiling; zero capacity grows to one. Replacement
still succeeds at that ceiling. Steady-state comparisons use half-full and
seven-eighths-full tables, with growth measured separately. This simple shared
policy isolates the representation comparison; it does not select an optimal
production load factor. The native sparse floor may rebuild directly into a
new backing and initialize only empty tags. It is not charged WF's temporary
planning arrays, whole-slot swaps, or zeroed inactive payload bytes.

The correctness discriminator includes hostile equality, collision-heavy and
full-table traces, zero capacity, replacement at the capacity ceiling, and
cleanup after every owner-returning outcome. No equality law justifies a
bound or permits a value to disappear. Rehash must retain all entries without
asking equality to deduplicate them. If an ordinary formulation fails, retain
its exact source and separate a specified limit from a compiler defect before
changing either interface or representation.

The sparse rehash candidate plans destinations in copyable metadata, then
grows the existing owning backing and permutes complete slots through `swap`.
This avoids discarding a temporary enum on an unproved vacancy assumption.
The dense candidate rebuilds only sparse metadata before publication, leaving
its separate dense owners in place. Both routes need checked complete source;
neither is selected on an assertion that the other is inexpressible. Temporary
planning allocations and payload relocation are part of the sparse candidate's
cost, just as reverse-index maintenance and the dependent lookup are part of
the dense candidate's cost.

A third admitted ordinary formulation gives each sparse cell
`Slots<Pair<K,V>, 1>` and a deleted flag. After
allocating an empty destination with at least the old materialized capacity,
swap the backing, drain each old cell, and append its single live pair into
an empty destination window. The ordinary append postcondition establishes
that the local source is empty, so cleanup needs no assumed enum refinement.
Hash each owner once; equality is unnecessary. At owner j, at most j-1 of the
M destinations are filled, so a complete cyclic scan finds a vacancy when
j <= old capacity <= M. The complete source and its scalar/owning-child
caller execute in default, sequential and parallel CLI configurations. The
allocation observer checks all eighteen identities, each released exactly
once; a zero-capacity rebuild allocates nothing. The vacancy argument is
not a published numeric contract or a general compiler proof of termination.

That route avoids permutation plans and the bulk old-backing grow copy, but
costs an extra word per cell: 32 rather than 24 bytes for the scalar pair,
280 rather than 272 for the wide pair. It also retains both payload backings
during migration. Ignoring fixed headers, growing C to M has peak bytes
`(C+M)*(B+8)`, versus the enum route's `(C+M)*B+8*M`: about 8*C more bytes.
Same-capacity rebuilding needs a second full payload backing rather than
only the enum route's metadata. Empty-cell construction and local cell/append
transfers remain costs to inspect. Its comparison criterion is material
copying/permutation cost or a split between sparse lookup/edit and dense
growth. Compare both growth and same-capacity rebuild, scalar and wide values,
with retained helpers. Do not reject it as inexpressible or infer a smaller
peak from fewer allocations.

The first matched timings trigger that additional comparison. At capacity
4096 and seven-eighths occupancy, the first cohort's wide growth trace takes
about 0.70 of the sparse time for dense storage under normal optimization,
and 0.74 with retained helpers, while ordinary sparse lookup and edit win
elsewhere. The same sparse growth is about 1.78 times its direct native
control but 1.21/1.04 times its planned-algorithm C control. The
[comparison record](../../experiments/container-representation/map-library/RESULTS.md)
owns the complete paired samples and both cohorts; these particular results
motivate a discriminator, not a layout selection. Extend the ordinary
one-slot route only far enough to compare its admitted owning operation chain
and matched growth/rehash traces. Count its larger cells and double-backing
peak, as well as actual transfer work. Select it only if the observed benefit
justifies those costs for the exposed contract; do not extend the full timing
matrix merely because a third representation exists.

The bounded discriminator uses only scalar/wide growth and same-capacity
rehash at capacity 4096 with 3584 entries, under the original seeds, complete
traces, optimization modes and reversed cohorts. Draining the old window
visits buckets in descending order, whereas the original direct C floor
visits ascending buckets. Retain that floor, then separate descending enum
storage, descending one-slot storage, the source's cell-take/append algorithm
in C, and the WF source. Keep the original sparse/dense implementations in
the same run as references. This prevents a changed insertion order or
source migration algorithm from being attributed solely to layout.

The extra per-bucket word is not yet established as necessary for direct
migration. An additional source discriminator keeps the enum buckets and
uses one local `Slots<Pair<K,V>,1>` as the pending-owner carrier: pop and
match the old slot, stage its pair, probe by occupancy, exchange at the
vacancy, and restage any complete pair the exchange returns. An empty
carrier ends that owner's migration. The earlier PROV-6 rejection concerned
an unconsumed displaced enum, not this exhaustive ownership protocol.
That formulation admits and executes against the unchanged scalar and
owning-child callers under sequential lowering. Their exact allocation
ledgers fall from 29/14 to 15/12, each identity released once: six initial
backings, five growths and four same-capacity rebuilds for the scalar chain;
ten children and two backings for the owning chain. The local staging window
allocates nothing. It therefore joins the same four rebuild cells before
charging direct migration the larger bucket layout. Each attempt hashes
the current staged key and never uses equality to deduplicate. Transfer
costs and paired timings are recorded in the same comparison: scalar growth
improves, while wide growth retains substantial staging and result work.
Admission and the conceptual vacancy argument alone select neither
implementation nor representation.

The optimized migration path exposes one further source choice before
promotion: it calls the general exchange helper even though migration needs
only to consume the displaced enum, not to construct a public insertion
result. A bounded alternative puts the same swap and exhaustive enum match
inside rebuild, restoring any displaced pair to the same pending window.
It may remove a wide intermediate result and let ordinary optimization use
the just-observed vacancy; no variant is discarded on that observation.
Select this form only if the unchanged owning and hostile-behavior callers
pass, its emitted migration removes the result construction without adding
storage, and the same four rebuild cells support the cost improvement.
The public insertion interface and helper-retention policy stay fixed.

The direct route exposes a separate contract boundary. After extending a
fresh `previous` backing to `capacity`, it publishes that owner with
`swap(first: &deref(map).cells, second: &previous)`. PRE-1 declares only
`writes(first), writes(second)` for swap. OP-11 exchanges both values and
keeps their ownership live, but MSR-3 does not define swap as a measure-fact
placement, and CALL-6 has no declared postcondition to publish. Consequently
the previous `previous.inner.len == capacity` proof does not establish
`deref(map).cells.inner.len == capacity` afterward. Re-reading the actual
extent permits bounded migration; it does not establish an entry/exit
monotonicity promise. This is the current proof contract, not a lost owner or
an observed lowering defect. Keep that limitation in the existing contract
backlog rather than add a runtime branch solely to satisfy an ensures.

Result layout is a separate source choice. The initial common result has three
variants, `Inserted`, `Replaced(previous: Pair<K,V>)`, and
`Full(offered: Pair<K,V>)`. Current product layout reserves both Pair regions.
An ordinary alternative is `Inserted | Returned(reason: ReturnReason,
pair: Pair<K,V>)`, with tag-only `Replaced / Full` reasons. It preserves every
ownership outcome while sharing one payload region, at the cost of a nested
match. For an eight-byte-aligned Pair of 16 or 264 bytes, the current layout
calculation predicts 40 or 536 bytes for the original and 24 or 272 bytes for
the shared form. A zero-byte Pair instead grows from four to eight bytes; this
is not an unconditional layout win. These are layout deductions, not timings.
Compare the same replacement and refusal/retry consumers under both source
forms before choosing either the public result or a compiler optimization.
Keep initialization, construction and consuming projection transfers separate
from the result's reserved width. The native control already has one Pair
region, so its matching semantic outcome alone does not establish ABI parity
with the initial WF result.

The first reusable library uses the compact returned-pair result and direct
enum migration through the shared exchange helper. Its complete
[public caller](../../../tests/programs/containers/hash-map-program.wf)
uses no representation fields and executes collision, replacement after a
tombstone, removal, reuse, bounded growth, rehash, borrowed editing and final
consumption. Separate key/value identities distinguish returning the old pair
from merely returning an equivalent offered key. Must-consume children,
zero-sized pairs, inconsistent equality and a fresh owned callback result
exercise the same source. The allocator observer accounts for seventeen
backings, ten child Boxes and the callback's returned Box, each released once.
The formal program test bundles the actual library; it has no research input.

The source selection has two distinct grounds. Sharing the public result's
payload reduces the measured wide replacement and churn cost as well as its
reserved width. Direct enum migration removes the planning arrays and avoids
the extra word in every one-slot bucket. The selected helper source's wide
growth trace is about 0.93 of the planned sparse source, but remains
1.64--1.69 times the direct C floor with the same descending migration
direction and 1.25--1.34 times the dense WF source. Same-capacity rehash also
retains two complete backings. These are reasons to preserve dense storage and
lowering improvements as measured opportunities, not to call this a universal
fastest map. The C floor, source-shaped C, raw paired samples and peak-byte
accounting remain separate in the
[comparison](../../experiments/container-representation/map-library/RESULTS.md#constant-interface-comparison-and-selected-helper-body).
No application-frequency distribution is inferred from the test matrix.

The inline-exchange criterion above was **not fully met**. Holding the compact
public interface fixed removes the private result's stack slot, call and
Inserted clearing, but reversed ABBA source controls do not establish a stable
independent timing benefit after the unchanged C controls' variation is
accounted for. The library therefore retains the shared exchange helper. The
inline source and its measurements remain a replayable rejected alternative,
not the delivered library or evidence that the removed clear alone costs the
observed timing difference. No compiler implementation, source-language rule
or conformance evidence is changed. The adopted
[HashMap storage decisions](../../../design/language/data-model/hash-map-storage.md)
record the enum-bucket/shared-helper and compact returned-pair choices;
the proof-contract and remaining performance limits stay in
[`docs/todo.md`](../../../docs/todo.md).

The following complete controls retain the trial's proof boundaries under
kernel v0.67 and the compiler identified in the comparison's
[build identities](../../experiments/container-representation/map-library/RESULTS.md#sample-and-build-identities).
Each standalone program and each listed variant was checked by guarded LLVM
emission with that executable; apply variants independently to their stated
baseline. The reported rejection is the first diagnostic, not a claim that
every alternative formulation fails. For a standalone block saved as
`control.wf`, the invocation is:

```sh
perl .github/run-check.pl map-proof-control compiler/target/gate/whitefootc --emit-llvm control.wf -o control.ll
```

**Contract formation and publication.** This complete baseline accepts:

```wf
struct Counter {
  length: u64;
}

fn scalar(map: &Counter) -> result: own unit writes(map) {
  set deref(map) = Counter(length: deref(map).length);
  return unit;
}

fn append_proof(values: &Slots<u64, 1>, value: own u64) -> outcome: own Result<unit, unit> writes(values) contract {
  requires deref(values).len < deref(values).cap;
  ensures deref(values).len == deref(entry(values)).len + 1_u64;
} {
  place_back(window: values, value: value);
  return Ok<unit, unit>(value: unit);
}

fn replace(cells: &Box<Slots<u64>>) -> result: own unit writes(cells) {
  let previous = box_slots_new::<u64>(capacity: 1_u64);
  place_back(window: &previous.inner, value: 7_u64);
  invariant prepared: previous.inner.len == 1_u64;
  swap(first: cells, second: &previous);
  return unit;
}

fn main() -> status: own ExitStatus pure {
  return exit_status(code: 0_u8);
}
```

The exact variants are:

1. Replace `scalar`'s first line with these three lines:

   ```wf
   fn scalar(map: &Counter) -> result: own unit writes(map) contract {
     ensures deref(map).length >= deref(entry(map)).length;
   } {
   ```

   The first diagnostic is FN-9 `InvalidPostconditionRelation` at that clause.
   FN-9 gives exit-state denotation to a written reference's measures, and
   MSR-3's `entry` adds no scalar snapshot family. Ordinary counter reads and
   writes remain available.
2. In `append_proof`, replace its ensures with
   `ensures when Ok(value: success): deref(values).len == deref(entry(values)).len + 1_u64;`.
   The first diagnostic is FN-9 `InvalidPostconditionSelector` at `Ok`.
   FN-9's routed form requires an integer success payload. The accepted
   baseline instead publishes an unrouted exit-measure relation from a
   `Result<unit, unit>` function; such relations are also legal on a plain
   unit-returning function. Unit results do not prohibit ensures generally.
3. Add the enum below, change `append_proof`'s result type from
   `Result<unit, unit>` to `DenseProofPut`, replace its ensures with
   `ensures when DenseProofReplaced(previous: old_value): deref(values).len == deref(entry(values)).len + 1_u64;`,
   and replace its return with `return DenseProofReplaced(previous: value);`.
   The first diagnostic is FN-9 `InvalidPostconditionSelector` at
   `DenseProofReplaced`. FN-9 admits only the prelude `Result.Ok` route, so a
   custom `Inserted / Replaced / Full` result cannot publish a relation for
   each outcome. An unconditional insertion interval alone does not prove
   that the Full arm preserves length before a retry.

   ```wf
   enum DenseProofPut {
     DenseProofInserted();
     DenseProofReplaced(previous: u64);
   }
   ```

4. Replace `replace`'s first line with these three lines:

   ```wf
   fn replace(cells: &Box<Slots<u64>>) -> result: own unit writes(cells) contract {
     ensures deref(cells).inner.len == 1_u64;
   } {
   ```

   The first diagnostic is FN-9 `UndischargedPostcondition` at its return,
   with relation `deref(cells).inner.len = 1` and disposition `Unproved`.
   PRE-1 declares swap's writes without an ensures; ENT-5 kills the supported
   facts, MSR-3 has no swap placement, and CALL-6 has no relation to publish.
   Starting from this variant, replacing only the swap statement with
   `set deref(cells) = move previous;` accepts through MSR-3's ordinary
   placement. That control discards the old destination owner; it does not
   implement the map's exchange-and-migrate algorithm.

**Equality at a call after a counted loop.** This complete baseline accepts:

```wf
struct Holder {
  cells: Box<Slots<u64>>;
}

fn preserve(cells: &Box<Slots<u64>>) -> result: own unit writes(cells) contract {
  requires deref(cells).inner.cap <= 1_u64;
  ensures deref(cells).inner.len == deref(entry(cells)).inner.len;
  ensures deref(cells).inner.cap == deref(entry(cells)).inner.cap;
} {
  let capacity = deref(cells).inner.cap;
  grow(cell: cells, capacity: capacity);
  return unit;
}

fn apply(cells: &Box<Slots<u64>>, plan: &Box<Array<u64>>) -> result: own unit writes(cells), writes(plan) contract {
  requires deref(cells).inner.cap <= 1_u64;
  requires deref(plan).inner.len <= deref(cells).inner.len;
  requires deref(plan).inner.len >= deref(cells).inner.len;
} {
  grow(cell: cells, capacity: 1_u64);
  if deref(plan).inner.len > 0_u64 {
    set deref(plan).inner[0_u64] = 0_u64;
  }
  return unit;
}

fn nested(holder: &Holder, capacity: own u64) -> result: own unit writes(holder.cells) contract {
  requires deref(holder).cells.inner.cap <= 1_u64;
  requires deref(holder).cells.inner.len == capacity;
  requires capacity <= 1_u64;
} {
  let plan = box_array_filled::<u64>(count: capacity, value: 0_u64);
  for (
    index in 0_u64..capacity,
    invariant retained: deref(holder).cells.inner.len == capacity,
    invariant planned: plan.inner.len == capacity,
    invariant bounded: deref(holder).cells.inner.cap <= 1_u64
  ) {
    preserve(cells: &deref(holder).cells);
  }
  invariant equal_extent: plan.inner.len == deref(holder).cells.inner.len;
  apply(cells: &deref(holder).cells, plan: &plan);
  return unit;
}

fn main() -> status: own ExitStatus pure {
  let cells = box_slots_new::<u64>(capacity: 1_u64);
  place_back(window: &cells.inner, value: 7_u64);
  let holder = Holder(cells: move cells);
  if holder.cells.inner.cap <= 1_u64 {
    if holder.cells.inner.len == 1_u64 {
      nested(holder: &holder, capacity: 1_u64);
    }
  }
  return exit_status(code: 0_u8);
}
```

Replace only `apply`'s paired length requirements with
`requires deref(plan).inner.len == deref(cells).inner.len;`. The first
diagnostic is FN-8 `UndischargedCallRequirement` at the call to `apply`, with
instantiated goal `plan.inner.len == deref(holder).cells.inner.len` and
disposition `Unproved`. Removing only that call from this rejected variant
accepts, including the immediately preceding `equal_extent` invariant.
INV-1 splits equality into two affine inequalities; ENT-6's affine
normalization of signed FN-8 goals admits ordering leaves only. Ordinary
L0 equality remains available, but these affine premises do not take that
route. The paired requirements preserve exact equality without runtime work.

**Conditional loop preservation.** This complete source rejects:

```wf
fn preserve(values: &Box<Slots<u64>>) -> result: own unit writes(values) contract {
  requires deref(values).inner.cap <= 1_u64;
  ensures deref(values).inner.len == deref(entry(values)).inner.len;
  ensures deref(values).inner.cap == deref(entry(values)).inner.cap;
} {
  let capacity = deref(values).inner.cap;
  grow(cell: values, capacity: capacity);
  return unit;
}

fn conditional(values: &Box<Slots<u64>>, enabled: own Bool) -> result: own unit writes(values) contract {
  requires deref(values).inner.cap <= 1_u64;
  ensures deref(values).inner.len == deref(entry(values)).inner.len;
} {
  let count = deref(values).inner.len;
  for (
    index in 0_u64..count,
    invariant retained: deref(values).inner.len == count,
    invariant bounded: deref(values).inner.cap <= 1_u64
  ) {
    if enabled {
      preserve(values: values);
    }
  }
  return unit;
}

fn main() -> status: own ExitStatus pure {
  let values = box_slots_new::<u64>(capacity: 1_u64);
  place_back(window: &values.inner, value: 7_u64);
  let enabled = True();
  conditional(values: &values, enabled: enabled);
  if values.inner.len != 1_u64 {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
```

The first diagnostic is INV-1 `UndischargedLoopInvariant`, name `retained`,
obligation `Backedge`, with diagnostic relation `deref(values).len <= count`.
Adding `invariant restored: deref(values).inner.len == count;` immediately
after `preserve` inside the true arm leaves that same first rejection.
Replacing the entire loop body with the following accepts:

```wf
let before = deref(values).inner.len;
let before_capacity = deref(values).inner.cap;
if enabled {
  preserve(values: values);
}
invariant rejoined: deref(values).inner.len == count;
invariant rejoined_capacity: deref(values).inner.cap <= 1_u64;
```

Removing only the `before_capacity` binding and `rejoined_capacity` invariant
from this accepted body moves the first rejection to INV-1's `bounded`
backedge obligation, relation `deref(values).cap <= 1_u64`. Replacing the
original loop body with the unconditional `preserve(values: values);` also
accepts. ENT-5 joins closed L0 relations over every reaching branch; ENT-6
retains only canonically identical affine inequalities over their immutable
images. Capturing both current measures supplies connections across this
small conditional. The rejected form does not establish a general inability
to preserve measures through conditionals.

The full sparse candidate remains a separate unresolved case. Use
[sparse-map.wf](https://github.com/mbbill/Whitefoot/blob/c206655d898ea32de9995bca2b19444c92c1f973/research/experiments/container-representation/map-library/sparse-map.wf)
and its
[sparse-check.wf driver](https://github.com/mbbill/Whitefoot/blob/c206655d898ea32de9995bca2b19444c92c1f973/research/experiments/container-representation/map-library/sparse-check.wf)
from `c206655d898ea32de9995bca2b19444c92c1f973`, bundled in that order for
`--emit-llvm`. In `sparse_map_rebuild`, replace only this loop-body statement:

```wf
sparse_map_clear_deleted::<K, V>(cells: &deref(map).cells, index: index);
```

with:

```wf
let deleted = sparse_map_is_deleted::<K, V>(slot: &deref(map).cells.inner[index]);
let current_extent = deref(map).cells.inner.len;
if deleted {
  sparse_map_canonicalize::<K, V>(cells: &deref(map).cells, index: index);
}
invariant unchanged_extent: deref(map).cells.inner.len == current_extent;
invariant rejoined_extent: deref(map).cells.inner.len == capacity;
```

Keep the existing helper and every other line. The first diagnostic is INV-1
`UndischargedLoopInvariant`, name `rebuilt_extent`, obligation `Backedge`,
with diagnostic relation `deref(map).cells.len <= capacity`. This reproduces
the full candidate's failure despite its written post-join bridges. Whether
that remaining refusal is required by the fixed proof rules or is a compiler
defect has not been established. The admitted candidate retains the
length-preserving helper around the conditional tombstone operation; that
helper does not relocate live payloads, and its call structure remains part
of the matched source-cost comparison. Neither this failed formulation nor
the admitted small control settles the full candidate's normative diagnosis.

The maintained [TODO](../../../docs/todo.md) remains the owner of unresolved
issues. This trial reopens must-consume sparse slot state, aggregate result
transfer costs, and any contract boundary its actual source crosses. Ring
two-span access, automatic reference-based rebase, ordered-drain movement,
header-plus-tail storage, retained membership, fixed-resource execution and
generic checking cost retain their own evidence and reopening conditions.
A passing map does not resolve those independent questions. New evidence
updates the existing entry rather than creating a second backlog here.

### Priority queue and ordered map

A priority queue compares two local references through an interface and swaps
their slots. Pop can swap root with the last slot, take the last slot, and sift
the new root. Unbounded T supports this spelling, including a copy instance.
With inconsistent comparison the returned order need not be meaningful, but
each sift must move strictly toward a parent or child and stay within checked
bounds. An indexed queue additionally repairs handle-to-position metadata on
every swap, matching the Kubernetes workload rather than a simpler bare heap.

For an ordered map, use a B-tree as the first move-only-key candidate: a split
*moves* the median key/value into a parent. A B+ tree that duplicates a leaf
key into an internal separator needs a copy key, an explicit cloning behavior,
or a distinct separator representation. The leaf-split u64 fixture cannot
establish that operation for arbitrary K. Full binary and B-tree alternatives
must compare allocation count, fanout, shifts and traversal cost, not just
whether one syntax is shorter.

Pool-indexed nodes make descent a scalar-ID loop under REF-1. Store an explicit
index path for rebalancing and reacquire references after growth or mutation;
avoid silently re-searching from the root after every iterator step. The early
snapshot refused loop-carried Box descent; current REF-1/REF-2 admit it, as
[owned_link_cursors.wf](../../../tests/programs/owned_link_cursors.wf) exercises.
Its affine list edit does not establish linear atomic replacement for arbitrary
owning K/V. Independent cursors under one descendant cover also do not acquire
equality or disjointness merely from matching suffixes. The current
[cursor design](../wildcard-path/DESIGN.md) supplies those distinctions; compare
actual pool, recursive and cursor algorithms before proposing more machinery.

### Ordered map trial at v0.68

**Registered before implementation.** Starting at `345e2966a`, this experiment
tested whether a complete arbitrary-K/V B-tree can retain ordinary ownership at useful
cost. The consumer is a reusable map and an ordinary writer that builds an
owning record index, replaces and edits records, removes keys, visits bounded
ranges and consumes the remainder. The existing scalar
[leaf split](../../../tests/programs/containers/ordered.wf) is only one component.
This trial selects boxed nodes before the family sketch's pool candidate to
avoid whole-pool relocation and a new node-ID protocol; measurements may reject
that choice. The owner subsequently adopted the completed baseline described
below; the initial ownership probe supports only its bounded
transfer and cleanup protocol, not yet the complete map.

Use one common pair region, fixed fanout, and an explicit root vacancy:

```wf
struct OrderedPair<K, V> { key: K; value: V; }
struct OrderedEntry<K, V, N> {
  pair: OrderedPair<K, V>;
  after: Slots<Box<N>, 1>;
}
struct OrderedNode<K, V> {
  leading: Slots<Box<OrderedNode<K, V>>, 1>;
  entries: Slots<OrderedEntry<K, V, OrderedNode<K, V>>, 15>;
}
struct OrderedMap<K, V, const ceiling: u64> {
  root: Slots<Box<OrderedNode<K, V>>, 1>;
  length: u64;
}
enum OrderedReturnReason { OrderedReplaced(); OrderedFull(); }
enum OrderedPut<K, V> {
  OrderedInserted();
  OrderedReturned(reason: OrderedReturnReason, pair: OrderedPair<K, V>);
}
interface OrderedKey<K, E> {
  fn compare(env: &E, left: &K, right: &K) -> order: i32 reads(env), reads(left), reads(right);
}
```

The generic `N` in `OrderedEntry` avoids a forward nominal reference: writing
`Box<OrderedNode<K,V>>` before declaring `OrderedNode` rejects as invisible
under TYPE-6 (the resolution diagnostic is classified TYPE-5). The later node
can instantiate that earlier entry with itself. This ordinary type argument
changes neither storage nor the ownership protocol.

The complete library and maintained caller using these shapes now admit and
execute as recorded below; the representation remains a cost candidate.
K/V have no copy/drop bound. The ceiling
limits logical entries; nodes grow by fixed-size Box
allocation, with no allocation-failure result. Replacement remains available
at the ceiling. A consistent comparator and environment determine map order;
negative/zero/positive results suffice, without a -1/0/1 requirement. Every
descent and local loop must terminate independently of comparison laws,
provided callbacks return. Environment-backed order is already exercised by
[the generic priority witness](../../../tests/conformance/cases/run-generic-priority-behavior.wf).

In the API shapes below, M abbreviates `OrderedMap<K,V,ceiling>`, E is the
comparison environment, and F is a disjoint callback environment. Generic
function-kind parameters follow the existing HashMap callback forms:

| Operation and typed arguments | Owned result and behavior |
| --- | --- |
| `ordered_map_new::<K,V,ceiling>()` | M, initially empty. |
| `ordered_map_len(map: &M)` | u64; observation requires no caller knowledge of node fields. |
| `ordered_map_put(map: &M, key: K, value: V, env: &E)` | `OrderedPut<K,V>`; insertion, old complete pair on replacement, or offered pair on logical Full. |
| `ordered_map_lookup(map: &M, key: &K, key_env: &E, visit_env: &F)` | `Result<R,unit>` from an observing callback, or absent. |
| `ordered_map_edit(map: &M, key: &K, key_env: &E, edit_env: &F)` | `Result<R,unit>` from a value-editing callback, or absent; the stored key stays read-only. |
| `ordered_map_remove(map: &M, key: &K, env: &E)` | `Option<OrderedPair<K,V>>`; caller owns every removed key and value. |
| `ordered_map_each(map: &M, env: &F)` | unit; visit every pair in order. |
| `ordered_map_range(map: &M, lower: &K, upper: &K, key_env: &E, visit_env: &F)` | unit; ordered half-open `[lower,upper)` visit, empty for equal/inverted bounds under lawful order. |
| `ordered_map_free(map: M, env: &F)` | unit; explicitly consume every remaining pair and release all nodes. |

Observe/visit callbacks read key and value and write their environment; edit
reads key and writes value/environment, and may return arbitrary owned R.
Consume takes owned K and V and writes its environment. Lookup/each/range read
the map; put/edit/remove write it. All comparisons read their explicit key and
environment paths. Each callback row is disjoint from other supplied paths
under EFF-5. No operation requires a borrowed result, stored reference, stable
node address or caller-proved internal shape invariant. Free's consume order
is unspecified; the ordered visitor is a separate operation.

The complete mutation chain is part of the experiment, including deletion:

- B-tree promotion moves a complete median pair, so it needs no cloned key.
  Each entry owns the subtree following its pair; the node's one leading link
  owns the preceding subtree. Upper entries move together with their links.
  Parent insertion shifts its filled prefix; re-form child selections afterward.
- Borrow exchanges the parent separator with a sibling boundary pair and
  swaps the carried entry's following link with the receiving or donating
  node's leading link. Prove distinct entry indexes below one captured parent,
  rather than trusting independent cursors.
- Merge first extracts the right child Box, then reads its local entry count,
  places the separator with that node's leading link and appends its entries
  into the left child. Append empties the local source; subsequent Box
  consumption/destructuring carries the empty fact to `free_empty`. Capacity
  depends on entry counts alone, without a separate child-count relation.
- Internal deletion takes a successor pair from the right subtree and then
  repairs underflow on return. An underflow at child zero borrows or merges
  with its right sibling; other children use their left sibling. Insertion
  similarly carries a promoted entry upward and splits a full receiving node.
  This bottom-up protocol keeps temporary ownership explicit. Root contraction
  extracts the root before inspecting its entry
  count. For an empty root, move its zero-or-one leading link into the now-empty
  root window and consume the emptied locals. Every detached owner is
  reinstalled, returned or consumed.
- Range traversal visits internal pairs as well as leaves, with O(h+k) node
  work for k results at height h under lawful order. Full visitation/cleanup
  is O(n); recursively consuming suffixes needs no stored reference stack.

The bounded probe isolates split, merge and root contraction; the maintained
caller supplies the complete insertion/deletion chain below. OP-12's
affine/copy update does not admit a linear `set link = f(move link)`. Swapping
through local None preserves owners but does not publish the vacant variant
needed to discard a linear temporary; explicit one-slot windows avoid that
assumption. MSR-3 does not transport nested measures through window extraction.
PRE-1's `split_off` does not publish the destination's exact transferred count;
reread measures and use real control flow where needed. REF-3 forbids escaping
references, and OWN-7/OP-11 retain their alias/ancestry obligations. Do not add
an impossible Some cleanup arm, silently drop an owner, or call an internal
shape refusal Full. If these API outcomes cannot express an actual required
path, report its exact source/rule boundary before altering the contract.

The original two-window candidate had `entries: Slots<Pair,15>` and
`children: Slots<Box<Node>,16>`. A generic nodrop probe of split, merge, root
contraction and complete cleanup admits and executes with counted
take/place/reverse transfers: LLVM admission 0.08 seconds, native construction
0.62 seconds, all 15 pairs consumed. Replacing its counted split with
`split_off(source: &deref(left).entries, index: 8_u64, destination: &deref(right).entries)`
rejects the written `ensures deref(right).entries.len == 7_u64` at FN-9
(0.03 seconds), since PRE-1 publishes no corresponding destination relation.
This is a contract limit, not a compiler defect. The counted version costs
additional movement, which the matched source control must retain.

Deletion also needs the bound on the separate child window after extracting
a node. A bound on its entry count alone supplies no relation to that other
window. With only the two entry-count bounds `left.entries.len <= 7` and
`right.entries.len <= 7`, removing the independent child-count requirements
from the otherwise admitted merge refuses this statement under FN-8:

```wf
append(destination: &deref(left).children, source: &deref(right).children);
```

It cannot prove the required
`right.children.len <= left.children.cap - left.children.len`; the actual
frozen-compiler check took 0.03 seconds. No language rule derives that relation
from the entries' lengths. The bundled candidate above removes this
particular capacity premise by attaching each following edge to its separator
entry. Its zero-or-one links are ordinary data, not stored references or a
new proof mechanism. Its nodrop split, sibling borrow/merge and root-contraction
probe builds in 0.62 seconds and executes in 0.44 seconds, consuming all 15
pairs on the expected merged branch. The full map subsequently passes the
independent operation-chain oracle below; cost measurements remain outstanding.
A two-window implementation with a genuine
checked operation protocol remains an alternative; an impossible failure arm
added only to discharge its missing relation does not establish that protocol.

| Alternative | Ground for retaining it, and present scope |
| --- | --- |
| Boxed AVL | Complete find/edit/replace/remove and range visitation via two one-slot links and a balance enum; rotations transfer Box owners. Per-entry allocation and dependent loads may lose to B-tree fanout, while wide payload occupancy may win. Include a bounded native comparison before claiming a default winner; no second WF library is budgeted initially. |
| Pool-indexed B-tree | Materialized vacant nodes can retain empty windows and copied child IDs, avoiding extraction/cleanup problems. Growth relocates materialized node storage and adds ID validation/reuse work. Existing Slab is bounded and its visit is read-only; it is not already this growing node pool. |
| B+ tree | A copied leaf separator requires copy/clone K; stable IDs to centrally owned keys avoid that bound but add ownership, indirection and separator-refresh protocols. Retain for a concrete scan/identity requirement, without assuming the scalar split generalizes. |

Current [target layout](../../../compiler/src/target.rs) gives both
`Option<Box<Node>>` and `Slots<Box<Node>,1>` 16 bytes; a native nullable pointer
is 8. Do not charge an extra word to Slots relative to WF Option. Repeated Pair
fields in Leaf/Branch variants duplicate storage under the current product
enum layout. For eight-byte-aligned pair size P, the bundled node is `15P+264`
bytes: 504 for P=16 and 4,224 for P=264, before allocation overhead. It spends
120 bytes more than the initial two-window node (`15P+144`). At 7--15 entries
per non-root node the new shape reserves about 72--34 or 604--282 bytes per live
pair. The lower-space two-window/direct C controls must not be relabelled as
layout-matched to this candidate.
A WF binary node with two links and a three-state balance enum is about P+40.
These are layout deductions; occupancy and timings remain measurements.

**Comparison budget and criteria, fixed before coding/timing.** Implement one
complete WF B-tree, a source-shaped C B-tree and a direct C B-tree. Both C
controls are required; compare ordinary optimization and a retained-public-helper
mode with public operations and callbacks kept out of line in every language.
Private node helpers remain ordinarily optimizable. Match comparator and
callback work, ownership outcomes and ceiling policy. The source-shaped control
matches WF headers/branches/transfers; the direct control may use efficient
native layout and movement. Add one complete native AVL within this matrix if
feasible; without it report B-tree costs without claiming a default ordered
representation. No fanout sweep, second WF tree or new language mechanism is
part of this first budget. Initial program construction is bounded to 120
seconds, complete correctness execution to 40 seconds and the timing matrix to
60 seconds, measured separately; compiler builds and the canonical gate have
their own recorded costs. Investigate any exceeded stage before extending its
budget, and record the reason before another run. These experimental budgets
never select source acceptance.

| Axis | Bounded selection |
| --- | --- |
| Payload | 16-byte scalar pairs and 264-byte inline pairs; separate owning/nodrop correctness instances. |
| Live count | 8, 256 and 4,096: one leaf and sizes forcing multiple levels with fanout 16. |
| Work | Build; balanced hit/miss lookups; fixed-cardinality remove/reinsert and replacement churn; bounded range visits; final cleanup. Identical deterministic operation streams and callback digests across controls. |
| Sampling | One warm-up and five paired samples with alternating execution order; shared batching, build time separate. Investigate abnormal stage cost; repeat only for unresolved variance or changed code. |
| Accounting | Execution time, allocations, node occupancy, reserved/peak bytes, pair/pointer transfers, retained guards and generated helper code. |

Reject whole-node copying per descent, quadratic full traversal/cleanup, or
comparator-dependent termination before interpreting timings. Split/merge work
must stay O(B), with no relocation of unrelated nodes. A representation
dominated across this matrix lacks default-selection grounds; explain mixed
results by workload instead of declaring universal native parity or an upfront
winner. Any proposed compiler/proof improvement needs the completed ordinary
consumer and an isolated material cost or exact rule discrepancy.

The first admitted source leaves two concrete insertion costs for that
comparison: it searches for replacement before a second descent for an absent
key, and passes the offered Pair by value through recursive insertion. The
source C control must preserve both. If either materially separates it from
the direct control, a single descent could combine replacement with insertion,
while an ordinary one-slot Pair carrier could keep descending arguments
pointer-sized and take the owner only at the leaf. These are source-level
alternatives, not a request for a new mechanism. The first must retain
replacement at the logical ceiling and return an absent offered owner without
mutation; the second must prove its entry/exit length through each recursive
helper, or use an ordinary occupancy outcome without assuming a recursive
postcondition. Neither alternative is selected by admission alone.

Correctness uses an independent sorted oracle and exact owner accounting:
zero/one entries, root splits, both borrow directions, merges, internal-key
deletion, contraction to empty, reuse, equal-key replacement with distinct
owners, edit returning an owned value, empty/inverted ranges and partial final
cleanup. Hostile comparisons check bounded progress and ownership, not sorted
semantics. The owning/nodrop chain must cover every operation above.

The complete [library](../../../lib/std/collections/ordered_map/ordered-map.wf) and
[maintained caller](../../../tests/programs/containers/ordered-map-program.wf)
now pass that chain through the existing container corpus harness. The
independent sorted-array oracle observes each mutation and traversal; owning
keys/values, an owned edit result, refusal/retry and hostile comparators share
an exact serial ledger. Sequential and parallel lowering both pass ordinary
execution and dirty/quarantined allocation observation. Each observer records
exactly 103 allocations released once: 22 scalar tree nodes, six owning nodes
and 75 payload Boxes. The original owning trace consumes serials zero through
38 once; a separate below-ceiling leaf/internal replacement trace consumes
serials zero through 35 once, checking old owners before final cleanup. The
observer's existing bounded record array grows from 64 to 256 entries to hold
the longer trace; its release and misuse checks are unchanged. No compiler
change or specification amendment was needed.

Clean gate-profile Rust harness construction took 48.84 seconds including its
guard. The focused corpus test then took 5.05 seconds: 1.174 seconds WF
compilation, 2.765 seconds native construction and 1.101 seconds across the
four executions. These establish the selected operation/ownership observations;
they do not establish native parity, all structural branch counts or a default
representation. The expanded replacement caller subsequently passed the same
four modes in 5.68 seconds after a 3.94-second incremental Rust construction.
The complete [matched cost matrix](../../experiments/container-representation/ordered-library/RESULTS.md)
now preserves 1,152 samples including warm-up: wide-pair costs remain material
against direct C and AVL, while the scalar/range results are mixed. The complete
canonical gate remains a separate validation stage.

The owner adopted the fanout-16 boxed B-tree as the reusable measured baseline,
as recorded in the [ordered-map storage decision](../../../design/language/data-model/ordered-map-storage.md).
Both later insertion candidates remain rejected under their registered criteria.

**Design suitability.** Packed boxed nodes trade more balancing source for
fewer allocations and avoid pool-wide movement. Bundling edges with separator
ownership removes a cross-window capacity premise at an explicit space cost;
the measurements bound the adopted baseline's costs. Wide-value
occupancy remains a representation risk and AVL a meaningful alternative. Indexed composite
work and prior container/lowering cost questions retain their own scope; this
trial does not close them or amend the language.

#### Single-descent insertion discriminator

The complete baseline matrix exposes two ordinary source costs: absent puts
descend twice and carry an owning Pair by value through each insertion level.
Before timing a replacement, the candidate combines search and insertion below
the logical ceiling and passes a reference to a one-slot Pair carrier. At a
leaf it takes the offered owner; replacement exchanges it with the resident
pair. Carrier occupancy then distinguishes insertion from replacement at the
public boundary. No recursive ensures is assumed: FN-9 withholds summaries
inside the same recursive component. At the ceiling, the existing replacement
search and unchanged-owner refusal stay in place. Split promotion still returns
an owned optional entry, so replacement may now pay an aggregate result cost
that the baseline Boolean search avoids. The comparison must expose this risk.

This is one bounded source candidate, not a second tree or a compiler change.
Its SHA-256 is
`ddc53f3bd12e3682c26ea72f33d94da5da787371afb7461fd7d69d1798aca4ec`;
the published baseline is
`affaee669a09320aafc9ec5badbea6c11fd95623e1ea8987ad9811742c4b70bb`.
Both have already passed the maintained caller in sequential/parallel lowering,
ordinary/dirty allocation modes, with the exact 103-allocation and owner ledger.
That establishes the tested outcomes, not cost selection. Preserve the candidate
as a reproducible patch in the existing experiment and reconstruct its source
only in the experiment build directory until selection.

The following criterion is fixed before candidate timings. Keep the original
1,152 baseline rows and their identities unchanged. Extend the common WF/C/oracle
trace with a fifth path that only replaces existing keys below the ceiling;
all five paths include the same construction, visitation and cleanup accounting.
Retain both pair widths, all three counts, normal/retained public helpers, the
same deterministic operation stream, one warm-up and five measured samples.
Native source C stays baseline-shaped, with its existing alias qualifications;
direct C and AVL stay alternative controls. Only the shared trace and harness
gain the replacement path. Freeze those sources before any timing.

Build baseline and candidate with the same frozen compiler and native toolchain.
First run A/A as two fresh processes of the unchanged baseline executable,
then A/B. Each comparison has two complete cohorts with arm/mode order reversed
and the within-cell implementation order counterbalanced. Each image/cohort has
1,440 rows including warm-up; two arms, two cohorts and two comparisons produce
11,520 new rows. Normalize each WF observation by the source-C observation in
its own arm/sample, and report direct-C/AVL normalization as cross-checks. Do
not reuse one control observation for both arms.

For each payload/count/path/mode cell, the material-change band is the largest
of three percent, the absolute A/A normalized median departure from one in
either cohort, the unchanged source-C median drift between arms, and four clock
resolution quanta divided by the smaller measured interval. Selection requires
an insertion/build or churn improvement beyond that band's lower boundary in
both cohorts, with no other cell showing a loss beyond its upper boundary in
either cohort. Confirm removed recursive Pair argument transfers in optimized
IR; do not assign all elapsed change to copying without isolation. No averaging
across paths or payloads may hide a replacement regression. If cohort direction
or unchanged-control variation prevents a determination, allow one repeat of
the entire fixed matrix and retain all rows; a repeatable material loss rejects
adoption, and an unresolved result remains inconclusive. No selective cell rerun
or post hoc narrowing is part of this experiment.

Combined initial construction/correctness/timing budgets are 120/40/60 seconds,
including that optional full repeat; compiler construction and the canonical
gate remain separate. An exceeded stage requires investigation before another
run. Expected improvement is reduced descent and owner transfer; possible cost
is optional-promotion materialization on replacement. This comparison selects
only between these two library sources, not a default ordered representation.

The candidate fails that criterion. The complete 11,520-row comparison is in
[the experiment](../../experiments/container-representation/ordered-library/RESULTS.md#single-descent-insertion-candidate).
Normal wide replacement at 256 entries costs 1.6214/1.6331 times the baseline
after source-C normalization in the two cohorts, outside the three-percent
cell band. Other wide replacement sizes and retained replacement also regress.
Build improvements therefore cannot select this source. No optional repeat was
needed to establish the loss. Optimized code removes recursive Pair arguments
but clears a 288-byte optional promotion on each replacement unwind. This is
an observed transfer cost, not an isolated or dominant share of elapsed time;
the follow-up below tests its removal.
The source-C controls retain different tag/result ABI and alias facts; the
unchanged-control WF-versus-WF comparison, not a claim of ABI equality, decides
this experiment. The published library stays at the measured baseline.

#### Borrowed promotion follow-up

The observed optional-result cost justifies one further ordinary source
candidate within the same B-tree. Recursive insertion borrows both the offered
Pair slot and one caller-owned promotion slot, returning unit. A split places
an entry in the promotion slot; a parent observes occupancy, takes that owner,
and either absorbs it or publishes its own split. Matching replacement never
creates a promotion. The public helper consumes any root promotion and inspects
the offered slot for insertion versus replacement. This uses the existing
capacity/occupancy rules, without a recursive-summary assumption, fabricated
failure arm, new tree shape or new compiler mechanism. The cost still includes
initializing the one promotion slot and inspecting its length; removing
recursive results alone does not establish improvement.

Candidate SHA-256 is
`5514ce2aecdf2a8074dc8897b0457d6858e7ecf16d2d2fa559dfefb6420f2c8f`.
On the same frozen v0.68 compiler it admits and passes all four maintained
native modes with exactly 103 allocations released once. This is source and
ownership evidence before timing. Preserve a separate reconstruction patch;
the first candidate and all of its failed comparison evidence stay intact.

Before timing, the follow-up reuses the unchanged five-path WF/C/oracle driver,
native control algorithms, payloads, counts, seeds, batching and public-helper
policy of the first candidate comparison. Verify their source/optimized-control
identities before any run. Compare this candidate against the original baseline,
with a fresh A/A process pair followed by A/B, both reversed cohorts. Retain
11,520 new rows including warm-up, separate from both earlier matrices. Apply
the same per-cell normalization, material-change bands, benefit requirement,
no-loss requirement and complete-repeat rule written above. In particular,
replacement losses cannot be averaged against build gains. A repeatable material
loss rejects adoption; one full fixed repeat is allowed only for unresolved
variation. No third source candidate is part of this follow-up.

Its combined construction/check/timing budgets are 120/40/60 seconds, including
any permitted repeat; investigate an exceeded stage before extending it. Confirm
that recursive helpers have no aggregate promotion result and count remaining
slot initialization and owner transfers in optimized IR. This experiment tests
the changed ordinary interface contract, not an inactive-storage compiler
optimization or a promise that zeroing alone caused the previous regression.

The follow-up also fails its criterion. The complete
[11,520-row comparison](../../experiments/container-representation/ordered-library/RESULTS.md#borrowed-promotion-follow-up)
records normal wide replacement ratios of 1.6885/1.6003 at 256 entries, outside
the 3.13% band, and 1.5313/1.5620 at eight entries, outside 3%. Retained wide
replacement at both sizes also loses. Build and larger-count churn gains do
not cancel those losses. No repeat was needed; both exact source candidates
remain rejected, the published baseline stays unchanged, and there is no third
candidate in this comparison.

Optimized recursive insertion has no aggregate result or optional-result
clearing in either mode. The public operation still initializes one 40/288-byte
promotion slot, and wide Pair placement, swap and returned-owner extraction
remain. A promoted Entry is staged only when a parent accepts it. Recursive
wide frames remain 352/368 bytes in normal/retained modes, unchanged from the
first candidate. These observations do not isolate execution counts or elapsed
shares. Removing recursive aggregate results did not cure the replacement
regression and does not establish their earlier clearing as its dominant cause.
The two candidate percentages come from separate baseline comparisons, not a
direct paired comparison between candidates. Further source or lowering work
needs a new discriminator for the remaining per-put storage, owner movement,
occupancy and frame costs; [the maintained TODO](../../../docs/todo.md) retains
that attribution task alongside node construction/cleanup and occupancy work.

## Ceiling challenges connected to real source contracts

| Challenge and external pressure | Ordinary x1 candidate | What remains to establish |
| --- | --- | --- |
| Redis incremental rehash: lookups can also advance migration | Two owning tables, migration cursor, explicit write row, bounded migration budget; query both tables until done | Complete partial-progress ownership and per-operation work bound. An eager rebuild is not the same latency contract. |
| DuckDB aggregate rows pinned while a sparse table grows | Slab/index table when logical identity suffices; boxed or segmented rows for fixed addresses | Distinguish address stability from a legal surviving reference. Include retention lifetime and address lookup/indirection costs. |
| DuckDB string construction into final payload storage | Full byte storage plus direct bounded filling, or an initialized Slots prefix built sequentially | Spare-byte initialization and extra aggregate transfers. No inference that `move` or OP-12 guarantees destination placement. |
| Redis listpack and SQLite variable-length page records | Initialized byte Array/Slots, offsets, codecs, local shifting and compaction | Encoding/decoding and validation cost, overlapping byte movement, relocation fixups and peak scratch. No typed pointer reinterpretation is required by the byte contract. |
| SDS-style pointer-sized handle with typed header immediately before dynamic tail | Existing boxed byte block with encoded header; compare an inline header plus Box data | A general source struct with a runtime-sized typed tail is forbidden by TYPE-9. `Box<Record>` containing a second Box uses two objects; plain Record plus Box data uses one allocation but a wider handle and separated header. Bytes are a real alternative with codec costs. |
| Large owning records or fixed inline arrays appended through helpers | Current ordinary construction followed by placement; inspect destination forwarding in optimized IR | Per-element temporary/result transfers with helpers retained; failures during input-driven construction must consume the initialized prefix. Heap exhaustion is not that failure case under STOR-8. |

The pinned [external traces](EXTERNAL-WORKLOADS.md#focused-revalidation-contracts-and-host-mechanisms)
already separate workload obligations from Vec, pointer, arena, or dictionary
choices in the host language. Their old WF adaptation comments are historical;
the candidates here use x1. No upstream application was newly benchmarked.

### Indexed composite trial

The registered experiment starts from the ordinary PriorityQueue at
`c3c2a50fd`. Its maintained caller establishes the non-indexed operation and
ownership chain described below; it establishes neither arbitrary-position
updates nor reverse-position repair. Preserve that source and its public
contracts as the plain-queue comparison baseline. The following criteria were
recorded before composite implementation or timings. The maintained composite
now establishes the complete operation and ownership chain below. The completed
comparison supports the owner's qualified shared-core selection recorded below; it does
not establish a speedup or native parity. No new language mechanism is selected.

The minimum consumer is a coordinated record store with several simultaneously
live records. One Slab owns each record and its payload, one HashMap maps IDs
to Slab handles, and an indexed min-heap schedules expiry. A heap entry contains
only its deadline, ID and copyable index/generation handle. Comparison reads
these ordering keys directly, with a deterministic tie-breaker, so a sift does
not repeatedly look up a payload in Slab. Large owning payloads stay in Slab.
Each record carries an ID-membership flag and a reverse heap position. The
candidate absent-position value is the heap's const ceiling: every resident
position is strictly below the current length, which is at most that ceiling.
This uses ordinary integers without an extra optional payload; actual layout
and cost remain measurement questions. A malformed non-sentinel position must
not be interpreted as permission to delete a retained object.

#### Coordinated operations and membership policies

The application-facing operations accept IDs or handles, never a saved heap
position. Before heap removal or rescheduling, resolve the handle's current Slab
generation and occupancy, read its position, establish `position < heap.len`,
and compare the entry's complete handle with the requested handle. Only then
call an internal position-based helper. Reacquire this relation after a heap
mutation; an in-bounds old position can name a different record. A helper may
publish a scalar result's bound on the current heap length through FN-9, but
that bound alone does not establish identity. The existing
`priority_queue_len` relation supplies nonemptiness for ordinary root access;
it does not validate a reverse position or establish a record-to-entry relation.
Invalid, expired and unscheduled requests return explicit outcomes before
mutating heap membership. This API boundary does not make ordinary fields or
constructors inaccessible; independently authored bookkeeping mutations remain
outside the composite protocol.

The complete first operation chain has these ownership outcomes:

| Operation | Required behavior |
| --- | --- |
| Insert an object | Slab receives the payload and returns a handle, or returns the offered owner unchanged at exhaustion. The new record starts without memberships. |
| Attach an ID; schedule a handle | Validate the live handle, then add the selected membership and its bookkeeping. Refusal leaves the object owned by Slab. Reattaching an existing membership has an explicit already-present outcome; it never increments a hidden count or inserts a second live heap entry. |
| Lookup and duplicate payload replacement | Lookup returns owned observations or a handle. Replacement swaps the offered payload into the existing record and returns its previous payload, preserving the ID, generation and memberships. A miss returns the offered payload. Replacing record identity itself is a different operation. |
| Reschedule; cancel | Update a live member's deadline in either direction, or remove its arbitrary heap position, repairing every moved entry. Neither operation consumes the record's payload. |
| Detach an ID; delete an object | Detach only the intended ID/handle association. Deletion reports missing, busy where required by policy, or the matching owned record. Removing one membership leaves the other usable. |
| Expire due entries; destroy the store | Pop due entries, repair survivors, validate generation, detach any matching ID association, and return or explicitly consume each expired owner. Final destruction retires both indexes before consuming remaining Slab owners and releasing all backings. |

Attaching an ID already naming a different live handle is refused. Under the
weak policy an expired association can be replaced by the new handle. Duplicate
payload replacement instead updates the record already named by that ID; these
outcomes keep the one-ID-membership bookkeeping unambiguous.

These operations coordinate the three containers without promising an atomic
three-container insertion. If attachment fails after object insertion, the
owner remains reachable through its returned handle; the caller can retry,
detach established memberships and delete, or consume it during final cleanup.
Do not silently assume that a later Slab removal returns `Some` because an
earlier insertion succeeded: the current contract does not publish that
indexed relation. The native control must use the same staged outcomes.

Run two distinct policies over this operation chain. Under the **weak** policy,
object deletion may leave ID and heap entries holding stale handles; lookup
reports expiry, and later heap processing skips that generation. A stale entry
may coexist with a replacement using the same slot or ID. Position reporting
must skip the old generation, and expiry must not remove an ID mapping that
now names the replacement. Those stale entries occupy space until explicitly
detached or popped; include that space and work in refusal and cost accounting.
Cancellation through a stale object handle reports expiry and does not promise
an indexed search for its remaining stale heap entry.

Under the **retained** policy, deletion is busy while either the target's ID
membership or heap membership remains. Test both detach orders and deletion
of one unheld record while unrelated records remain indexed. Expiry first
retires the target's heap membership and matching ID membership, then removes
its owner. The two stored membership states avoid an additional counter whose
consistency would itself need maintenance. These are ordinary protocol
invariants maintained by the composite operations, not an unrestricted static
theorem about arbitrary source mutations. The existing
[one-object caller](../../../tests/programs/containers/slab-membership-program.wf)
remains evidence for its narrower contract only.

The trial's public handle includes an ordinary store-ID check before its Slab
handle is interpreted. The wrong-store witness must use two distinct store IDs and
show rejection without changing either store. Caller-chosen IDs and source
constructors do not authenticate a store or establish global uniqueness; no
unforgeable handle, external retention ticket or surviving reference is claimed.

#### Ordinary edit and movement boundaries

The reusable Slab addition to try is an edit callback with the same handle
validation as `slab_visit`. The prospective signature below omits its body and
has not been checked as a complete source function:

```wf
interface SlabEdit<T, E, R> {
  fn edit(env: &E, value: &T) -> result: R writes(env), writes(value);
}

fn slab_edit<interface SlabEdit<T, E, R>, const ceiling: u64, const generation_limit: u64>(slab: &Slab<T, ceiling, generation_limit>, handle: SlabHandle, env: &E) -> result: Result<R, unit> writes(slab.cells), writes(env) contract {
  ensures deref(slab).cells.inner.len == deref(entry(slab)).cells.inner.len;
  ensures deref(slab).cells.inner.cap == deref(entry(slab)).cells.inner.cap;
}
```

It calls the member exactly once for a present generation and not at all for
a missing, reused or retired handle. The callback receives the payload, not
the containing Slab cell, generation or free list; it may return ordinary
owned data. This serves both position edits and owning payload replacement.
For replacement, pass a local offered owner by reference as the environment
and swap it with the record's payload. Success leaves the previous payload in
that local; a failed validation leaves the offered payload there. Both exits
can return their owner without extracting an unproved occupied slot.

The separate heap movement callback has this prospective boundary:

```wf
interface PriorityPosition<T, P> {
  fn placed(env: &P, value: &T, index: u64) -> result: unit reads(value), writes(env);
}
```

Keep the existing read-only comparator separate. The composite's position
implementation writes its Slab through `&store.objects`, while heap operations
write `store.due.storage`; OWN-7 and EFF-5 can separate those fields. Passing
`&store` as the writable callback environment overlaps the heap operation.
REF-3 also excludes an environment aggregate containing borrowed fields as a
way around that overlap. The position callback reads its borrowed heap entry
and uses Slab edit to record the new position only for the matching live handle.

Every placement reports the entry's actual resident position, including an
insertion that performs no swap. An exchange reports both resident entries
after it completes. Arbitrary removal takes the last entry, installs it in the
removed position when that position remains in range, reports that placement,
and repairs upward or downward; removing the former last entry needs no repair.
Rescheduling validates its position, changes the key and chooses a strictly
progressing rise or sink. Clearing the removed member's position is explicit
in the composite operation, not a sentinel index sent to `placed`. If the shared
core supports indexed heapify, initialize every position, including untouched
leaves, before its ordinary bottom-up repair. The consumer may start empty;
changing the shared core still requires preserving ordinary heapify behavior.

Retain the current sift's arithmetic certificates, bounded child-selection
result and length/capacity contracts. Internal arbitrary-position helpers
require the position to be below the current length and publish preserved
capacity and the appropriate unchanged or decremented length. The callback's
declared `writes(env)` does not write the disjoint heap or justify any subscript.
Comparator consistency and faithful position bookkeeping are conditions for
the intended logical contents, not assumed laws authorizing memory access or
termination. For every returning callback, the heap's own bounds and monotone
sift progress must stand independently; callback bodies obey ordinary ownership.

The known source limits remain explicit. REF-3 forbids returning or storing
references. FN-9/CALL-4 admit `slab_find_index`'s outer scalar bound but not an
indexed cell's occupancy or generation postcondition, nor arbitrary nominal
result-field relations. A caller must validate locally or keep validation and
access inside its callback. Writes retain their ordinary fact invalidation and
reference-validity rules; stable slot numbers do not preserve a reference
across a destructive ancestor write. These constraints are not grounds for
adding casts, hidden proof assumptions, runtime proof traps or a language change.

#### Comparisons registered before implementation

The first shared-core library implementation keeps the plain queue's public
signatures and uses ordinary no-op reporting wrappers; indexed operations use
arbitrary-start rise/sink, explicit initial placement and repair in either
direction. The unchanged plain queue caller, existing Slab/membership callers
and a small indexed/SlabEdit caller admit and emit LLVM on the frozen main
compiler in 0.34, 0.22 and 0.18 seconds respectively. These are source-admission
observations, not native correctness or cost results for the composite.

The spelling `fn helper<interface PriorityOrder<T, E>, interface PriorityPosition<T, P>, ...>`
is refused with TYPE-6 `DeclarationCollision` on the second `T`: each group
import declares fresh binders under FN-3. The implementation retains one
`PriorityOrder` group and writes `P, fn placed(env: &P, value: &T, index: u64)
-> result: unit reads(value), writes(env)` as ordinary additional parameters.
No unused second interface descriptor is added. This is a spelling workaround,
not a claim that different named groups cannot be instantiated at the same
concrete type, and it introduces no new behavior mechanism.

The complete consumer also exposed an implementation defect in FN-4 effect
refinement. The ordinary Slab edit formal writes its environment and value;
the position callback only reads the environment and writes the position:

```wf
fn edit(env: &E, value: &T) -> result: R writes(env), writes(value);
fn record_set_position<T>(env: &u64, value: &Record<T>) -> result: unit
  reads(env), writes(value.position) {
  set deref(value).position = deref(env);
  return unit;
}
```

The unchanged compiler rejects that binding under FN-4 because it compares
actual reads only with formal reads. EFF-1 already states that a write covers
reads at the same path, and FN-4 requires normalized subset coverage. The
repair reuses the ordinary effect-path coverage relation: actual reads may
be covered by formal reads or writes, while actual writes still require formal
writes. The selected actual must still exhibit its own exact row under EFF-2,
and the caller still uses the authoritative formal row under FN-5/EFF-2.
No dummy write or row padding is an admissible workaround. The focused
`formal_writes_cover_actual_reads_by_parameter_and_path` regression covers
named/raw bindings, renamed parameters, whole/field coverage, wrong-direction
and uncovered/sibling negatives, and the actual's unchanged exact-row check.
The existing conformance negative keeps its verdict and body; only its
incorrect same-category explanation is corrected. This changes no language
rule or lowering policy.

The next failure was a transitive nominal allocation layout, not an OP-9
source rejection. A generic `empty_store<T, fn consume>` constructs
`Slab<Envelope<T>>` and passes a specialized envelope consumer to cleanup.
During template checking the allocator's nominal element can still contain
the outer `T`; a shallow concrete-substitution test treated that nominal as
resolved and raised `InvalidResolution`. The repair uses the existing
recursive substitution stabilization on the allocation operation's arguments.
Only an actually unresolved type/const vector defers its schema obligation;
concrete replay still computes and proves the same byte ceiling. The
`transitive_nominal_allocation_layouts_remain_symbolic_until_replay` regression
checks an unused schema, its concrete invocation, the exact 16-byte-stride
u64 allocation bound, and OP-9 rejection one element above it. The boundary
positive is semantic evidence, not a claim that the selected native target
can allocate that extent. This repair changes neither OP-9 nor the target
layout limit. It removes the semantic failure in the complete composite.

The remaining lowering failure reduced to an unused function formal whose
parameter mentions `Envelope<T>`. Declaration formation materialized that
symbolic nominal outside the existing scratch checkpoint, so lowering saw a
generic field in its executable prefix. Formation now preserves structurally
concrete roots, restores the nominal checkpoint and reifies those roots through
the existing stable-type bridge. Concrete types mentioned only by an unused
formal remain present; malformed unused contracts still reject. The bridge
also handles ordinary ordered result lists and their captured-region
substitution, retaining field names and order. No lowering filter, new type
classifier or instance-selection rule is introduced. The focused
`formal_nominal_inventory_keeps_only_concrete_types_and_result_lists` and
`nominal_formal_contract_queries_survive_scratch_rollback` regressions cover
mixed symbolic/concrete raw and named formals, result lists, concrete contract
queries and both lowering modes. These reduced compiler cases do not substitute
for the full consumer execution recorded below.

#### Partially concrete reserve diagnostic

A separate standalone-control attempt fixes the element to a four-u64
`ProbeDue` while retaining a symbolic capacity ceiling. Bundled with the
preserved plain queue at `c3c2a50fd`, compiler image `c71aaf16` reports INV-1
`UndischargedLocalInvariant` at `room`:

```wf
fn probe_room<const ceiling: u64>(queue: &PriorityQueue<ProbeDue, ceiling>, value: ProbeDue) -> result: unit writes(queue.storage) contract {
  requires deref(queue).storage.inner.len < ceiling;
  requires ceiling <= 576460752303423487_u64;
} {
  priority_queue_make_room::<ProbeDue, ceiling>(queue: queue);
  invariant room: deref(queue).storage.inner.len < deref(queue).storage.inner.cap;
  place_back(window: &deref(queue).storage.inner, value: value);
  return unit;
}
```

The caller without the second requirement also fails; replacing the symbolic
ceiling with literal `8192` admits. This does not demonstrate a publication
defect. The called reserve body has only `requires total <= ceiling`, then
passes `total` to `grow`. Once its element layout is known, OP-9 requires
`total <= 576460752303423487` there. ENT-2 gives the callee only its own
requirements, not the caller's additional premise. The existing
`transitive_known_layouts_do_not_take_the_direct_opaque_deferral` regression
covers withholding a summary when a partially concrete callee's allocation
proof fails. This is a read-only diagnosis; no scratch-proof trace confirmed
the particular withheld summary.

The remaining opportunity is diagnostic attribution. Validate a bounded
reserve/caller pair, propagating the same extent bound through any intervening
helper, and an intended OP-9 rejection one element above the bound. A useful
diagnostic would expose the failed callee allocation obligation and its
unavailable summary instead of only the downstream invariant. No acceptance
rule changes are justified by this probe. The admitted standalone comparison
retains generic element opacity and uses the same comparison/reporting formals
as the shared core; it is the viable control used for measurement.

#### Maintained composite correctness

The [record-store helper](../../../tests/programs/containers/indexed-store.wf)
and [maintained caller](../../../tests/programs/containers/indexed-membership-program.wf)
now execute the complete multi-object protocol with the three current-rule
compiler repairs above. The five-source bundle combines those two sources
with Slab, HashMap and PriorityQueue. Using compiler image `c71aaf16`, its
sequential and CLI-parallel native images pass with ordinary deallocation and
with dirty allocation plus quarantined release observation. Each observed
image reports exactly 129 allocations, each released once: 67 container
backings and 62 payload Boxes. The bundle is registered in the ordinary
[container corpus](../../../compiler/tests/programs/containers.rs); these
focused results do not claim a completed canonical gate.

Four traces combine weak and retained policies with droppable Box and wide
nodrop payloads. Each holds eight simultaneous owners through a colliding ID
map, checks membership against an independent flat dictionary model, sorts
the model's due entries independently of the heap comparator, and records
each returned or consumed payload identity. Coverage includes duplicate
payload replacement, upward/downward rescheduling, root/middle/last removal,
both detach orders, busy deletion, refusal/retry, wrong-store and stale handles,
same-slot/same-ID reuse, malformed reverse positions, generation retirement
and partial final cleanup. Separate witnesses check all initial heapify
reports, no report when removing the last entry, a nodrop SlabEdit result,
and no edit callback after expiry. No fixture workaround replaces these
ordinary generic callbacks.

This establishes the coordinated protocol's demonstrated correctness and
ownership boundary. Weak memberships may outlive their object and expire;
retained deletion stays busy until both memberships retire. It does not make
bookkeeping unforgeable, authenticate caller-selected store IDs, provide
independently held retention tickets, or preserve references across mutation.
The old one-object caller retains its narrower evidence. The registered
comparison below owns the measured validation/storage costs and grounds for
shared-core selection; this correctness result alone establishes no native parity.

#### Registered cost comparison

The following controls and criteria were registered before implementation and
timing; the measured disposition follows them.

| Candidate or control | Discriminating property |
| --- | --- |
| Compose the current public heap and scan to repair positions | Establishes an ordinary executable fallback, but an O(n) scan after each update/removal fails the selected O(log n) indexed-operation requirement. It is not the proposed production path. |
| Push a new entry for every priority update and lazily discard old entries | Changes bounded space, cancellation, expiry work and exhaustion outcomes. It cannot substitute for the selected one-entry-per-live-membership indexed contract. Weak stale entries caused by actual object deletion remain a separate, explicit policy. |
| Standalone indexed sift with direct position reporting | Supplies the same swap algorithm, validation and callback work without changing the plain queue. Its cost is the extra maintained sift implementation. Keep it a bounded comparison control rather than introduce a second full public queue family. |
| Shared rise-from-index/sink core with supplied position reporting | Can serve both queues with one progress and ownership implementation. Compare its indexed specialization against the standalone control and its no-op specialization against the preserved plain queue before selecting it. |

The shared candidate is preferred for investigation because it can avoid sift
duplication while preserving the required algorithm. Direct-call specialization
does not establish that an empty position callback, extra argument, environment
load or state reload disappears. Inspect emitted code and actual surviving
calls under normal optimization and retained public operations. Match retained
boundaries in the standalone and C controls; private sift and child-selection
helpers remain ordinarily optimizable. Do not force every helper to remain or
charge one candidate for a callback boundary the other inlines by construction.

Use one source-shaped full-slot-swap C control and one direct indexed C control,
both implementing the same selected policy, staged outcomes, capacity/growth
rules, generation retirement and owner returns. The latter may use a hole sift;
its advantage is an algorithm/control-flow comparison, not evidence that WF
emits the same operations. Match external invalid-handle behavior and include
all required stale-generation checks. Attribute additional internal checks,
result transfers or position updates with the source-shaped control rather than
silently deleting them from the WF contract.

The composite matrix covers live lengths 16, 256 and 4096, small and wide owning
records, both membership policies, and normal/retained public-operation modes.
Use a reserved mixed lookup/replacement/reschedule/cancel/expiry/reuse trace and
a separate construction/growth/final-consumption trace. Keep heap entry size
fixed when payload size changes. Fix input seeds, operation counts, collision
distribution and stale-entry fraction before timing; run repeated samples in
two reversed implementation orders with unchanged C controls. Record phase
times without subtracting setup estimates. Preserve the existing plain-queue
matrix for the baseline/no-op comparison, including its owning cases and native
controls. Add no further storage layouts, notification interfaces or callback
policy variants to this discriminator.

Before timing, require agreement with an independent sorted-vector/ID oracle
over the full operation transcript, including returned payload identities,
expiry order and retained busy outcomes. The maintained composite caller must
exercise several live objects, upward and downward rescheduling, root/middle/
last removal, collisions, duplicate replacement, both detach orders, refusal
and retry, stale and wrong-store handles, same-slot and same-ID reuse, bounded
generation exhaustion, and partial final cleanup. Include droppable owning
payloads and a nodrop instance. A per-owner identity ledger must distinguish
each offered, returned and consumed owner; a sum or checksum alone cannot
detect loss paired with duplication. Exercise the same source bundle under
sequential and parallel lowering, with ordinary releases and the existing
allocation observer. No successful timing establishes heap-allocation failure
behavior; STOR-8 still owns that boundary.

Report comparison, swap, position-report, validation and hash-probe counts;
complete backing and result layouts; payload transfers; allocation/release
identities; and requested/peak bytes, including stale entries and simultaneous
growth backings. Keep diagnostic counting separate from timed images unless
the same instrumentation is deliberately present in every control. The
selection criterion is complete operation/ownership agreement, no material
indexed regression beyond unchanged-control variation, and no unexplained
material regression from the shared no-op path in the plain queue. A claimed
indexed benefit must repeat beyond control variation in both cohorts and have
an emitted-code or algorithmic attribution. Equivalent code and costs
permit reuse on maintenance grounds; an unexplained shared-core loss leaves the
standalone implementation viable and the selection open. Do not average the two
membership contracts or workload cells into an unmeasured application mix, or
infer native parity from transfer counts, accepted source or one timing cohort.

The prospective experiment has separate budgets of **120 seconds for artifact
construction, 40 seconds for native correctness execution, and 60 seconds for
the timing matrix**. Compiler construction and the canonical repository gate
are separate guarded stages, not charges hidden in those execution budgets.
Investigate a stage exceeding its budget before extending it; elapsed time
selects neither source acceptance nor a successful correctness verdict. Run
heavy stages through the repository guard and inspect an existing owner before
starting another. These budgets do not narrow the canonical gate.

Reusable Slab/heap support belongs with the existing source libraries; its
maintained consumer and oracle wiring belong in the existing container corpus.
Any comparison-only sources, harness and results belong under
`research/experiments/container-representation/indexed-library`, with one
explicit experiment caller and no correctness-gate dependency. Their purpose
is this complete consumer and sharing/cost discriminator; retire them when
superseded and no maintained claim needs their replay. Retire a maintained
fixture only when equivalent maintained coverage replaces it. A packed byte-page
payload, full ordered tree, externally held retention tickets and concurrent
reclamation remain separate consumers rather than added variants of this trial.

#### Measured shared-core selection

The [complete comparison](../../experiments/container-representation/indexed-library/RESULTS.md#measured-result)
retains 36,288 rows from the initial series and its sole permitted complete
repeat. All 48 indexed cells are within their control-variation bands in both
series. Each plain-queue series has 59 equivalent cells and one single-cohort
excursion. The initial apparent loss, normal scalar growth at 4096 entries,
has normalized ratios 1.035093/0.997795 against a 3% band; its raw first-cohort
ratio is 1.001912. The complete repeat places both cohorts inside the band.
The repeat's only excursion is a possible benefit for retained scalar
pop/push at 16 entries, 0.969773/0.990951 against a 3% band. It establishes
neither a repeated benefit nor a remaining regression. The result is not
"all cells equivalent"; every initial and repeated sample remains evidence.

The owner selected shared-core reuse on maintenance grounds, with that
uncertainty retained. Indexed costs stay within the registered comparison
bands, and the independently built standalone/shared indexed executables are
byte-identical in normal and retained modes with the same runtime objects and
link order, using Apple Clang 21.0.0 (`clang-2100.3.34.2`), `-O2`, target
`arm64-apple-darwin25.6.0`. There is no repeatable material plain-queue
regression. Common native instruction counts match there; the rise-test spelling
difference is equivalent. Plain no-op reporting calls disappear in the measured
code without imposing an artificial retained callback boundary. Actual indexed
callbacks retain the selected boundary and the complete ownership/position
validation protocol. One core avoids a second maintained sift implementation;
this selects neither a demonstrated speedup nor uniform native parity. The
choice is recorded in the existing
[priority-queue storage decision](../../../design/language/data-model/priority-queue-storage.md)
and adds no language mechanism.

The native comparison still has operation-specific costs. Retained small
payload growth/cleanup at 4096 records costs 1.354--1.368 times swap C and
1.392--1.408 times hole C across policies, cohorts and both series. Wide mixed
traces favor WF against swap C: 0.640--0.710 in normal mode and 0.846--0.888
with retained boundaries. Native hole sifting reduces assignments, reports
and Slab validations without changing comparisons, hash probes or allocation
counts; those algorithm-level counts do not isolate elapsed-time causes.
The shared/standalone executable identity separates these native costs from
the sharing choice. The maintained TODO keeps their attribution open.
Construction, native correctness and timing took 27.46, 14.49 and 51.57 seconds
against the separate 120/40/60-second budgets; compiler construction and the
canonical gate remain separate stages.

**Design suitability.** Small heap entries separate scheduling movement from
payload ownership, and ordinary Slab edit addresses a concrete reusable access
need. The complete multi-object protocol and independent identity/allocation
ledgers establish the selected correctness boundary. The shared core is
selected because the comparison supports source reuse without a repeatable
material regression, while preserving the single-cohort uncertainty and native
cost qualifications above. Independent retention authority and surviving
references remain outside this protocol. No broader mechanism follows from
the container experiment.

## Findings rechecked against merged PR #70

These rows distinguish the merged `8c02e875` restoration baseline from the
subsequent library trial. The restoration alone changed no container algorithm,
contract, specification rule or compiler implementation.

| ID | Exact witness/source | Status at the merged baseline | Next action |
| --- | --- | --- | --- |
| X1-P1 | Baseline `grow_vector_drain` used repeated `remove_at(..., index: 0_u64)`; OP-10 | The later Vector trial replaces quadratic movement with suffix reversal and back consumption in [grow-vector.wf](../../../lib/std/collections/vector/grow-vector.wf). | The original-order callback contract is preserved. The [comparison](../../experiments/container-representation/vector-library/RESULTS.md) measures the remaining cost against a direct consumer; O(n) is not a minimum-transfer claim. |
| X1-P2 | [unbounded-reserve.wf](../../experiments/container-representation/x1/unbounded-reserve.wf) records the old missing-requirement shape | Resolved in the shipped GrowVector: `const ceiling`, `requires total <= ceiling`, bounded doubling and saturation replace unrestricted growth. MSR-4 now supplies the specified affine-left/L0-right bridge needed by the ordinary caller proof. The deliberately unbounded probe should still reject under OP-9. | Keep the size requirement. A library/application Full outcome may return the offered owner when its selected limit is reached; heap allocation itself has no refusal arm. Do not carry this old finding forward as a compiler or current-library defect. |
| X1-P3 | [linear-ring-publish.wf](../../experiments/container-representation/x1/linear-ring-publish.wf):18; OP-12 and WIN-3 versus the atomic-update paragraphs in [CANDIDATE-X1.md](../access-effects/CANDIDATE-X1.md) and [affine-replacement.md](../../../design/language/ownership/affine-replacement.md) | The active affine/copy restriction remains. A nodrop Ring cannot use this atomic-publication route; the candidate's general linear-assignment refusal also remains. The broader atomic paragraph alone does not establish a selected linear exception. | Obtain an explicit intended-domain ruling before changing OP-12 or its record. In parallel, test ordinary swap/contract and consuming-rebase alternatives without claiming all deque designs impossible. No language widening is part of this restoration. |

The following were specified limits at #70, not bugs to silently fix in that
restoration. The cursor row records its subsequent change:

| Exact fragment | Rule | Ordinary workaround and limit |
| --- | --- | --- |
| `count(part: &ring[0_u64..0_u64])` in [ring-range.wf](../../experiments/container-representation/x1/ring-range.wf) | REF-4 | Element visitation, copying into Slots, or a full copy-element Array with two physical spans. Only the last preserves zero-copy ranges, and it requires initialization/filler. |
| `ensures deref(destination).len == deref(entry(destination)).len + deref(entry(source)).len;` in [append-contract.wf](../../experiments/container-representation/x1/append-contract.wf) | FN-9 admits only one datum plus a constant on each relation side | Reread lengths and use ordinary control flow where necessary; an affine postcondition extension is separately proposed work, not assumed here. |
| `struct Record { header: Header; tail: Array<u8>; }` (declaration fragment) | TYPE-9 permits a runtime-capacity shape only directly as Box content | Encoded byte block, or a separate Box for the tail. Neither is an implicitly packed typed trailing member. |
| `set cursor = &deref(cursor).next.Some.value.inner;` carried by a loop (path fragment) | Historical baseline refusal, superseded by current REF-1/REF-2 | Current [owned cursor tests](../../../tests/programs/owned_link_cursors.wf) exercise iterative descent and affine edits. Each new payload selection still needs its current variant fact, and destructive ancestor writes invalidate references. Linear atomic replacement and independently selected cursor aliasing retain their separate limits; no musttail claim follows. |
| An `ensures` exporting a returned handle's indexed generation/variant relation | FN-9's relation datums and routes exclude that shape | Return a bounded scalar index, then validate/match locally or inside the consuming callback. Measure repeated checks before widening contracts. |

## Library home and evidence after the merge

The owner selected root `lib/` for reusable WF source. The restoration at
`8c02e875` placed the merged GrowVector implementation, byte for byte, at
`lib/containers/grow-vector.wf`, now the implementation record of
[`std::collections::vector`](../../../lib/std/collections/vector/grow-vector.wf).
Its caller and C allocation observer remain under `tests/programs/containers/`;
the existing corpus test still builds the same source bundle in sequential
and parallel modes, then executes each normally and with the observer. The
test reads the relocated source with a separate, valid logical source name;
filesystem parent components are not source-envelope names. No separate
Makefile, test group, import mechanism or library ABI is restored.

This ownership split makes the implementation available to user programs
without making test support or research models library dependencies. The
narrower container fixtures remain component evidence:
`priority.wf` is a u64 heap of capacity 16, `ordered.wf` exercises leaf splits,
and the behavior map requires droppable keys and fixes the payload to a Box.
The complete generic families have separate sources and callers in the
completion matrix below.

At that restoration baseline the GrowVector caller covered scalar and owned droppable Box elements,
zero capacity, doubling, ceiling saturation, insertion, removal and drain.
Its release observer checked exactly twelve allocations. That evidence did not establish
must-consume-element construction/cleanup, an order-sensitive drain oracle,
large-element costs, or native parity of the merged implementation. The
retained v0.58 experiment has a different storage and allocation-refusal
contract and must not be used as current performance evidence. The later
Vector trial below supplies the expanded ownership and current cost evidence.

## Current completion boundary at v0.68

This consolidated delivery contains Vector, Deque, Slab, HashMap,
PriorityQueue and OrderedMap, plus the complete multi-object indexed consumer.
All seven operation chains use local source and maintained callers. The
matrix supersedes the earlier implementation-order recommendation; the trial
sections retain their original measured revisions and rejected alternatives.
Completing an operation/ownership chain does not close its performance or
language questions in
[the maintained TODO](../../../docs/todo.md).

| Family or consumer | Established operation chain and source scope | Remaining cost boundary |
| --- | --- | --- |
| [Vector](../../../lib/std/collections/vector/grow-vector.wf) | Reserve/growing append, insert, ordered and swap removal, truncate, ordered drain and release; copy/drop/nodrop callers | Selected library chain complete. Extra drain movement and short-cycle lowering costs remain measured questions. |
| [Deque](../../../lib/std/collections/deque/deque.wf) | Both endpoints, wrap, logical visitation, consuming grow/shrink rebase, drain and release | Selected endpoint/rebase chain complete. Automatic reference-based growth and Ring two-span access are separate interfaces; scalar costs remain unresolved. |
| [Slab](../../../lib/std/collections/slab/slab.wf) | Lazy bounded slots, validated visit/edit with owned results, returned-owner exhaustion, removal, reuse, expiry, generation retirement and consumption | Selected stable-slot chain complete; the indexed composite below exercises multi-object memberships. Aggregate transfers, the extra cell word, independent retention tickets and surviving references remain separate questions. |
| [HashMap](../../../lib/std/collections/hash_map/hash-map.wf) | Generic owning collision/replacement/removal/reuse, lookup/edit, growth/rehash, visitation and consumption | Selected map chain complete. The owner rejected the [inactive-payload omission trial](#inactive-payload-initialization-compiler-trial), retaining baseline production clearing; wide result/migration costs and double-backing peaks retain their own evidence. |
| [PriorityQueue](../../../lib/std/collections/priority_queue/priority-queue.wf) | Arbitrary-T growth, peek/pop/replace-top, heapify, ordered drain and physical cleanup; copy/drop/nodrop callers and exact release ledgers | Plain chain and [matched comparison](../../experiments/container-representation/priority-library/RESULTS.md) complete, with qualified result-boundary and wide-sift costs. The [shared no-op comparison](#measured-shared-core-selection) shows no repeatable material regression; its remaining single-cohort possible benefit is not a speedup claim. |
| Indexed composite | [Multi-object weak/retained caller](../../../tests/programs/containers/indexed-membership-program.wf): Slab ownership, HashMap ID lookup/replacement, indexed reschedule/removal with reverse-position repair, expiry/reuse and complete owner cleanup | Complete correctness chain passes both lowering modes and exact 129-allocation ledgers. The owner selected shared-core reuse from the [matched comparison](../../experiments/container-representation/indexed-library/RESULTS.md#measured-result): all 48 indexed cells stay within variation in both series, while native costs depend on payload and operation. Independent tickets, unforgeable membership and surviving references remain unestablished; this is no native-parity claim. |
| [OrderedMap](../../../lib/std/collections/ordered_map/ordered-map.wf) | Arbitrary owning keys/values, find/edit/insert/replace, split/promotion, delete/borrow/merge/root contraction, ordered/range visitation and complete cleanup; the [maintained caller](../../../tests/programs/containers/ordered-map-program.wf) checks 103 allocations and each owner identity | The [complete baseline comparison and two rejected insertion trials](../../experiments/container-representation/ordered-library/RESULTS.md) preserve replacement, node-transfer and occupancy costs; no default tree or native-parity claim. |

The six libraries' maintained callers and the indexed composite are registered
through
[`compiler/tests/programs/containers.rs`](../../../compiler/tests/programs/containers.rs)
with sequential/parallel lowering and exact allocation-release ledgers. Those
checks establish their stated operation/ownership coverage, not native parity.
Research comparisons retain their measured source identities and explicit
replay commands; no research experiment is a correctness-gate dependency.

The composite exposed three compiler defects repaired under existing rules:
FN-4 read-under-write refinement, transitive nominal allocation layouts and
symbolic formal nominal inventory, as recorded above. Ring spans, richer
contract publication and whole-owner swap facts are specified limits; the full
sparse-map conditional-preservation
refusal above remains unclassified. Header-plus-tail storage, compact byte
pages, generic construction placement and concurrent reclamation remain
independent research consumers, not additional requirements on this slice.

### Reusable PriorityQueue trial

The question is whether an ordinary boxed binary heap supports the full
generic owning chain at competitive executable cost, including growth and
retained helper boundaries. The current comparator witness already mutates
through references and exchanges arbitrary T without holes; its scalar
instances and historical small-heap timings do not establish this larger
contract. This trial ran independently of the unchanged-source
inactive-storage comparison, which the owner subsequently rejected, and
selected no new storage or proof mechanism.

The candidate owns `Box<Slots<T>>` in `PriorityQueue<T, const ceiling: u64>`.
The ceiling bounds concrete allocation sites; growth doubles or saturates at
that ceiling, with zero capacity growing to one. `PriorityOrder<T, E>` supplies
`compare(env: &E, left: &T, right: &T) -> order: i32 reads(env), reads(left),
reads(right)`, where the sign selects order. Comparator consistency is needed
for meaningful heap ordering, not for bounds, ownership or termination of
the library's loops. All progress claims are conditional on callbacks returning.
No equal-priority stability, escaping reference or stable slot identity is
promised. The indexed consumer above supplies its own checked identity/position
relation; that protocol is separate from this plain-queue contract.

Proposed signatures follow; they are interface sketches with bodies omitted,
not checked source or a settled library API. `T` has no copy/drop bound.

```wf
fn priority_queue_new<T, const ceiling: u64>() -> made: PriorityQueue<T, ceiling> pure

fn priority_queue_len<T, const ceiling: u64>(queue: &PriorityQueue<T, ceiling>) -> length: u64 reads(queue.storage) contract {
  ensures length == deref(queue).storage.inner.len;
}

fn priority_queue_reserve<T, const ceiling: u64>(queue: &PriorityQueue<T, ceiling>, total: u64) -> capacity: u64 writes(queue.storage) contract {
  requires total <= ceiling;
  ensures capacity == deref(queue).storage.inner.cap;
  ensures capacity >= total;
  ensures deref(queue).storage.inner.cap >= deref(entry(queue)).storage.inner.cap;
  ensures deref(queue).storage.inner.len == deref(entry(queue)).storage.inner.len;
}

fn priority_queue_push<interface PriorityOrder<T, E>, const ceiling: u64>(queue: &PriorityQueue<T, ceiling>, value: T, env: &E) -> result: Result<unit, T> reads(env), writes(queue.storage)

fn priority_queue_peek<T, F, R, fn observe(env: &F, value: &T) -> result: R reads(value), writes(env), const ceiling: u64>(queue: &PriorityQueue<T, ceiling>, env: &F) -> result: R reads(queue.storage), writes(env) contract {
  requires deref(queue).storage.inner.len > 0_u64;
}

fn priority_queue_pop<interface PriorityOrder<T, E>, const ceiling: u64>(queue: &PriorityQueue<T, ceiling>, env: &E) -> removed: T reads(env), writes(queue.storage) contract {
  requires deref(queue).storage.inner.len > 0_u64;
  ensures deref(queue).storage.inner.len + 1_u64 == deref(entry(queue)).storage.inner.len;
  ensures deref(queue).storage.inner.cap == deref(entry(queue)).storage.inner.cap;
}

fn priority_queue_replace_top<interface PriorityOrder<T, E>, const ceiling: u64>(queue: &PriorityQueue<T, ceiling>, value: T, env: &E) -> removed: T reads(env), writes(queue.storage) contract {
  requires deref(queue).storage.inner.len > 0_u64;
  ensures deref(queue).storage.inner.len == deref(entry(queue)).storage.inner.len;
  ensures deref(queue).storage.inner.cap == deref(entry(queue)).storage.inner.cap;
}

fn priority_queue_heapify<interface PriorityOrder<T, E>, const ceiling: u64>(storage: Box<Slots<T>>, env: &E) -> made: PriorityQueue<T, ceiling> reads(env) contract {
  requires storage.inner.cap <= ceiling;
}

fn priority_queue_drain<interface PriorityOrder<T, E>, F, fn consume(env: &F, value: T) -> result: unit writes(env), const ceiling: u64>(queue: &PriorityQueue<T, ceiling>, order_env: &E, consume_env: &F) -> result: unit reads(order_env), writes(queue.storage), writes(consume_env) contract {
  ensures deref(queue).storage.inner.len == 0_u64;
  ensures deref(queue).storage.inner.cap == deref(entry(queue)).storage.inner.cap;
}

fn priority_queue_free<T, F, fn consume(env: &F, value: T) -> result: unit writes(env), const ceiling: u64>(queue: PriorityQueue<T, ceiling>, env: &F) -> result: unit writes(env)
```

`new` starts empty at zero capacity. `push` returns the offered owner unchanged
in Err when length has reached the ceiling, and Ok after insertion otherwise;
this is an application capacity outcome, not allocation failure. Reserve
follows Vector's caller-proved bound. Heapify consumes an already initialized
boxed prefix without allocation and builds bottom-up. Drain consumes in pop
order while preserving capacity, so it costs O(n log n); final free instead
consumes in reverse physical-slot order in O(n), then releases the backing.
Both callbacks receive their disjoint environment and current owner only.

The first public-access discriminator is a caller that tests the result of
`priority_queue_len`, then peeks or pops using its published relation without
reading representation fields. Proved-nonempty operations return the owner
directly and let one loop-bound proof serve repeated pops. The alternative
`Option<T>` pop and optional callback result handle emptiness dynamically but
add a tagged owning-result boundary. Compare that alternative if the ordinary
writer chain cannot use the published relation or its boundary remains costly;
do not add parallel try/unchecked APIs or a missing-fact branch merely to make
the implementation pass. Refusal/retry and empty/pop outcomes must match on
both sides of any performance comparison. No richer FN-9 rule is assumed.

Before selecting the candidate, require geometric growth, strictly decreasing
parent indices or increasing bounded child indices in every sift, O(log n)
sifts and O(n) bottom-up heapify. Prove child arithmetic before computing it,
including zero-sized payload instances whose capacity has no positive-stride
bound. Comparator answers must not restart a scan or authorize a partial
operation. The O(n log n) ordered drain and O(n) final cleanup are distinct
contracts; compare each with the same native order and ownership outcome.

The formal caller belongs beside the existing library callers and bundles the
actual library through the existing corpus runner. Use an independent sorted
sequence oracle and exact owner identities/releases for copy, owning drop and
nodrop elements: zero/singleton/irregular ceilings, growth and refusal/retry,
peek, replacement, heapify, drain/reuse and partial final cleanup. Equal-priority
tests check contents without assuming stability; a tie-breaker supplies exact
order where required. Always-equal, always-greater and cyclic comparisons must
still finish the bounded loops without losing or duplicating owners.

The prospective cost comparison uses the same source contract, capacities,
growth policy, inputs and checksums in WF and direct C, with scalar and wide
inline owning payloads, ordinary optimization and retained helpers. Separate
reserved churn/replacement, growing fill/pop, heapify/pop and setup/cleanup;
record full backing/peak bytes, allocations and actual transfers, including
any C hole-sift advantage over whole-element swaps. A source-shaped C control
can isolate that algorithmic cost. Keep construction and execution time
separate and repeat in reversed orders with unchanged C controls. Select an
optimization only when the same-contract improvement repeats beyond control
variation and its emitted-code or algorithmic cause is established; retain
tradeoffs per workload instead of averaging an unmeasured application mix.
A large unexplained cost reopens a bounded implementation/algorithm comparison
before broadening the slice. No universal parity threshold or new mechanism
follows from acceptance, copy counts or a single timing.

The registered matrix has 30 workload cells: `u64` (8 bytes) and an inline
`nocopy`/`nodrop` 32-word owner (256 bytes), each at lengths 16, 256 and 4096,
for reserved pop/push churn, reserved replace-top churn, geometric fill/pop,
initialized-prefix bottom-up heapify/pop, and initialized-prefix setup/cleanup.
The wide inline owner stresses movement; nested Box ownership and exact
release identities belong to the separate formal caller. Each cell compares
WF, source-shaped full-slot-swap C, and direct hole-sift C in normal and
retained-public-operation modes, with seven deterministic samples and two reversed
cohorts: 2,520 rows. The sample work target is 16,384 scalar or 4,096 wide
items, with fixed repetitions of at least one. Complete traces include their
construction, comparator and consume callbacks, growth where selected, and
cleanup; setup/cleanup is a separately reported trace rather than a subtracted
estimate. All implementations use the same accounting allocator and backing
header, capacity policy, input stream, comparator, ownership outcomes and
callback order. Report requested and peak backing bytes, including zero-capacity
headers and simultaneous old/new allocations during growth.
Retained mode preserves `new`, `len`, `reserve`, `push`, `peek`, `pop`,
`replace_top`, `heapify`, `drain`, `free`, and payload callbacks. Private
child-selection, room-making and sift helpers remain ordinarily optimizable
in both languages; inspect their actual surviving calls without forcing a
separate boundary. A normal/retained difference does not isolate call latency.

Before timing, both C implementations and the WF scalar/owning traces must
match an independent sorted-sequence oracle, checksum and allocation ledger
over the full operation chain. The hypotheses are that full-slot swaps explain
part of the wide direct-C gap, retained aggregate boundaries may add transfers,
and a remaining scalar gap against source-shaped C may expose address or
comparison lowering. These are hypotheses, not conclusions from transfer
counts. The hole-sift control distinguishes algorithms, not language parity.
Use emitted code and actual transfer/comparison counts to attribute a gap;
report remaining uncertainty and the direction in each cohort. The selection
criterion above requires improvement beyond unchanged-control variation in
both cohorts before selecting an optimization. Construction has a 60-second
budget and the full timing matrix another 60 seconds; investigate an overrun
before extending either. Build and execution time remain separate. The
explicit experiment Makefile and its `RESULTS.md` own the reproducible evidence
under `research/experiments/container-representation/priority-library`; no
ordinary correctness gate depends on that research directory. Retire its
sources or harness when the comparison is superseded and no maintained claim
depends on their replay.

**Design suitability at registration.** A boxed prefix and borrowed comparator build on the
current generic witness, preserve arbitrary ownership and give the indexed
consumer a reusable heap core. The proposed nonempty interface needs the
public-access proof discriminator above; wide-element sift movement and result
transfers need the matched experiment. Indexed updates and the full ordered
chain were reserved for the later trials now recorded above. The existing cost and language
questions keep their own reopening criteria rather than becoming implied
prerequisites for this implementation.

### PriorityQueue source and proof boundary

The [ordinary library](../../../lib/std/collections/priority_queue/priority-queue.wf) implements
the registered operation chain. The maintained
[caller](../../../tests/programs/containers/priority-queue-program.wf) admits
and executes under sequential and CLI-parallel lowering on the unchanged
v0.68 compiler. Its independent insertion-sort oracle checks scalar ordering;
separate identity ledgers check droppable Box owners, wide nodrop owners,
refusal/retry, replacement, drain/reuse and partial final cleanup. Always-equal,
always-positive and cyclic comparisons still finish and preserve every owner.
Both native allocation observers report exactly 63 allocations, each released
once: 23 backings and 40 payload Boxes. Full canonical-gate and cost evidence
remain separate from these focused checks.

The public-length discriminator succeeds: testing `priority_queue_len` supplies
the ordinary contract fact needed to call peek, replace-top and pop without
reading the queue's representation. A fieldless zero-byte element also admits
all sift arithmetic with ceiling `18446744073709551615`. Unit is one byte on
this target and cannot serve as that zero-byte qualification control.

Two exact refused forms explain the extra source proof work. With
`parents = count / 2_u64`, after the leaf exit has established `at < parents`,
the automatic proof alone does not establish:

```wf
invariant children: 2_u64 * at + 2_u64 <= count;
```

The library supplies INV-1's ordinary finite certificate instead:

```wf
invariant children: 2_u64 * at + 2_u64 <= count {
  use (2_u64 * parents <= count);
  use 2 times (at < parents);
}
```

For mutable child selection, the following branch is semantically bounded,
but its post-join strict index bound is not retained by ENT-6:

```wf
let best = left;
if right < count {
  let sibling_order = PriorityOrder::compare(env: env, left: &deref(queue).storage.inner[right], right: &deref(queue).storage.inner[left]);
  if sibling_order < 0_i32 {
    set best = right;
  }
}
invariant selected: best < count;
```

Here `left = 2*at+1` and `right = 2*at+2`. The joined offset interval is
`[1,2]`, while the separately proved child arithmetic gives only
`2*at+2 <= count`; the stronger right-child guard is not a fact common to
every predecessor. The read-only `priority_queue_child` helper proves the
bound at each selected return and publishes it through FN-9. This adds no
run-time guard or source acceptance exception. Private helper inlining remains
an ordinary optimizer choice in both cost modes.

Internal sift contracts state their loop-header length facts as paired
inequalities. Drain additionally restates preserved capacity at its loop exit,
following the existing Deque form. These are ordinary proof spellings, not
compiler or specification changes. Generic array snapshot forwarding, indexed
position repair and the full ordered-container chain are not claims made by
this first queue implementation.

The [complete cost matrix](../../experiments/container-representation/priority-library/RESULTS.md)
retains 2,520 samples, both cohorts and all independent-oracle checks. Large
scalar pop/push and growth are comparable to the native controls in this run;
retained small/medium scalar pop/push is 1.510--1.722 times swap C. Wide sifts
also retain more movement than native hole sifting. The Result ABI and
inactive-result stores are specific code differences, not isolated timing
attributions. This establishes a reusable ordinary implementation and a
replayable cost baseline, not uniform native parity or an optimal heap fanout.
The owner adopted this baseline and its distinct ordered-drain/physical-cleanup
boundary in the [priority-queue storage decisions](../../../design/language/data-model/priority-queue-storage.md).
The maintained TODO retains these separate validation questions.

## Vector consumption trial

The first library implementation continues from merged `8c02e875`. Its
question is whether ordinary prefix-window operations can provide ordered
consumption, arbitrary removal and complete ownership cleanup at a competitive
cost, including when the element is large or must be explicitly consumed.

Use the existing `GrowVector<T, ceiling>` and `VectorDrain<T, E>` boundary.
Ordered truncation takes a retained length, requires it not to exceed the
current length, and hands the removed suffix to the member in original order.
Drain is truncation to zero. Both preserve capacity and publish their exit
length. Unordered removal swaps the selected slot with the last slot and
takes the last value; OP-11 admits equal indexes, so removing the last element
needs no special alias branch. Empty-vector destruction consumes its owner
under the ordinary OP-14 empty-window requirement.

The consuming member's `writes(env)` and the vector helper's
`writes(values.storage)` must be disjoint at the call under EFF-5. The member
receives no reference to the vector. Consequently it cannot inspect the
remaining suffix while the helper is rearranging it. Reverse just that suffix,
then take from the back: the member observes the original order, the retained
prefix is unchanged, and no scratch allocation or invalid slot is introduced.
This is a synchronous operation, not a resumable drain value or an interface
that lets a callback inspect partial container contents.

The trial compares this composition with two C controls: the same
reverse-and-take algorithm and a direct ordered consumer over the same
unobservable backing. Both have the same observable callbacks, retained
capacity and allocation policy. Their difference isolates the source
composition's extra element movement instead of charging it to language-call
overhead. An allocation/free/memmove growth control matches the current
lowering; it is not evidence against a separately measured realloc policy.

Before measuring, use these discriminators:

- Ordered drain/truncation must have O(removed length) element movement,
  preserve the exact callback sequence and retained prefix, and allocate
  nothing. Swap-remove must have O(1) element movement and return the selected
  owner. Test empty, singleton, same-index, boundary and large-length cases.
- Extend the existing formal corpus bundle instead of adding another native
  test framework. Exercise copy, droppable owning and nodrop owning elements
  through construction, growth, insertion, removal, consuming truncation,
  drain, reuse and final release. Check every allocated identity is released
  once and every nodrop payload is explicitly consumed; no synthetic OOM
  outcome belongs to the global-heap contract.
- Measure the actual library, not a benchmark-only implementation, for scalar
  and large owned records, short and long windows, ordinary optimization and
  retained helpers. Compare complete outputs and allocation bytes before
  timing; interleave implementations and retain raw samples. Separate
  construction/growth from a preallocated consumption loop where possible.
- Count optimized transfers and retained calls to explain a gap. The
  algorithm-matched control distinguishes lowering cost; the direct control
  distinguishes the ordinary composition's cost. If a material gap remains,
  record its concrete operation and size domain before choosing another
  representation or proposing language support. O(n) alone is not a parity
  claim, and no percentage threshold is invented for all workloads.

The original consumption decision is recorded in
[`design/language/data-model/vector-consumption.md`](../../../design/language/data-model/vector-consumption.md).
The follow-up below proposes replacing its separate reversal pass.

The initial trial implemented the selected operations, including explicit cleanup
of a nodrop element vector. The formal bundle observes original callback order,
retained contents and capacity, reuse and 25 exact-once allocation releases in
sequential and parallel lowering. The initial
[cost comparison](../../experiments/container-representation/vector-library/RESULTS.md)
used the actual library, matched reverse C and direct C, and scalar/256-byte
elements. It isolated an unnecessary Slots wrap computation and retained the
measured cost of suffix reversal after that repair. That source form established
an O(n) baseline, not native parity across workloads. Both its residual
performance question and the separate Box-measure placement defect motivated
the follow-up below.

### Vector source obligations

An ordinary loop's header hypothesis does not survive its exit [ENT-5].
Consequently, this source does not establish its declared postcondition:

```whitefoot
loop @truncate (
  invariant prefix: deref(values).storage.inner.len >= retained
) {
  if deref(values).storage.inner.len <= retained {
    break @truncate;
  }
  let value = take_back(window: &deref(values).storage.inner);
  VectorDrain::accept(env: env, value: move value);
}
return unit;
```

FN-9 cannot prove the exit length equal to `retained`. Publishing
`invariant exhausted: deref(values).storage.inner.len == retained;` immediately
before the break uses INV-1's existing local theorem route, which ENT-5 retains
across that edge. The theorem is proved from the header and the exit guard;
it introduces no runtime branch. This is a specified proof-writing boundary,
not a compiler bug or a relaxed postcondition.

The initial compiler rejected the following admitted consume under PROV-6:

```whitefoot
fn grow_vector_free_empty<T, const ceiling: u64>(values: own GrowVector<T, ceiling>) -> result: own unit pure contract {
  requires values.storage.inner.len <= 0_u64;
} {
  free_empty(window: move values.storage);
  return unit;
}
```

The only field moves into the operation, leaving no residual. WIN-3 consumes
the whole wrapper; PROV-6 refuses a linear *remaining* part, not the moved
field. The checker instead tested the complete original type. The repair
judges the residual inventory and keeps unbounded generic and fieldless
nodrop residuals in that inventory even when their release emits no action.
Positive and negative compiler tests cover that distinction; the library's
nodrop chain exercises the complete source-to-native cleanup path.

The approved ENT-2/MSR-3 clarification makes the owned descendants explicit
at an ordinary naming event such as taking this wrapper apart:

```whitefoot
let GrowVector(storage: storage) = move values;
free_empty(window: move storage);
```

The `efe41016` compiler loses `values.storage.inner.len == 0` at that naming
event and rejects the second line under OP-14 because its placement walk
stops at Box content. The follow-up carries the established emptiness fact
through both forms. The
[`descriptor_invalidation` regressions](../../../compiler/src/semantic/tests/descriptor_invalidation.rs)
cover this exact generic witness, projected and recursive Box moves,
constructor placement into Box content and elements, and overlapping writes
that must still kill an old fact. Completion review found that v0.62's
placement table did not explicitly cover measured descendants of an unmeasured
owner and that its datum identity omitted their relative projection. The
owner-approved v0.63 amendment states that boundary and identity; the repair
implements it using existing measure datums. This does not require a new
representation of an empty owning vector.

FN-8's Signed Goal affine route has a separate spelling boundary [ENT-6]:

```whitefoot
requires deref(values).storage.inner.len == 0_u64;
```

With only a proved loop-header theorem supplying emptiness, this equality
requirement is unproved even when a local invariant can restate that same
equality. ENT-6's affine Signed Goal leaves are the four order comparisons,
not equality. The equivalent `requires deref(values).storage.inner.len <=
0_u64;` succeeds because the length is an unsigned measure. The reuse work
helper and empty-owner consumer use that spelling, without a runtime test or
weaker domain. FN-9's numeric postcondition route still permits equality.

## Box placement and Vector consumption follow-up

This comparison starts from `efe41016d10379325ed4513d0ac7457ec7f24c5b`,
kernel v0.61, including the Vector trial and the wildcard-reference amendment.
Earlier observations retain their recorded baselines; the old absence of
wildcard traversal is not a premise of this follow-up. The two remaining
questions are ordinary Box-content measure transport and the extra transfers
required by ordered Vector consumption.
The integrated measurement revision also includes main `f3cf41d4`, kernel
v0.62. Paired source measurements use one rebuilt integrated compiler; earlier
v0.61 compiler comparisons retain their own identities and conditions.
Following completion review, the owner approved the v0.63 ENT-2/MSR-3
clarification: existing within-body placements carry current facts through
exact owned fields, enum payloads and Box content, with the relative descendant
projection included in datum identity. Existing kills, cross-function contract
boundaries and the exclusion of implicit window slots remain. The amendment
adds no runtime check and does not change the measured lowering or library
source. The dated measurements remain v0.62 evidence, not a new v0.63 run.

The Box witness above is a naming event, not a new relation theorem. The
selected repair transfers available measure facts through the owned descendants
now explicit in MSR-3 and preserves ENT-5's invalidation after an overlapping write,
replacement or call. Check both directions: an admitted consume becomes
provable after rebinding, and an obsolete pre-mutation measure cannot authorize
a later operation. Recursive nominal types must not cause infinite path
enumeration or an arbitrary depth limit. The v0.61 distinction between a
possible descendant cover and an exact captured target still applies.

For consumption, reuse the actual library and the existing reverse-C and
direct-C controls. Keep callback order, retained prefix and capacity, nodrop
ownership, allocation policy and helper-retention settings fixed. A candidate
that introduces scratch allocation, omits consuming callbacks or weakens their
allowed effects does not meet this operation contract. Distinguish an ordinary
algorithm or representation change from a general lowering repair and from an
operation the current source rules cannot express.

Before selecting a candidate, compare it with both the unchanged WF baseline
and the native controls in paired runs. Count the source-required element
transfers and inspect optimized IR with helpers retained; then measure the
existing scalar/large-record and short/long-window cases with ordinary and
retained helpers. The candidate must remove or demonstrably reduce the
identified consumption cost, with reproducible improvement in the affected
large-record path and an explicit account of changes to the other operations.
A new attribution alone is not an improvement result. Any native-parity claim
must be supported by the resulting comparisons, including measurement
variation, rather than inferred from O(n) complexity or fewer IR instructions.

The placement repair uses the existing measure datums and invalidation path.
Owned fields, payloads and Box content share the structural inventory. When
a nominal recurs, the structural walk stops and exact already-interned source
measure paths supply deeper descendants after replay against the operand's
type. Earlier numeric evidence already requires those finite terms; even a
capacity- or head-only read registers the sibling length term. Unmentioned
descendants need only their ordinary standing type facts. The inventory does
not retain old facts or convert an alias cover into an exact owner: a datum
is equated to its source only in the current fact state. Each placement
finishes collecting paths before minting datums, and the entailment walk
visits each static statement once, including loop bodies. Acyclic operands
skip the supplementary scan.

Unbounded type unfolding cannot terminate on a recursive nominal; merely
cutting the cycle loses known facts at deeper written paths, and a fixed
depth cutoff has no language ground. Reusing existing exact terms avoids a
second path-analysis pass. The approved compiler-tree addition records this
choice. The focused `descriptor_invalidation` group passes all 17 cases,
including distinct measures beyond two recursive nominal cycles,
capacity/head-only evidence, an overwritten recursive descendant and a cursor
whose possible-descendant fact cannot establish the owner's measure. The
original generic witness fails under `efe41016` and passes after the repair.
The recursive cases also exposed a constructor-placement omission at
`set Box.inner = ...`; using the existing exact destination path repairs that
naming event without changing commit order or invalidation. These focused
checks distinguish the placement repair; the canonical gate also covers its
other semantic, lowering and native-program consumers. The normative
[`owned-descendant` positive](../../../tests/conformance/cases/msr3-pos-owned-descendant-measures.wf)
carries two distinct lengths through fields, Box content, owner rebinding,
an enum payload and destructuring. Its
[`replacement` negative](../../../tests/conformance/cases/msr3-neg-replaced-owned-descendant-measure.wf)
requires an overlapping replacement to invalidate the old length.

The consumption candidate takes a rear element into an owned local before
exchanging it with the next suffix slot and calling the consumer. At offset
`k < floor(removed / 2)`, the post-take length still exceeds `retained + k`;
the source writes that bound as an ordinary finite certificate. The reversed
remainder is then consumed from the back. Only removed elements move, so
almost complete retention does not turn truncation into a prefix walk. The
interface, permitted callback effects, retained contents and capacity are
unchanged. This still relocates rear elements and is not a minimum-transfer
algorithm.

Two target choices address the demonstrated extra temporary copies. A complete
take captures the old physical slot, updates the window descriptor, then
transfers the element. Its header and element bytes are disjoint, and no call,
release or source observation occurs between those steps. Zero-size elements
touch no bytes. Separately, unrelated storage groups become independent entry
stack allocations only after complete target qualification establishes positive
sizes, one common natural alignment, no requested-alignment discrepancy and
no inter-group or tail padding. Every eligible ordering has the same complete
extent. Mixed alignment, padding and zero-size roots retain the qualified
frame struct; complete parents, storage interference, result destinations,
probing and parallel lifetimes are unchanged. The old canonical offsets
remain qualification accounting, not physical offsets between independent
objects.

The [transfer experiment](../../experiments/container-representation/vector-library/RESULTS.md)
records the alternatives and isolating evidence. Early capture annotations or
independent slots alone did not reduce the candidate's four transfers per
first-half iteration; the descriptor order and independent slots together
reduced them to two after local Clang 21 optimization. Apple Clang 15 retains
an additional immutable argument snapshot in the straight-line regression,
so this is not an optimizer-independent copy-count guarantee. The regression
checks the compiler-owned allocation and descriptor-order properties and the
native result; optimized transfer counts remain toolchain-specific evidence.
Forwarding a consumed mutable local into an ordinary call is a separate
opportunity: it needs a liveness and interference argument across all arguments
and result/input reuse, and the existing result-coalescing path does not cover
a consumer returning unit. That opportunity is retained in the maintained
TODO rather than broadening the present changes without that argument.
The unchanged v0.61 compiler emits one aggregate frame for the regression's
distinct roots and copies the taken element before updating the descriptor;
each new raw-IR assertion therefore distinguishes its corresponding lowering
change without depending on a downstream optimizer.
Broad ABI promises and
unrestricted frame splitting were therefore not adopted. The general mixed-
alignment case needs a separate complete-frame argument if a concrete workload
later demonstrates a benefit. The owner approved the library and compiler
choices, including the measured large-record benefit alongside the repeatable
8.2–10.6 percent short-scalar regression. Remaining consumption and lowering
costs stay in `docs/todo.md`; the selection claims neither uniform improvement
nor native parity. Measurements compare both source algorithms through the
same integrated compiler and keep the historical compiler comparisons separate.

## Slab and Deque trial over v0.63

This comparison began at merged `3a969235`, kernel v0.63. Box descendant
placement and the selected Vector consumption repair were available; the
remaining Vector timing costs retain their explicit reopening conditions.
The end-to-end question is whether stable-slot reuse and a two-ended
queue can support copyable, owning droppable and must-consume elements over
ordinary values at an attributable native cost. The candidates below began as
implementation trials; the selection and remaining-cost sections record their
outcomes without a claim of native parity.

For Slab, first try one boxed backing of cells, each containing an inline
`Slots<T, 1>`, a generation and a free-list link. The inner window expresses
vacancy with its ordinary zero-or-one length; insertion and removal use the
existing place/take rows. An enum of vacant/live cells is a useful compact
native control, but a generic source exchange currently loses the variant
needed to consume an extracted vacant enum without an impossible ownership
arm. Separate metadata plus dense payloads is another ordinary alternative;
it spends a second backing and repairs reverse indexes on removal. The
single-slot candidate is selected for a trial because it keeps one backing,
constant-time reuse and each live payload in its slot without that repair.
Its possible extra metadata word must be measured, not treated as free.

The Slab constructor accepts a runtime capacity within a written const ceiling,
which bounds allocation size, and materializes cells only on first insertion.
Free-list reuse concerns already materialized empty cells. A slot retires
when its generation reaches a caller-selected limit rather than wrapping;
small limits exercise that same production rule. Exhaustion returns the
uninserted owner. Handles are relative to the supplied slab and are ordinary
data, not brands authenticating the allocation. The membership caller compares
weak indexes with a composite retained protocol: removing one index preserves
a remaining reader, object deletion expires weak handles, and the retained
protocol refuses object deletion until its memberships are removed. This is
not a theorem about arbitrary clients maintaining or being unable to forge
ordinary bookkeeping fields.

For Deque, keep endpoint mutation over references to `Box<Ring<T>>` and make
reallocation an explicit consuming rebase that returns a newly allocated
owner. A counted front-take/back-place loop can prove exact length
preservation and release the emptied old backing for arbitrary T. This is a
new-owner conversion, not an automatic-growth promise on a reference helper.
The ordinary `swap` row does not publish exchanged measures, so exchanging
old/new owners does not by itself establish the old-empty and new-length
relations needed by that other interface. REF-4 also still refuses a Ring
range, including an empty one: per-slot visitation does not supply zero-copy
two-span access. Retain the exact rejected programs and these distinctions
alongside the executable candidate.

Before selecting either representation, require complete operation and cleanup
chains with independent results, generation/expiry and wrap boundary cases,
and exact allocation-release accounting through the ordinary formal tests.
Reuse their existing sequential and parallel construction paths; the older
three-mode and allocation-refusal prescriptions are not the v0.63 contract.
Experiments stay outside daily correctness checking. For each candidate,
compare the actual library with C implementing the same representation and
operation contract, then a compact-slot or bulk-rebase control where it
separates a source/layout cost. Record actual bytes per capacity unit,
construction cost separately from steady-state lookup or churn, allocation
counts, and emitted transfers with ordinary and retained helpers. Use scalar
and large owning records, short and longer capacities, and checksum-sensitive
operation sequences. A runtime branch or extra word is a measured cost;
acceptance alone does not select it, and a changed ownership or callback
contract is not a faster implementation of the same operation.

### Executed library boundary

The [Slab](../../../lib/std/collections/slab/slab.wf) and
[Deque](../../../lib/std/collections/deque/deque.wf) sources have complete native callers
in the formal corpus, reusing its sequential and parallel modes and shared
allocation observer. Slab covers copy, owned Box and nodrop elements, lazy
materialization, genuine full outcomes, wrong/expired handles, reuse and
generation limits zero, one and u64 maximum. Deque adds zero-sized elements,
zero/one/full capacities, wrapping from both ends, growth and shrink through
rebase, callback order and reuse. Its precise back-append row also permits an
existing filled-slot reference to remain valid. The independent release
ledgers count 19 successful Slab allocations and 21 Deque allocations, including
the membership caller; both ordinary deallocation and observed images execute.
Heap allocation itself is total under STOR-8, so these are not allocation-
refusal tests. Slot exhaustion is a separate ordinary Slab outcome.

The [membership program](../../../tests/programs/containers/slab-membership-program.wf)
answers the retained-membership question for a concrete composite: removing
one index preserves the other reader; weak indexes expire on owner deletion;
the retained composite refuses deletion with two memberships and still with
one, then permits it after both retire. Its two index fields cover one central
object. The later [indexed composite](#maintained-composite-correctness)
establishes the coordinated multi-object operation and ownership chain.
Neither example protects ordinary bookkeeping from independently authored
mutations or supplies independently held retention tickets or surviving
references.

The matched [Slab comparison](../../experiments/container-representation/slab-library/RESULTS.md)
and [Deque comparison](../../experiments/container-representation/deque-library/RESULTS.md)
own cost conditions and results. Source acceptance and the formal execution
above are not performance selection evidence.

### Selection and remaining costs

The selected Slab choice keeps one allocation and stable cell positions for
arbitrary owned T. Its scalar cell costs 32 bytes against the tagged C
control's 24; the 256-byte payload costs 280 against 272. The C window/tagged
comparison isolates that extra word, while WF/window timings also include
result layout and call lowering. Retained wide removal and consumption still
perform redundant transfers, so these results do not select the representation
as a performance ceiling. Keep this ordinary implementation available for
composition, and test the remaining transfers before using a Slab cost alone
to justify a compiler-known sparse layout. The approved
`slab-storage` decision records that qualified choice.

The selected Deque choice keeps precise endpoint rows and an explicit new-owner
conversion. Its strongest measured gap is ordinary scalar forward churn;
retaining helpers largely removes that gap, exposing optimization of the
inlined address path rather than an unavoidable reference-interface cost.
A bounded GEP-fact probe removes the redundant descriptor traffic, but its
general target qualification and timing recovery remain unverified. Keep the
library interface and the measured baseline while qualifying that optimization;
do not add a Ring growth primitive solely from this comparison. The approved
`deque-rebase` decision records the interface and its two-span limitation.

These selected library choices supplement the typed-constant representation
choice and the two approved corrections to storage/range correspondence. The
two new leaf nodes do not increase the tree's depth.
The affected current guidance is the container writer pattern and library
README; unresolved source interfaces and lowering opportunities remain in
`docs/todo.md`. No specification or conformance verdict change is part of this
library slice.

The original CSVs and artifact hashes remain evidence for the recorded
pre-merge compiler. Main `7127bcb6` adds checker and aggregate-lowering repairs;
its clean merge also required supplying the const-type inventory to a newly
added unit-test context. The earlier emission comparison established only the
const-forwarding repair's correspondence, not the main integration. The
integrated compiler at `dcbfdc0f` was then checked separately: Deque raw and
optimized IR were byte-identical to the preserved baseline, while Slab's
optimized IR differed only in SSA names after the alias removal. Compiling
the old/current optimized modules with Apple Clang 21 on arm64 produced
byte-identical complete WF module assembly in both normal and retained modes.
The [Slab integration record](../../experiments/container-representation/slab-library/RESULTS.md#integration-verification-at-dcbfdc0f)
and [Deque integration record](../../experiments/container-representation/deque-library/RESULTS.md#integration-verification-at-dcbfdc0f)
give the identities, method and rerun correctness checks. This establishes
that emitted-module correspondence, not linked-image identity or a new timing
run; the original CSVs keep their original compiler and measurement identity.

The subsequent merge of main `95b21cfd` at `4da1710e` was checked with the
rebuilt compiler as well. Fresh raw and normal/retained optimized LLVM for
both libraries matched the saved `dcbfdc0f` artifacts byte-for-byte; all four
C-control optimized modules were unchanged. The same integration records
give this compiler identity and the bounded re-emission commands. The earlier
native, assembly and timing evidence retains its original scope and identity.

Main `e8e1c411`, integrated at `ce9a3870`, subsequently changed Ring front
predecessor lowering and the resource-record writer. Slab's raw module changed,
but both complete optimized modules remained byte-identical, so its earlier
timing evidence was retained with that qualification. Deque's reverse-churn
path changed and was measured again with the unchanged full matrix and C
controls: 2,592 correctness executions passed and 6,336 fresh samples are in
[`measurements-v0.64.csv`](../../experiments/container-representation/deque-library/measurements-v0.64.csv).
The RESULTS records separate this new measurement from the preserved baseline
and identify the changed helper instructions; no Slab timing or optional GEP
probe was rerun.

### Exact unavailable source forms

These are the rejected additions or functions in the linked library's type
context. They state current rules, not proposed amendments.

| Rejected source | Rule and cause | Implemented alternative |
| --- | --- | --- |
| In `slab_new`: `ensures result.cells.inner.len == 0_u64;` | FN-9's result-selector domain does not include an arbitrary aggregate result field. A nominal Deque wrapper's `made.storage.inner.len` has the same limit. | Slab retains its necessary free-list state; the caller establishes length through an ordinary read/branch. Deque needs no extra wrapper state and uses direct `Box<Ring<T>>`, whose `made.inner.len` is admitted. |
| In `slab_find_index`: `ensures when Ok(value: index): deref(slab).cells.inner[index].storage.len > 0_u64;` | FN-9/CALL-4 do not admit this indexed postcondition target. | Export the outer index bound; use `slab_visit` to keep validation and callback in one helper, or re-read occupancy before direct access. |
| The signature `fn borrow_out<T>(value: &T) -> result: &T reads(value) {` | Current GRAM-3 admits a value type at the result; REF-3/FN-1 prohibit reference escape. | Return owned callback data or a validated index. |
| After `let values = box_ring_new::<u64>(capacity: 4_u64);`: `let count = observe::<u64>(first: &values.inner[0_u64..0_u64], second: &values.inner[0_u64..0_u64]);`, with `observe` taking two `&[T]` arguments | REF-4 refuses even empty Ring ranges. | Per-slot visitation; this remains an explicit missing zero-copy two-span interface. |

The vacant variant does not travel through swap's row to the extracted local.
This complete generic helper rejects under PROV-6 because the admitted facts
cannot show that `offered` no longer holds a must-consume T:

```wf
fn insert<T>(slot: &Option<T>, value: own T) -> result: own Result<unit, T> writes(slot) {
  match deref(slot) {
    Some(value: occupied) => {
      return Err<unit, T>(error: move value);
    }
    None() => {
    }
  }
  let offered = Some<T>(value: move value);
  swap(first: slot, second: &offered);
  return Ok<unit, T>(value: unit);
}
```

The library's `Slots<T,1>` carries vacancy as an ordinary length whose
place/take contracts are available. No impossible occupied cleanup arm or
implicit nodrop discard is introduced. A native tagged cell remains a layout
comparator; the Slab result does not solve every map operation.

Swap also does not publish exchanged measures. Its ordinary write row kills
both operands' old facts under ENT-5, so this helper fails its FN-9 return
obligation:

```wf
fn replace_empty<T>(old: &Box<Ring<T>>, built: own Box<Ring<T>>) -> previous: own Box<Ring<T>> writes(old) contract {
  requires deref(old).inner.len == 0_u64;
  ensures previous.inner.len == 0_u64;
} {
  swap(first: old, second: &built);
  return move built;
}
```

A reread could validate an empty branch, but would not consume a nonempty
nodrop remainder on the other branch. The existing
[atomic-publish probe](../../experiments/container-representation/x1/linear-ring-publish.wf)
also rejects under OP-12/WIN-3 for a nodrop target. The implemented rebase
consumes the original owner directly and returns a genuinely new backing;
no exchange fact is needed. Its counted element loop proves exact length;
append's current lower-bound contract does not state the two-entry sum.

### Ring range correspondence

The [range-reference decision](../../../design/language/ownership/range-reference.md)
previously listed Ring. Commit `a907d9d6` adopted that candidate text. The earlier
`c79188a1` investigation's
[amendment map](../access-effects/SPEC-AMENDMENT-MAP.md) had asked whether to
refuse Ring ranges or admit proved non-wrapping ones, recommending refusal
under its owner-rulings-needed heading. The subsequent `7bc07c04` specification
draft introduced today's blanket refusal. Neither its commit message nor the
retained PR #70 discussion supplies a Ring-specific ruling. The earlier
released v0.59 VIEW-2 had admitted proved non-wrapping views, including empty
ones. This established tree/spec drift and an unrecovered selection
ground; it does not establish that the narrowing was unauthorized.

The owner selected retention of REF-4 as a fresh ruling: the tree now excludes
Ring ranges, including empty and non-wrapping ones. The alternative of admitting
proved-contiguous Ring ranges still needs an explicit empty-range rule plus
formation, invalidation and lowering evidence. Arbitrary wrapped logical ranges
would also exceed the selected pointer/count representation. The ruling does
not reconstruct the missing historical ground and does not make slot visitation
a replacement for a two-span consumer; that capability remains a follow-up.

### Compiler corrections exposed by composition

The Slab visitor uncovered a partial-instantiation spelling defect. A generic
callee binding R to unit while forwarding a type, const or function parameter
was treated as its own symbolic spelling authority; `move` was then rejected
as a copy move. OWN-1/FN-2 assigns that authority to the written template, not
every partly substituted body. The correction compares declaration identities
with the template's own symbolic substitution. The generic tests retain
canonical copy-move and repeated-consume negatives; no ownership or overlap
rule is weakened.

A separate const-guard probe exposed lost source type information:

```wf
fn increment<const limit: u64>(value: own u64) -> result: own u64 pure {
  if value < limit {
    return value + 1_u64;
  }
  return value;
}
```

The missing implicit maximum prevented ENT-2/ENT-4 from proving the OP-2
increment domain. A local alias or a redundant written invariant happened to
supply enough evidence through a different path. Conversely, the affine image
path assigned u64 bounds even to a signed const parameter, incorrectly
accepting a symbolic assertion that every i8 parameter is nonnegative.
This demonstrated an incorrect template judgment, not an executable negative-
value misuse. MSR-6 already requires the exact declared integer type.

The correction retains that type with the declaration-anchored term and its
affine image. A read-only declaration-type inventory serves the ordinary
entailment and isolated refinement contexts, avoiding type propagation through
every storage-extent and captured-index representation. This distinction
matters when a u8 const parameter supplies a u64 formal or storage extent:
the constant must remain one identity with its source bounds. It adds no
automatic proof family. The corresponding compiler choice is recorded in
checker-facts; domain, insufficient-guard and forwarding regressions
exercise the existing proof checker.

## Ring payload address qualification

The next lowering experiment starts from merged main `1b916975`. The
[Deque comparison](../../experiments/container-representation/deque-library/RESULTS.md)
records a 2.256–2.405x normal-mode scalar forward-churn cost against its C
control. Its separate, untimed four-GEP probe identifies an unsigned offset
fact that removes repeated descriptor traffic in one concrete instance.
That is a reason to investigate the general lowering contract, not evidence
that adding the flag to every element address is valid or faster.

The question is whether existing source bounds and selected-target layout
qualification establish the complete LLVM promise for ordinary Slots/Ring
payload projections. The argument must cover the actual padded header,
element allocation stride, enclosing object extent, intermediate offsets,
pointer-index width, zero capacity, zero stride, and any one-past use of the
shared projection. Logical Ring wrap arithmetic is a separate operation;
the candidate must not attach an unsigned no-wrap assertion to it merely
because a final physical element is in bounds. The
[LLVM instruction contract](https://llvm.org/docs/LangRef.html#getelementptr-instruction)
is the target obligation; STOR-6 and DIAG-2 remain the language authorities.

Selection criteria, recorded before this experiment's timings:

- Every emitted optional fact has a complete argument from existing checked
  and selected-target facts. Source acceptance and target-layout qualification
  agree with this optional fact enabled or withheld. Unsupported assembler spelling
  retains a correct portable lowering rather than rejecting a WF program.
- Existing maintained regressions cover relevant ownership, wrap, zero-size,
  nested-layout and target-domain boundaries. Add only observations absent
  from those cases. Successful timing runs do not establish allocation-failure
  behavior: the maintained exhaustion tests own the resource-failure boundary,
  while the container caller and allocator observer own the exact release ledger.
- Reuse the existing Deque workload, independent checksums, allocation ledger,
  scalar and wide-record payloads, normal and retained helpers, C controls and
  counterbalanced matrix. Compare fresh baseline and candidate emissions and
  timings on the same toolchain. Withholding this fact is a correctness
  control, not a substitute for isolating its performance contribution; this
  experiment does not require a new public facts-mode switch.
- Select production emission only if the complete target contract holds and
  the paired results show a repeatable benefit beyond cohort and control
  variation, without an unexplained material loss elsewhere in the matrix.
  Attribute any remaining gap rather than claiming universal native parity.

Keep the implementation within ordinary address emission and the existing
toolchain-capability path where possible. Do not change source syntax,
container representation, Ring range admission, or host/runtime protocols to
obtain this result. The material compiler choices were reviewed as amendments
beside the design tree; the owner approved both revisions after DCR.

### Representation premise and target contract

Inspection found a representation mismatch before extending optional facts.
The target calculator gives `Slots<T, 0>` its eight-byte descriptor with
eight-byte alignment. Emission instead retained `{ i64, [0 x T] }`.
LLVM arrays keep their element's ABI alignment even at length zero
([DataLayout implementation](https://llvm.org/doxygen/DataLayout_8cpp_source.html)).
For the ordinary opaque `OutputStream` type, whose emitted representation is
16-byte aligned, that LLVM aggregate has size and alignment 16. An outer
runtime Ring of such values is therefore checked with header/stride 24/8
while its emitted allocation arithmetic uses 32/16. This is a mismatch in
qualification; the allocator uses the emitted size, so these calculations
alone do not demonstrate an observed undersized allocation.

OP-9's zero repetitions contain no element layout. Raising the checked size
and alignment would force the nested allocation to fail the unchanged
language ceiling. The selected repair instead extends the existing
`Array<T, 0>` emission convention to the empty payload of constant-capacity
Slots and Ring: `[0 x i8]`, retaining their length/head words. Nonzero-capacity
zero-size elements and runtime-capacity typed tails keep their distinct
representations. A maintained regression must compare the complete nested
object and actual emitted allocation against the checked byte boundary,
including Array conversion and cleanup, rather than only repeat the
checker's preexisting size result.

The maintained backend regression
`zero_capacity_windows_keep_header_layout_inside_nonempty_storage` embeds
both empty window shapes between scalar sentinels in one record. The checked
record is 40 bytes with alignment eight, and its one-slot runtime Ring needs
64 bytes including the header. A same-source native before/after run with
retained helpers and an allocation observer measured 96 bytes from the saved
`1b916975` baseline and 64 from the repair: the independent 64-byte oracle
exits six before the fix and zero afterward. The test also checks a synthetic
64-byte allocation limit and the rejection one byte below it, plus empty
range formation, Array conversion, sentinel preservation and cleanup. These
observations distinguish actual layout correspondence from an assertion
about the calculator alone.

Once representation and qualification agree, let `H` be the actual padded
tail-field offset, `S` the actual allocation stride of that GEP's element,
`C` the capacity, and `E` the complete window extent. On the currently
supported targets the pointer index is i64 and qualification establishes
`E <= M = 2^63 - 1`. For a positive-stride payload and an admissible physical
offset `p`, including a one-past pointer when permitted:

```text
0 <= p <= C <= floor((M - H) / S)
0 <= H <= H + p*S <= E <= M
```

The GEP's successive offsets are zero, the nonnegative header offset, and
the nonnegative scaled physical index. Each fits both signed and unsigned
index arithmetic. For a nested window at parent offset `q`, complete-parent
qualification also establishes `q + E <= A`, the parent allocation extent.
Every intermediate pointer remains in that same allocation. LLVM allocations
cannot cross the unsigned address-space boundary, including their one-past
pointer ([allocated-object contract](https://llvm.org/docs/LangRef.html#allocated-objects)).
These facts establish all four `nuw` requirements, not only index scaling.
For zero stride, ordinary emission already uses physical address operand
zero; no bound on the potentially huge logical capacity is needed. For an
erased constant-capacity-zero payload, the actual tail type is i8 and its
only admissible boundary offset is zero. Ring wrap operations retain their
own wrapping semantics and receive no new arithmetic assertion from this
argument.

Two target expressions deserve comparison before selection: direct GEP `nuw`
with a supported-spelling probe and plain-`inbounds` fallback, or an
`llvm.assume` that the actual normalized address index is signed nonnegative
beside the existing `inbounds` projection. The latter states a subset of the
same proved domain with portable syntax; whether optimization recovers the
same benefit is an empirical question. GEP `nuw` first appears in
[LLVM 19](https://releases.llvm.org/19.1.0/docs/ReleaseNotes.html#changes-to-the-llvm-ir);
[LLVM 18](https://releases.llvm.org/18.1.8/docs/LangRef.html#getelementptr-instruction)
already defines the nonnegative inbounds implication and the assumption
intrinsic. Use the existing four-position probe only to discriminate these
expressions, then validate the selected compiler path and full matrix.

The four-position comparison on Apple Clang 21 admits both expressions and
passes the same independent oracle. Both reduce the scalar forward loop from
four loads/four stores to one payload load/one payload store; their hot-loop
assembly is identical. Their complete scalar functions are not identical,
so this observation does not establish timing parity elsewhere. The selected
candidate for the full compiler experiment is the nonnegative-index
assumption: it recovers the observed local optimization without a new LLVM
dialect requirement. A build-time syntax probe would inspect the build's
compiler, while the native builder uses its selected native compiler; those
need not be the same consumer. The existing [comparison record](../../experiments/container-representation/deque-library/RESULTS.md)
owns the probe commands, counts and artifacts. After the complete
normal/retained paired comparison, the owner selected production emission
provisionally with the tradeoff recorded below.

The implementation stays in the shared run projection and intrinsic registry.
A test-only withholding choice exercises that same emitter after the same
target validation; it is not a source mode or a second qualification path.
The corresponding decision is in `compiler/backend-facts`; the zero-capacity
representation decision is in `compiler/storage-representation`. This keeps
the representation repair independent of the optional optimization and needs
neither a second table of per-address qualification nor a public compiler
switch. No source rule or container contract changes.

### Retained reverse-path discriminator

The first complete Clang 21 baseline/candidate sweeps show a reproducible
retained-scalar reverse-churn regression, about seven percent after the
in-binary C normalization, alongside the large forward-path gain. Independent
disassembly comparisons find identical reverse-loop and endpoint-helper
instructions, register dependencies and memory operations. The loop stays at
the same address, but both endpoint helpers move 32 bytes earlier. This is
evidence of a code-placement difference, not yet evidence that placement
causes the timing difference.

Before additional timing, the discriminator is to give both scratch retained
modules' scalar `pop_back` definition 128-byte alignment. Verify that the
two endpoint helpers then occupy identical addresses and preserve their
instructions, and that the calling loop remains unchanged; otherwise the
experiment does not isolate the proposed cause. Use the existing native
oracle and counterbalanced retained measurements, with the same source,
seeds and C controls. If the regression collapses after this normalization,
report the production-layout cost as placement-sensitive; if it persists,
keep the regression unexplained. The scratch alignment is an instrument,
not a proposed production alignment policy or a reason to select facts by
element identity.

The 128-byte-alignment construction failed that isolation check: it made
the two endpoint addresses agree, but the linker also moved both complete
WF text regions by 96 bytes, including the trace and callbacks. No oracle
or timing was run on those images. The follow-up instrument therefore keeps
the original section alignment and adds exactly 32 bytes of unreachable
text padding before the candidate's scalar `pop_back` symbol. Before timing,
verify against the original binaries that the trace and callbacks retain
their addresses and instructions and both endpoints recover the original
baseline addresses and instructions. If this narrower construction also
fails isolation, do not time it or reinterpret the first experiment as a
success. This padding is likewise confined to scratch evidence.

The 32-byte padding control met the isolation checks and passed both native
oracles. Its two measurement orders disagree: the normalized retained scalar
reverse ratio stays about 1.06–1.08 in the first pair, then approaches one in
the reversed pair because the last baseline itself slows. The original
production-layout regression therefore remains unexplained; neither the
first alignment failure nor the second mixed result establishes a particular
cache or branch-prediction cause. No padding enters production.

The predeclared selection criterion is **not fully met**. After DCR, the owner
selected provisional retention of the qualified fact for its reproducible
inline scalar forward and rebase improvements, explicitly accepting the
recorded retained-reverse cost and its unresolved cause. The alternative is
to remove the optional fact and deliver the independently verified
zero-capacity representation repair alone. No application-frequency
distribution has been measured, so the results do not establish that every
Deque consumer benefits from the tradeoff. The owner-approved backend-facts
decision records the first choice; it does not establish that the original
criterion passed. Reopen that choice for a consumer
dominated by retained reverse calls, a changed native toolchain, or a further
material regression under the same matched-contract comparison. The
maintained TODO keeps the remaining scalar gap and cost attribution open.

## Inactive payload initialization compiler trial

The owner rejected adoption of both candidates in this completed trial.
Production lowering retains baseline destination initialization; the following
records the registered protocols and negative outcomes, not a selected
optimization.

The maintained owning map selected the enum-bucket representation, compact
returned-pair result and shared exchange helper. Its optimized construction
and rebuild still initialize inactive payload storage, while the native
controls initialize occupancy only. The trial was framed on merged main
`45ef2d53e` and rebased onto v0.68 main `345e2966a` before measurement. The
question is whether the compiler can omit that work while preserving
defined active values and the same ordinary ownership and call semantics.

The experiment keeps the actual library source, representation, ABI,
allocation policy and algorithm fixed. It neither reopens the library
selection nor combines clearing with aggregate forwarding or enum overlay.
Slab and Vector serve as bounded cross-consumer checks; their recorded
transfer, layout and short-cycle questions remain open unless separately
resolved by evidence. No source rule or container interface change is proposed.

The prospective workload matrix, same-image null comparison, selection and
stopping criteria, correctness boundaries and execution budgets are recorded
once in the existing [map comparison](../../experiments/container-representation/map-library/RESULTS.md#same-source-inactive-storage-lowering-comparison),
before compiler implementation or timing. That record owns the detailed
measurement evidence; instruction counts alone will not establish a speedup.

### Initialized-state and representation boundary

WIN-1 and WIN-2 admit element access only within the initialized window;
PROV-6 visits that window and the active enum variant during cleanup. Enum
payload projection requires the selected-variant refinement. Array differs:
every declared element is a value and still requires initialization. STOR-7
permits relocating the representation, without exposing an owner's address
or inactive storage as source data. These existing rules support omitting
writes to inactive storage; no acceptance rule or new initialized-state
analysis is proposed.

The bounded candidate changes destination construction only: write an enum's
tag and selected fields, every struct field, and an empty inline window's
length plus Ring head. Runtime-capacity windows already initialize their
header without filling their free slots. Keep capacity and reference
descriptors defined, including empty and zero-size cases. Retain complete
aggregate transfers, existing indirect calls and return placement, and the
current zero-based SSA constructors. All active state must be initialized
before an ordinary return or parallel publication; join still precedes
consumption.

Inactive payload fields are LLVM value fields, not ABI padding. The
[LLVM 21 memory rules](https://releases.llvm.org/21.1.0/docs/LangRef.html#memory-access-and-addressing-operations)
allow loads of uninitialized storage to produce undefined parts; transporting
them is different from using them as an active value, condition or address.
The existing aggregate ABI has no whole-value
[`noundef` promise](https://releases.llvm.org/21.1.0/docs/LangRef.html#parameter-attributes).
`dereferenceable` concerns the pointer and accessible storage, not complete
pointee initialization. This is a qualified argument for leaving bytes
unwritten, not for inserting poison or freezing a missing active field.
The same ordinary-value contract applies to linked bodies: there is no
native-origin exception or requirement that an inactive representation be
zero. Future ABI attributes or representation-wide observations must revisit
this argument.

The trial also identified possible clear removal in the stable-scatter
consumer recorded in the maintained TODO. Its staging copies, packing costs
and whole-call performance remain outside this bounded comparison; neither
the historical profile nor a map improvement closes that question. Broader
SSA construction, aggregate forwarding and overlay layouts remain separate
opportunities because they change more paths or require distinct interference
and ABI evidence. The smaller unchanged-source comparison was the first
discriminator.

### First result and bounded constructor follow-up

The first candidate fails its prospective performance criterion. The
[complete comparison](../../experiments/container-representation/map-library/RESULTS.md#completed-comparison-gains-with-unresolved-regressions)
retains the wide-map rebuild improvements, the repeated normal wide
replacement regression and the retained scalar Slab lookup regression.
Private exchange inlining increases Map's stack temporaries and payload
transfers. In Slab, writing a small result's fields separately loses the
baseline's single combined store. These are observed code differences, not
measurements of their isolated causal shares. Do not select the candidate
merely from fewer initialization writes or average away the consumer losses.

After the first failure, one further general emission form received a bounded
discriminator: form an
aggregate value from a poison seed, insert its tag and every active field,
then store that aggregate once. Materialize every source operand before the
store so overlapping result placement cannot overwrite an unread operand.
This changes destination construction only; it does not change layouts,
source acceptance, allocation, calls or window descriptors. Other existing
SSA constructors keep their current behavior. LLVM's
[partial-aggregate example](https://releases.llvm.org/21.1.0/docs/LangRef.html#insertvalue-instruction)
supports defined inserted components beside inactive poison components.
No inactive component may be observed as a condition, address or source
value, including across an ordinary linked call. Do not introduce an undef
seed or a whole-value noundef promise. A whole aggregate store permits LLVM
to write inactive bytes physically; omission of every such write is not its
semantic contract.

This form can also lose: the existing emitter deliberately avoids loading
large stored payloads into SSA because SROA can expand their arrays into
individual operations. Before implementing or measuring this candidate,
fix the following discriminator. Build it on an isolated local branch and
use the existing unchanged Map and Slab sources, target and C controls.
First inspect optimized IR and native code: require at least one fewer
surviving 256-byte transfer than the first candidate on normal wide
replacement's matched-key path,
continued omission of the large vacant-bucket and empty-window clears, and
no extra successful-path instructions in retained scalar Slab lookup relative
to baseline A. Also inspect large-array construction for new bulk
scalarization and preserve operand snapshots through overlapping placement.
If any required screen fails, stop this variant without timing or thresholds,
identity tests, forced inlining, or another emission variant to rescue it.

If the screen passes, qualify active fields, all variants, ordinary linked
and parallel boundaries with the maintained regressions before performance
selection. Then conduct a fresh A/A and A/candidate comparison over the same
complete Map and triggered Slab matrices, preserving every cohort, mode,
seed, allocation oracle and raw tail. Use the earlier per-cell null/quantum
screen and require both a primary rebuild improvement and no unexplained
material regression. This is a new compiler candidate, not another replay
of the completed first trial. Allow no timing replay in this discriminator;
an ambiguous outcome remains ambiguous. Keep its separate construction,
correctness and timing totals, bounded at 90, 30 and 40 seconds respectively;
compiler construction and the canonical gate are separate. Investigate any
overrun before extending work.

Generic copy forwarding remains deferred: one redundant optimized Map copy
is between nonescaping private allocations created by LLVM inlining, not a
general proof that the backend's exposed owner snapshots can be forwarded.
Supporting that case before optimization needs a separate interference,
liveness and representation argument. The maintained TODO records that
scope; rejecting this trial establishes no benefit for those separate paths.

The isolated SSA constructor discriminator also failed, before correctness
execution or timing. Its measured local revision is
`fa50c0d873a6c1f9c48d4a87b98890fd295bf4fc`, based on `e31d9422f`; the frozen
compiler SHA-256 is
`b95c6167e483010b4a49b294488b047adc7b25494217db9c67b3c0a14e76c29f`.
The durable [replay patch](../../experiments/container-representation/map-library/inactive-ssa-construction.patch)
and [reconstruction instructions](../../experiments/container-representation/map-library/RESULTS.md#rejected-ssa-construction-follow-up-and-replay)
preserve this rejected source independently of the local commit.
All Map/Slab source, harness and shared runtime inputs match the retained A
identities, and both generated C controls are byte-identical to A. These are
negative structural observations on the same Apple Clang target, not timings
or a replacement production implementation:

| Required screen | Observed result |
| --- | --- |
| Slab scalar successful lookup no larger than A | 28 executed native instructions versus A's 23 and the first candidate's 27; tag plus two byte stores remain. Fails. |
| No new large-array expansion | Wide Map put grows from 6 to 105 LLVM loads with 102 insertvalue operations; its native body grows from 113 to 211 instructions. Fails. |
| Fewer full replacement copies | The matched path has three full 256-byte transfers versus five in the first candidate, but a private exchange call and 48 bytes of payload spills remain. |
| Vacant/pending construction clears remain absent | Absent in the inspected bodies; separate 272-byte inactive-result clears reappear. |
| Inputs captured before aggregate store | Present in raw IR; runtime alias qualification was not run after the independent screen failures. |

Compiler construction took 45.23 seconds and artifact emission, linking and
disassembly 7.13 seconds. Correctness execution and timing each took zero:
the predeclared stopping rule applied. The prototype's original measured
two-file diff, including context, has SHA-256
`4e4d04400d0ea0ec7c11e25499490a492505b8827f2b828a5cc9ffd39aa21e1f`;
the zero-context replay overlay has its separate identity in the reconstruction
record and produces the same source. It is excluded from production.
The owner rejected the initialization omission
and this constructor follow-up because they failed their registered criteria.
Baseline clearing is retained, with the measurements and rejected source kept
as evidence. No further constructor form or timing replay is selected here;
reopening requires distinct grounds that address these optimizer losses.
