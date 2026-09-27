# Whitefoot writer patterns

This is non-normative writer guidance. The active
[specification](../spec/kernel-spec.md) defines accepted source, conformance
evidence states what the compiler implements, and the
[constitution](constitution.md) defines the project objectives.

The examples below use the current source language. They are fragments unless
they name a maintained program. A pattern explains how to express an admitted
design; it grants no extra acceptance rule and makes no implementation-status
claim.

## P1. Put mutation in the reference parameter's effect row

A reference is a local name for a path. It has no shared or exclusive marker.
The callee's exact effect row says what it reads and writes, and the call site
proves that overlapping actual paths are safe [REF-1, EFF-1, EFF-5].

```whitefoot
struct Counter {
  value: u64;
}

fn store(counter: &Counter, next: u64) -> result: unit writes(counter.value) {
  set deref(counter).value = next;
  return unit;
}
```

An effect path is rooted at the bare parameter: write `writes(counter.value)`,
never `writes(deref(counter).value)`. Use the narrowest truthful path. A body
that reads a whole parameter and writes one field of it declares both,
`reads(stats), writes(stats.count)`, so a call kills only the caller facts
whose support overlaps that field. An entry another entry already states is
never listed: a write states every access at or below its path and a read
every read below it, so `reads(stats.count)` beside `reads(stats)` or
`writes(stats)` is refused [EFF-1]. Two reads may overlap; a read/write or
write/write pair must be proved disjoint when two arguments supply it, or when
one argument supplies it at positions such as `values[i]` and `values[j]`
[EFF-5].
For long call chains, compute owned commands in `pure` or read-only helpers and
apply them in one shallow writer. This keeps the mutation boundary visible in
signatures without an interior-mutability mechanism.

## P2. Choose the storage shape from its occupancy rule

The three storage shapes have different invariants [TYPE-9, WIN-1]:

- `Array<T, n>` is a full fixed run. Every element exists.
- `Slots<T, n>` is an inline prefix window with `len` and `cap`.
- `Ring<T, n>` is an inline wrapped window with `len`, `cap`, and `head`.
- Omitting `n` makes a runtime-capacity form. It may exist only as the
  `inner` content of a `Box`.

Use the construction and window operations instead of manufacturing a layout:

```whitefoot
let fixed = slots_new::<u8, 16>();
place_back(window: &fixed, value: 7_u8);

let growing = box_slots_new::<u8>(capacity: 64_u64);
place_back(window: &growing.inner, value: 9_u8);
```

Read measures as readonly fields: `fixed.len`, `fixed.cap`, and, for a ring,
`ring.head` [MSR-1, OP-15]. There is no `room` measure; write the needed
relation over `len` and `cap`. `Array` has only `len`.

A readonly integer field reached through subscripts is a term just as a
measure is [ENT-2]. In an index-based tree, declare per-node structure such as
`readonly count: u64;` and use `deref(nodes)[i].count` directly as a counted
endpoint, in a `requires`, or as a `let` source equal to its copy. Mark a
field readonly when only whole-element replacement should change it; an
ordinary field below a subscript is no term, so copy it with `let` before
relying on it. Replacing or exchanging the element, a window operation that
moves elements, a call whose row writes the storage, and a write to the index
binding each end the facts about it; bind a computed or field-valued index
with `let` so the place's offset is a plain binding.

Use `take_back`, `remove_at`, `insert_at`, `append`, `split_off`, `grow`,
`place_front`, and `take_front` for their declared transformations [OP-10]. A
source subscript always owes `index < run.len` [OP-4].

Growth policy can be ordinary source. The maintained
[grow-vector library](../lib/std/collections/vector/grow-vector.wf) wraps
`Box<Slots<T>>` in `GrowVector<T, const ceiling: u64>`. The selected ceiling
supplies each concrete growth call's OP-9 bound; the policy doubles capacity
while it fits and otherwise saturates at that ceiling. A zero ceiling admits
an empty vector but no append. Reference-parameter contracts publish each
operation's length and capacity relationships. FN-9 does not publish the
constructor's measure through its aggregate result field. Its caller first
establishes that nested measure through ordinary control flow, as the
[program](../tests/programs/containers/grow-vector-program.wf) shows.

`grow_vector_remove` preserves the remaining order. Use
`grow_vector_swap_remove` when filling the selected position with the last
element is acceptable; it transfers a constant number of elements.
`grow_vector_truncate` preserves a chosen prefix, while `grow_vector_drain`
consumes the complete window. Both invoke the supplied `VectorDrain` member
in the removed elements' original order and preserve capacity for reuse.
For the first half of the removed suffix, they take the rear element into a
local, exchange it with the next suffix element and consume that local. The
remaining suffix can then be consumed from the back. Work is proportional
to the number removed, with constant auxiliary element storage; rear-element
relocation remains extra movement compared with a direct native consumer.
The callback's environment must be effect-disjoint from the backing [EFF-5].

These operations also accept `nodrop` elements: the callback explicitly
consumes each one, then `grow_vector_free_empty` consumes the empty owner.
Its `len <= 0_u64` precondition means empty because length is unsigned. This
ordering spelling also lets FN-8 use ENT-6's affine Signed Goal route when
emptiness comes from a proved invariant; that route does not introduce an
equality goal. An ordinary-loop header hypothesis itself expires at loop
exit. Publish the required outer conclusion as a local `invariant` before
`break` when the continuation needs it [ENT-5, INV-1].

The [deque library](../lib/std/collections/deque/deque.wf) uses `Box<Ring<T>>` directly.
Endpoint helpers take a reference and require the caller to prove room or
nonemptiness. `deque_rebase` consumes the old owner and returns a genuinely new
backing, with the same logical length and head zero; it can grow or shrink to
any sufficient capacity within the written ceiling. `DequeVisit` borrows each
element in logical order, while `DequeDrain` consumes them in that order and
leaves the allocation reusable. Per-element visitation does not provide two
contiguous ranges: REF-4 refuses all Ring range references, even after a
non-wrap test. The [caller](../tests/programs/containers/deque-program.wf)
also shows an existing filled-slot reference surviving a back append whose
row writes only the next slot and length.

The [slab library](../lib/std/collections/slab/slab.wf) reserves one bounded backing and
materializes cells lazily. Each cell has an inline `Slots<T, 1>` for its
zero-or-one occupant, a generation and a free-list link. An exhausted insert
returns the offered owner; removal returns its occupant and retires the slot
at the generation limit instead of wrapping. `SlabVisit` supplies borrowed
access with an owned result, and `SlabConsume` handles every live element at
teardown. Handles are ordinary index/generation data relative to a slab. The
[membership example](../tests/programs/containers/slab-membership-program.wf)
distinguishes an index that may expire from a composite protocol that refuses
deletion while another index retains the object; ordinary public bookkeeping
does not prove that arbitrary client functions preserve that protocol.

The [owning hash map](../lib/std/collections/hash_map/hash-map.wf) stores keys and values
inline in ordinary enum buckets, including `nodrop` values. Supply hashing
and equality through `HashMapKey`. `hash_map_try_put` uses existing capacity;
`hash_map_put` may grow up to the written ceiling. Replacement installs the
complete offered pair and returns the complete old pair. A full table returns
the offered pair unchanged, distinguished by the reason in
`HashMapReturned`; consume that pair explicitly when either member is
`nodrop`. Allocation itself remains total under STOR-8.

Use `hash_map_lookup` for borrowed observation and `hash_map_edit` to update
the stored value through a callback, without removing and reinserting it.
Both callbacks can return owned results. `hash_map_each` visits live pairs,
and `hash_map_free` supplies every remaining pair to `HashMapConsume` before
freeing the backing. Read the current count and capacity through
`hash_map_len` and `hash_map_capacity`; reserve and rehash may relocate all
payloads. Consistent key laws determine ordinary map behavior, but they do
not grant ownership or bounds authority. The
[caller](../tests/programs/containers/hash-map-program.wf) also exercises
non-reflexive equality, zero-sized pairs and owned callback results.

## P3. Reach heap content through `Box.inner`

`Box<T>` owns one heap cell. Its content is the ordinary field `inner`;
`deref` is only for a reference [TYPE-7, TYPE-9].

```whitefoot
nocopy struct Record {
  id: u64;
}

let record = Record(id: 7_u64);
let cell = box_new::<Record>(value: move record);
let id = cell.inner.id;
```

Moving an affine field consumes the whole owner, and the remaining owned parts
take their compiler-derived releases [WIN-3, STOR-3]. A direct
`let value = move cell.inner;` consumes the cell and frees it. A
runtime-capacity `Array<T>`, `Slots<T>`, or `Ring<T>` cannot be moved out of
its cell; leave it there, or empty a boxed window and consume it with
`free_empty` [TYPE-9, OP-14].

Use one `Box` for independently owned heap storage. Do not add a second
descriptor object around a runtime-capacity shape; the cell already owns that
shape.

## P4. Form a reference where its source path is visible

References may be local variables and call arguments. They are never stored in
aggregates and never returned [REF-3, TYPE-8]. Return an owned decision or
index, then let the caller form the reference from its own place.

```whitefoot
let index = choose_index(table: &table);
if index < table.len {
  let selected = &table[index];
  inspect(value: selected);
}
```

A write, move, or release of a proper prefix invalidates a reference. Moving
its owner also invalidates a reference to that owner itself. Form it again
after that event [REF-2]. A loop-carried reference can descend through owned
links [REF-1]. The checker summarizes unknown depth as a containing subtree;
the runtime reference remains an ordinary pointer. A selected payload
reference survives the selecting match's exit, but replacing its enum still
invalidates it. A single link-slot cursor can edit a list through an atomic
owned update. Independent cursors in the same subtree may invalidate each
other on structural writes; reform them after such an edit. The executable
[owned-link examples](../tests/programs/owned_link_cursors.wf) exercise list
walks, removal, tree descent and cursor resets.

Use `return musttail f(...);` for a direct self call whose stack must not grow
with the number of transfers [FN-10]. The call must be the return's only
expression. Reference arguments must come from reference parameters, possibly
through a selected field or subrange; a reference into an owned parameter or
local cannot survive replacement of that activation. Move owned arguments as
usual. An unreferenced affine local is released before the transfer; a live
valid reference to a local with nonempty release prevents that transfer.
The marker preserves every ordinary call proof and does not prove termination.
The compiler also optimizes unmarked direct self calls that meet the same
conditions. If a condition is unavailable, an unmarked call stays ordinary;
use `musttail` when failure to make the transfer must be a compile-time error.
The [consuming linked sequence](../tests/programs/tail_list.wf) demonstrates
moving the next heap cell into a self transfer while releasing the old one.

Use a range reference for one contiguous run [REF-4]:

```whitefoot
let part = &bytes[first..end];
let count = deref(part).len;
let byte = deref(part)[offset];
```

Formation proves `first <= end <= bytes.len`. The range's own `len` is
`end - first`, and every subscript is relative to that range. A range reference
over a `Ring` is refused because a wrapped window need not be one extent.

## P5. Use indices as durable pool handles

A pool is a `Slots` window plus owned indices used as handles [OP-13]. Keep the
payloads in one storage object, return indices from searches, and form short
references only at the access site. This avoids stored references and
per-element heap cells.

An index is not a generation-safe handle by itself. If a slot can be removed
and reused, store a generation in the program's data and compare it. A stale
index that remains in bounds names the new occupant; bounds safety does not
make it the old object.

For sparse ownership, make vacancy a value such as `Option<T>` and use `swap`
to exchange a complete element without creating a hole [OP-11, WIN-3]. The
maintained examples in [option_slots.wf](../tests/programs/option_slots.wf) and
[block_pool.wf](../tests/programs/block_pool.wf) show those two shapes.

## P6. Ask for the capabilities a generic body uses

Copy and drop are structural capabilities [OWN-1, PROV-6]. Choose a bound from
the body:

- `<T>` requires no capability. The body must consume each `T` exactly once.
- `<T: drop>` accepts droppable values. The body may leave one for its derived
  release.
- `<T: copy>` accepts copyable values. The body uses them bare and may use one
  more than once.

```whitefoot
fn forward<T>(value: T) -> result: T pure {
  return move value;
}

fn discard<T: drop>(value: T) -> result: unit pure {
  return unit;
}

fn duplicate<T: copy>(value: T) -> (left: T, right: T) pure {
  return value, value;
}
```

`move` on a copy value is an error. A generic `move` written under `<T>` or
`<T: drop>` remains that generic body's consuming spelling even when a concrete
argument happens to be copy.

A declaration may remove a capability. `nocopy` removes copy; `nodrop` removes
drop and copy. `nodrop` is valid on a struct or enum, including a tag-only enum:

```whitefoot
nodrop enum Ticket {
  Open();
  Closed();
}

fn spend(ticket: Ticket) -> result: unit pure {
  match move ticket {
    Open() => {
      return unit;
    }
    Closed() => {
      return unit;
    }
  }
}
```

A `nodrop` value must be consumed explicitly on every exit. Its modifier states
an ownership obligation; it does not attach a finalizer.

## P7. Package compile-time behavior with `interface` and `binding`

An `interface` is an ordered group of function signatures. A `binding` supplies
one concrete function for every member [FN-3, FN-4]. Both are compile-time
declarations; no dictionary, closure, or dynamic dispatch value is formed.

```whitefoot
interface Identity<T> {
  fn same(value: T) -> result: T pure;
}

fn same_u8(value: u8) -> result: u8 pure {
  return value;
}

binding ByteIdentity : Identity<u8> {
  same = same_u8;
}

fn apply<interface Identity<T>>(value: T) -> result: T pure {
  return Identity<T>::same(value: move value);
}
```

Every group introduction writes the marker, including a zero-argument group:
`fn read<interface Source>()`. Forwarding, a concrete generic argument, and a
qualified member call do not repeat it:

```whitefoot
fn forward<interface Identity<T>>(value: T) -> result: T pure {
  return apply::<Identity<T>>(value: move value);
}

let result = forward::<ByteIdentity>(value: 9_u8);
```

The member's parameter kinds, result types, effects, requirements, and postconditions are
the generic caller's boundary. A binding may refine that boundary only as
[FN-4] permits. Calls retain their ordinary syntax; `interface` and `binding`
replace the retired group-declaration keywords, not the call form. See
[grow-vector.wf](../lib/std/collections/vector/grow-vector.wf) and
[grow-vector-program.wf](../tests/programs/containers/grow-vector-program.wf) for a behavior
that consumes owned elements while updating an environment.

## P8. State maintained arithmetic at its boundary

Every partial operation must have its domain proved before lowering. Put a
loop relation in the loop header so the checker proves its base and every
reachable backedge [INV-1]:

```whitefoot
invariant capacity: output.cap >= limit;

for (
  at in 0_u64..limit,
  invariant filled: output.len == at
) {
  place_back(window: &output, value: 0_u8);
}
```

The counted loop supplies `at < limit` in the body. Derived expressions still
owe their own exact integer and subscript obligations. Header invariants have
no `use` block.

Use a local invariant for a relation proved at one program point. When the
fixed automatic families cannot combine the needed premises, direct the finite
proof with explicit `use` steps [PRF-1]:

```whitefoot
invariant total_limit: first + second + third <= first_limit + second_limit + third_limit {
  use first_bound;
  use second_bound;
  use third_bound;
}
```

The target is published only after every use and the final combination have
been checked. Proofs are erased and add no runtime branch.

## P9. Put a contract on a true API requirement

Use `requires` when every valid caller must establish the condition, and
`ensures` when a callee can prove a relation every normal caller may use
[FN-8, FN-9]. State window transitions with entry and exit measures:

```whitefoot
fn pop<T, const n: u64>(window: &Slots<T, n>) -> value: T writes(window.last), writes(window.len) contract {
  requires deref(window).len > 0_u64;
  ensures deref(window).len + 1_u64 == deref(entry(window)).len;
} {
  let value = take_back(window: window);
  return move value;
}
```

Contracts are proof-only. They do not insert a check or an alternate result.
If insufficient capacity, malformed input, or another false condition is an
expected outcome, branch before the partial operation and return an ordinary
`Result`, `Option`, or domain enum. The true edge supplies the fact:

```whitefoot
if index < input.len {
  return Some<u8>(value: input[index]);
}
return None<u8>();
```

Do not turn recoverable input failure into an invariant or a requirement merely
to make a later operation compile.

## P10. Transform owned state without a hole

Use the operation that owns the complete transition:

- `swap(first: &a, second: &b)` exchanges two places and permits the same place
  twice [OP-11].
- Window operations move their declared slots and measures [OP-10].
- `set place = value;` releases the displaced affine value. It refuses an
  overwrite of a linear value [SET-1, WIN-3].
- `set place = transform(value: move place);` is an atomic in-place update only
  under [OP-12]'s result and row conditions.

```whitefoot
set state = advance(state: move state);
```

No program point contains a hole during an admitted atomic update. A complete
binding whose value was consumed may be reinitialized by assigning that whole
binding; a projected, dereferenced, or subscripted place below a dead root may
not [SET-1, LIV-1].

At a branch or loop join, every outer binding must have the same live/dead
status on every incoming edge [LIV-1]. Move a value on every arm and publish one
replacement, or keep the move inside the arm that exits.

## P11. Consume must-use resources explicitly

A `nodrop` owner must be consumed on every exit [PROV-6]. Destructure a
non-opaque aggregate whole when its parts need different consumers:

```whitefoot
let std::process::Inputs(args: args, cwd: cwd, stdout: out, stderr: err, handles: factory, stdin: input) = move inputs;
std::fs::close_directory(factory: &factory, directory: move cwd);
```

The host declarations belong to the standard library modules `std::io`,
`std::text`, `std::fs`, `std::net` and `std::process` [PRE-2]. Write their
`std` paths, or give a record an alias header for the names it uses often
[MOD-4, MOD-10].

Host failures are ordinary `Result` values. Match them or use `propagate` in a
function returning the same error type [ERR-1, ERR-3]. A helper that acquires a
linear handle closes it or returns it on every path. Passing a handle by
reference does not consume it; passing it to a value parameter does.

The maintained [stdin_echo.wf](../tests/programs/stdin_echo.wf) shows an inline
window passed to `read_next` and `write_once`, with the invocation's owners
consumed explicitly.

## P12. Use fixed arrays for immutable tables

A named const may contain primitives, const-eligible structs, and
constant-capacity `Array` values [CONST-2]. A full array literal writes every
element:

```whitefoot
const digits: Array<u8, 4> =[48_u8, 49_u8, 50_u8, 51_u8];
```

Borrow and subscript it under the ordinary rules. Const storage is immutable;
`Slots`, `Ring`, `Box`, and runtime-capacity arrays are not const-eligible.

## P13. Keep parallelism a proved implementation permission

Write the correct source-order program first. [PAR-1] may overlap independent
adjacent statements, and [PAR-2] may map or reduce a counted loop only after
the checker has retained every required disjointness, bounds, and arithmetic
proof. Failure to derive permission keeps sequential lowering and never changes
source acceptance.

For a map, partition one origin into proved-disjoint range references and make
each helper's declared row stay within its actual range. For a reduction, keep
one associative and commutative accumulator update in the admitted operation
family. Do not add locks, scheduling calls, or runtime alias tests to seek
permission.

Maintained examples live under
[tests/programs/compute](../tests/programs/compute) and
[tests/programs/parallel](../tests/programs/parallel); their source contracts
and ordinary sequential behavior remain the authority.

## P14. Keep branchless classifier state in `Bool`

For byte classification and scanner state, keep predicates in `Bool`, combine
them with `band`, `bor`, `bxor`, and `bnot`, and update the state directly.
When a numeric contribution is needed, select it with a value-producing
`match`:

```whitefoot
let increment = match starts_word {
  True() => {
    give 1_u64;
  }
  False() => {
    give 0_u64;
  }
}
```

This states Boolean dataflow directly and leaves control flow for genuine
program alternatives. Use an exact integer operation when overflow is excluded
by proof, and a `.wrap` operation only when modular arithmetic is the intended
result [OP-2].

## Known gaps

Current unresolved language and compiler questions are recorded in
[todo.md](todo.md) and the relevant investigation directories. A missing
pattern does not authorize retired syntax or a new mechanism. Reduce the need
to a small source case, identify the specification rule that admits or refuses
it, and record measured cost only when performance selects between alternatives.

## P15. Keep a Result's evidence with its value

A local `Result` with an integer success payload retains its verified success
relations when named, copied, moved, assigned or delivered by `give`. A match's
own `Ok` binder and a successful `propagate` make those relations available.
The error edge keeps its ordinary return and cleanup behavior [FN-9, ENT-5].

```whitefoot
let outcome = bounded(count: limit);
let saved = outcome;
let index = propagate saved;
```

If `bounded` declares its Ok payload less than `count`, `index < limit` is
available after propagation while that relation remains valid. The fragment
assumes that contract and a compatible enclosing Result return. A wrapper may
return the named outcome or the call directly; its own routed `ensures` must
still be proved. The
[complete transport case](../tests/conformance/cases/fn9-pos-result-value-transport.wf)
shows both forms and executes success and error paths.

Evidence describes the value that was evaluated. Replacing the original
binding does not change an earlier copy. Changing supporting storage does not
retarget an old relation to the new contents, and merely holding an outcome
does not assert that it is Ok. A branch join keeps only common consequences:
`payload < 8` on one path and `payload < 10` on another retain `payload < 10`.
An unchanged outcome can cross a loop head; one changed by a continuing
backedge cannot reuse the initial payload's evidence there.

`cvt.checked` between integer types supplies the same kind of conditional
evidence: its success payload equals the input value evaluated by that call.
Saving the outcome before changing the input preserves the old payload's
bounds. A conversion involving a float supplies no such numeric relation or
domain fact; use its payload directly or test `cvt.defined` on the input.

## P16. Choose a conversion interface from the intended behavior

Use bare `cvt` when the surrounding invariant proves exact representability.
Its result is always the destination type, and its proof adds no runtime
validity branch [OP-6, ENT-6].

```whitefoot
fn byte(value: u32) -> result: u8 pure contract {
  requires value <= 255_u32;
} {
  return cvt::<u32, u8>(value);
}
```

Use `cvt.checked::<Src, Dst>(value)` when out-of-domain input is an intended
failure; it always returns `Result<Dst, NarrowError>`, including widening and
identity pairs. Use `cvt.defined::<Src, Dst>(value)` when the program needs a
Boolean domain answer. Its true branch proves a bare conversion of that same
value and type pair; calculating and ignoring the Bool proves nothing.

Integer bounds can prove narrowing or signedness changes. An integer operation
bound by a `let` already carries the interval its operation row gives from its
operands' bounds, so a shift, mask, minimum or remainder whose result fits the
destination converts with bare `cvt` and needs no mask or `cvt.checked` added
only for the proof [ENT-3]:

```whitefoot
fn high_half(word: u64) -> result: u32 pure {
  let high = ishr(word, 32_u32);
  return cvt::<u64, u32>(high);
}
```

For conversion to f32, the interval from -2^24 through 2^24 is a sufficient
automatic proof; larger exactly representable constants also work. A float's
integer range alone does not prove integrality: branch on the exact domain
query or declare that query as a requirement. Generic helpers can use `Int` or
`Float` endpoint bounds and a `cvt.defined` requirement without changing their
return type when the selected pair changes. Same-type conversion copies bits,
while conversion between float formats uses the destination's canonical quiet
NaN [OP-6].
