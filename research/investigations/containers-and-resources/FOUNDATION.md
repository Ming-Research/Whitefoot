# Container performance ceiling and the foundation decision

[Containers over x1](X1-LIBRARY.md) re-derives the next library comparison under
PR #70's decided framework. The interface selections and measurements below
retain their original conditions, including interfaces that x1 has superseded.

The question is how Whitefoot can support efficient system containers while
preserving machine-checked safety. Performance has priority over breadth;
common containers are a basic coverage requirement, and kernel, database and
cache-server cases test where a representation ceiling would exclude useful
applications. A lower-level interface is a candidate means, not an objective.

The owner clarified this ground on 2026-09-08: inability to eliminate a runtime
check is not by itself a failure, especially when validation is off the critical
path. Compare executable cost, including initialization, lookup, mutation,
allocation, movement, peak storage, tail work and necessary metadata. A compact
proof or a small kernel inventory is not a substitute for those results.

**Keep ordinary valid values as the baseline and test ordinary reusable helper
boundaries before selecting a new storage permission.** The
[full-array experiment](#selected-full-array-experiment) now has
[native ownership and layout evidence](../../experiments/container-representation/foundation/RESULTS.md#dense-full-array-operations).
The [generic brand boundary](#generic-brand-parameters) now preserves explicit
type brands through ordinary helpers. The selected [exclusive two-state interface](#exclusive-two-state-run-contracts)
removes run mutation's owned round trip. The D2 enum-slot measurements do not
yet justify a compiler-known projected sparse layout; ordinary owning slots
remain the measured baseline. C2 later removes owned-state routing altogether
under the [ordinary-value boundary](../ordinary-host-values/DESIGN.md); the
routing investigations below retain their original revision and scope as
historical evidence. A general library resource-permission system remains a bounded challenger,
not the public foundation. Neither choice claims coverage of every system
container.

The earlier pool-driven recommendation of nominal invariants and full-array
conversion did not establish a container-wide priority. Its controlled contract
remains useful evidence, but neither it nor the new map controls measure demand
distribution. Common-family coverage and independent lifetime/layout ceilings
remain explicit below.

The [active specification](../../../spec/kernel-spec.md) defines accepted
programs. The conformance report records implementation results; the root
[verification guide](../../../AGENTS.md#checks) explains how to run it.
[REASSESSMENT.md](REASSESSMENT.md) records the merged owned-place work.
The [external study](EXTERNAL-WORKLOADS.md) owns pinned source observations, and
[representation experiments](../../experiments/container-representation/README.md)
own executable evidence. The completed implementation experiments below include
general compiler repairs and bounded storage-reuse optimizations; the earlier work had no new
source interface. The selected full-array experiment is a language amendment
with two kernel rows; its design is not evidence that implementation is complete.
Version labels attached to earlier experiments below identify their measured
branch revisions. The combined publication over main is v0.56; it does not
retroactively change the versions or runtime used for those measurements.

## Ground and evidence

The [constitution](../../../docs/constitution.md) chooses large-system and
embedded capability, machine-checked safety, and runtime performance within
practical development and verification costs. Those aims do not uniquely select
a container representation. This investigation requires correct ownership and
refusal behavior, useful common-family operation chains, and competitive
operation-specific allocation, layout, and transfer costs. Comparison must
distinguish four questions:

1. Does a complete operation have a correct current-language expression?
2. Is a rejected form excluded by the specification or blocked by the compiler?
3. Does the expression force expensive representation or work?
4. Does a competing mechanism actually remove that cost while preserving the
   same ownership, failure, lifetime and operation contract?

Use the established families as the coverage set, then test critical operation
chains. Current Whitefoot fixtures establish capabilities, not prevalence.
Pinned upstream source reveals actual contracts and plausible cost mechanisms,
not measured hotness. A native model is not a checked Whitefoot implementation.

Runtime work is classified by purpose and position:

- Algorithmic tests such as missing-key lookup, collision resolution and format
  decoding remain necessary unless a stronger input contract removes them.
- Validation may reject malformed external bytes or a mismatched view, then
  expose a checked domain for subsequent work. The false edge is ordinary
  behavior; a one-time validator is not free if objects are short-lived.
- A source guard can establish the proof required by an operation on its true
  branch. This does not grant arbitrary raw bytes valid-T status or revoke loans.
- Pure proof bookkeeping erases. Existing required static proof is not replaced
  by a trap or an impossible-case error arm added merely to rescue a program.
  Any different checked-API rule is an explicit candidate amendment.
- Compare retained checks where they execute. Cache misses, layout, allocation
  and algorithm choice can dominate a predictable branch. Conversely, a repeated
  scan or metadata check can be material. Neither conclusion is assumed.

A preliminary native scalar-map control now compares the same linear-probing
algorithm with interleaved tags, separate control/payload, a retained per-access
extent check, and batch extent validation; a Rust standard-map comparator uses
the same keys, hash outputs and results with its different algorithm.
[Source, samples and limits](../../experiments/container-representation/costs/RESULTS.md)
separate representation effects from compiler acceptance and do not establish a
generic-map or system-wide performance winner.

## Coverage and discriminating contracts

This table is a research frontier, not a list of completed library implementations.
Each row needs a checked operation witness and, where cost selects between forms,
a matched executable comparison before it can support a performance conclusion.

| Family | Critical operation chain | Current route to test | Ceiling or missing evidence |
| --- | --- | --- | --- |
| Full arrays and fixed sequences | Construct non-copy elements, index, replace, consume, clean up | General-element fixed runs and complete owning arrays, with two consuming conversions | Native dense-layout and ownership evidence exists; residual transfers and final-place construction remain separate |
| Growable vector and strings | Reserve, append, refuse without losing input, relocate, drain | Store-backed run with source-written allocation and movement | No current realloc row; initialization/copy costs; general helper contracts |
| Deque and ring | Both ends, wrap, two-span processing, grow/rebase | Existing circular window | Two-span views and helper provenance; extra work when a consumer needs contiguous data |
| HashMap and HashSet | Collisions, duplicate insertion, lookup, delete, reuse, rehash | Initialized optional entries; initialized byte/control and copy-payload alternatives | Fixed-capacity owning-value operations and allocation refusal execute; owning growth/rehash, generic behavior and sparse layout cost remain |
| Ordered maps/sets and priority queues | Search, range, insert/delete; sift/split/merge/rotate | Dense heap; recursive owning boxes; fixed-capacity node arrays | Dynamic disjoint access, mutable traversal, non-copy movement and full operation evidence |
| SmallVector and short strings | Inline use, spill, refuse, retain or shrink | Enum of inline and store-backed owners | Tag/layout, store-region and ABI cost; no completed matched spill implementation |
| Lists, sparse sets and stable slots | Remove by identity, reuse, preserve other identities | Indexed owners or recursive boxes | Index validation/generation costs; multi-membership; retained borrowing |
| Packed records and byte pages | Decode, insert/delete, overlap move, compact, validate | Initialized byte arrays/runs and codecs | Bulk lowering, compact handles, validation reuse; variable-sized inline records |
| Intrusive kernel structures | Link an externally owned object into multiple relations, unlink | Owning containers or IDs are possible different contracts | Stored membership references, stable placement and reclamation are independent needs |
| Shared/concurrent containers | Publish, observe, mutate, retire, reclaim | Existing staged lexical access covers only its stated scope | RCU/epoch/shared lifetime is not supplied by a sequential slot API |

The first [family-witness runs](../../experiments/container-representation/families/RESULTS.md)
executed a bounded optional-entry hash table, a dense binary heap, a
B+ tree leaf-split component, and a variable-record byte page with insertion,
deletion, overlapping movement and ordinary invalid-input/refusal outcomes.
An additional boxed-entry component executed runtime-indexed migration and
collision probing, with earlier map operations prepared at selected positions.
The whole-effect refinement restores the optional-entry map and nested boxed
helper in all three execution modes. Known back-endpoint images now also
restore boxed migration in the maintained three-mode native loop.
The migration component does not
establish a general map API or return/resume migration contract;
the leaf component is not a complete ordered map. The boxed-tree replacement
reproducer now uses a borrowed owner slot and executes in the ordinary native
gate. It restores the owning-node correctness baseline; it is not a full tree
implementation.

The binary heap also has a same-algorithm native comparison and an independent
sorting oracle. It exposes retained complete-run transfers at ordinary helper
boundaries despite the earlier fresh-destination improvements. That is a measured
implementation cost to investigate before attributing dense-heap performance to
the absence of a lower-level storage language. The helpers explicitly preserve
the contiguous head-zero property through verified contracts.

The [byte-growth comparison](../../experiments/container-representation/families/RESULTS.md#explicit-byte-run-growth-and-refusal)
executes source-written allocation, copy, fill and refusal, including injected
first/second allocation failure. Existing storage preserves the old bytes on
failed growth inside the trace. Matched loop, bulk-copy and realloc controls show
that growth policy and complete-operation cost must be measured: realloc loses
at the smallest tested size, while larger operations include generation and
digest work that can dominate copying. No spare-byte initialization is forced
by this source. Retained circular-index arithmetic and scalar copying provide
an ordinary lowering target; these timings do not select a new storage authority.

The Linux, Redis and SQLite observations in the external study make the last rows
concrete. They do not impose pointer tagging, GC, C callbacks or any upstream
threshold as a Whitefoot requirement. A byte-page codec does not need arbitrary
typed holes merely because its physical entry lengths vary. A borrowed cursor or
multi-index object cannot be claimed covered by copying values into a run.

The [permission-mechanism counterchecks](EXTERNAL-WORKLOADS.md#permission-mechanisms-as-design-counterchecks)
likewise separate a branded access discipline from backing lifetime, and raw
permission splitting from checked typed layout. Their proof/trust models are
comparators, not acceptance authority for WF.

Generic behavior is a separate axis: hash/equality/comparison and callbacks must
have an admitted invocation mechanism and effects. A concrete u64 table does not
establish a reusable arbitrary-key library. FN-2/3 provide built-in numeric and
ownership bounds; FN-5 supplies no user behavior invocation through `contract`
or `conform`. A generic storage/probing core with concrete caller-side hash and
equality is a candidate decomposition, not a completed generic lookup API.
Precomputed hashes do not remove collision equality, and the cost of transferring
candidates across that boundary still needs a complete operation witness.
Memory safety also differs from
ordinary map correctness: fully initialized indexed storage can be memory-safe
without proving the entire abstract map algorithm. Prove additional semantic
properties when a contract or partial-operation domain actually needs them, and
test behavior independently.

## Competing foundation strategies

| Candidate | Possible advantage | Discriminating failure |
| --- | --- | --- |
| Current runs, views and ordinary library code | Small implemented safety basis; cheap range facts; usable byte and dense algorithms | An important matched operation forces extra tags, copies, scans, allocation or inaccessible lifetime |
| General compiler-checked state/layout operations | Can support full values, initialization destinations, compact optional layouts or checked ranges without exposing an arbitrary resource logic | New operation for each container; closed layout inventory prevents an important representation; compiler complexity without measured benefit |
| Library-selected typed storage with checked resource evidence | Allows library algorithms to choose initialized sets, placement and transitions | Dynamic state cannot be checked compositionally, proof effort explodes, erasure adds metadata, or borrowing/cleanup remains inexpressible |
| Checked runtime validation with reusable access | Can move proof work to construction/boundary paths while keeping hot operations simple | Validation repeats on hot mutation or cannot establish the needed ownership/initializedness authority |

These rows are not four mutually exclusive architectures. Runtime validation is
an access strategy available to each representation, and a projected enum is a
layout extension of ordinary valid values. The unresolved storage choice is
narrower: can compiler-maintained valid values with library-selected layouts meet
the required costs, or must libraries also compose initialization, borrowing and
release permissions? Improvements to ordinary place/ABI handling are relevant to
both. Do not build four infrastructures or choose the broader proof surface
because its claimed coverage is larger.

### A narrower competitor to arbitrary storage permissions

A general projected layout for ordinary slot enums deserves a direct comparison
with library-selected resource proofs. Conceptually, a slot has `Empty`,
`Deleted`, or `Occupied(key, value, hash)` state. Its authoritative discriminant
can be stored separately from payloads while each logical slot remains one valid
enum. Normal construction, matching, replacement and destruction could preserve
the safety relation. Replacing an occupied slot with a tombstone transfers the
old payload exactly once. Read-only control projection could support probing;
writing a byte alone must not create an occupied generic payload.

This is a selected experiment, not existing source syntax or a validated
implementation. It could obtain sparse typed storage without requiring writers
to prove an arbitrary ownership-set predicate. The expected benefit for authors
of custom container representations is compact layout through checked ordinary
value operations without unchecked implementation steps. This is not a claim
that using Rust's existing safe collections requires
unsafe code, nor a measured speedup over Rust or a demonstrated WF authoring win.
It has concrete limits:

- A tag plus an arbitrary byte fingerprint and extra empty/deleted states do not
  fit one byte. A compact control encoding needs a checked finite range/variant
  layout or pays extra metadata. A second occupancy bitmap is not presumed free.
- Projection does not imply safe simultaneous mutable access to arbitrary slots.
  The borrowing rules must admit the actual operation and conserve exclusivity.
- Ordinary insertion receives a complete value. Eliminating a fallible producer's
  large temporary still needs result-destination routing or checked construction.
- A fixed enum layout does not cover arbitrary mixed-type overlays, compact
  variable-tail objects, stored memberships or deferred reclamation.

For the broader resource-proof candidate, the decisive missing mechanism is
symbolic focus and framing: open the permission for a runtime-selected slot while
retaining responsibility for every other live slot. A finite list of concrete
tokens or enumerated examples is insufficient for arbitrary runtime capacities.
Allocation identity must distinguish two allocations in the same store region;
splitting evidence cannot duplicate it. Explicit finite proof terms with a fixed
resource grammar and checked induction are a candidate, but this is new proof
machinery rather than a widening of numeric `ensures` clauses. Its checking and
erasure have not been implemented or validated by the current finite model.

### A bounded symbolic resource candidate

The broader route needs an actual checking rule for a runtime index, not a
compile-time token for every element. One candidate restricts predicates to
finite sums, separating products and structurally indexed segments. The notation
here is a research sketch, not Whitefoot syntax or implemented acceptance:

```text
Slot(ac, ap, i) = Ctl(ac,i,Empty)   * Raw(ap,T,i,i+1)
               | Ctl(ac,i,Deleted) * Raw(ap,T,i,i+1)
               | exists fp:checked7, v.
                   Ctl(ac,i,Full(fp)) * Init(ap,T,i,v)
Seg(ac,ap,lo,lo) = emp
Seg(ac,ap,lo,hi+1) = Seg(ac,ap,lo,hi) * Slot(ac,ap,hi)

focus(Seg(ac,ap,lo,hi), proof(lo <= i < hi))
  -> exists fresh k.
     Seg(ac,ap,lo,i) * Slot(ac,ap,i) * Seg(ac,ap,i+1,hi)
     * FocusKey(k,ac,ap,lo,hi,i)
```

`ac` and `ap` are fresh existential allocation identities produced by allocation;
they cannot be manufactured from an integer, equal capacity or a reused address.
`*` conserves linear responsibility. Focus generates a fresh linear closing key;
unfocus consumes the three pieces and that same key. It cannot recreate a segment
while a piece is borrowed or missing. Finite split/join, checked sum
introduction/elimination, construct/take,
borrow/end and release rules form a syntax-directed checker. Predicate definitions
cannot add axioms or arbitrary executable tests. A `Full` control byte by itself
never introduces an initialized `T`. The seven-bit fingerprint is ordinary
algorithmic data; spatial validity does not establish that it matches the key.

Opening consumes the selected `Slot` and its `FocusKey`, producing
`Open(k,ac,ap,i,control_state,payload_state)`, an exclusive linear resource retaining
that key and owning the slot's initialized control byte and raw or initialized
payload responsibility. Taking a payload changes its payload state to raw and
yields an ordinary owned value, leaving `Open(...,Full(fp),Raw)` until the control
is changed. Closing consumes the open resource and returns a valid `Slot` and
that same `FocusKey` only when its two states satisfy one of the alternatives above.
Until `Deleted` is written, the physical control can still say `Full`. That
transient state is safe only because the closed slot/segment authority has been
consumed: other code cannot observe it as a valid closed table. Every transition
updates the open responsibility, and returning a normal table requires restoring
the relation and closing it. Letting a shared closed-table view coexist with
this transition would permit an uninitialized read and must be rejected.

A written loop invariant can own a processed empty/deleted source prefix, its
remaining segment, the complete destination segment, and an optional separately
held entry. The checker verifies entry, one symbolic iteration and exit, with
explicit arithmetic steps and one structural fold/unfold. Runtime capacity does
not cause proof unrolling or occupancy enumeration. Migration focuses source
index `i`, extracts its entry, closes that source as deleted, then probes dynamic
destination indices. Each focus retains the entire complement. Successful
placement consumes the held entry; exhausted probing or intended refusal returns
both table owners, cursor and still-held entry as partial progress. It does not
promise rollback. A second simultaneous focus requires checked disjoint indices.

Cleanup uses the same induction to consume live values and recover vacant
coverage. `Ctl(ac,i,state)` owns one initialized byte. After payloads are retired,
cleanup unfolds and consumes vacant slots rather than rebuilding their segment.
A `retire_ctl` step consumes each control byte's responsibility and yields
`Raw(ac,u8,i,i+1)`; adjacent raw spans fold/join to cover `ac` without clearing its
physical bytes. Freeing a backing requires its complete raw coverage and no
remaining loan, initialized cell or open responsibility *for that allocation*. An extracted
Box may remain independently owned after the old slot backing is freed. Allocation
identity, segment/focus/open evidence and induction erase; actual control loads,
probing, payload movement, destruction and allocation execute. A runtime bounds
guard is possible only with an intended false outcome, not an injected trap.
No runtime proof table, second occupancy bitmap or additional cleanup scan is
part of this candidate.

The sketch's `ac` and `ap` name separate allocations. It does not yet cover two
typed planes inside one backing, which is the fair primary native layout
comparison: charging only the split representation for a second allocation would
mix layout with provider/refusal costs. One backing needs a checked layout rule
in addition to focus. It must consume raw bytes under one root allocation
identity, establish aligned, non-overlapping control and payload planes, and
retain padding responsibility and the root release authority. Plane identities
are not independently freeable allocations. Before freeing the root, all planes
and padding must return their complete raw byte coverage with no live loans.
Integer extent/alignment checks can occur in a total layout constructor with an
intended size-refusal result; this is different from revalidating ownership on
each payload access. This shared layout obligation applies to a projected enum
as well as to a library resource implementation. Ordinary byte splitting and a
cast do not establish typed validity, alignment or release authority.

This is a plausible restricted proof design, not a demonstrated sound checker,
erasure result or authoring-cost measurement. Checked, definition-justified
predicate introduction provides authority; privacy could hide representation but
is not what makes a proof unforgeable. The design covers fixed-stride sequential
storage. General zero-sized linear elements would still need counted ownership
obligations even when their byte extent is empty. It does not supply arbitrary
overlays, variable tails, hash correctness,
stored membership, concurrent mutation or backing keepalive. Ordinary enums
provide the control/payload validity relation; a projected-enum layout would
have to preserve it while admitting the required dynamic operations. If their
representation and operations match, this larger proof surface has no established
performance advantage. A concrete counterexample
to that narrower route remains the condition for selecting it.

### Runtime-checked identity and retained membership

A stable slab with two indexes has two meaningfully different contracts. Under
weak identity, deleting an object may leave an index entry whose later lookup
returns `Expired`. Bounds, owner identity, generation and state checks can make
that access safe; removing stale index entries remains application correctness.
A finite generation must retire/refuse on exhaustion or justify safe reuse, not
wrap and silently revive an old identity. A generation alone does not prevent a
handle being applied to the wrong store.

Under retained membership, an object must remain the same accessible object while
the membership exists. Returning `Expired` after deleting it changes the
contract. Deletion must be statically prevented, delayed, or return an intended
`Busy` outcome through an authoritative lifetime protocol. Neither projected enum
validity nor a live-slot permission alone supplies that retained lifetime. A
runtime check also cannot preserve memory after it returns a borrow unless the
access protocol keeps the backing alive. These candidate identities are not a
claim that the arena-index ownership pattern rejected in STOR-1 has been admitted.

A [two-index native control](../../experiments/container-representation/authority/RESULTS.md#two-indexes-and-object-lifetime)
now distinguishes those contracts through weak expiry, two retained memberships,
an independent access ticket, wrong-store rejection and finite generation
exhaustion. A rejected ticket return must preserve the ticket so it can still
release the original access. This experiment does not supply WF admission,
store-identity uniqueness or backing lifetime: its logical ticket is not a borrow,
and its separate actual borrow relies on Rust's lifetime rules. It confirms the
protocol distinction without pricing a proposed WF representation.

The matched comparison should use the same non-copy payload in a deleting/rehashing
map and a slab with two indexes, testing weak identity and retained membership
separately. Price current complete-value storage, projected enum layout, and
resource evidence against the same operations, including intended runtime
checks. If a narrower representation matches the broader candidate's costs on
both contracts, generality alone does not justify a new resource logic. Conversely,
duplicate metadata, forced payload copies or an unrepresentable retained
lifetime can supply a concrete reason to widen the foundation.

### What a lower-level candidate must actually expose

A useful candidate separates allocation identity and extent, typed positions,
initialization/ownership responsibility, loans and executable cleanup.
Reservation gives storage, not a valid value. A position can form an ordinary
borrow only when its content is valid and the appropriate permission is held.
Split/join operations conserve exact ownership; reuse cannot resurrect old
permissions. Address stability lasts through the actual last use, including
staged join return, result consumption and retirement.

The hard new part is a checked abstraction over these resources. Separate
hash-control bytes cannot authorize reads of arbitrary uninitialized payload
merely because a control byte says occupied. A checked relation must connect the
metadata to the live owner of the same allocation/slot, or the implementation
must retain an authoritative safe runtime representation such as optional values.
For copy payloads, initializing every slot is another candidate; measure its
initialization cost rather than assuming it is unacceptable.

Ordinary scalar requires/ensures are insufficient for arbitrary dynamic ownership
sets. Finite explicit resource split/join/open/close certificates and specified
induction are candidates, not current INV-1 or FN-9 capability. The checker must
finish deterministically, need no SMT, and check written steps rather than
rediscover a global mutation history. Proof erasure must not generate a token
table, extra control tags or scans. Algorithmic metadata and actual destruction
still execute.

A proposed slot capability does not itself provide compact variable-tail layout,
generic callbacks, membership in multiple containers, stored borrow provenance,
incremental rehash progress, lock protocols or deferred reclamation. These are
separate ceilings and cannot be silently deferred while claiming a universal basis.

## Construction without a public hole in T

Current internal aggregate-result destinations and eligible fresh-binding reuse
are implemented. A public vacant initialization destination is not. The current
boundary operation consumes a complete element; an optimizer's removal of a
temporary is not a source guarantee of construction in the final vacant slot.

The retained [large-result probe](../../experiments/container-representation/foundation/RESULTS.md#current-whitefoot-large-result-boundary)
shows a complete fallible result and payload surviving a retained producer
boundary. Two candidates remain: general internal result-destination routing, and
an explicit checked initialization destination. A field destination must remain
vacant, exclusive and address-stable across the producer's effects. Success
publishes a complete value; failure handles every initialized field and returns
the promised state. Returning an existing value may require a move, and failure
does not undo unrelated effects.

The [finite construction model](../../experiments/container-representation/foundation/RESULTS.md#finite-result-tree-construction-model)
tests concrete transfer and cleanup strategies, not a symbolic language extension
or an implemented new ABI. Original recommendations excluding a public
initialization permission were confined to the pool experiment and do not settle
the broader foundation choice.

## Empty backing across library and pool boundaries

The prior pool is a local capability witness. Its strengthened contract combined
fixed backing, failure-prefix cleanup, return of that same empty backing and a
full-owner conversion. This lifecycle was synthesized rather than observed as
a complete workload in the upstream application sample.

A fixed-capacity inner run preserves its capacity through its type and retains
its runtime length. Reading that length remains possible. Current boundary
operations do not transport incidental element measures through their implicit
slot. A direct scalar contract can describe each run-length transition; it
cannot by itself describe all nested element contents. Adding content models
or checked state evidence would be a substantive proof extension, not simply
remembering how often a helper was called.

This does not establish empty-state transport as the principal container gap.
Runtime admission of a block, an already-validated interface, a specialized
state or a richer contract are candidates under the actual pool contract.
Do not invent an impossible failure branch to mask a missing required fact.

### Specialized slot versus a checked source wrapper

The previous finite nominal-invariant proposal remains a local alternative to a
specialized empty-block owner. Neither has established container-wide priority.
A Box changes placement and possibly address stability; it does not prove empty
length. A wrapper preserving arithmetic facts also does not justify reading an
uninitialized generic payload.

[The original proposal at its recorded revision](https://github.com/mbbill/Whitefoot/blob/370bf3249d68d2aa635fb671af4446de636c6f8e/research/investigations/containers-and-resources/FOUNDATION.md)
retains its detailed transition and descriptor comparison. Its implementation
selection is superseded by this document; the executable evidence remains useful
under its stated conditions.

## Evidence-backed implementation sequence

The owning-box replacement, nested store-polymorphic helpers, and complete
run-element paths now have bounded native evidence in the
[family results](../../experiments/container-representation/families/RESULTS.md).
The [consumed-input result](../../experiments/container-representation/families/RESULTS.md#consumed-input-and-result-destination-reuse)
removes the caller's complete-run transfer after a one-result push; its matched
heap remains about 2.8 times the C control at 16 rounds under that measurement.
The [result-field placement](../../experiments/container-representation/families/RESULTS.md#compiler-selected-result-fields)
extends the general storage planner to a consumed input and one consuming
field of a complete call result. Independent sorting checks preserve the pop
contract while optimized code removes the measured post-pop transfers.
The complete parent allocation, field offsets, surviving sibling reads, input
snapshots, CFG conflicts and exposed loans remain explicit obligations; a
child-sized external result never supplies the parent's backing. This changes
physical placement without changing source ownership or acceptance. It does
not supply arbitrary destination aliases or deferred-call retirement.
The [borrowed full-array heap](../../experiments/container-representation/families/RESULTS.md#borrowed-full-array-heap)
uses the same scalar operation with an ordinary array borrow and count
contracts. It removes whole-heap transfers and measures close to the same C
algorithm with both ordinary optimization and retained mutation calls. This
supplies an efficient current-language competitor for that bounded scalar heap;
general payload construction and comparison behavior remain separate requirements.
The separate [alternative-return result](../../experiments/container-representation/foundation/RESULTS.md#alternative-return-destinations)
reduces the measured producer's frame and removes two whole-result transfers;
element-to-record transfers and general construction destinations remain open.
These are scoped repairs and costs, not a completed container foundation.

Owner routing has a separate correctness gap in ordinary reusable mutation,
including mutation through a borrow. The counterexamples and candidate boundary
below are not covered by the native Box writeback or transfer measurements.

The full-array contract below now executes through the same typed element,
place, ownership, and release machinery. Preserve its native controls and the
generic brand reader controls while repairing current-owner transfer.
Prototype projected layout for
ordinary slot enums against the native owning sparse control. Keep that sparse
experiment's construction, matching, transfer and cleanup on general valid-value
operations, with one backing, compact control and payload planes, runtime-indexed
access, and no second occupancy/token table. The resource-proof route remains a
challenger with explicit symbolic checking and erasure obligations. Neither
experiment blocks a separately justified repair to general transfer or byte-loop
lowering.

### Selected full-array experiment

The concrete requirement is a completed fixed-size record sequence whose
elements may own resources: build a prefix with fallible element constructors,
freeze only the complete sequence, pass it through ordinary helpers, inspect
and replace elements, then consume or reclaim every owner. A completed array
must need no mutable length/head descriptor, per-element tag, or allocation of
its own. These are useful contracts for fixed records, table entries, and node
contents, not a claim about their frequency in a production WF workload.

The chosen mechanism generalizes the existing `array<T, N>` element domain and
adds `array_from_fixed` and `fixed_from_array` to BLK-3. It does not introduce a
parallel full-value family. The first conversion requires `len_of(vector) == N`
and transfers the complete window in logical order; the reverse conversion
produces a full, zero-head fixed run. Both consume their input, preserve each
element's exact type and store identity, and allocate and release nothing.
Ordinary moves and loans still govern when a transfer is legal. T and N are
supplied by the operand under BLK-0, so neither call writes generic arguments.
The existing `array_new` remains copy-fill and CONST-2 remains unchanged.

This separates the conditional safety argument from the provisional interface
choice. Given a full initialized window and exclusive ownership for its consume,
transferring each logical element exactly once produces N initialized owned
elements without a hole. Reversing that transfer produces the full window with
known measures. The argument does not prove compiler correctness or make these
two spellings uniquely necessary. Reusing the already expressible partial run
lets the first experiment test full values without selecting a public vacant-T
permission or a new proof system.

Two alternatives remain useful comparisons. Keeping only FixedVector requires
a variable-window representation and associated operations at completed-value
boundaries, even when all N elements remain present; an optimizer may remove
those costs locally, but the full-value interface should not depend on that
success. A public checked initialization destination could remove a large
temporary, but it also needs partial-construction and failure ownership rules;
the existing destination evidence does not establish that broader mechanism.
The selected slice isolates the full-value question while retaining that
construction comparison.

A `head == 0` premise is unnecessary for the inline conversion. A full wrapped
run has the same N logical values, and moving its two spans into dense order is
safe. Requiring source rebase would add work without strengthening the result's
contract. Inline movement is permitted in either direction; zero-copy and
address stability are not promises. For an unwrapped source, lowering should
use the ordinary direct destination or one contiguous transfer where possible;
for a wrapped source, inspect the actual logical-order transfer. An adoption of
an existing store-backed allocation is a separate question: it must preserve
its original base and release identity, which an inline conversion followed by
`heap_box` does not establish. No allocation-adoption operation is selected in
this amendment.

Before evaluating the implementation, the discriminating witness is an array
of records owning modern Heap- or Arena-backed Boxes, with a generic helper
boundary. Exercise construction failure after a nonempty prefix, full success,
wrapped input, actual-place indexed reads and replacement, thaw, drain, and
ordinary final cleanup. Verify returned values and an exact allocation/release
ledger in facts-off, facts-on, and completion modes. Include zero extent and a
zero-byte affine element representation: physical byte count cannot erase
logical ownership, and zero extent must execute no element access or release.
Measure the dense array layout and retained transfers against the equivalent
fixed-run and native dense representations; do not infer throughput from a
descriptor count or a green correctness test. Reopen the route if it forces an
extra allocation, persistent metadata, or material unavoidable transfer at a
required boundary, or if the ordinary proof path makes the witness impractical.

This first slice needs actual-place access and replacement; reading a copied
snapshot cannot stand in for borrowing an affine element. General view
implementation remains a named compiler gap and must report Unsupported where
the language admits the source. The conservative element-type linearity closure
also remains: even a zero-extent array or an empty run of explicitly linear T
has no new terminal discharge here. Affine elements can complete the lifecycle
while required providers are held; this is not evidence of complete explicitly
linear drain support. Stored loans, provider payloads, type cycles, effect
attribution, and target layout retain their existing judgments.

The amendment's META-5 delta is numbered rules +0/-0, grammar productions +0/-0,
fixed tokens +0/-0, writer operation spellings +2/-0, kernel declaration records
+2/-0, nominal families +0/-0, and exceptions +0/-0. Its selection ground is
evidence-selected: existing flat-array and general-element-run implementations
and the retained layout/transfer controls identify a concrete full-value gap.
They support this bounded experiment. The subsequent
[native result](../../experiments/container-representation/foundation/RESULTS.md#dense-full-array-operations)
establishes dense layout, correct ownership and remaining transfer costs, not
throughput parity. Under META-6 the safety consequences above are
conditional deductions and the representation/interface choice is provisional.

The affected rules are TYPE-2 (complete element domain), TYPE-4 (scope its
`cvt` statement to numeric value conversion), PROV-6 and STOR-3
(element ownership and release closure), STOR-1 (dense inline representation),
BLK-0 and BLK-3 (two inferred-argument conversion records), BLK-1 (whole-window
transfer), BLK-4/VIEW-2 (retain arrays and their existing recursive and view
judgments), and CALL-3 (apply its existing inner-descriptor kills to every
admitted viewed element). Their current index points here for this scoped amendment; other
grounds remain with the general rule assessment. The directly affected VIEW-1
grounds now distinguish the compiler's flat-element limit from the language's
view domain. OWN-1/5, SET-2/LIV-2, OP-1/4/9, CONST-2, FN-2, STOR-5/6, MSR,
and the other CALL rules are unchanged dependencies, not
new permissions. The array-retirement proposal in the historical S34 design is
superseded for this choice. The existing META-5 editorial question stays open;
this amendment does not redefine its evidence/minimality labels.

### Views formed through storage holders

The next view control passes an array by shared or exclusive borrow, forms a
view inside an ordinary helper, reads through a copied shared descriptor and
another helper, then resumes parent mutation after the last shared use. Its
exclusive view writes and reads the same original array, including when that
array is a field. The preceding formation dispatcher sends non-view holder
dereferences into the legacy arena path and stops before these operations.
VIEW-2 already admits the storage domain; this is an implementation experiment.

First reuse the ordinary argument-reborrow judgment and the existing typed
storage address. A result's possible storage origins are not authority to
ignore a live unique ancestor. If the existing loan representation cannot
carry that delegation, retain the checked parent-loan relation separately from
origins through descriptor copies, arguments and permitted view results.
The criterion is an executable direct/helper operation with no array snapshot,
preserved parent freezing while a child is live, restored access at its stated
endpoint, ordinary lifetime and overlap refusals, and unchanged staged
retirement controls. No field or origin match alone may manufacture a loan
permission, and this repair selects no new public container or pointer API.

The origin-only prototype reached the helper read after carrying the actual
parent holder, then rejected the later exclusive view's own write. Registering
every descriptor on every loan with the same storage origin made that new
writer a holder of the older, already dead shared loan. A shared copy, an
own-view relay and a newly formed exclusive view require distinct treatment
even when their possible storage origins are equal.

The implemented candidate carries a finite structural loan key with each
view: data region, exact protected place, strength and actual checked parent
holder. Formation publishes that claim; binding and copying register on only
the carried keys. An own-view result substitutes the supplying actuals' keys
under VIEW-6's complete signature ceiling. A borrowed-view result publishes
new shared keys at the result region, inheriting those actual parent routes.
An incoming formal view carries its declared permission without inventing a
local formation record. Equal keys can share the union of descriptor uses;
these represent identical continuing restrictions, not an inferred exact
runtime supplier. None of this metadata reaches the runtime descriptor.

A separate incoming-view control moves a formal exclusive descriptor into a
local binding, obtains a shared child, then transfers the parent to another
helper before reading the child. The preceding implementation accepted that
transfer because the formal permission had no local exclusive-formation record.
The descriptor check now includes its carried exclusive keys in addition to
every association in the existing loan-state scan. Reading the last child
before the transfer is the positive control; a live child still forbids even
transfer to a pure helper. The compiler does not inspect a callee's body to
recover permission to transfer the parent.

Borrowed-view effect projection exposed another independent defect: a write
through a helper receiving the moved descriptor by exclusive borrow was
attributed only to that local descriptor and disappeared from the enclosing
row. EFF-2 already requires projection through the view's complete backing
origins regardless of the descriptor's parameter mode. The implementation
retains the descriptor's call-loan check and projects effects through those
origins. Missing writes and spurious writes remain separate negative controls.
Passing an already-bound descriptor holder also needs that check to apply the
child freeze to its projected write; there is no new borrow formation at that
call. The live-child and previous-statement-last-use pair now distinguishes
that route as well as direct exclusive borrowing and own-view transfer.

The native control
`borrowed_array_views_preserve_delegation_and_original_storage_across_calls`
executes in all three lowering modes, normally and with helper calls retained.
It observes shared and exclusive array parameters, descriptor copies and
relays, subsequent writes, unchanged neighboring elements and fields, previous
scalar snapshots, local holders and const arrays. No array snapshot is needed
to form the view. A separate array is written only by the exclusive-borrow
helper after its formal view moves into a local binding; no direct write can
hide a missing projected effect. The caller observes that write and unchanged
neighbors. This is behavior and address evidence, not a throughput
comparison. General affine-element views remain a separate capability gap.

### Generic brand parameters

A reusable reader of `SmallBytes<'s>` must accept the caller's exact store brand
without borrowing a provider it does not use. The earlier FORM-8 multiplicity
rule rejects a named `'s` in a single input, while TYPE-2 requires a source
nominal's brand arguments in type position. For built-in `Vector<u8>`, elision
instead selects the concrete entry heap under PROV-1. Elision therefore cannot
stand for an arbitrary brand in either case. This is a language-rule conflict,
not an inability to recover a runtime container length.

The selected amendment distinguishes invariant type brands from loan regions.
A formal brand stays named even at one input position. The input's explicit
type structure determines each actual brand, including multiple nominal region
arguments, nested explicit type arguments and PROV-1's declaration-local elided
slots. A type parameter remains opaque; its eventual argument adds no new
formal positions. Matching a nominal's name does not inspect its fields or
unfold a recursive ownership graph. A constructor applies the same correspondence
to its selected variant's direct declared field types, without an expected type.

Exact identity takes precedence over loan adaptation. If one formal names both
an input brand and an input loan, the actual brand fixes it and every actual loan must outlive
that fixed region. No parameter order may shorten the brand. Formals appearing
only in non-brand positions retain their previous region judgment. Complete type
equality is checked after the final substitution: outlives alone does not convert
one direct view type into another. VIEW-2's explicit shared child through a
borrowed `Slice` or `MutSlice` is supported. The separate probe whose `parent`
borrows an array previously stopped at `RegionsAndBorrows` before the OWN-6
judgment. The storage-holder repair above now checks that same source and its
exact brand relation; v0.55 does not reject a child merely because its local
region contains several statements. Stored-content
restrictions and provider-release bounds remain intact.
The old single-region extractor is insufficient even after fixing spelling:
`Pair<'a, 'b>` and `Vector<'a, Vector<'b, u8>>` require both positions.

Rejected alternatives are an unused provider parameter, which falsely ties a
reader's API to allocation capability; equating its loan with its backing brand,
which requires an unnecessarily long loan; and expanding concrete generic types
or nominal fields, which makes the boundary depend on instantiation or recursive
representation. An anonymous brand syntax could express genericity but is not
needed to resolve this conflict and would introduce another grammar choice.

The conformance additions pin a single-brand reader, multi-brand field and
parameter correspondence, repeated-brand mismatch and the short-loan refusal.
Compiler tests must additionally cover parameter order, opaque generic arguments,
recursive nominals and inherited field brands. This amendment adds no grammar,
kernel operation or runtime metadata, and claims no new optimization. Inline
variants whose operands supply no brand, composite Result measure postconditions,
ordinary owning-element spill and compact enum layout remain independent gaps.

### Owner identity across replacement and calls

The normal-return defect also occurs with ordinary memory owners. An
`exchange(target: &uniq box<u64>, incoming: own box<u64>)` helper performs
`let previous = replace deref(target) = move incoming;` and returns `previous`.
A caller then reads its remaining `owner`. At published revision `d53ffe95`
(whose compiler sources equal the tested `e376025d` executable), writing the
exchange directly requires `reads(owner, incoming), writes(owner)`, but routing
the same exchange through the helper accepts an omitted `reads(incoming)` and
rejects the complete row as extra. The same comparison with `Box<'s, u64>` and
an explicit `Heap<'s>` parameter gives the same discrepancy; the provider adds
its ordinary release write. These are compilation observations, not evidence
for a file-specific rule or for a new public container operation.

The selected repair extends the callable summary to the current state left in
exclusive actuals. If entry owners are F and I, the result holds F and the
original target location holds I on exit. Reusing a spelling or pointer slot
does not make I the same input state as F. The semantic regression
`ordinary_box_owner_transfer_preserves_incoming_reads_across_helpers` compares
both Box forms, direct/helper code, and complete/omitted effect rows. The native
`ordinary_box_owner_transfer_keeps_values_and_release_across_two_helpers`
retains both helper boundaries and observes old=11, current=22 and the exact
two-allocation/two-release ledger in all three execution modes. It establishes
ordinary owner transfer for that case, not nested-content routing, timing
parity, or container-family completion.

Two small source checks expose lost identities, without requiring a new storage
permission. In a full `FixedVector<ReadFile, 1>`, replace slot zero with an
incoming `ReadFile`, return both the run and the displaced file, and release both
in a caller. The caller must retain the incoming file's release effect. A focused
check against revision `1f545791` instead rejects that caller's
`writes(files, incoming)` as having an extra `writes(incoming)`. The
[replacement analyzer at that revision](https://github.com/mbbill/Whitefoot/blob/1f545791/compiler/src/semantic/check/result_state_origin.rs)
updated ordinary binding/static-field origins without updating an indexed
`Storage` target's contained-owner image;
the body checker and the result analyzer also disagreed about the extracted
origin. The defect is lost ownership flow, not lost runtime length information.
An independent static-field precision defect is repaired in `16a20bb9`. For an
owned formal `Holder { before: u64; file: ReadFile; after: u64; }`, resource
borrows now project their origins to `holder.file`, rather than charging all
three fields. The body checker and result-origin replay share this projection.
Eight focused cases cover direct and nested fields, reborrows, exact read/write
rows, rejection of omitted or extra effects, and the displaced owner's returned
field origin. This repair does not supply the post-call owner update below.
The conformance case
[`systcp-connection-field-effect-paths`](../../../tests/conformance/cases/systcp-connection-field-effect-paths.wf)
had declared both TCP directions although its helper only calls `receive_next`.
Its row is corrected to `reads(link.receive, scratch), writes(link.receive,
scratch)`: EFF-1 admits the field path, SYS-18 makes the directions ordinary
fields, and EFF-2 requires declared and exhibited effects to match exactly. The
accept verdict and executable behavior are unchanged; the correction removes
the spurious sending-field effects exposed by the selected-field repair.

The same failure needs no container. Consider this complete helper body:

```wf
fn exchange(target: &uniq ReadFile, incoming: own ReadFile) -> previous: own ReadFile reads(target), writes(target) {
  let previous = replace deref(target) = move incoming;
  return move previous;
}
```

A caller with owned parameters `file` and `incoming` invokes it with
`target: &uniq file, incoming: move incoming`, then releases the returned owner
and the owner still in `file`. Revision `9e731905` rejects the caller's correct
`reads(file), writes(file, incoming)` row and accepts a control omitting
`writes(incoming)`. These are semantic compilation observations, not native I/O
measurements. The accepted control's emitted LLVM has a further physical defect:
the helper receives both resource descriptors by value, returns the old one,
and never writes back the new one; the caller's two closes use the old descriptor.
An independent native observer subsequently invoked the retained helper with
inert descriptor identities 17 and 29: it observed current=17 and previous=17,
confirming the duplicate old descriptor without performing OS I/O.

The physical defect is repaired in `2caf694e` (integrated as `a1c19710`). An
opaque-resource borrow passes the resolved owner's address; source reborrows
retain that address, while qualified system calls load its current descriptor
at the call's argument evaluation point. Native scalar `ReadFile` and aggregate
`HostString` cases exercise shared/exclusive parameters, selected fields,
reborrows, borrowed results, and pointer/extent observations in all three
execution modes. The qualified runtime ABI is unchanged. These tests establish
physical writeback and descriptor access; that physical repair did not provide
the semantic post-call owner routing selected here.

The required transfer has two outputs. If the entry target owns F and the
incoming argument owns I, the returned owner is F and the target's exit content
is I. A second exchange with J must return I and leave J. A result-only summary
cannot express the caller storage left behind by an exclusive borrowed actual.
The v0.54 FN-1 amendment extends the former result-only boundary with that
entry-to-normal-exit transfer and its application to the actual resolved place.
All returned and written-back origins must be instantiated from one entry
snapshot and committed together, including static fields, nested owning paths,
and multiple disjoint unique actuals. Reborrows must update the ultimate owner,
not merely a copied holder record. For an operation that normally completes,
inability to represent its transfer cannot establish fresh or unchanged content;
an unavailable case must remain an explicit compiler limitation.

This requirement does not by itself decide the summary of a call with no normal
return. For example, this witness uses only an ordinary owning memory object:

```wf
fn unclosed(value: own box<u64>) -> result: own box<u64> pure {
  let next = unclosed(value: move value);
  return move next;
}
```

It cannot alone establish an incorrect returned-object identity: no invocation
returns an object. An empty may-origin set may be vacuously correct on normal
returns; whether the internal analysis must retain an unresolved state is a
separate representation and structural-effect question. A failing internal
assertion demanding `Unknown` rather than an empty set does not settle that
question or justify a language amendment. Evaluate the ordinary-object case
under FN-1 and EFF-2, including normally returning controls, before attributing
the issue to an I/O API or selecting a new origin-analysis mechanism.

Accordingly, the prototype's internal `Unknown` assertion for this unclosed
recursion was replaced with three ordinary Box behavior checks: an unused
nonreturning helper is valid, a helper that returns its input after at most one
recursive call preserves that input's required read effect, and a read before
unbounded recursion still contributes its structural effect. This changes the
test's unsupported premise rather than selecting a new summary lattice.

The earlier nested `box<ReadFile>` unit-returning test also assumed an exact
`Unsupported` outcome without settling how the outer allocation and current
contained owner's identities are represented separately. Its replacement used an
ordinary `box<box<u64>>` helper that returns the inner Box displaced by SET-2; its
caller reads that old result. SET-2 and FN-1 already determine the old value's
identity. The prior implementation reported `InvalidResolution` at the read;
the diagnostic-only repair then reported `OwnerStateRouting` there. The typed
referent extension now preserves that old owner, and the same source is the
positive `ordinary_displaced_box_result_keeps_the_extracted_owners_origin` case,
paired with an extra-effect rejection. Exact internal replacement also updates
the stored owner when no owner is returned. An unrepresented interior path still
cannot be counted as unchanged contents or as completed container support.

The first representation implemented whole owners and static product fields,
kept callable effects and all output components on one entry snapshot, and
applied simultaneous updates only to exact actual places. A returned borrow's
signature ceiling is insufficient to identify such a place. The typed-path
extension below preserves that call-entry rule and adds owning referents,
selected enum payloads and literal slots. The source-bound recovery below adds
composition through dynamic and implicit-boundary transfers without supplying
their exact content placement; recursive contents remain incomplete.
Neither permanent unions of replaced owners nor
treating unrepresented transfers as fresh was selected.

For a returned exclusive borrow, a narrower declaration-only argument can
establish the whole location without deriving a new body summary. FN-1 confines
the mutable result to its sole signature candidate; OWN-10 excludes callee-local
storage, and immutable constants cannot supply an exclusive result. If the
complete result type equals the candidate's referent type and no proper typed
subplace can have that same type, the only possible location is the entire
candidate. This does not recover precision for an already inexact actual.
The finite type graph follows product fields, sum payloads, owning referents,
and element types, without enumerating capacities; a repeated target type or
an unresolved generic prevents this conclusion. Shared results remain inexact.

The discriminating checks for this deduction are the unchanged retained
returned-borrow native case, ordinary Box identity/reborrow helpers, and
negative controls for a returned field, recursive same-type containment,
shared constant alternatives, and forwarding an already inexact actual. A
helper that writes before returning the same scalar location must still kill
the previous value's proof facts. A body-derived location summary was considered
but adds a new callable-boundary component unnecessarily for these whole-place
cases; it remains a separate option for genuinely selected subplaces. Location
precision must not narrow the existing loan ceiling or change its suspension
and kill rules.

These controls now pass: ordinary Box helpers retain both displaced-owner
reads and installed-owner writeback, and the original opaque-owner native
source needs no change. A retained-call Box-field executable observes the old
value, the installed value, and both adjacent fields in all three lowering
modes. The field control initially exposed an independent implementation gap:
legal static-field reborrows stopped before existing typed address lowering.
They now use that ordinary address path, with no ABI change. Shared-constant
requirements and prewrite scalar facts remain negative controls; forwarding
an inexact field result through a whole-type identity remains unsupported.

The revised region rule also invalidates the old shared-option-view rejection.
Its original lookup/helper bodies now use the ordinary typed storage path for
a child borrow of an element's field. The executable command checks present and
absent entries in three modes. Indexed child formation retains the normal bound,
holder-strength and sibling-exclusivity judgments; explicit negative controls
isolate each one. A separate retained-call array-field writer checks the changed
element and three unaffected fields. This is reuse of ordinary addressed storage,
not a new subscript permission or container-specific reborrow rule.

The proposed `deref(deref(owner)).next` return is not a legal OWN-14 form:
the rule admits `deref(h)` followed by suffixes, and the extra dereference is
not a suffix. It therefore tests the OWN-14 rejection, not returned-location
routing. A separate terminating identity helper over a recursive type tests
the conservative declaration predicate. At `5aaef50a`, the new Box witness's
two-statement child region violated OWN-6; its helper retained the displaced
owner read. The region restriction was then reconsidered separately under
[the experiment below](#temporary-child-regions-and-statement-endpoints).

The related LIV-2 amendment distinguishes a complete binding already dead at
statement entry from a same-statement read-out. Reinitializing the former
writes no previous owner's state; the latter still performs its atomic read
and write. This is an explicit change to LIV-2's former universal commit-write
sentence, not merely a compiler interpretation of FN-1. It prevents a moved-out
value from acquiring a spurious write effect when its old variable is reused,
while retaining the RHS effects, commit kill, term identity and loan checks.

Selection is evidence-selected for ordinary direct/helper equivalence and
preservation of transferred owners; the finite joint-summary mechanism and
the dead-binding initialization exception are provisional choices with those
grounds. The alternatives considered were result-only summaries (lose the
stored output), inspecting callee bodies at every ordinary call (breaks the
selected callable boundary), and retaining every replaced owner indefinitely
(loses current-state precision). Reopen the representation when ordinary
nested/indexed cases cannot retain precise identities at practical checking
cost. The affected rules are FN-1, EFF-2, SET-2 and LIV-2. META-5 delta: rules
+0/-0 (four amended), grammar productions +0/-0, tokens/spellings +0/-0,
exceptions +1/-0 (the entry-dead complete-binding write exception). Runtime
layouts, system contracts and target milestones are unchanged. The active
rule index, compiler capability map and container decision memory follow this
scope; no claim about general I/O resource API design follows from it.

The broader compiler suite challenges this implementation boundary: the native
cases `heap_full_arrays_preserve_elements_across_calls_replacement_and_refusal`,
`borrowed_enum_payload_replacement_updates_the_child_owner_in_its_box`, and
`opaque_resource_borrows_write_back_through_calls_fields_and_reborrows` all
passed at `d53ffe95` but stopped with `OwnerStateRouting` in the published
`f586e04c` and `6f29022c` prototypes.
The first two need a displaced value to cross a helper after indexed or enum
payload replacement; the third needs precise writeback through a returned
borrow. The published `5aaef50a` canonical and both-host CI unit runs pass the third
case with its original source and executable assertions. The typed-path
extension restores the boxed enum child's original retained native consumer
and exact allocation/release trace. Finite source bounds now restore the heap
full-array and wide-owned-result consumers when their suppliers resolve to fresh
local state. Complete call coverage and the whole-effect experiment below also
restore the imported-owner generic array helper and boxed-run read-out without
claiming precise extracted-element images. All original executable assertions
remain intact; finer content selection remains a separate capability gap.

The `f586e04c` CI run exposed two further implementation defects in loops. The
preliminary body check compared unresolved call-result origin images at the
backedge before those bodies could supply callable summaries. Deferring only
that metadata comparison in the preliminary pass restores the existing
[valid loop-invariant snapshot](../../../tests/snapshot/cases/contracts/contracts__adversary-r2__p06_loop_invariant_requires_valid.wf)
and [off-by-one rejection snapshot](../../../tests/snapshot/cases/contracts/contracts__adversary-r2__p07_loop_invariant_offbyone_attempt.wf);
all liveness, loan and other binding fields still agree. Final checking now
uses a stable origin header derived after callable summaries converge and
requires each backedge's origins to be contained in it. Counted-loop exhaustion
uses that header too, rather than only the pre-loop image. The ordinary Box
relay loop checks both loop forms. A two-iteration Box swap with both writes
declared but only the first input's read now reports exactly the missing second
read. Its paired complete row passes. A separate post-exhaustion read control
has the same precise negative and positive, and native retained-call tests cover
zero, one and two iterations. Ignoring origin equality without checking the body
against the stable header would hide a later iteration's read and was not selected.
The existing FixedVector identity-helper loop also now succeeds. Its former
`OwnershipJoin` expectation recorded the same preliminary-checking limitation,
not a source-language rejection. The exact program remains in the capability
test with a success assertion; the other unsupported controls remain unchanged.

The same run's three recursive Buffer cleanup cases exposed a different
mistake. A legacy indexed mutation's effect access names the containing Buffer,
but its `CheckedSetTarget::BufferIndex` still names one element. Treating the
access projection as a whole-owner target overwrote the Buffer's origin image
with the element's. Consulting the typed target before selecting a strong
update repaired that whole-root overwrite but did not yet implement indexed
contents. The current extension admits exact literal element updates. An
unrepresented dynamic update now retains a complete finite supplier bound when
its root and replacement have one. Its placement remains unresolved; it cannot
be treated as unchanged or as a precise slot image.

The byte-string and fixed-run failures exposed another defect: the generic
kernel-expression fallback unions argument origins at the result root, although
`take_back` and `take_front` deliver an ordered result list. Projecting ordinal
zero at a commit could therefore discard the run's formal origin. For copy
elements, BLK-3 returns the same run while EFF-2's removed copy observation owns
no state. Both body checking and callable-summary analysis now place the
unchanged run image at result ordinal zero and no origin at ordinal one.
Unknown inputs remain unknown. A wrapper and a two-iteration drain check both
rows through helper boundaries; the retained growable-byte-vector and byte-string
programs compile and execute again.

Restoring an empty-run helper also exposed an independent proof-closure defect.
A returned zero-capacity fixed run retained its correct type and measure bounds,
but the closure omitted the capacity term as a transitive middle for the implicit
length-at-most-capacity edge. Reading capacity in an extra source statement
accidentally restored the proof. Both proof-retaining closure and contradiction
checking now derive useful implicit middle terms from the complete bound inventory.
The original helper proves its length is zero without an extra read; the paired
capacity-one case with unknown length still fails the kernel precondition. No
runtime branch or special rule for a zero-capacity container supplies that proof.

Noncopy elements need separate remainder/element images and their contained-state
transfer. A finite supplier bound now preserves both outputs without asserting
that every supplier actually remains in each one. An imported bound used as an
exact origin still exposes `OwnerStateRouting`; this is a missing implementation
capability, not a new source rejection. The retained boxed-run read-out case
can instead establish its complete boundary-effect union without an exact
extracted image, as the experiment below shows. The wide-result native case and
`block_pool.wf` resolve their actual suppliers and execute again. Their
executable success assertions remain unchanged. Ignoring a loop's changed
origins, treating an unknown summary as fresh, or counting duplicated bounds as
exact output contents does not repair the remaining gap. These defects
under existing ordinary-object operations supply no evidence for a new
container API or an I/O-specific source rule. The repairs change no language
rule or normative conformance verdict.

One tempting recovery is to instantiate an unknown summary as formal-free
when every actual currently has an explicitly empty origin set. That would be
valid if those sets were sound upper bounds on all currently reachable state.
The incomplete content updates did not establish that premise. For example,
create a fresh `box<box<u64>>`, replace its inner cell with the owned
formal `incoming`, then pass the outer Box to a helper that returns its old
inner cell. The later read is a read of `incoming`. If the unrepresented
replacement leaves the outer Box's old empty metadata, the proposed recovery
incorrectly frames that read out. The recovery was rejected without applying
it; treating `None` and explicitly unknown actuals conservatively does not
repair the falsely empty actual. A complete content-state upper bound or an
explicit completeness argument must precede any such refinement. No test is
retired or expectation relaxed to bypass this challenge. The current typed
referent routes preserve the incoming owner in that witness, including through
the extracting helper. Its positive and omitted-effect cases are retained in
the compiler's ordinary effect tests. That local repair does not establish the
complete-upper-bound premise for every unrepresented container operation.

The finite-path prototype implements the exact-path part of this candidate.
Its selectors distinguish product fields, owning referents, active enum payloads
and literal elements. A route denotes a whole input subvalue, with relative
subtree exclusions for contents that have been replaced. Strong replacement
excludes the old contents and installs the incoming routes at the target.
Body checking and summary replay share projection, exclusion, join and call
substitution. Effect projection uses the selected subvalue where that path is
exact; an effect-row prefix alone cannot select an interior overwrite. Normal
checking still judges loans against their existing conservative places.

Two representation identities are essential for termination. Reinstalling the
same selected subvalue is a no-op, not an expansion into ancestor and child
routes. Otherwise a recursive scalar-only tree fold generates deeper routes
at every iteration despite never changing an owner. Projection also merges
routes that become identical after selecting a child. A partial fixed-point
bottom excludes only the overwritten subtree, retaining independent siblings.
The existing shared and unique recursive folds and successive-exchange controls
distinguish these cases. This is not a general complexity proof.

The current path-list representation stops with an explicit compiler capability
gap when a selector needs to revisit a recursive type; even some finite written
paths through recursive contents remain outside it. Whole-owner and scalar-only
recursive calls do not require that unfolding. Recursive extraction needs a
finite summary representation beyond this prototype, not a language rejection
or an acceptance time budget. Kernel reservation creates invocation-local empty
storage, not an owner inherited from its borrowed provider. Cell formation
places the value's routes under the success referent or the refusal payload.
Cell destructuring must use that same referent selector: treating its sole
binder as an ordinary product field discards an imported payload's route.
Direct and helper-returned destructuring controls require the payload's read
effect after an Arena-backed cell is consumed. The legacy arena value retains
its existing explicit runtime capability boundary.
Implicit run-boundary placement and extraction of owning elements still lack
an exact slot/content transfer and cannot use a root-level argument union as one.
Front insertion also shifts existing logical indices: leaving a route at slot
zero can misattribute a later extraction from slot one. The corresponding
normal-return witness keeps the transferred supplier as an unlocated bound;
its established effects can close the whole row without selecting that slot.
An unchanged child union is not a conservative upper bound.

The remaining candidate keeps an owning Box or run's storage anchor separate
from its current contained owners. Use exact product fields, sparse literal-slot
overrides, and a residual may-origin set; a repeated access may share a symbolic
slot only when it uses the same captured index value. A strong overwrite removes
the old origin from that slot. It does not remove that origin from unknown other
slots without sufficient cardinality information. At loop joins, combine origin
metadata separately from ownership liveness and fold dynamic index versions into
finite sets. A priority insertion loop's pending affine payload can alternate
between an input payload and a displaced queue entry; it does not require an
unbounded history to describe those possibilities. Stable header export now
provides the origin join, but it does not supply an exact dynamic-slot image.
Preserve the record's field precision so a payload
join does not turn a priority-field read into a payload access.

The next recovery experiment isolates that framing obligation. A dynamic
replacement at `record.slots[index]` has an unknown element selector but a known
`record.slots` prefix. Bound only the selected subtree, preserving unrelated
fields in both body checking and callable replay. The existing optional-slot
program must execute unchanged; direct and helper-returned field readers must
retain the required sibling effect and reject omitted or spurious effects.
Controls must keep both old and incoming suppliers in the uncertain subtree
without making either supplier an exact extracted value. This experiment does
not establish precise descriptor, residual-slot or release selection.

The prefix repair restores the unchanged optional-slot program's native fill,
pop and cleanup checks. Source controls return the changed aggregate by value
or update it through an exclusive helper borrow, then read an independent
Box field. Omitting that field's read or adding the incoming element's read
rejects by EFF-2. Direct overwrite of a live affine indexed element still
rejects by STOR-1; the prefix does not grant new overwrite permission.
Both checkers share the subtree selection and update algebra. Unknown slots
retain old and incoming supplier bounds only within the represented prefix;
the displaced value remains bounded, and unrepresented borrowed-result
locations remain unknown. This repairs lost field framing under the existing
typed-route design without adding a slot-identity or content-selection rule.

The complete candidate must use one transfer semantics for normal checking and callable
summary derivation, preserving the existing typed storage paths. Origin data is
erased and is never a runtime graph, ownership permission, or allocation identity.
Reject permanent unions of replaced owners: they retain effects of values no
longer present. Reject loop or helper-history expansion as the termination
argument. A finite graph can share snapshots internally, but its queries must not
distribute branch alternatives or unfold recursive types/calls without a bound.
The intended bound is polynomial in checked source/type nodes, explicit tracked
paths, and formal-origin atoms, independent of numeric capacity and runtime
iterations. This is an implementation criterion, not an established complexity
result; unknown selections and residual contents remain a precision boundary.

Two attribution choices remain to be stated precisely. Existing EFF-1 names the
complete state supplied by a bare formal, and current call projection includes
all current origins under that path. Retaining that broad projection for a
container helper is the initial comparison: a helper that only permutes elements
and one that retires them may both declare `writes(run)`. Finer storage/content
contracts need a concrete lost-independence or cost witness; a larger effect row
alone is insufficient when the imported owner has already moved under the run's
exclusive ownership. Always assigning extracted resources to the container's
place is invalid because it loses the incoming owner's identity.

Separately, SET-2 says a commit touches the target's ultimate storage origin,
while ordinary binding and static-field replacement change the current value
origin and EFF-2 adds no permanent parent ancestry. The selected whole-owner and
static-field rule keeps the borrowed address/loan fixed, changes the current
owner F to I, and attributes a second direct replacement to I. The complete
representation of an owning allocation and its changing run contents remains open:
an interior run-slot mutation retains the run's actual storage anchor while
transferring its contained owners. The scalar Box direct/helper controls do not
select that internal representation or settle every nested effect projection.
Adding a permanent cell ancestry to every local or reference is not an accepted
repair.

Completion of the remaining typed-content extension requires both
consumed-and-returned owners and exclusive borrowed writeback through Box
contents and exact singleton/literal-slot exchanges, preserving the completed
whole-owner/static-field paths. Its falsifiers include two successive
replacements and helper calls with independently
released old/current owners; omission and spurious addition of each formal effect;
reborrowed and disjoint-field writeback; same captured versus changed indices;
branch and priority-loop joins; and retained native descriptor replacement with
an exact release observer. Check source-size growth with long overwrite chains,
branch diamonds and repeated helper composition. Unknown residual indices,
recursive contained-state summaries, finer source contracts and proof-informed exclusion
remain explicit questions. A fix limited to own-input/result helpers, a green
Box ABI test, or one straight-line graph cannot close this gap.

The next typed-selection control distinguishes the complete read query from
the exact prefix available to a weak update. After literal slots receive
records with fresh scalar keys and imported Box payloads, `rows[index].key`
must not acquire the payload owners' read effects. The corresponding payload
read must retain every selectable supplier, and omitted or spurious effects
must still fail. The prefix-only query stopped at the dynamic index and lost
the field suffix, incorrectly charging a key read to those payloads. Preserve
dynamic element queries followed by fields, variants and referents through
ordinary checking and callable replay; do not turn such a query into an exact
writeback location or narrow its loans. This experiment does not establish
literal positions after append, exact residual membership or loop migration.

The implemented query matches dynamic element selectors against represented
literal elements before applying the remaining typed suffix. A selected
value's source routes remain bounded but retain their relative field layout;
later borrowed-helper projection therefore still separates key and payload. The paired
source controls in `semantic/tests/system_effects.rs` accept direct and helper
key reads, reject spurious payload effects, and require both selectable payload
sources for the payload helper. Nested dynamic indices and a FixedVector
control use the same query. Weak updates retain the existing prefix bound;
no wildcard query becomes an exact destination or a loan permission.
An unrepresented suffix keeps the old unlocated bound rather than reusing the
prefix value's field layout for a different selected type. At `c6cd99e2`,
cross-call instantiation flattened every bounded owned result. The later
correspondence experiment below distinguishes a complete typed choice from
that arbitrary unlocated supplier bound.

A direct/helper comparison isolates that correspondence loss at `19771851`.
A slot holds a resource owner and a separately constructed empty scratch run.
After a dynamic replacement, its caller returns the resource and releases only
the scratch field. The direct form retains the fields' separate origins; an
owned result crossing a helper is flattened into an unlocated bound, and the
resource's source is then attributed to the unrelated scratch release. Both
forms return normally and perform no I/O. This is a compiler composition
counterexample, not evidence for a new empty-container API or usage frequency.

The next comparison distinguishes a complete typed selection from arbitrary
unlocated contents. Only a choice whose source and selected value retain their
internal type correspondence may preserve mapped field layout across a call.
It still supplies a bound, not an exact selected supplier, complete coverage,
or a loan location. Selecting from already rearranged contents must not regain
that correspondence. The criterion is equivalent direct/helper behavior with
two possible source elements, retained omitted/spurious effect rejections,
and algebra controls showing that whole, partial and unknown actuals cannot
acquire precise layout or complete coverage merely by passing through a helper.
Caller length facts remain independent and are not forwarded by this relation.

The implemented typed choice passes the two-element direct, helper and relay
controls with the same declared effects. Omitting the second supplier's write
or declaring an unobserved incoming-field read still rejects at EFF-2. The
scratch field retains its separate source image; no empty-length fact is
invented for it. Algebra controls keep selected suppliers below exact identity
and complete coverage, and keep rearranged or unknown actuals conservative.

The boxed migration witness also needed an ordinary source correction:
`replacement` is read after installation and later exchanged out, requiring
both categories in its declaration. At `2451cc5e` the incomplete union concerned
`inserted`, the last append's payload. Earlier append calls already establish
the first and second owners' effects, while literal replacement establishes
the replacement owner's effects; the last appended owner's position was lost
before its later selection. Widening its row or the meaning of a supplier
bound cannot supply that missing placement evidence.

The endpoint experiment carries known logical run lengths independently
of owner routes. Empty run constructors establish zero; a known length locates
back insertion and extraction without changing earlier logical indices.
Structural composition transports facts with fields, while control-flow joins
retain only common lengths. Callable owner identity alone must not preserve an
input length: a helper may keep an owner while changing its window. Unknown
lengths and front shifts retain conservative content bounds. The selection
criterion is the unchanged boxed migration together with repeated extraction,
nested helper, branch-length mismatch and large-capacity loop controls; the
analysis must not enumerate capacity or iterations. Descriptor-only length
reads must remain separate from imported element effects. This experiment
does not establish symbolic endpoint contracts or general front-shift images.

The candidate restores the maintained boxed migration and the complete native
family runner without altering their source or assertions. Its length facts
carry known enum alternatives: a None return contributes no constraint on a
Some payload that does not exist on that edge, while another Some with a
different or unknown length invalidates the fact. Heap and Arena allocation
success use Option's Some tag; Box allocation continues to use Result's Ok tag.
These conditional facts describe a payload only when it exists; they neither
prune control-flow edges nor grant more precise loans.
Descriptor-only queries and typed release projection accompany precise element
routes, preventing unrelated payload reads and memory-only sibling writes.
Owning arrays use their elements' release rows, with no element action at zero
extent. The omitted and spurious release controls retain their negative meaning.
The caller's input lengths are deliberately not substituted through plain owner
correspondence; symbolic unchanged/changed-window contracts remain future work.

A recovery experiment distinguishes an unresolved source from a finite source
bound whose destination placement is unresolved. Only a completely described
value transfer may construct the latter: moving run elements cannot introduce
an owner other than one supplied by its operands. An unlocated route denotes a
bound, never an exact structural correspondence. Selecting below it retains
the complete source subtree; instantiation must not append that destination
selector to the source. A nonempty surviving bound establishes no effect by
itself; the whole-effect experiment below compares independently established
contributions with all possible contributions. Unknown remains unknown even
for apparently fresh actuals.

The experiment's criterion is whether this distinction restores
ordinary fresh-state construction and helper composition while preserving the
normal-return imported-owner counterexamples, exact-field controls and all
required rejection expectations. An excluded fresh replacement must not fall
back to an older bound. This is a precision recovery experiment, not a proposed
replacement for exact slot transfer, residual contents or cardinality evidence;
those remain necessary for general imported-owner container operations. Reject
the experiment if its bound loses a possible supplier, if a placement query
silently treats the bound as exact, or if it adds unbounded call-history paths.

The implemented transfer distinguishes this finite bound from a wholly unknown
source. Kernel expressions capture the already checked argument images; storage
read-out captures its resolved value image, independently of the access list
used to calculate its address. Callable replay uses the same kernel transfer and
preserves side effects while evaluating index and loop-endpoint expressions.
The ordinary helper control constructs, relays and extracts a fresh Box. The
normally returning imported-owner control now establishes its full read/write
row, so its original `pure` declaration rejects at EFF-2 rather than stopping
at the earlier routing capability boundary.
The route-algebra control checks fresh overrides, sibling bounds and source
selection during substitution. A literal-element LIV-2 read-out retains its
formal element route across a helper. The attempted dynamic-index read-out is
rejected by LIV-2's current literal identity rule; it is a negative control, not
evidence of an accepted dynamic read-out defect.

The source-bound recovery restores 26 of the preceding 29 unit failures with
their original successful execution expectations. The three then remaining
failures separate complete run-to-array transport, coverage at a nested helper
projection, and the storage/residual distinction after owning extraction.
The bound alone selects none of those precise images. Broader family readiness
and performance remain the experiment's original completion requirement.

The next recovery experiment retains complete source coverage when an operation
transfers every supplied value but loses their positions within the result.
This is stronger than a supplier bound and weaker than a structural slot map.
Declared whole-value call effects may use this coverage; selecting an unknown subvalue or
replacing part of it must reduce it to a bound. A call can preserve coverage
when projecting the complete returned component, but cannot upgrade a bound
supplied by its actual. Owning insertion conserves all of its operands;
extraction partitions its operand and cannot give either result complete
coverage without additional evidence.

Accept this refinement only if it restores full-array/helper transport with
the existing effect rows, rejects omitted and spurious suppliers, and refuses
to infer exact selected-element sources after replacement, extraction, or a
helper wrapping an already incomplete image. Test the route algebra as well
as ordinary WF composition. No runtime token, capacity enumeration, recursive
call history or additional acceptance budget is part of the experiment.

The complete-coverage refinement restores the imported-owner array's retained
native Heap/Arena helper control and the nested run's generic-region control.
A two-supplier run, returned in a result list and then repacked into one static
field, reaches an ordinary conversion helper with exactly those suppliers.
Omitting either read or adding the unrelated sibling's read rejects by EFF-2.
This control also exposes an independent kernel projection defect: after the
selected value image was available, address-evaluation access roots were added
again, attributing a field operation to its entire containing struct. Kernel
calls now use the same owned-actual fallback as user calls; argument evaluation
and borrowed-place effects retain their separate contributions. Algebra controls
keep selected elements, overwritten contents, both extraction outputs, and
incomplete helper actuals below complete coverage. Repeated owning extraction
still requires the storage/content distinction and is not restored by this
refinement.

Local descriptor reads and type-directed release are separate queries. A fresh
run holding an imported Box does not read that Box merely to observe its length;
releasing a run whose element contains a state-writing resource and an ordinary
memory Box does not write the latter. Both ordinary source controls retain
their specified rows; the endpoint experiment above repairs these selections
using descriptor roots and type-directed release components.
Complete coverage may not turn either into a demand for a spurious effect.
Likewise, two equal supplier bounds are not an exact subvalue identity:
replacement's no-change shortcut requires exact selected routes in both body
checking and callable replay. These controls narrow the complete-coverage
claim to its actual call-boundary evidence.

The whole-effect experiment separates exact whole-function effects from exact
placement at every contributing operation. For each written effect category, retain a
lower set of established body-syntactic contributions and an upper set including
every possible contribution of a finitely bounded query. EFF-2's exact union is
known when these sets agree; the declaration is compared only afterwards and
never supplies either set. For example, the first removal from an incoming run
already establishes its read and write, so a later removal bounded by that same
effect atom need not change the union. This does not determine which element
either removal returns.

The discriminator is the unchanged native boxed-run read-out plus ordinary
direct/helper and loop controls. Require omitted and extra effects to reject
after the exact union is established, while an uncovered possible supplier,
an unknown source, or an uncertainty in a different effect category retains
the capability stop. Source paths are discrete atoms: a struct root does not
cover its field paths. The bounded initial experiment therefore stops when an
uncertain source itself has nameable struct fields rather than guessing their
contribution. Do not enumerate capacity, unfold recursive contents, consult the
declared row to settle uncertainty, or promote an owner route to exact or whole.
The bounds exist only during boundary-effect checking; source locations, loans,
kill information, parallel footprints and runtime code remain independently
derived. Descriptor-only selection and type-directed release remain separate
requirements. This candidate is selected provisionally if it restores the
discriminating operation without losing those negative controls; it is not a
replacement for a precise container-content image.

The unchanged boxed-run native witness now executes both consecutive removals
and checks the original element values. Direct and helper-mediated controls
establish a possible supplier by a separate read, then check the exact row;
omitting that read from the declaration or adding an unrelated source rejects.
Removing the establishing operation leaves an explicit capability gap even when
the declaration lists every possible source; establishing only a write does not
close the possible read. Product-field controls reject substituting a struct
root for its fields. The former extraction and shifted-slot capability sentinels
now inspect the retained bounded result summaries, and the imported `pure`
helper checks its actual missing read and write. These changes refine compiler
effect knowledge without changing source rules or any native success expectation.
The whole-effect refinement also restores the unchanged fixed-run library,
optional-entry map and nested boxed helper. It leaves the boxed migration at a
still-incomplete effect union, as observed at `2451cc5e`; the endpoint experiment
above then restores that migration by retaining the last appended owner's
position. These outcomes establish composition for those witnesses, not general
dynamic residual-content selection.

### Sparse experiment contract and decision boundary

The [owning sparse control](../../experiments/container-representation/costs/RESULTS.md#owning-sparse-layout-and-migration)
executes the native operation chain, resource conservation, refusal, actual
growth, returned migration progress and cleanup. At 4096 slots on the measured
ABI, matched one-backing layouts use 24 versus 17 bytes per capacity unit;
separate resource allocations and descriptors are counted outside those figures.
Half-full same-capacity rehash plus digest has a lower split-layout median in
this run. The narrow timing does not establish general hash-table throughput.
These are physical and protocol targets for the next WF experiment, not evidence
that either proposed WF authority is already admitted.

The ordinary [owning-map witness](../../experiments/container-representation/families/owning-map.wf)
now executes runtime probing, replacement, deletion, lookup past tombstones,
full-table refusal returning its input owner, and reuse after a full scan.
The source uses a complete eight-element array of ordinary enum slots and
modern owning boxes. Its [native allocation observation](../../experiments/container-representation/families/RESULTS.md#operation-contracts-and-recorded-runs)
checks all 192 allocation-refusal positions and the successful execution,
including exact owner cleanup. This closes the fixed-capacity owning-value
operation chain. The later
[ordinary owning-growth comparison](../../experiments/container-representation/families/RESULTS.md#owning-sparse-growth-over-ordinary-values-d2)
additionally executes growth/refusal, returned progress and exact cleanup,
and measures it against the native owning sparse controls. Generic behavior,
construction and selection of a projected sparse authority remain open.

Use one concrete map payload containing an owning resource. Execute collision
insertion, duplicate replacement, removal, lookup past a tombstone, reuse, growth
rehash and final cleanup. Specify allocation refusal and an intended stop during
migration, returning all still-owned state; do not assume rollback of completed
work. Compare the same capacity, key/hash, control encoding, migration policy and
failure contract across ordinary optional values, projected enums and resource
permissions. A seven-bit fingerprint with empty/deleted states is one matched
encoding; an arbitrary eight-bit fingerprint is a different layout comparison.

Entry relocation is permitted in this first contract. Its owning resource retains
its separate backing, but the key and owner descriptor may move. A future stable-row
comparison must retain an actual borrow of the row/descriptor, not infer that need
merely from an owning payload. Do not charge every map for address stability it
does not promise. Track control examinations, migration work, allocation count,
peak backing, initialized bytes and payload movement; timing claims apply only
to the operation actually measured.

The native control is the physical target, not a checked implementation of either
WF authority. The projected-layout prototype must then admit the same dynamic
operation chain and conserve every resource through refusal, returned progress
and cleanup. A separate fallible large-element insertion must compare complete
results, internal result destinations and any proposed vacant destination with
the same effects and cleanup. This belongs to the construction/placement
implementation experiment: the existing fresh-result control does not already
establish in-place generic map construction.

If projected enums and resource permissions produce the same representation and
operations, timing cannot distinguish their proof authorities. The broader route
must additionally demonstrate symbolic runtime-index focus, retention of the other
slots, transfer/cleanup and erasure without an extra token table or scan. A finite
concrete model is insufficient. Conversely, a required layout or retained access
that the narrower route cannot express is a real discriminator even if a scalar
lookup benchmark happens to match.

Two independent controls prevent the map from replacing the overall coverage
question. The byte-page witness supplies insertion, deletion and malformed
input behavior, and the growth trace prices one complete byte-run operation.
Bulk page movement and reusable validation costs remain distinct from that trace.
The two-index control distinguishes weak lookup returning `Expired` from retained
membership making deletion return `Busy`, including store mismatch, reuse and
generation exhaustion. It does not establish an efficient checked WF admission
or backing lifetime. Neither experiment requires implementing concurrent RCU
first or silently changing the current STOR-1 boundary.

### Independent ceilings and dispositions

The available evidence supports different next actions for different families:

| Area | Current disposition | Evidence that would change it |
| --- | --- | --- |
| Dense/fixed sequences and priority queues | Ordinary valid values admit a scalar heap with borrowed full-array storage and matched native costs; improve remaining owning-run transfer independently | Resource-owning or growing operations still force material initialization, descriptor or movement cost; generic comparison behavior cannot retain the operation contract |
| Full arrays of general elements | The selected two-conversion experiment executes build/failure/freeze/use/replace/thaw/drain; general views, construction transfers and explicit linear-empty termination remain separate gaps | Matched final-place construction and transfer cost, and ordinary checked views of owning elements |
| Hash and ordered containers | The ordinary owning-value map executes probing, replacement, deletion, full/refused input return, reuse, growth, returned migration progress and exact cleanup; the ordered split component also executes. The measured hash gap still includes interface/initialization/cleanup costs and does not select projected sparse authority. Generic behavior and complete ordered mutation remain open | Isolated compact-layout benefits after accounting for ordinary lowering/interface costs, or a required operation beyond ordinary values; a complete ordered mutation trace |
| Deques and rings | A wrapped logical-index trace works; a wrapped run is correctly refused as one contiguous view | A two-span consumer/growth trace that prices any required copying and admits the actual loans |
| Growable runs, strings and inline/spill forms | Source-written byte growth/refusal executes and has loop/bulk/realloc controls; no WF realloc or finished spill result | General owner-return helper, repeated reserve/spill and copy-heavy resize controls including peak storage and address validity |
| Packed byte records | Current initialized byte storage executes variable records and overlapping movement | Measured bulk/initialization/compact-handle cost, or an actually required typed layout that byte codecs cannot preserve |
| Stable slots, sparse sets and multiple memberships | The two-index native control distinguishes weak/retained memberships and logical access; bounds alone supply neither | Priced lookup/metadata, authoritative store identity and a checked source admission/backing-lifetime design |
| Shared/concurrent containers | Sequential storage selection grants no reclamation protocol | A separately specified publish/read/retire contract with the actual memory model and scheduling behavior |

The later [D7 behavior witnesses](BEHAVIOR.md) supply generic
hash/equality/comparison invocation through explicit formal/actual groups and
retained direct calls. The earlier concrete-key representation experiments
alone did not establish that reusable interface. Stored member provenance is
separately restricted by STOR-5. Allocation/resize and variable-tail layout need
their own provider and layout contracts. A slot permission alone supplies none
of these, and the architecture must not claim those system needs solved.

Ordinary valid values remain the baseline, with the selected full-array
experiment extending its completed-value domain. The earlier projected-layout
prototype proposal is superseded by D2's ordinary owning-map measurements:
the observed gap does not justify selecting compiler-known sparse authority.
General resource permissions become the preferred candidate only if a required
operation, layout or lifetime cannot be retained through ordinary/projected
values and the alternative supplies a credible deterministic checking and
erasure path. Equal native code cannot choose between proof authorities.

Research can therefore hand off to those implementation experiments without
claiming a universal container substrate. Complete ordered
mutation, two-span consumers, inline spill, general array views and construction
destinations, stored lifetime, variable tails and concurrent retirement remain
named capability questions.
They are not silently counted as solved or prerequisites to fixing the observed
compiler defects. Reopen the selected route when one supplies a concrete
contract/cost counterexample; do not infer either universal coverage or universal
failure from the bounded map alone.

### Temporary child regions and statement endpoints

The question is whether a non-candidate argument child needs its local region
block confined to the receiving statement. The prior
[bounded-reborrow study](../reborrow-investigation/DOSSIER.md#4-option-b--relax-to-bounded-non-escaping-statement-scoped-reborrows)
chose that syntactic restriction to keep suspension simple. OWN-4 and the
later completed-header rule already separate a temporary loan's endpoint from
its region's formation and type-validity ceiling.

The [criterion recorded in the pre-experiment revision](https://github.com/mbbill/Whitefoot/blob/5aaef50a4c37b43de1780f08abfa3db211ce7bf8/research/investigations/containers-and-resources/FOUNDATION.md#owner-identity-across-replacement-and-calls)
was to admit the direct displaced
Box read without a helper while retaining surviving result/view loans,
overlapping-sibling rejection, parent suspension throughout argument and
statement evaluation, and the existing parallel retirement obligations.
Choosing an endpoint at each individual call return would fail the same-
statement controls. General last-use inference was not needed for this question.

The selected v0.55 change removes only the one-statement region-block condition.
The child still needs a local region, the parent must outlive that region,
and OWN-11 still excludes a region outside an enclosing loop. Its temporary
ends at the complete statement or the existing non-escaping header boundary.
A borrow-result candidate and a surviving view retain their own loans. Storage
brands in returned Box/Vector values are type identities, not loans keeping a
provider borrowed. No borrowed storage becomes movable or reusable merely
because an internal task flag says DONE; PAR-1/PAR-3 permissions and join,
result access, and retirement requirements remain unchanged.

The discriminating executable is
[the longer-region case](../../../tests/conformance/cases/own6-pos-statement-children-use-a-longer-local-region.wf).
It exchanges a Box through a child, reads both the displaced and installed
values on later statements, allocates twice through one retained provider, and
reuses a parent after child calls in a loop body. The native test executes it
in all three lowering modes, both normally and with helper calls retained.
These executions pass; the original v0.54 compiler rejects the direct Box
form at OWN-6. This is evidence of expressibility and preserved behavior on
these cases, not a throughput measurement or a proof of complete soundness.
For the unchanged `x-child-reborrow-run.wf` witness, the v0.54 and v0.55
compilers emit identical LLVM after excluding the qualification-version
comment. This supports the absence of a runtime change for that accepted
source; it does not measure compilation cost or general performance.

Implementation needs no new lifetime analysis: the existing statement-loan
stack already retires each statement's entries. Removing the block-containment
check removes a syntax-tree scan. The experiment also exposed a separate old
implementation defect: later arguments checked only overlap with a previous
child, overlooking suspension of its complete unique parent. A shared helper
now checks parent suspension before the existing call-overlap judgment in all
three call families. It covers a direct parent read and a bare holder transfer
whose expression has no ordinary access entries; explicit sibling formation
remains allowed. The
[parent-argument negative](../../../tests/conformance/cases/own5-neg-later-argument-uses-suspended-parent.wf)
rejects OWN-5. Shared parents, earlier reads and disjoint sibling children remain
positive controls; overlapping ordinary own-root actuals retain OWN-12.

The view controls require care. A live shared child view must still forbid a
parent write. A shared child's *last use* should instead restore permission
under VIEW-2. The probe below is legal, but the pre-fix compiler rejected its
write at OWN-5 even without a preceding ordinary child call:

```whitefoot
fn reuse(view: &uniq MutSlice<u8>) -> result: own u8 reads(view), writes(view) contract {
  requires len_of(deref(view)) == 1_u64;
} {
  region {
    let shared = slice_of(&deref(view));
    let previous = shared[0_u64];
    set deref(view)[0_u64] = 9_u8;
    return previous;
  }
}
```

This was an implementation gap, not another required rejection or a reason to
retain the old region restriction. The formed loan had no registered descriptor
when its origin was a formal slice. The initial repair registered formal-view
and local-storage descriptors through the same origin-to-place projection.
The storage-holder experiment above exposed that projection's inability to
distinguish a new exclusive claim from an old shared one. Registration now uses
the value's exact continuing keys; the existing last-use judgment still ends
each child's freeze at its specified point.

The helper-return control exposed the opposite defect. A returned shared child
was registered only where the caller had a local exclusive-formation loan.
An incoming `MutSlice` supplies its permission without such a formation record,
so a parent write followed by a read of the returned child was incorrectly
accepted. Publication now registers the child on that formal view as well.
Its copies register on the same loan; every remaining use keeps the freeze.
Neither repair changes regions, source rules, the existing liveness algorithm,
nor emitted runtime mechanisms.

The semantic
[`formal_view_children_release_at_the_last_use_of_all_descriptors`](../../../compiler/src/semantic/tests/slices.rs)
controls pair direct and helper-returned formation with dead and surviving
copies. Local holders of an incoming view have corresponding controls. The
native
[`formal_view_child_last_use_restores_parent_writes_across_retained_calls`](../../../compiler/src/backend/tests/slices.rs)
observes the former element and the subsequent parent write in all three
lowering modes, both normally and with helper calls retained. The genuine
surviving-view and inner-region-ending controls remain unchanged. These cases
establish the repaired formal-view boundary, not general container readiness.

The change is evidence-selected under META-5. Its more precise selection
grounds are conditional deduction (parent suspension and independent
surviving loans preserve the one-usable-mutable-path invariant), empirical
(the executable and rejection controls), and provisional (the retained lexical
ceiling and statement endpoints, without a global optimality claim). They serve
ordinary systems code without a new runtime check, pointer escape or callee-body
inspection. Reopen if a legal result form can carry an untracked loan past the
endpoint, or a staged access can outlive its permission/retirement boundary.

The affected normative set is OWN-6 and FORM-8's explanation of narrower loop
regions: rules +0/-0, grammar productions +0/-0, tokens/spellings +0/-0, no new
exception. OWN-4/5/10/11, FN-1, STOR-5, VIEW-2/6 and PAR-1/3 were checked as
premises and retain their judgments. The rule index, writer forms, capability
map and ownership decision memory follow this change. Qualification rows and
runtime layouts are unchanged. At `87a1462d`, this investigation recorded the
owning-array and contained-state gate failures as separate blockers. The later
owner-routing experiments above restore the owning-array witnesses and bounded
container operations; general contained-state routing remains incomplete.
At checkpoint `19771851`, a new loop fixture was rejected for an invalid header
invariant equality before its intended assertions executed. `8482c0cb` rewrote
that header as the two admitted inequalities. That historical gate failure is
not a current language limit. The loan-endpoint result and those repairs do not
by themselves complete the broader container scenario and performance requirements.

### Exclusive two-state run contracts

The owner selected this interface after the D1 transfer attribution and D2 owning
map experiment. Mutation takes `&uniq`; owned input/output remains appropriate
for constructing or converting a value. D1's proposed guaranteed same-place
owned call contract was dropped. Its dated measurements remain evidence, not a
pending interface proposal. The [D5 measurements](../../experiments/container-representation/families/RESULTS.md#exclusive-two-state-contracts)
compare the selected interface against the same retained C helper controls.

The requirement is modular proof with native mutable-call cost, not a smaller
surface at any cost. A caller must know which state the contract names without
reading a helper body. A helper must be able to prove its new window length at
its normal returns. These requirements select two explicit times:

```wf
fn push(values: &uniq FixedVector<u64, 16>, value: own u64)
  -> result: own unit reads(values), writes(values) contract {
  requires room_of(deref(values)) > 0_u64;
  ensures len_of(deref(values)) == len_of(deref(entry(values))) + 1_u64;
} {
  region {
    place_back(vector: &uniq deref(values), value: value);
  }
  return unit;
}
```

This is a declaration fragment; the canonical layout puts the signature on one
line. `entry` applies directly to an exclusive formal name, only in ensures;
normal dereference and field projections follow it. It snapshots proof terms,
not container bytes. Bare exclusive measures name the selected exit. Own
parameter measures retain their existing immutable entry meaning. Equality
requires both affine directions; it is not trusted because it appears in a
contract. FN-9 checks each selected normal return before CALL-6 can publish it.

At the caller, the exact declared effect row is projected onto resolved places.
A write kills every fact whose support it overlaps. A whole-run write therefore
kills the old descriptor facts even when the actual is a holder or a field.
A disjoint field survives. Verified entry terms use pre-transfer call datums;
verified exit terms use the resolved actual place after the call. Result-free
clauses publish on normal call completion and are not published again when a
result is bound. Result-bearing and routed clauses retain their existing
selection and destination obligations. No contract supplies a guarantee merely
because its parameter is exclusive.

The rejected competitors are discriminating, not stylistic: keeping old length
across an arbitrary whole-place replacement is unsound; interpreting both sides
of a length increment as the same time establishes a contradiction; killing all
facts and adding an impossible failure branch merely conceals a missing proof.
The [whole-place replacement](../../../tests/conformance/cases/exclusive-neg-whole-referent-replace-kills.wf)
and [same-time increment](../../../tests/conformance/cases/exclusive-neg-exit-is-not-own-increment.wf)
remain negative controls. [Counted push/pop](../../../tests/conformance/cases/run-exclusive-push-pop-counted.wf),
[owning map put](../../../tests/conformance/cases/run-exclusive-owning-map-put.wf),
[nested-field framing](../../../tests/conformance/cases/run-exclusive-nested-field.wf)
and [multiple results](../../../tests/conformance/cases/run-exclusive-take-multiple-results.wf)
exercise the positive boundary. The no-ensures controls distinguish a lost fact
from an intentional runtime reread; the live-view control retains OWN-5.
The owning-map success witness exits nonzero on allocation refusal so that
success requires the whole insertion/replacement chain. Its backend observer
also refuses each of its four allocation positions and checks the exact
release sequence of the owners acquired before refusal.

The amendment is v0.56, archiving the exact outgoing v0.55 bytes. META-5 delta:
rule IDs +0/-0; fixed terminals +1 (`entry`)/-0; grammar productions +0/-0;
public formers +1 (`entry`); four boundary rows redeclared, no extra row;
record-only `at the call` notation removed; exception clauses +0/-0.
The selection ground is evidence-selected: D1 identifies transfer as the major
retained-helper cost, D2 demonstrates the useful missing mutable/equality
contracts, and the negative controls exclude the two invalid shortcuts.
The single `entry(name)` spelling and its narrow admission are a provisional
minimality choice within that evidence-selected interface.

Affected rules are BLK-0/3/4, FN-9, MSR-3, CALL-5/6 and ENT-3.S12/S13; the grammar
and syntax inventory derive the former without a second proof or runtime path.
BLK-4 removes the entire recursive exclusive-parameter refusal, including
opaque type parameters. Whole replacement through an opaque `T` is a declared
write, so the same storage-overlap kill removes caller facts about nested run
elements. No callee-body inspection or generic proof family is needed. The
confinement and loan-bearing position judgments remain. The
[generic append](../../../tests/conformance/cases/run-exclusive-generic-append.wf),
[whole replacement](../../../tests/conformance/cases/exclusive-generic-whole-replacement.wf)
and [nested stale-fact negative](../../../tests/conformance/cases/exclusive-neg-generic-element-replacement-kills.wf)
distinguish this judgment from simply retaining a fact across an opaque write. Existing
v0.54 result/borrowed-owner routing still handles real owning payload movement;
run-descriptor round trips no longer motivate it for these mutation witnesses.
The routing text and machinery remain in place. These measurements do not
settle general recursive origin summaries, projected sparse layout, staged
address stability, or behavior parameters.
