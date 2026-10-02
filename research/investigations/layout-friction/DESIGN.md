# Writer friction in the Snowghost layout stage

## Question

The agents that wrote the layout stage of Snowghost
(mbbill/Snowghost#27, branch `research/layout-stage`, directory
`renderer/layout/`) reported four frictions while writing about 15,000 lines
of Whitefoot:

1. After a write through a reference the checker no longer knows a
   structure's length, so later subscripts and `take_back` calls are guarded
   again with `if i < len`; the layout code holds a hundred or more such
   guards.
2. A module-level function name collides with a local binding in another file
   of the same module [TYPE-6]: table layout's function `narrow` against grid
   layout's local `narrow`. Parts written by separate agents merged only after
   renaming.
3. The count given to `box_array_filled` or `box_slots_new` is wrapped in
   `imin(n, ceiling)` before the allocation-fit check [OP-9] accepts it.
4. A function receives every value its requirements name, so data used only
   in a proof is passed at run time, as mbbill/Snowghost#26 recorded.

For each this record gives the smallest program that shows it, the rule of the
active specification (v0.84) that produces it, how often it occurs in the
layout code, and directions with their costs. It changes no rule, compiler or
test; the directions are candidates for the owner, not selections.

## Method

The probes are in [`probes/`](probes/). Each was checked with
`whitefootc --check <probe>.wf`, and a module probe with
`whitefootc --check --graph modules.wfg --entry main` in its directory, using
the gate-profile compiler built from `main` at `3629be15` (specification
v0.84). Probes the checker accepts were built with `whitefootc <probe>.wf -o
<out>` and run. Diagnostic lines are quoted exactly; lines repeating the source
are omitted.

The census reads Snowghost `research/layout-stage` at `d046160`: 18 records
and 16,725 lines under `renderer/layout/` in two modules, `pkg::layout`
(`module.wfm` and 12 implementation records) and `pkg::layout::text`, with 487
functions in their implementation records. That branch pins Whitefoot
`290b575b` (specification v0.81), and its layout code does not check under
`main`: v0.84 made `forall` and `apart` fixed grammar atoms [GRAM-5], so
`let apart = bor(below, above);` at `flow.wf:412` is refused (FORM-3), and
`main` also refuses three bodies of `grid.wf` under FORM-1. Experiments that
recheck the layout code therefore use a compiler built from `290b575b`, with
`whitefootc --graph modules.wfg --check-module pkg::layout` in `renderer/`;
the unmodified module is accepted in about 20 s. Between v0.81 and v0.84 the
rules cited below changed only by additions for shared objects, type
invariants, atomic blocks and propagated error exits, which none of these
programs uses.

## 1. Length facts after a call through a reference

### Witness

[`len-whole-row.wf`](probes/len-whole-row.wf):

```wf
fn bump(context: &Ctx) -> result: unit writes(context) {
  set context^.width = context^.width +sat 1_i32;
  return unit;
}

fn after_a_whole_row(context: &Ctx, at: u64) -> result: i32 writes(context) {
  if at < context^.blocks.inner.len {
    bump(context: context);
    let y = context^.blocks.inner[at].y;
    return y;
  }
  return 0_i32;
}
```

```text
len-whole-row.wf:21:34: error[OP-4]: UndischargedBoundsObligation
  residual: at < context^.blocks.inner.len
  disposition: Unproved
  mechanical_fix: `at < context^.blocks.inner.len` is not proved here: when facts that reach the access imply it, prove it with an `invariant` whose `use` steps name them (a loop's header `invariant` for a value the loop computes); or guard the access with `if at < context^.blocks.inner.len` where skipping it is the intended behavior, adding to the effect row any read that condition makes which the row does not yet declare
```

`bump` writes only `width`, yet the fact about `blocks` is gone after the
call. A write through a reference in the function's own body does not lose it:
in [`len-kept.wf`](probes/len-kept.wf), accepted and exiting 0, the fact
survives a write to a field of the indexed element and a `take_back` on
another window. It also survives a call whose row is narrower:

| Probe | The call between the guard and the access | Verdict |
|---|---|---|
| `len-whole-row` | `bump`, row `writes(context)`, writes `context.width` | OP-4 |
| `len-kept` | `bump_width`, row `writes(context.width)` | accepted |
| `len-kept` | `lift`, row `writes(context.blocks.inner[at])` | accepted |
| `len-kept` | `lift_run(run: &context^.blocks.inner[0..count])`, row `writes(run)` | accepted |
| `len-kept` | `bump_keeping`, row `writes(context)` and `ensures context^.blocks.inner.len == entry(context)^.blocks.inner.len` | accepted |
| `len-window-row` | `lift_all`, row `writes(context.blocks.inner)`, writes every element at a loop index | OP-4 |

The loss also reaches a loop that calls such a function.
[`len-loop-header.wf`](probes/len-loop-header.wf) reads `splits[k]` for `k`
below a count taken from the splits' length while its body calls
`add_fragment`, row `writes(context)`; the first statement of the body fails
OP-4 (`residual: k < context^.splits.inner.len`). With the helper's row
narrowed to `writes(context.fragments)`,
[`len-loop-field-row.wf`](probes/len-loop-field-row.wf) is accepted and exits
0.

### Cause

- A call kills every caller fact whose support overlaps a path the callee's row
  writes, after substituting the actual arguments [EFF-2, ENT-5]. A length
  fact's support is the window's descriptor word, and an entry naming the
  whole window, or any path above it, overlaps all of its descriptor words
  [MSR-2]. `writes(context)` therefore kills `at < context^.blocks.inner.len`
  whatever the callee's body does: the row, not the body, is the boundary
  ([design: effects](../../../design/language/effects.md)).
- A row may be coarser than the body. An entry is exhibited when the body
  accesses storage at or below its path [EFF-2], so `writes(context)` is a
  valid row for a body that writes only `context.width`. EFF-2's two-way
  exactness forbids an entry nothing exhibits; it does not ask for the
  narrowest entry.
- A callee that writes elements at a computed index has no row narrower than
  the whole window. Its body's write `context^.blocks.inner[k]`, with `k` a
  loop variable, is attributed to the nearest statically nameable path that
  encloses it, the window `context.blocks.inner` [EFF-2], and a write is
  admitted only below a reference parameter whose row writes that path
  [SET-1]. [`len-filled-row.wf`](probes/len-filled-row.wf) declares
  `writes(context.blocks.inner.filled)`, the slots below the length [WIN-2],
  and [`len-range-row.wf`](probes/len-range-row.wf) declares
  `writes(context.blocks.inner[lo..hi])` with `hi` at most the length; both
  are refused at the element write:

  ```text
  len-filled-row.wf:15:9: error[SET-1]: InvalidSetTarget
    root_class: a reference whose declared row does not write this path
    required_classes: a live own-mode value binding, or a path below `^` of a reference whose row declares that write
  ```

  The window entry the body must declare covers the length, so a callee that
  changes no length still kills the caller's length facts
  ([`len-window-row.wf`](probes/len-window-row.wf)). A window part names
  slots that a built-in operation such as `remove_at` writes, and a user
  function may declare the same rows [WIN-2], but a user body's own element
  write never lands under one.
- A loop header keeps only the facts that hold on entry and on every backedge
  [ENT-3, ENT-5]. A call in the body kills the fact before the backedge, so the
  header loses it and the next iteration's subscript needs a guard even when
  the loop bound was the length itself.

Four ways keep the fact today, each shown in `len-kept`: a row naming only the
fields written; an element entry `writes(w[i])` with the index a value
parameter [EFF-5]; a range reference argument, whose writes kill no measure of
the origin [CALL-3]; and a postcondition stating the length unchanged, which
publishes the relation again after the kill [CALL-6].

### Frequency

The layout code declares 248 `writes` entries, every one naming a whole
parameter and none a path below one, in 188 of its 487 functions, and calls
those functions at 470 sites; no postcondition uses `entry(`. Its `if`
conditions compare an index with a length 254 times, 219 with `<` and 35
with `>=`.

Two deletion experiments separate the guards that the mechanism above makes
necessary from the others. Each replaces one guard's condition with a
constant, `0_u64 < 1_u64` for a guard around an access and `0_u64 > 1_u64`
for a loop exit, so that the block and its scopes stay as written and only the
fact the guard supplied is lost, and rechecks the guard's module. Accepted
means the guard is not needed today; each refusal was checked to be OP-4 at an
access the guard protected.

The 194 guards written `if X < P.len {` with `P` free of subscripts:

| Outcome | Guards |
|---|---|
| accepted: not needed | 44 |
| needed: the index is a position read from storage: an enum payload (35), a field of a struct element (24) or an integer element (11) | 70 |
| needed: the index is a parameter, and no requirement bounds it | 41 |
| needed: a loop runs over a range whose ends are read from storage | 20 |
| needed: the index is computed, or carried around a loop with no header invariant | 16 |
| needed: the window may be empty (`0_u64 < P.len`) | 2 |
| needed: a loop over the indexed window's own length | 1 |

Every needed guard here but the last protects an index that no fact bounded
before the guard: the guard establishes the relation for the first time, and
the calls between do not matter. The last, `table.wf:719`, is this item's
mechanism: its loop's count is `table.cols.inner.len`, read before
`distribute_fixed` or `distribute_auto`, whose row `writes(cols)` covers that
length. Of the 44 that are not needed, 19 repeat an
identical guard or a loop over the same index and window earlier in the same
function; the fact had survived, and the writer guarded it again.

The 34 loop exits written `if k >= P.len { break; }` (one `return`), at the
top of a loop over a count read from `P`'s length:

| Outcome | Exits |
|---|---|
| accepted: not needed | 24 |
| needed: a call whose whole-parameter row covers `P` sits in the loop body or between the count and the loop | 8 |
| needed: the index is carried around a loop with no header invariant (`flex.wf:667`, `grid.wf:923`) | 2 |

The eight are this item's mechanism too. `copy_fragments` (`flow.wf:1263`)
holds four of them: it walks `context^.blocks` and then each paragraph's
fragments, and calls `add_fragment`, whose row is `writes(context)` while its
body appends to `context^.fragments` only. A third experiment narrows three
helpers' rows to the fields their bodies write (`add_fragment` to
`writes(context.fragments)`, `add_own_fragment` to `reads(context),
writes(context.fragments)`, `place_splits` to `reads(context),
writes(context.splits)`) and removes five of the eight exits (`flow.wf:180`,
`1267`, `1278`, `1285`, `1288`) together; the module is accepted.

So the layout code's guards come mostly from indices nothing bounds, which no
rule about kills would remove. Kills by coarse rows account for 9 of the 228
guards examined, and 68 guards are not needed at all.

### A second cause: positions stored in another window

The largest group, 70 guards plus the 20 loops over stored ranges, reads a
position from one window and subscripts another with it, as `entry_extent`
(`flow.wf:505`) reads `Open(block: b)` from `context^.flow` and subscripts
`context^.blocks` with `b`. The writer knows each stored position is in
range, since the stage that built both windows put it there, but no rule lets
that knowledge reach a subscript:

- An ordinary fact is a relation between terms at one point [ENT-2]; there is
  no fact about every element of a window, so a requirement or postcondition
  cannot bound all stored positions at once.
- A range fact can state one [RANGE-1], but no ordinary obligation consumes
  one [RANGE-2]. In [`stored-position.wf`](probes/stored-position.wf) a requirement
  `forall bounded(j in 0_u64..slots^.len): slots^[j] < blocks^.len` holds,
  and `blocks^[slots^[k]]` still fails OP-4 with residual
  `at < blocks^.len`. Citing the instance in a local invariant,
  `invariant within: at < blocks^.len { use bounded(k); }`, is refused
  ([`stored-position-use.wf`](probes/stored-position-use.wf), RANGE-4: "a
  local invariant's proof instantiates a range fact"), since a written
  instance is admitted only in a certificate.
- The layout code stores positions in fields of struct elements and in enum
  payloads, and a range term reads only an integer element, never a field
  below it ([`stored-field.wf`](probes/stored-field.wf), RANGE-1: "a range
  term selects below an element").

This is the unique-keys question of where a stored index's facts live
([unique-keys](../unique-keys/DESIGN.md)), met here for bounds rather than
distinctness.

### Directions

- **1A. Narrow rows, written by the writer.** No language change. A callee
  declares the fields it writes, and a helper that rewrites elements of a run
  takes a range reference. Cost: rows name every written field (the layout
  code's `Context` has 12 `Box<Slots<...>>` fields); a helper that writes
  elements of a window that its caller also indexes still declares the whole
  window; the writer has no signal that a guard is caused by a row rather
  than by a missing fact.
- **1B. The OP-4 repair names the kill.** A compiler change. When the
  residual's relation held at an earlier point on every path and a call's
  projected `writes` killed it [ENT-5], the diagnostic names that call and
  the entry, and offers narrowing that entry or stating the measure in the
  callee's `ensures` beside the guard. Today the repair offers an invariant,
  a callee `ensures` or a guard; at the nine killed facts the writers took the
  guard, and 19 unneeded guards repeat a fact that had survived, which a
  writer shown the kills would have no reason to write.
  Cost: the checker keeps the kill event for the residual's relation; a repair
  alternative must succeed when carried out [DIAG-1], so "narrow the row" is
  offered only where the callee's exhibited writes allow it.
- **1C. A dynamic element write lands in `w.filled` when the row writes no
  measure of `w`.** A language change to EFF-2's attribution and SET-1's
  writability. In a body whose row writes no measure of a window `w`, `w.len`
  is its entry value throughout, so every element access, whose subscript
  OP-4 has proved below the current length, lies in `w.filled` as WIN-2
  interprets it at call entry; likewise a proved index inside a declared range
  lies in that range. The helper then declares
  `writes(ctx.blocks.inner.filled)`, and `r.len` overlaps no slot [WIN-2], so
  callers keep their length facts. Cost: attribution depends on the row's
  measure entries and on discharged subscripts; a body that appends and then
  writes the appended element still declares the window.
- **1D. The narrowest row is the only row.** A language change: a `writes`
  entry must be the most precise path EFF-1 admits for some exhibited access,
  as FORM-1 asks one spelling of other quantities, and the compiler prints the
  required row. It removes the writer's choice that causes most kills, but not
  the window entry of 1C's case. Cost: rows grow with bodies, and a body edit
  changes the row more often; public operations keep the decision that they
  name the nearest accessible path covering private state.
- **1E. Rows of module-private functions are inferred.** A language change:
  a private function's row is its exhibited set, and callers in the module use
  it. Cost: it reverses, for private functions, the decision that a row is a
  boundary a reader can rely on without reading the body; module verdicts
  already cover all of a module's records [MOD-8], so no other module is
  affected.
- **1F. A range fact bounds a subscript.** A language change in two parts:
  RANGE-4 admits a written instance in a local invariant, so
  `use bounded(k)` proves `at < blocks^.len` and OP-4 consumes it as any
  invariant target [INV-1]; and RANGE-1 admits a range term that reads an
  integer field of an element. It reaches the guards over positions in
  fields and integer elements, 35 of the 70, and the 20 loops over stored
  ranges, provided the stages that build the windows state and keep the
  facts; the 35 positions in enum payloads need range terms over payloads as
  well. Cost: the range judgment runs after ordinary entailment [RANGE-2],
  so an invariant proved by an instance is ordinary authority that only the
  later judgment establishes, which orders the two judgments differently;
  every write to the windows owes the fact again, which is proof work the
  writers do not do today; and enum payloads still have no range term.

1B is independent of the others and directs writers to 1A today. The census
splits the nine killed facts between them: five follow calls that write other
fields only, which 1A, 1D or 1E remove (the narrowing experiment did it by
hand); three follow calls that write elements of the very window whose length
the loop reads, `lay_out_children` on `context^.children` before
`flow.wf:1927`, `flex_place_lines` on `lines` before `flex.wf:1561` and
`distribute_fixed` or `distribute_auto` on `table.cols` before
`table.wf:719`, which need 1C or a range reference argument; `flow.wf:965`
follows several calls and was not attributed. The nine are few against the
90 guards over stored positions, so none of 1C to 1E would change the layout
code's guard count much; 1F is the direction with that reach, and 1B, with
the 68 unneeded guards, the cheapest help to its writers.

## 2. Module-level names closed to every local of the module

### Witness

[`probes/collision/`](probes/collision/) is a module `pkg::layout` with two
implementation records. `table.wf` declares a private function:

```wf
fn narrow(width: u32) -> result: u32 pure {
  doc "The table's narrower column width.";
  return width / 2_u32;
}
```

and `grid.wf` binds a local of the same spelling:

```wf
fn grid_width(width: u32) -> result: u32 pure {
  doc "Names the narrower track width narrow, as the grid author wrote it.";
  let narrow = width / 2_u32;
  return narrow;
}
```

```text
main: rejected: ./layout/grid.wf:3:7: error[TYPE-6]: DeclarationCollision
  spelling: narrow
  conflicts: [{domain: LexicalIdentifier, class: Function, origin: ./layout/table.wf:1:4 "fn narrow(width: u32) -> result: u32 pure {"}]
  mechanical_fix: a declaration's scope ends with the block that declares it, and not where its value is consumed: a binding whose value was moved is dead as a value while its declaration stays live, so an inner declaration of the same spelling still collides with it. Rename the inner declaration, or close the block that declares the outer one before this point
```

With `table.wf` removed the same module is accepted. A file alias collides
the same way: in [`probes/collision-alias/`](probes/collision-alias/)
`grid.wf`'s `alias Track = pkg::layout::Span;` is refused against
`table.wf`'s private `struct Track`, in the nominal-type and constructor
domains, with the same repair text.

### Cause

- Every top-level declaration of a module's records enters one inventory, and
  a declaration of an implementation record is visible in every
  implementation record of the module [MOD-3]. The design records the reason:
  files organize one private implementation, and calls between them do not
  route through the interface
  ([design: name resolution](../../../design/language/name-resolution.md)).
- A nested lexical declaration may not shadow a live entry, and every
  top-level declaration is live throughout its module, so no parameter, local
  or generic anywhere in the module may take its spelling in its domain,
  including in a record written before it [TYPE-6]. An alias collides with
  every declaration of its module's inventory [MOD-4].
- The function and the local share the lexical IDENT domain, although no role
  admits both: a `callee` and a function argument admit a function or a
  function parameter, and a `pbase` admits a runtime value, a definition, a
  result datum, a named const or a const generic [TYPE-6]; there are no
  function values [FN-5].
- No design-tree decision states why a local may not shadow a module-level
  declaration. The modular-compilation research kept "the existing
  collision/no-shadowing rules" and noted that new globals can invalidate
  lookup or scope checks
  ([modular compilation](../modular-compilation/DESIGN.md)); the rule predates
  modules.
- The repair text is the one for a local block collision. Neither of its
  alternatives fits a module-level declaration: the outer declaration's block
  is the module, which cannot be closed, and the moved-binding explanation
  does not apply. A repair alternative must let the judgment succeed when
  carried out [DIAG-1], so this is a compiler defect, recorded in
  `docs/todo.md`.

### Frequency

`pkg::layout` declares 622 top-level names (436 functions, 137 constants,
45 structs, 4 enums) across its 13 records, and its records hold 209 aliases;
each of those spellings is closed to every parameter and local of the module.
Snowghost commit `269a29a`, made when the table branch merged into the stage
branch, renames three of table layout's declarations that grid layout's
names had taken: the struct `Track` (grid's file alias of
`pkg::css::values::Track`), the function `narrow` (a local in grid) and the
function `first_baseline` (a destructured local in grid). The guard
experiment of item 1 met the same rule in another form: unwrapping a guard
block, rather than replacing its condition, moved its locals into the
enclosing block, where they collided with later locals of the same spelling,
so the experiment keeps every block.

### Directions

- **2A. Correct the repair.** A compiler change: for a collision with a
  module-level declaration or alias, name the record that declares it and
  offer renaming either declaration. Needed under every direction below.
- **2B. Separate callables from values in the shadowing check.** A language
  change to TYPE-6: a value declaration may share a spelling with a top-level
  function, and a function parameter with a value, since no role admits both.
  It covers `narrow` and `first_baseline`, two of the three renames, and not
  `Track`. Cost: one spelling can read as a function and as a value in one
  body.
- **2C. Locals may shadow module-level declarations.** A language change to
  TYPE-6: inside its scope a local hides a module declaration of its domain.
  Adding a declaration to a module then never breaks another record's locals.
  It covers the same two renames. Cost: in a `pbase` position a local can hide
  a named const of the same spelling, so a reader must know the local scope to
  read a name.
- **2D. Record-private declarations.** A language change to MOD-3: a
  declaration marked private to its record is visible only there and occupies
  no spelling elsewhere in the module. It covers all three renames, aliases
  included if an alias is checked against record-visible declarations only.
  Cost: a new modifier, and moving a declaration between records can change
  what it may see.
- **2E. A naming convention.** No change: the writers prefix record-local
  helpers with their subject (`table_narrow`), as Snowghost did after the
  merge. A project-local fix in the sense of AGENTS.md; it leaves the rule's
  cost on every merge of separately written records.

2A is a defect fix. Among 2B to 2D the one measured case does not separate
them; a replay of the stage's merges under each rule would.

## 3. Allocation counts clamped to a program ceiling

### Witness

[`alloc-from-length.wf`](probes/alloc-from-length.wf) gives a window of
16-byte marks one slot per element of a `&[u64]`:

```wf
fn marks_for(source: &[u64]) -> result: Box<Slots<Mark>> reads(source) {
  let count = source^.len;
  let marks = box_slots_new::<Mark>(capacity: count);
  return move marks;
}
```

```text
alloc-from-length.wf:9:15: error[OP-9]: UndischargedAllocationFitObligation
  residual: count <= 1152921504606846975_u64
  disposition: Unproved
```

The check stage is one of two. [`alloc-byte-counts.wf`](probes/alloc-byte-counts.wf)
sizes one-byte arrays by a `&[u64]` length and by a product; OP-9's limit for
a one-byte element is `2^64 - 1`, so `--check` accepts it, and the build
stops:

```text
alloc-byte-counts.wf:4:15: target layout failure in TargetLayout: AllocationCountExceedsTarget
  count: "count"
  proved_count_bound: 18446744073709551615
  target_count_limit: 9223372036854775799
  target: x86_64-unknown-linux-gnu
```

With that function removed the build stops the same way at the product. A
bound at OP-9's limit does not suffice either:
[`alloc-language-bound.wf`](probes/alloc-language-bound.wf) requires
`count <= 1152921504606846975`, passes `--check`, and stops at target layout
with `target_count_limit: 576460752303423486`. Clamping to a program ceiling,
as the layout code does ([`alloc-ceiling.wf`](probes/alloc-ceiling.wf),
`imin(count, 1073741824)`), passes both stages and exits 0. A count read from
the length of a `Box<Array<u64>>` sizes a one-byte array with no clamp
([`alloc-shape-length.wf`](probes/alloc-shape-length.wf), checked, built,
exit 0): target qualification bounds a runtime-capacity shape's length by the
target's allocation domain [STOR-6].

### Cause

- Each runtime-capacity construction carries the obligation
  `n <= floor((2^64 - 1) / stride_ceiling(T))`, discharged only by a fact
  about `n` [OP-9]. No standing fact bounds an existing storage's length by
  its element type's limit [MSR-2], although every shape's allocation proved
  it: a count read from a `&[u64]` is known only to be a `u64`.
- Target qualification then multiplies the bound OP-9's proof retained by the
  target's stride and requires the product to fit the allocator and address
  domains [STOR-6]. Every supported target admits fewer elements than OP-9's
  limit, so a bound at that limit fails here, after checking. STOR-6 gives
  the length of a runtime-capacity shape the target's bound; it gives none to
  a range reference's length or to arithmetic.
- The OP-9 repair says so and asks for "the largest count the program needs"
  ([design: diagnostic repairs](../../../design/compiler/diagnostic-repairs.md)).
  Writers who do not know that count pick a ceiling once and clamp every
  allocation to it.

### Frequency

The layout code makes 94 allocations through `box_array_filled` and
`box_slots_new`: 58 with a literal count and 36 with a runtime count. Of the
36, 23 are bounded by one of 14 `imin` clamps (14 allocations directly, 9
through `count + 1_u64` of a clamped count), 8 by a requirement that states
the ceiling (`requires text^.len <= 1073741824_u64`), and 5 by a guard that
returns an error or allocates nothing. `module.wfm` declares the ceilings
`scalar_ceiling` and `item_ceiling`, both `1073741824_u64`. The ceilings
create failure behavior of their own: `build_layout` returns `too_large`
for a document of more than `item_ceiling` nodes, and `push_item` returns
`False` and drops its value when a window holds `ceiling` items.

Removing each of the 14 clamps alone, by writing `let bounded = count;` for
`let bounded = imin(count, item_ceiling);`, and rechecking the module:

| Outcome | Clamps |
|---|---|
| accepted: a one-byte array sized by a window's length (`flow.wf:725`, `inline.wf:600`) or by a product (`grid.wf:1065`) | 3 |
| OP-9 at the allocation: the count is a window's length whose element is at least as wide as the allocated one (`build.wf:370`, `grid.wf:2269`, `table.wf:767`) | 3 |
| OP-9 at the allocation: a window's length with a narrower element (`table.wf:183`, `Item` from `TableCell`), a computed count (`grid.wf:1067`), or a parameter (`tablegrid.wf:73`, `83`, `93`, `120`) | 6 |
| OP-2 at `count + 1_u64`: the clamp also bounds the addition that sizes the next allocations (`inline.wf:151`, `866`) | 2 |

No build was run for these variants, so the target stage is not observed
here: by STOR-6 the two window lengths of the first row qualify and the
product does not. Four clamps also set behavior rather than only a bound:
the three in `grid.wf`, against `grid_max_tracks` (1000) and
`grid_max_cells` (4,000,000), limit the grid the code builds, and
`build.wf:370` reserves at most 4096 slots.

### Directions

- **3A. A standing fact bounds every measured place by its type's limit.** A
  language change to MSR-2: for a runtime-capacity place `P` over `T`,
  `P.cap <= floor((2^64 - 1) / stride_ceiling(T))` holds wherever `P` is live,
  as `P.len <= P.cap` does, and `P.len` obeys the same bound for a range
  reference. It is sound because every runtime-capacity construction and
  `grow` proved it [OP-9], a range reference's run lies inside its origin,
  and `Segments` storage is bounded by its own predicate [OP-13]. A count read
  from a length then sizes any element type no wider than the source's
  without a fact. It does not reach the target stage.
- **3B. Target qualification bounds a range reference's length.** A change to
  STOR-6: the target bound it gives a shape's length also holds for the
  length of a range reference and of a segment, whose elements lie in a
  materialized shape. Together with 3A a count read from any existing
  storage's length passes both stages. Arithmetic counts still need a bound.
- **3C. Target qualification at check time.** A compiler change already in
  `docs/todo.md` ("Checking accepts a program whose build stops at target
  layout"). It moves the second failure to the first round and removes no
  clamp.
- **3D. An unrepresentable allocation size is heap exhaustion.** A language
  change reversing OP-9: the allocation computes its byte size with a
  saturating multiplication, and a size the allocator cannot serve terminates
  the program as heap exhaustion does, outside the language [STOR-8, SCOPE-3].
  OP-9 and the target count qualification disappear, and with them every
  allocation clamp and the failure paths the ceilings create. A count that
  passes OP-9 today and asks for more memory than the machine has already
  ends this way, so the proof distinguishes only sizes above the address
  space. Cost: one comparison per runtime allocation; OP-9 explicitly
  refuses a runtime multiplication guard, and the data-model decision keeps
  allocation-size arithmetic under the ordinary overflow proof, so both are
  reopened; the constitution's "expected input and environment failures must
  have defined program behavior" is met by exhaustion's defined termination,
  which the owner would have to accept for this case too.

3A and 3B are narrow and sound by construction, but here 3A discharges only
the census's second row; the narrower element, the computed count and the
parameters keep their clamps. 3A also bounds the two `count + 1_u64` sums,
since a `u32` text's length plus one cannot overflow, but the four-byte
arrays they size would then exceed OP-9's limit by one element. 3D removes
the nine clamps OP-9 needed, and with 3A the two sums' as well, but reverses
a recorded decision.

## 4. Parameters only a contract reads

### Witness

The conformance case
[`range5-pos-scatter-through-left-inverse.wf`](../../../tests/conformance/cases/range5-pos-scatter-through-left-inverse.wf)
is the smallest form:

```wf
fn scatter(order: &[u64], pos: &[u64], out: &[u64]) -> result: unit reads(order), writes(out) contract {
  requires pos^.len == out^.len;
  requires forall inv(k in 0_u64..order^.len) when order^[k] < out^.len: pos^[order^[k]] == k;
}
```

`scatter`'s body never reads `pos`, and its row does not name it; `pos` is
there so the requirement can say that `order` names distinct slots. Its
caller fills a `pos` array for that purpose alone. The IR that
`whitefootc --emit-llvm` emits for this case passes `pos`'s pointer and
length:

```text
define i8 @wf_scatter(ptr noalias nonnull nocapture %wf.arg.v0.data, i64 %wf.arg.v0.len, ptr noalias nonnull nocapture %wf.arg.v1.data, i64 %wf.arg.v1.len, ptr noalias nonnull nocapture %wf.arg.v2.data, i64 %wf.arg.v2.len) #0 {
```

### Cause

- A range term's places are rooted at live bindings [RANGE-1], and in a
  requirement the live bindings are the function's parameters, so storage a
  requirement mentions must arrive as a parameter.
- Every parameter is a runtime parameter: the callable boundary has no
  proof-only parameter [FN-1], and while contracts are erased [EFF-2], the
  parameters they name are not.
- The distinctness fact the certificate needs is stated through a left
  inverse because range facts are pointwise: one element and elements at
  values it holds. Candidate N chose that form over a ghost model
  ([unique-keys](../unique-keys/DESIGN.md)), with a runtime witness.

### Frequency

In Snowghost `proved-level-cascade` at `8b4f332`, which pins Whitefoot
`5fc912d9`, three parameters of its 2,316 functions are read only by their
contract, all of them `cascade_level`'s (`proto/style/shapes.wf`):
`positions`, `depths` and `level`.
`level_index` allocates `positions`, one `u64` per element, and stores each
element's position in its level only so that `cascade_level`'s `listed`
requirement can state distinctness; nothing else reads it. `depths` is read
while `level_index` buckets the elements and then kept in `LevelIndex` for the
`up` requirement. The layout stage, pinned before range facts, has none.

This meets the reopening condition of the `docs/todo.md` item "Parameters a
contract names but the body does not use are passed at run time", a question
the owner left open until a program needed it: a writer computes a value only
to pass it.

### Directions

- **4A. An order fact instead of an inverse.** No change. Where the data is
  ordered, a two-binder clause states distinctness directly.
  [`order-fact.wf`](probes/order-fact.wf) is the scatter above with `pos`
  removed and `requires forall descending(a in 0_u64..order^.len, b in
  0_u64..order^.len) when a < b: order^[b] < order^[a];`, proved by its
  builder's loop invariants; it is accepted and exits 0. Without the
  requirement ([`order-fact-missing.wf`](probes/order-fact-missing.wf)), and
  with `<=` in place of `<` ([`order-fact-weak.wf`](probes/order-fact-weak.wf)),
  the certificate fails RANGE-5. It does not reach the cascade: each level's
  slots are increasing, but stating that per segment of a `Segments` needs
  three binders, and a clause has at most two [RANGE-1]
  (`range1-neg-three-binders`).
- **4B. Proof-only parameters.** A language change: a parameter marked
  proof-only may appear in contracts, invariants and `use` steps and as the
  actual of another proof-only parameter, and lowering erases it. It removes
  the passing; `level_index` still computes `positions`.
- **4C. Proof-only state.** A language change: locals, fields and arrays whose
  writes are erased, so `positions` is never computed at run time. It is the
  ghost-field candidate the unique-keys investigation left unselected; it
  needs the separation rules that candidate lists (a ghost value chooses no
  runtime branch, index, result, allocation or write) and erased allocation.
- **4D. Three-binder range clauses.** A change to RANGE-1's limit that lets
  4A reach `Segments`. Cost: instances per fact grow with the cube of the
  reads in a problem [RANGE-3].

The cost of the passing itself is three arguments per level, not per element,
and was not measured; the computed `positions` array is one store per
element. 4A is available now; 4B and 4C answer the open todo item.

## Summary

| Item | Rule | Witness | Defect found | Narrowest language direction |
|---|---|---|---|---|
| 1, killed facts | EFF-2, MSR-2, SET-1 | `len-whole-row`, `len-window-row` | no | 1C, element writes in `w.filled` |
| 1, stored positions | RANGE-2, RANGE-4, RANGE-1 | `stored-position`, `stored-position-use`, `stored-field` | no | 1F, range facts bound subscripts |
| 2 | TYPE-6, MOD-3 | `collision` | TYPE-6 repair text | 2B, callables apart from values |
| 3 | OP-9, STOR-6 | `alloc-from-length`, `alloc-byte-counts` | no | 3A and 3B, length bounds |
| 4 | RANGE-1, FN-1 | left-inverse case | no | 4B, proof-only parameters |

No item is a soundness defect: each rejection follows the rule the checker
cites. Item 1 as reported holds two causes. Of the 228 length guards
examined, the mechanism reported, a call whose coarse row kills a length
fact, makes 9 necessary; 68 are not needed today; and 151 protect an index
no fact bounded, 90 of them a position stored in another window. The
compiler changes are 1B and 2A, recorded in `docs/todo.md` with 3C, which
was there already; 2A restores what [DIAG-1] requires of a repair.

## Ruling

On 2026-10-02 the owner selected 1A with 1B, keeping the language for killed
length facts; A for stored positions, keeping their guards; 2B, separating
callables from values in the shadowing check; and 3D, making an
unrepresentable allocation size heap exhaustion
([design log](../../../design/log.md)). Item 4 stays open: the owner asked for
a way to avoid the run-time cost of data only a contract reads, which a
separate record answers.
