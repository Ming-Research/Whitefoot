# Clearing a window while retaining its storage

## Question and evidence boundary

Firn needs to reuse a `Box<Slots<u8>>` between AOF batches: set its initialized
length to zero, retain its capacity and backing allocation, allocate nothing,
and do work independent of the old length. The reported current workaround
allocates a new 64 KiB buffer per batch; that downstream behavior is the task's
input, not a measurement made here.

This is a design proposal against Kernel Specification v0.149 in
[the active specification](../../../spec/kernel-spec.md). Rule identifiers below
refer to that text. The two research files preserve the gap and proposed
resolution; they are not a language amendment or an implementation claim.
No compiler, build, test, benchmark or GitHub operation was run. All costs below
are deductions from specified operations and inspected lowering, not timings.
The witness and examples have been read against the grammar and existing
programs but remain uncompiled.

The comparison that can answer this question is a retained-buffer reset made
from existing operations versus one primitive that ends all element lifetimes
and resets the boundary. Reject the proposed primitive if an existing general
composition provides all four requirements without relying on optional loop
elimination, or if the primitive cannot preserve ownership, invalidation and
release semantics while meeting the stated cost. For an eventual implementation,
reject the O(1) claim if its empty-release body visits an element or a page, or
does any work proportional to length or capacity. Reject the allocation claim
if reset or refill within capacity performs an allocation.

This serves the constitution's performance objective subject to its safety
constraint ([constitution](../../../docs/constitution.md#performance)), using
the investigation method in [the research guide](../../README.md). It introduces
no writer access to window metadata and no unproved partial operation.

## Selection and implementation

The owner selected the general drop form, `clear<W, T: drop>` over `Slots`,
`Ring` and `Paged`, on the status board. Whitefoot#486 implements it: the
specification text in OP-10, OP-13 and PRE-1, the checker's drop bound and
reference invalidation, and the lowering below. The acceptance cases of the
last sections became the `op10-`, `prov6-` and `ref2-` clear cases in
`tests/conformance/`, the backend tests of the emitted clear body in
`compiler/src/backend/tests/windows.rs`, and the program
`tests/programs/containers/clear-program.wf`, whose allocation observer counts
every allocation and requires each to be released exactly once. The rest of
this record is the proposal as written before selection: its statements that
nothing was compiled or run describe that time. One difference from
it, observed in CI: the checker does not derive `p.pages.len == 0` from a cleared `Paged`'s
length, so the case checks the page count at run time instead of stating it
as an invariant.

## Existing operations do not supply the required reset

WIN-1 makes `len` the entire initialized-window boundary and reserves changes to
OP-10 and OP-13. PRE-1 declares `len` readonly; TYPE-2 refuses a source
`set bytes.inner.len = 0_u64;`. WIN-2's `filled`, `last`, `next` and `free` are
effect/overlap vocabulary, not assignable places. WIN-3 forbids moving an
element out to make a hole, and moving an owner field consumes its owner.
TYPE-9 also prevents moving runtime-capacity storage out of its `Box`.

Let n be the entry length and assume a fixed element type. The applicable
PRE-1 records and OP-10/OP-13/OP-14 give this exhaustive operation comparison:

| Existing operation or composition | Result and cost relevant to reset |
| --- | --- |
| `place_back`, `place_front`, `insert_at` | Increase length; cannot empty the owner. Front operations admit Ring only. Insertion shifts elements. |
| Repeated `take_back` | Preserves backing and capacity, allocates nothing, but removes only one element per call: n calls and Theta(n) boundary work. Returning `u8` needs no release. Droppable owning elements additionally take their derived release. |
| Repeated `take_front` | Ring only; one removal per call, hence n calls. It also changes the origin. |
| Repeated `remove_at` | One removal per call and a shift of the filled run; at least n boundary operations. Repeated removal at the front additionally moves a quadratic number of elements. |
| `append(destination: &other, source: &bytes.inner)` | Makes the source empty and retains its capacity, but transfers all n elements into a distinct window with sufficient spare capacity. OP-10 specifies a run copy. Existing spare storage avoids allocation, not linear movement of positive-size elements. |
| `split_off(source: &bytes.inner, index: 0_u64, destination: &other)` | The actual signature takes `index` and a caller-provided destination, not `at` and an allocating return value. It empties the source by transferring all n elements. It has the same movement and spare-storage problem as append. |
| `grow`, `grow_paged` | Retain length by their explicit postconditions and may allocate. Their capacity precondition forbids shrinking. Neither resets length. |
| Construction and replacement | Constructors produce a new empty owner. Replacing a positive-capacity boxed buffer releases its old owner and allocates another; replacing it with capacity zero loses its retained capacity. Array/window conversions consume owners and require or produce full windows; they do not empty this runtime-capacity owner. |
| `free_empty` | Requires the window already empty, consumes its owner and frees backing storage. It provides neither the reset nor retained ownership. |
| Scope release or `release` | May skip byte-element cleanup under PROV-6, but STOR-3 still frees the Box. It destroys the allocation to be retained. |
| `swap` with a spare empty owner | Exchanges whole owners, not the length of the original buffer. It leaves the previous contents in the other owner. Sustained reuse still needs that full owner cleared, released or replaced; it does not reset the same backing. |

Thus existing source can express the final length and retained capacity without
allocation by draining, but not a specified length-independent reset of this
byte buffer. A library wrapper has the same restriction: it cannot write
readonly metadata and must compose those transitions. An optimizer might erase
a particular byte drain or a dead transfer; that is a possible compiler
optimization, not a general operation with this cost guarantee. The current
transfer lowering also contains an element loop in
[`emit_run_transfer`](../../../compiler/src/backend/emitter/runs.rs).

For a general `Box<Slots<T>>`, there is a second obligation. OWN-1 and PROV-6
permit discarding only `T: drop`; a linear element must be explicitly consumed
by the program. For a droppable T with a non-empty release action, discarding n
elements must perform their required recursive release work. No general O(1)
claim can include n independent owned allocations that must be reclaimed.
Zero-size element types or optimizer-specific elimination do not establish the
required general result for `u8` or arbitrary T.

The natural current program is [witness.wf](witness.wf). It allocates the 64 KiB
backing once, inserts two bytes, drains them, and refills the same storage. Its
reset costs one `take_back` per previous byte; filling and final scope release
are outside the reset interval. A split-and-release alternative needs an empty
destination with room for n elements, transfers n bytes, then releases that
destination. Creating the destination adds an allocation; reusing it still
leaves the same reset problem. `free_empty` cannot consume that destination
while it still holds the transferred bytes.

## Alternatives and recommendation

The important distinction is **permission to discard** versus **release work**.
`copy` and `drop` are existing capability bounds (OWN-1, PROV-6). `drop` means
the value may take compiler-derived release; it does not mean that release is
empty. `Box<u8>` has drop and a non-empty release. A `nocopy` struct with only
scalar fields has drop and empty release, although it is not copy. A fieldless
`nodrop` struct has an empty release graph but is linear and must not be
silently discarded. PROV-6 already defines a type's empty/non-empty release
action through its ownership graph, separately from these capabilities; no
existing generic bound names exactly “droppable and empty release”.

| Alternative | Benefit | Limitation and disposition |
| --- | --- | --- |
| A1: `clear<W, T: copy>` | One existing bound admits bytes; every admitted element has empty release, so a boundary reset suffices. | Rejects affine empty-release types unnecessarily and provides no clear for droppable owners. Viable but not recommended. |
| A2: clear only if T has drop and an empty release action | Admits exactly the safely discardable types needing no traversal, including affine scalar wrappers. | Requires a new built-in admission predicate or bound, plus a second route for droppable elements needing release. Empty release alone is unsound for logical `nodrop` obligations. Viable but not recommended. |
| B: `clear<W, T: drop>` using existing derived release | One operation discards any legally droppable contents. Empty-release instantiations do no element work; owning instantiations release their contents correctly. | Cost depends on T. The contract must state the distinction, and linear elements remain refused. Recommended. |
| C: a source library over current operations | No new primitive; a drain can retain backing without allocation. | Performs n boundary operations or a transfer of n positive-size elements. It cannot promise the requested cost. Not sufficient. |

Recommend B. It supplies the exact byte-buffer operation and applies the same
ownership rule to other element types, without inventing a capability solely
for this optimization. PRE-1's `map_clear<V: drop>` uses the same discard
permission; SHARE-1 says it empties entries and releases their values. That is
a semantic precedent, not a cost or storage-retention precedent: neither its
record nor the map rule promises a zero-allocation, capacity-preserving O(1)
operation. The map's hold and deferred release machinery in
[its investigation](../map-enumeration/DESIGN.md#clearing) is not needed here.

This recommendation reopens the explicit choice in
[kernel minimality](../../../design/language/data-model/kernel-minimality.md)
to begin clearing and truncation as source libraries. Its grounds already
distinguish expressibility from equal runtime cost. The present evidence is
the exhaustive boundary-transition analysis above, not a new measurement.
It supports promoting this one reset operation, not adopting a built-in
growable-vector policy or claiming a speedup number. The existing
[ordered Vector consumption](../../../design/language/data-model/vector-consumption.md)
decision remains about callbacks and suffix consumption; clear has no callback
and does not replace that behavior. No design-tree decision is changed here.

## Proposed PRE-1 record and behavior

```wf
fn clear<W, T: drop>(window: &W) -> result: unit writes(window.filled), writes(window.len) contract {
  ensures window^.len == 0_u64;
  ensures window^.cap == entry(window)^.cap;
};
```

W and T are compiler-owned operand-derived parameters, as with `place_back`;
the caller writes `clear(window: &bytes.inner);` without type arguments. T is
the element type of W, and PROV-6 checks its drop bound. W admits
`Slots<T, n>`, `Slots<T>`, `Ring<T, n>`, `Ring<T>`, and `Paged<T>`. The operation
takes the window, not its Box. It rejects Arrays, Segments, range references
and a Box passed directly, using OP-10's ordinary shape diagnostic.

The spelling `clear` is proposed, not reserved today. PRE-1 and TYPE-6 put a
new prelude function in every module's ordinary callable inventory. Existing
source helpers already use that name; for example, the
[range-postcondition case](../../../tests/conformance/cases/range2-pos-reference-postcondition.wf)
defines `clear(out: &[u64])` to write zeros. Adopting this spelling requires
renaming conflicting helpers and their calls while preserving each case's
original property and verdict. In particular, a negative case must not begin
failing earlier for a name collision. No name-resolution exception is proposed.
Downstream name collisions have not been audited; that compatibility work is
part of selecting and implementing the final spelling.

The operation ends the lifetime of every initialized element, performs its
compiler-derived release, and leaves no initialized elements. It retains the
window's backing storage and logical capacity, and allocates nothing. It
retains a Ring's head and a Paged's allocated pages and directory. It neither
zeros the old bytes nor promises erasure of sensitive data. Subsequent placement
initializes a slot before that slot can be read again (WIN-1 and OP-4).

The release order is PROV-6/STOR-3's ascending logical element order, with the
same recursively derived actions and no source finalizer. The clear operation
releases elements only, not the window's own storage. For fixed T whose release
is empty, PROV-6's rule that such elements are never visited makes the operation
O(1) in both old length and capacity. This includes u8 and affine empty-release
types. Otherwise its cost is O(n + R), where R is the actual recursive release
work of the n initialized elements; this is O(n) only when each element's
release is bounded. An enum with a non-empty release graph is visited even
when a particular value's active variant owns nothing.

`writes(window.filled)` describes the end of the elements' lifetimes, including
the empty-release case; `writes(window.len)` describes the boundary update.
`writes(window.len)` alone would omit the affected element storage.
`writes(window)` would unnecessarily kill cap and head facts. WIN-2 already
supplies the relevant overlap answers, while MSR-1 supplies `len` and `cap`
and MSR-3 supplies the exact `entry(window)^.cap` spelling. There is no measure
named `capacity`. MSR-2 preserves facts supported only by cap or head because
neither is written. A uniform record cannot add a head clause because Slots
and Paged have no head measure; leaving it outside the write row preserves it
for Ring without a shape-specific signature. Paged's `pages.len` becomes zero
because it counts initialized pages as a function of len, not retained pages.

Every pre-existing element, range and page reference into the window becomes
invalid at clear, as at a boundary-ending take. This must be explicit in
OP-10, including an empty range or a clear of an already-empty window; merely
rechecking `end <= len` would leave an empty range alive. References to the
window itself and its descriptor words still name existing storage under
REF-2; they are not element references. Old facts about length and contents
are killed through the ordinary projected effects (ENT-5). A later refill
does not revive an old element reference; the program must form a new one.

Ring needs no special exclusion: it retains the origin, and release visits
the old wrapped logical window. Paged needs no special exclusion: its single
length controls the initialized prefix and each page's initialized extent;
clearing a byte Paged does not walk or free its retained pages. These are the
same window semantics rather than an extra container policy.

The no-heap rule remains unchanged. `clear` is not an allocating prelude row.
It is available for inline Slots and Ring whose contents meet STOR-8; a
no-heap bundle still cannot name Box, runtime-capacity storage or Paged. A
no-heap entry's concrete execution closure still excludes types holding those
heap resources, including elements whose release would free them. `pure`
alone is never evidence of zero allocation: STOR-8 gives allocation and
release no effect entries. The nonallocation promise belongs to OP-10's
operation semantics and its implementation evidence.

## Complete before and proposed after programs

The current-language program below is the witness with its explanatory doc
string omitted. The
postconditions state the same boundary result proposed for clear, while the
body exposes why composition still takes n operations.

```wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn clear_by_taking(window: &Slots<u8>) -> result: unit writes(window.last), writes(window.len) contract {
  ensures window^.len == 0_u64;
  ensures window^.cap == entry(window)^.cap;
} {
  loop {
    if window^.len == 0_u64 {
      return unit;
    }
    let removed = take_back(window: window);
  }
}

fn main() -> status: ExitStatus pure {
  let bytes = box_slots_new::<u8>(capacity: 65536_u64);
  place_back(window: &bytes.inner, value: 65_u8);
  place_back(window: &bytes.inner, value: 66_u8);
  clear_by_taking(window: &bytes.inner);
  invariant empty: bytes.inner.len == 0_u64;
  invariant retained: bytes.inner.cap == 65536_u64;
  place_back(window: &bytes.inner, value: 67_u8);
  if bytes.inner[0_u64] != 67_u8 {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
```

Under the proposed amendment, the complete program becomes:

```wf
alias ExitStatus = std::process::ExitStatus;
alias exit_status = std::process::exit_status;

fn main() -> status: ExitStatus pure {
  let bytes = box_slots_new::<u8>(capacity: 65536_u64);
  place_back(window: &bytes.inner, value: 65_u8);
  place_back(window: &bytes.inner, value: 66_u8);
  clear(window: &bytes.inner);
  invariant empty: bytes.inner.len == 0_u64;
  invariant retained: bytes.inner.cap == 65536_u64;
  place_back(window: &bytes.inner, value: 67_u8);
  if bytes.inner[0_u64] != 67_u8 {
    return exit_status(code: 1_u8);
  }
  return exit_status(code: 0_u8);
}
```

These demonstrate reset and reuse, not a timing benchmark or an allocation
counter. After the change, the byte reset has constant work even when previous
batches fill the entire 64 KiB capacity; filling the batch itself remains
proportional to its size.

## Specification and decision changes needed if selected

| Rule or maintained decision | Before | Proposed change |
| --- | --- | --- |
| PRE-1 | No clear record. | Add the record above at the window-operation position; keep declaration order and diagnostic ordinals consistent with the prelude. |
| OP-10 | “The ten operations” enumerates the current boundary operations. | Make the list eleven; add clear's W/T shape inference, drop requirement through PRE-1, element release, retained storage, nonallocation and conditional cost. State all window-selection references die. |
| WIN-1 | Only OP-10/OP-13 change len. | No changed restriction: the additional operation belongs to OP-10. |
| WIN-2 | Four parts and separate measure-word effects. | No new part or overlap rule; use filled and len exactly as written. |
| WIN-3, PROV-6, STOR-3 | Linear elements require explicit consumption; compiler owns release; empty-release children are never visited. | Preserve these rules. OP-10 invokes element release without invoking the enclosing window's backing release. The operation cannot clear linear values even when len is zero. |
| REF-2, ENT-5, MSR-1/2/3 | Invalidation, effect kills and entry/exit measures are shared judgments. | No new fact source or measure. OP-10 supplies clear's boundary-ending event. |
| OP-13 | Describes a bump allocator as Slots plus a “library reset”. | Clarify that a library may use the clear operation for droppable reset; do not imply that the previous wording guaranteed O(1). |
| OP-14 | free_empty requires empty and consumes backing. | Unchanged; still needed for explicitly emptied linear windows. |
| STOR-8 | Lists allocating rows and forbids heap-bearing types in no-heap use. | Unchanged; clear is nonallocating but does not relax any type prohibition. |
| Kernel-minimality design decision | Clearing begins as a source library. | If approved, record why the general retained-buffer cost requirement promotes clear alone. Update the storage-shape and prelude-lowering decisions where their operation inventories or release behavior need it. |

An implementation amendment must follow the repository's archive/version and
conformance process. This investigation changes no normative file, compiler
source, tree node or approval log. No approval is claimed.

## Compiler path and proposed lowering

The existing path is declaration data, ordinary semantic checking, a shared
specialized prelude body, typed IR, then backend emission. Relevant inspected
owners and the work they would need are:

| Current source | Implementation work if the proposal is approved |
| --- | --- |
| [`compiler/src/prelude.rs`](../../../compiler/src/prelude.rs) | Add the bodyless PRE-1 record with the drop bound, effects and postconditions. |
| [`semantic/check/generics/operands.rs`](../../../compiler/src/semantic/check/generics/operands.rs) | Extend the operand-derived row table using the existing window-and-element projections and BackWindow shapes; enforce the ordinary drop bound. |
| [`semantic/check/expressions/calls/user.rs`](../../../compiler/src/semantic/check/expressions/calls/user.rs) and [`semantic/check/references.rs`](../../../compiler/src/semantic/check/references.rs) | Add clear to the compiler-owned boundary-ending domain and apply existing window-reference invalidation. A prelude call cannot rely on the ordinary source-function writes(filled) fallback. Preserve the window and descriptor references. |
| [`semantic/check/cleanup.rs`](../../../compiler/src/semantic/check/cleanup.rs) | Keep release selection based on the existing type-owned release graph and checked cleanup model; do not add user finalizers or a second capability classification. |
| [`lowering.rs`](../../../compiler/src/lowering.rs), [`lowering/builder/prelude.rs`](../../../compiler/src/lowering/builder/prelude.rs), and [`ir.rs`](../../../compiler/src/ir.rs) | Extend the complete compiler-owned row inventory and shared body dispatch. Current take lowers through row_take to RunTaken; clear needs a typed whole-window element-release/reset operation, not a loop of RunTaken operations. Carry the selected release obligation explicitly before backend lowering, as STOR-3 requires. |
| [`backend/emitter/runs.rs`](../../../compiler/src/backend/emitter/runs.rs) | Use the existing run shape and logical element addressing; the current length field is word zero. For empty-release T, emit only bounded descriptor work and the reset store. |
| [`backend/emitter/cleanup.rs`](../../../compiler/src/backend/emitter/cleanup.rs) | Reuse the type-based element cleanup machinery and ascending logical traversal for non-empty release. The Slots/Ring run helper releases elements without reclaiming its backing; the enclosing Box release frees its cell. Full Paged cleanup also frees pages. Reuse element-only traversal, never enclosing Box release or full Paged cleanup, and retain all window allocations. |

For Firn's positive-capacity byte window, the operation body can store zero to
its length word and return, with no load, copy, zeroing or release of elements.
One representation boundary matters: STOR-1 puts a zero-capacity runtime
window in a shared **immutable** empty header. An unconditional store there
would be invalid even if it writes zero. Use a bounded zero-capacity guard for
runtime windows, returning without a store for that shared header, or an
equivalent correctly proved writable-header path. This guard remains O(1) and
is unrelated to an element loop. Inline zero-capacity windows have their own
descriptor, but may use the same harmless early return.

For non-empty release T, capture the old length, release exactly those
elements using the existing logical coordinates and release graph, then
store zero on the writable-header path. Keep Ring head and all capacity
words untouched. For Paged, do not sweep directory entries after element
cleanup. No allocating cleanup worklist is introduced. The current
[cleanup traversal decision](../../../design/compiler/cleanup-traversal.md)
already rejects such an allocation and uses recursive derived release.
Exact IR variant names and all mechanically affected consumers should be
settled during implementation; this document selects the behavior and path,
not an unreviewed internal API.

## Acceptance cases and implementation evidence

These are proposed cases, not tests run or added to conformance in this change.
Each negative must first satisfy unrelated formation, ownership and bounds
requirements so that the refusal tests the intended rule.

1. **Byte reuse:** construct capacity 65536, fill, clear, statically establish
   len 0 and cap 65536, refill within that capacity and read the new bytes.
   Repeat with entry lengths 0, 1 and full capacity and a second clear. Native
   result must match independently specified new contents. Instrument allocation
   calls over clear and refill, excluding initial construction and final owner
   release: the required count is zero. Stable retained-heap bytes alone are
   insufficient, because allocate/free churn could cancel.
2. **Reference invalidation:** bind `old = &bytes.inner[0_u64]` after placing a
   byte, clear, refill, then read `old^`. Refuse that read under REF-2 and cite
   clear as its invalidating event. Include a range, an empty range, and a
   Paged page reference; also positively keep the reference to the window
   itself usable for refill. Refill must not restore a dead reference.
3. **Drop capability:** a window of `Box<u8>` clears successfully and releases
   every initialized element exactly once while retaining its own backing.
   A fieldless `nodrop` element must make clear fail at the call under PROV-6,
   including a zero-length window; consume that test's elements explicitly and
   use free_empty later so its intended error is not scope leakage. Also accept
   an affine scalar-only struct, separating empty release from copy.
4. **Restricted alternative, if chosen instead:** reject a filled window of
   `Box<u8>` at clear for the selected copy/empty-release restriction; retain
   a separate rejection of a fieldless nodrop element. These are not the
   expected verdicts for the recommended general drop form.
5. **Storage shapes and retained measures:** exercise inline Slots, a wrapped
   Ring with head unchanged, boxed Ring, and multi-page Paged. Require len 0,
   cap unchanged, Paged pages.len 0, and refill across the wrap/page boundary
   without a new backing allocation. Check the empty shared-header case for
   each runtime shape and no write to immutable storage.
6. **No-heap and shape boundaries:** inline scalar Slots and Ring clear in a
   no-heap bundle; existing heap-bearing type prohibitions still reject boxed
   storage. Arrays and range references are refused as clear operands under
   OP-10 rather than treated as adjustable windows.
7. **Backend constant work and release:** compiler tests inspect a reachable
   empty-release clear body with a runtime length so whole-program dead-code
   elimination cannot hide the operation. Its pre-optimization emitted body
   and any callees must have no element/page loop, element access, memcpy,
   memset or allocation; only bounded descriptor work and the permitted store.
   For release-bearing elements, check the old-length traversal, recursive
   ownership release, and absence of backing release. Runtime allocation/free
   counts or heap accounting plus an independent cleanup observation must
   distinguish a leak, duplicate release, and accidental backing replacement.

Specification acceptance belongs in `tests/conformance`; emitted-work and
allocation instrumentation belong in compiler/runtime tests, and end-to-end
reuse may also use `tests/programs`. A wrong implementation must fail the
corresponding check: keep len nonzero, change cap, retain an old reference,
omit element cleanup, visit byte elements, allocate during clear, and write
the shared zero header are distinct faults to detect. Use the ordinary CI
paths after implementation; run nothing locally for this investigation.

## Uncertainties and disposition

The gap conclusion is about the currently specified general operations and
their explicit movement, not a proof that every optimizer emits a loop for
every byte drain. No current compiled witness or performance result exists
from this task. The recommendation and the extension from Slots to Ring/Paged
need the owner's language-design decision before implementation. The source
review identifies the lowering owners and immutable-header constraint; it
does not establish that a particular new IR design has already satisfied
every backend consumer. Allocation instrumentation must count calls rather
than infer absence from net bytes. These are implementation/validation work
following selection, not reasons to change existing conformance verdicts or
route around the gap in Firn.
