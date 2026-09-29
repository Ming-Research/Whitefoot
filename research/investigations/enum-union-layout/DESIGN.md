# Union layout for payload enums

## Question

The compiler lays out an enum as a product: an `i32` tag followed by every
field of every variant, so a value's size is the sum of all payloads although
only one is ever present. The Slab comparison recorded the cost and
[docs/todo.md](../../../docs/todo.md) deferred general enum layout until a
workload dominated by it appeared. Snowghost's CSS tokenizer is such a
workload: it stores a stylesheet as `Box<Slots<Component>>`, where
`Component` has 27 variants, 15 of them with a small payload (a `Span` of two
`u32`, or a `Numeric` of `f64`, tag and `Span`), and the renderer reports
about 1.4 GB for a 15 MB stylesheet. The planned DOM `NodeData` and display
list have the same shape, and the standard library's `IoError` (28 variants
with the same two-field payload) is carried inside every I/O `Result`.

This investigation establishes:

1. whether the specification fixes enum layout, or states anything an
   overlapping-payload (union) layout would contradict;
2. where the compiler depends on the product layout;
3. what the product layout costs today and what a union layout would give;
4. a design, its soundness argument and the sites it changes; and
5. its cost and a recommendation.

It edits no specification and no compiler code. The census patch below was
applied to a scratch build and reverted.

## Criterion

The criterion for recommending a compiler change, stated before the
alternatives of section 5 were weighed (the layout and emission census of
section 3 is deterministic data gathered while locating the dependent
sites):

1. **No language change.** The specification must admit the layout as it
   stands: no rule may fix payload offsets, the sum of payload sizes, or the
   disjointness of payloads of different variants, and the language layout
   ceiling [OP-9] must hold for every enum, so target qualification [STOR-6]
   cannot newly fail on an accepted program.
2. **Backend only.** The checked program, the ownership and permission
   judgments, the IR and the conformance evidence must be untouched; every
   dependent site must be in target layout, emission, the call ABI, cleanup
   emission or the linked host library.
3. **A material gain where layout dominates.** The motivating values must
   shrink by at least a factor of two (Snowghost's `Component` stride,
   `IoError`-bearing results), and no value may grow.
4. **No regression of a recorded decision's measured gain.** In particular,
   results that return in registers under
   [compiler/result-registers](../../../design/compiler/result-registers.md)
   must keep doing so.

A prototype would be warranted only if the dependent sites were confined to
the memory forms and the soundness argument needed no new representation
choice the owner has not ruled on; section 4 shows it needs one (the
first-class carrier question and the cleanup walk), so the investigation
stops at the design.

## Method

All numbers are for x86-64 Linux, `whitefootc` built from `85e2c89bf`
(specification v0.77), with the LLVM data layout of clang 18.1.3.

**Probe types.** A probe program declares Snowghost's `Span`, `Numeric`,
`Component` and error enums exactly as
`renderer/css/syntax/module.wfm` declares them (Snowghost `5fa313b`), a
256-byte `nocopy struct Record { words: Array<u64, 32>; }`, the Slab's
`SlabHandle { index: u64; generation: u64; }`, and a DOM-shaped enum that is
this investigation's own guess at `NodeData`, not Snowghost's:

```wf
enum NodeData {
  Document(quirks: u8);
  Doctype(name: Span, public_id: Span, system_id: Span);
  Element(name: u32, namespace: Namespace, attributes: Span, template_contents: u32);
  Text(text: Span);
  Comment(text: Span);
  ProcessingInstruction(target: Span, data: Span);
}
```

It constructs one value of each measured type and emits LLVM
(`--emit-llvm`). The product sizes are those of the emitted `%wf.t.*` types.
Union sizes are the sizes of per-variant view types `{ i32, fields... }`
written in the same LLVM module, rounded to the largest view alignment; both
columns come from LLVM's own `getelementptr null, 1` size computation.

**Corpus census.** [census.patch](evidence/census.patch) (applies to `85e2c89bf`)
adds, to a scratch dev build only:

- a layout census: for every enum with at least two payload-carrying
  variants, its product size from `compiler/src/target.rs` (`LayoutComputer`),
  its union size computed by the same code with nested payload enums also in
  union form, and whether its product representation fits the return-register
  budget of `compiler/src/backend/abi.rs` (`fits_return_registers`);
- an emission census: a marker at every emission path that touches a value
  of an *eligible* enum (defined in section 4: at least two payload variants
  and a product representation that does not fit the return registers), or
  a struct or inline array that contains one.

Every `tests/programs/**/*.wf` that compiles standalone (81 of 89; the other
eight need a module graph or other inputs) was compiled with the scratch
build. The preserved `tests/codegen` corpus is not an active gate and does
not parse under the current surface syntax, so it is not counted.

## 1. What the specification says

The layout is a compiler representation choice. The specification states one
ceiling and several rules about payload *places*; none fixes payload offsets,
and the place rules already describe overlapping storage.

- **[OP-9] layout ceilings.** "A payload enum, including `Option` and
  `Result`, sequences a `(4,4)` tag followed conservatively by every variant
  payload field in variant and field declaration order." This is a ceiling:
  "Before emitting a stored type S, target qualification verifies that its
  actual size, alignment, and stride do not exceed the three language
  ceilings." The word *conservatively* and the verification direction make
  the product sum an upper bound, not the representation. The source
  allocation obligation `n <= floor((2^64 - 1) / stride_ceiling(T))` reads
  the ceiling, not the actual stride, so a smaller actual layout changes no
  source judgment. Section 4 proves the proposed layout never exceeds the
  product layout, and therefore never exceeds a ceiling the product layout
  already met.
- **[STOR-6]** requires the compiler to compute "size, alignment, field or
  payload offsets, element stride, and padding under that target's ABI"; it
  fixes the obligation, not the offsets.
- **[STOR-1]** makes an ordinary owned value frame-resident "inline in its
  owner or the stack frame" and an `Array<T, N>` "N stride-spaced element
  representations"; the element representation and stride are the target's.
- **[STOR-7]** "Any value may be relocated by copying its bytes" and "no
  accepted program can observe" an address. There is no size or offset
  operation in the operation table, so layout is unobservable.
- **[OWN-7] overlap.** "Two payload steps naming different variants of one
  enum select the same storage and therefore overlap", while "two payload
  steps of one variant selecting different fields of that payload" separate.
  This is exactly the union model: fields of one variant are disjoint, fields
  of different variants share storage. [EFF-1]'s effect-path overlap (two
  declared paths overlap "when the first pair of steps at which they differ
  is ... two payload steps naming different variants"), [EFF-5] call-site
  disjointness, [PAR-1] statement overlap and [ENT-5] fact kills all use this
  relation, so a write through one variant's payload already kills facts
  about, and denies independence from, every other variant's payload.
- **[REF-1]/[REF-2] payload references.** A payload step "is available only
  under the refinement fact that the enum currently holds that variant, which
  a `match` arm establishes [ENT-3.S15] and which any write to the enum
  invalidates". A selected payload reference survives the end of its arm but
  "replacing its enum or any containing owner invalidates the existing
  reference". Section 4 uses these two rules for the soundness argument.
- **[OWN-13] match binding by reference** binds "each payload as a reference
  naming the scrutinee path extended by that payload step"; own-mode match
  consumes the whole scrutinee.
- **[WIN-3] moves.** "A move out of a field or out of `Box` content consumes
  the whole owner"; there is no hole, so no partially moved enum exists whose
  remaining payload bytes would have to survive.
- **[PROV-6]/[STOR-3] drop.** The one release walk visits "an enum's active
  variant's payload selected by the discriminant"; inactive payloads are
  never released or read.
- **[STOR-5] reference-free storage** and the uninitialized-read rules: no
  payload holds a reference, and no source read reaches a payload without the
  refinement fact, so inactive bytes are never read as a value.
- **`reinterpret`** [OP-1] is defined only on equal-width primitive pairs;
  it never reinterprets an aggregate.
- **`readonly`** [TYPE-2] restricts writes through a path; it says nothing
  about storage.
- **`eeq`/`ene`** are defined only on tag-only enums; "payload-carrying
  enums, enum ordering, and enum/integer conversion remain outside the
  operation table" [OP-1], so equality has no payload representation to
  depend on (the [enum-equality investigation](../enum-equality-investigation/DOSSIER.md)
  concerns tag-only operands only).
- **Constants** [CONST-2]: enums are not const-eligible, so no static enum
  representation exists.

One sentence could be read the other way: [MSR-3] says of the payload
placement that "an enum more than one of whose variants carries fields needs
no separate treatment: each arm's path names its own variant's storage". It
concerns which tracked place a binder denotes, not disjointness, and [OWN-7]
states the storage relation explicitly, so it is not a conflict. No
specification change is needed or proposed. Criterion 1 holds.

## 2. Where the compiler depends on the product layout

The checker already implements the union model:
`compiler/src/semantic/places.rs` `separation` answers `Overlapping` for two
payload steps of different variants and `Separate` only for different fields
of one variant, and `steps_provably_same` requires equal variant and field.
The OP-9 ceiling is computed independently of the target layout
(`semantic/check/expressions/flat_storage.rs`
`instantiated_layout_ceiling`, `lowering/builder/prelude.rs` `layout_ceiling`)
and stays the product formula. Lowering emits payload access as
`IrOperation::ProjectVariant` and `IrPlaceStep::EnumVariant` naming
`(variant, field)`, never an offset. Every dependency is in the backend:

| Concern | Site | Product assumption |
| --- | --- | --- |
| Layout | `target.rs` `LayoutComputer::nominal_layout` | tag then every variant's fields through `struct_layout` |
| LLVM type | `backend/emitter.rs` `emit_nominal_declarations` | `%wf.t.<link> = { i32, <all fields> }` |
| Field index | `backend/emitter.rs` `variant_field_base` | flattened index `1 + fields of earlier variants` |
| Construction | `emitter/places.rs` `emit_place_definition` (`ConstructEnum`) and `construct_at` | zero-fill, tag at field 0, fields at flattened indices |
| Projection | `emitter/places.rs` `emit_place_definition` (`ProjectVariant`) | GEP at the flattened index, then load or copy |
| Payload references | `emitter/places.rs` `projected_address_pointer` (`EnumVariant`) | same GEP; every `&p^.V.f`, by-reference match binder and effect-path argument goes through it |
| SSA construction | `emitter/operations.rs` `emit_enum`, `emit_enum_insert_sequence` | `insertvalue` at flattened indices (used when the value has no slot) |
| SSA projection | `emitter/operations.rs` `emit_variant_projection` | `extractvalue` at the flattened index |
| Match | `backend/emitter.rs` `match_tag` | `extractvalue ..., 0` of a first-class load, or a field-0 load |
| Moves and copies | `emitter/places.rs` `copy_storage` (memmove of the LLVM type's size), `value_operand`, `save_value_result`, `emit_place_edge` | whole-value first-class loads and stores of the product type on edges, saved results and materialized operands |
| Drops | `emitter/cleanup.rs` `emit_resource_drop_helpers`, `emit_enum_cleanup_body`, `emit_cleanup_jobs`, `emit_run_drop_helper`; `backend/emitter.rs` `prepare_drop` | the helper takes the enum as a first-class value, `extractvalue`s the tag and each field at flattened indices; struct fields, `Box` referents and run elements reach it as first-class values |
| Call ABI | `backend/abi.rs` `ReturnLeaves::add`, `fits_return_registers`, `FunctionAbi::build` | counts the tag and every variant's leaves; stored aggregates pass by `ContentPointer` |
| Checked-result producers | `emitter/integer.rs` (checked arithmetic), `emitter/conversion.rs` | build `Result<T, E>` as `{ i32, T, E }` with `insertvalue` at 1 and 2 |
| Backend facts | `backend/emitter.rs` reference `dereferenceable` via `target::validate_static_storage` | reads the target layout, so it follows it |
| Frames, lanes, qualification | `target.rs` frame plans, `parallel_lane_frame_layout`, `validate_program` | read the target layout, so they follow it |
| Host mirror | `backend/ordinary_values.h`, `ordinary_values.c`, `ordinary_values_probe.c`, `ordinary_values.ll` | C structs `{ tag; value; error }` and `wf_io_error = { tag; detail[28] }`, written as `error->detail[tag]` |

Not dependent: `eeq`/`ene` (tag-only), tag-only enums
([compiler/tag-only-lowering](../../../design/compiler/tag-only-lowering.md)),
struct field reuse in
[compiler/storage-placement](../../../design/compiler/storage-placement.md)
(struct fields only), the permission and parallel-lowering planners (they
read the checker's overlap answer), and `lib/std` Whitefoot sources.

## 3. Measurements

### Probe types

| Type | Variants (payload) | Product bytes | Union bytes | Align |
| --- | ---: | ---: | ---: | ---: |
| Snowghost `Component` | 27 (15) | 168 | 40 | 8 |
| DOM-shaped `NodeData` (this investigation's guess) | 6 (6) | 84 | 28 | 4 |
| `std::io::IoError` | 28 (28) | 228 | 12 | 4 |
| `Result<u64, IoError>` | 2 (2) | 248 | 16 | 8 |
| `Result<u64, ReadStop>` | 2 (2) | 248 | 24 | 8 |
| `Result<unit, IoError>` | 2 (2) | 236 | 16 | 4 |
| `Result<ReadFile, IoError>` | 2 (2) | 288 | 48 | 16 |
| `Result<SlabHandle, Record>` | 2 (2) | 280 | 264 | 8 |
| `Option<Record>` | 2 (1) | 264 | 264 | 8 |
| `Option<u64>` | 2 (1) | 16 | 16 | 8 |
| `Result<u64, u64>` | 2 (2) | 24 | 16 | 8 |

`Component`'s largest view is `Dimension(number: Numeric, unit_name: Span)`:
tag, padding to 8, 24 bytes of `Numeric`, 8 of `Span` = 40. The next are
`Number` and `Percentage` at 32, `Function` and `Hash` at 16 and the
`Span`-only variants at 12. The product is 168 at this revision (the report
gave 176 per slot), a factor of 4.2.

For Snowghost, `push_component` lets the component window grow to
`2 * 16,777,216 = 33,554,432` slots for the largest admitted source: 5.64 GB
of slots in product layout and 1.34 GB in union layout. At the reported
stylesheet (about 8 million components) the slots take about 1.34 GB in
product layout and about 320 MB in union layout; each `push_component` call
also copies 40 instead of 168 bytes into its parameter.

For the Slab, the insertion result `Result<SlabHandle, Record>` becomes 264
bytes, equal to the C union control in the
[Slab comparison](../../experiments/container-representation/slab-library/RESULTS.md#storage-and-allocation);
`Option<Record>` has one payload variant, so its product and union layouts
are the same 264 bytes, and the Slab cell stride is a struct and unchanged.
The comparison's extra transfers are a separate cost this layout does not
remove.

### Corpus census

Across the 81 programs, 41 distinct enum instances have at least two
payload-carrying variants.

- **14 fit the return registers** under the product layout (for example
  `Result<u64, Overflow>`, `Result<u64, Utf8Error>`, `Result<unit, u64>`,
  `PutOutcome`). Their union layout would save at most 8 bytes (24 to 16 for
  seven of them, nothing for the rest).
- **27 do not.** Of these, the standard library's file and text results are
  instantiated in 79 programs and its network results in 5: `IoError` 228 to 12, `Result<u64, IoError>` 248 to 16,
  `Result<unit, IoError>` 236 to 16, `Result<unit, ListStop>` 240 to 20,
  `Result<ReadFile, IoError>`, `Result<DirectoryRead, IoError>`,
  `Result<DirectorySource, IoError>` and `Result<TcpListener, IoError>` 288
  to 48, `Result<TcpConnection, IoError>` 320 to 80,
  `Result<AcceptedConnection, IoError>` 352 to 112. Program-defined ones
  shrink less: `KeyPut<u64>` 48 to 32, `LNode` 72 to 48, `Node` 40 to 32,
  `Tree` and `Expression` 32 to 24, `Result<Record, u8>` 4104 to 4100, and
  `Result<unit, PriorityTestTicket>` stays 72.

No union layout is larger than its product layout, as section 4 proves.

### Emission paths eligible enums reach

Emission events for eligible enums over the same 81 programs (events count
world clones separately):

| Path | Events |
| --- | ---: |
| Construction into a slot (`construct_at`) | 75 |
| Payload projection from a slot (`ProjectVariant`) | 528 |
| Payload place step (`EnumVariant`: references, binders, effect arguments) | 366 |
| First-class load of the scrutinee for `match_tag` | 185 |
| First-class load of a drop snapshot (`prepare_drop`) | 6 |
| Enum release helpers taking the value first-class | 11 |
| Block-edge transfers, saved SSA results, SSA construction or projection, other operands, struct insertion, context or lane arguments, struct-field release | 0 |

Every construction, projection and reference already goes through memory.
The first-class paths that do occur are the tag read and the release helper.
The zero rows are reachable in principle (a `value_match` delivering an
eligible enum through a block parameter, a struct holding one released by
value) but no maintained program reaches them.

## 4. Design

### Eligibility

An enum takes the union layout exactly when

- at least two of its variants carry payload fields, and
- its product representation does not fit the return-register budget of
  compiler/result-registers (three integer-class words and two floating
  leaves), counting a nested union enum as not fitting.

Every other enum keeps its current representation:

- tag-only enums keep compiler/tag-only-lowering;
- an enum with one payload variant has a product layout that is already
  byte-for-byte that variant's view, so the rule adds nothing for
  `Option<T>`, `ReadStop` or `Option<Record>`; such an enum shrinks only
  through a nested eligible enum, as `ReadStop` does through `IoError` (232
  to 16 bytes);
- an enum whose product representation returns in registers keeps it, its
  register return and its first-class construction by the checked-arithmetic
  and conversion producers; the census bounds its union saving at 8 bytes.

The rule is evaluated per concrete instance after monomorphization, like
every layout, and is target-independent because the register budget is.

### Representation

For an eligible enum with variants `V0..Vn`:

- **Views.** Variant `k` is laid out as the sequence `{ i32 tag, fields of Vk
  in declaration order }` by the ordinary sequence rule from offset 0, with
  nested enums in their own selected layout. Its LLVM type is a named view
  `%wf.t.<link>.v<k> = type { i32, <fields> }`.
- **Value.** Alignment `A` is the largest view alignment; size
  `S = round_up(max view size, A)`. The LLVM type is
  `%wf.t.<link> = type { i32, [S - 4 x i8], [0 x %wf.t.<link>.v<m>] }`, where
  `v<m>` is a view of alignment `A`; the trailing zero-length array raises
  the alignment to `A` at offset `S`, so the size is exactly `S` and the tag
  stays field 0. `target.rs` computes `S`, `A` and the view offsets and is
  the one layout authority; the emitter prints `S - 4` from it, and a unit
  test compares LLVM's size of every emitted enum and view with the target
  layout.
- **Tag.** `i32` at offset 0, as today and as the OP-9 ceiling's `(4,4)`
  tag. A narrower tag is a separate refinement (see Alternatives).

This is the layout Rust gives a tagged enum before niche and field-reordering
optimizations, and C expresses it as a union of per-variant structs that each
begin with the tag. It is not a C union of payload structs placed after the
tag: that places every payload at one offset aligned to the most aligned
payload, which can exceed the product layout (`enum E { V(a: u8, b: u64) }`
is 16 bytes in product layout and 24 with its payload struct at offset 8) and
so fail OP-9 qualification.

**Never larger than the product.** Let `end(o, F)` be the offset after laying
out fields `F` from offset `o` by the sequence rule (round up to each field's
alignment, add its size). `end` is monotone in `o` and in every field size.
In the product layout variant `k`'s fields start at some offset `o_k >= 4`
after the tag, so the product's unrounded end is at least `end(4, F_k)`, the
unrounded end of view `k`. Both layouts have alignment `A = max(4, every
field alignment)`, and each view's own alignment divides `A`, so
`S = round_up(max_k end(4, F_k), A) <= round_up(product end, A)`, the
product size. By induction over nesting, a nested union enum is no larger
than its product layout and has the same alignment, so the monotonicity
applies at every level. Size, alignment and stride therefore never exceed
the product layout's, which target qualification already accepted against
the OP-9 ceilings; STOR-6 can fail on no program it accepts today.

### Soundness: overlapping payloads

The physical overlap is invisible exactly when no execution reads or writes
one variant's payload while another variant's payload is live. Payload
storage of variant `k` is live exactly while the tag is `k`.

1. **Access requires the current variant.** Every payload read, write,
   projection and reference formation passes a payload step, and every
   payload step needs the refinement fact that the enum currently holds that
   variant [REF-1, ENT-3.S15]. The fact is a proved fact, so an access to
   variant `j`'s payload executes only while the tag is `j`.
2. **References cannot outlive the variant.** A reference to `e.A.x`
   survives the end of its arm, but it is invalidated by any write, move or
   release of `e` or an owner of `e` [REF-2]. Changing the tag requires
   writing `e` or an owner (a `set`, a `swap`, a consuming move or a callee
   row writing `e`), so while the reference is valid the tag stays `A`, and
   by point 1 no write through `e.B.*` can execute. The union therefore never
   overwrites storage a valid reference names.
3. **Alias and independence facts already assume sharing.** [OWN-7] and its
   implementation in `semantic/places.rs` answer "overlapping" for payload
   steps of different variants, so no call receives `noalias` for two such
   arguments [EFF-5], no two statements or lanes touching them are granted
   overlap [PAR-1], and a write to one variant's payload kills every fact
   about another's [ENT-5]. Fields of one variant stay disjoint because a
   view is an ordinary sequence layout.
4. **Release reads only the active variant** [PROV-6]; the release helper
   switches on the tag and addresses that variant's view.
5. **Inactive bytes are never read as values.** Whole-value moves and copies
   copy bytes [STOR-7]; construction zero-fills the `S` bytes before storing
   the tag and one variant's fields, as `construct_at` already does for the
   larger product.
6. **Construction into reused storage.** Operands of a construction are SSA
   values or slots that the storage plan's interference check keeps apart
   from the destination
   ([compiler/storage-placement](../../../design/compiler/storage-placement.md)),
   so writing one view cannot clobber an operand that shared bytes with
   another view of the same destination.

### Memory-only values

LLVM has no union type, so a first-class value of an eligible enum would need
a carrier aggregate. Every carrier has a defect this design avoids:

- a byte array `[S - 4 x i8]` is loaded, stored and passed element by
  element (compiler/result-registers already records that first-class copies
  "are lowered one element at a time"), so a 40-byte `Component` would move
  as 37 separate operations;
- a word carrier covering the occupied bytes loads parts of different fields
  and padding as one integer, and moves a `Box` payload's pointer through an
  integer;
- either carrier moves an `i1` leaf (a `Bool` or two-variant tag-only enum,
  stored as `i1` inside structs such as `Numeric`) through an `i8` or wider
  integer, and LangRef leaves a load of a non-byte-sized type undefined when
  the value was not written by a store of that type.

So an eligible enum is a memory-only value in the backend: it is always in a
slot, its payload fields are loaded and stored with their own types through
view GEPs, whole values move by `memmove` (`copy_storage`), calls pass it by
`ContentPointer` and return it through the destination pointer, and its
release helper takes its address. A struct, inline array or window element
that contains one is moved by `memmove` as well; the emitter already copies
stored aggregates this way between slots.

### Changes by site

| Site | Change |
| --- | --- |
| Eligibility | One predicate on the concrete nominal (payload-variant count, product leaf count), used by `target.rs`, the emitter and `abi.rs` |
| `target.rs` `nominal_layout` | Union branch: views by `aggregate_layout`, maximum size and alignment |
| `emit_nominal_declarations` | Emit the views and the value type above; named-type dependencies as for structs ([compiler/structured-emission](../../../design/compiler/structured-emission.md)) |
| `variant_field_base` users | Replace by one `variant_field_pointer(nominal, variant, field, address)`: the product index for other enums, a view GEP for eligible ones (`construct_at`, `ProjectVariant`, `EnumVariant`) |
| `match_tag` | For an eligible enum, load the `i32` tag from the scrutinee's slot instead of a first-class load |
| `emit_enum`, `emit_variant_projection` | Not reached by eligible enums in the census; make a slot for every eligible value an explicit invariant with a test, so construction and projection always take the memory forms |
| `value_operand`, `save_value_result`, `emit_place_edge`, `InsertStruct`, context and lane argument materialization | For a type containing an eligible enum, transfer by `memmove`; an edge copies each source into its destination after every source is captured, using a frame temporary where the storage plan cannot prove the destination distinct from another transfer's source |
| Cleanup | Eligible enum helpers take `ptr`; add an address-based release job for struct fields, `Box` referents and run elements that contain an eligible enum, so the walk passes addresses instead of first-class values; release order is unchanged ([compiler/cleanup-traversal](../../../design/compiler/cleanup-traversal.md)) |
| `abi.rs` `ReturnLeaves::add` | An eligible enum never fits the return registers, so a result containing one keeps its destination (it already does today) |
| Host mirror | `wf_io_error` becomes `{ uint32_t tag; uint32_t code; uint8_t origin; }` (all 28 views are identical); the eligible results (`wf_value_result`, `wf_copy_result`, `wf_write_result`, `wf_read_result`, `wf_list_status`, `wf_close_result`, `wf_open_result`, `wf_connect_result`, `wf_accept_result`) become unions of per-variant structs that begin with the tag; the result-field accesses in `ordinary_values.c` and the probe's assertions (at most 43 and 95 lines by a textual count) change from `r.value`, `r.error` and `error->detail[tag]` to the view members; `wf_utf8_result` and the register-returning `host_utf8_len` in `ordinary_values.ll` keep the product layout |

Unchanged: the checked-arithmetic and conversion `Result` producers (their
results fit the registers), the OP-9 ceiling computations, reference facts
(`dereferenceable` reads the smaller target size), frames and lane frames
(they read the target layout), and the checker.

## 5. Alternatives

- **Keep the product layout.** Rejected: it multiplies storage by the number
  of payload variants, 4.2 times for `Component`, 19 times for `IoError`,
  and every I/O result frame slot carries 228 bytes of error payloads.
- **Writers box large variants.** Rejected: under the product layout every
  variant keeps its own slot, so boxing all 15 `Component` payloads still
  leaves a 128-byte product of pointers and adds an allocation and an
  indirection per component; it moves a compiler representation cost into
  every interface.
- **Writers restructure the interface** (struct of arrays, one shared
  payload, as the owning HashMap's insertion result does in
  [language/data-model/hash-map-storage](../../../design/language/data-model/hash-map-storage.md)).
  Rejected as the general answer: it is available to one library at a time,
  and a CSS component list, a DOM and an error enum have genuinely different
  payloads. The HashMap's choice stays a valid library tradeoff.
- **C-style union of payload structs after the tag.** Rejected: every payload
  starts at one offset aligned to the most aligned payload, which can exceed
  the product layout and OP-9's ceiling (section 4) and is never smaller
  than the per-variant views.
- **Union layout for every multi-payload enum, with a first-class word
  carrier so small results keep returning in registers.** Deferred: it saves
  at most 8 bytes on the 14 register-sized instances in the corpus, while the
  carrier moves padding, pointers and `i1` leaves through integers and
  amounts to the "packing leaves into integer words" that
  compiler/result-registers refused. Reopen when a workload stores many
  small two-payload enums (`Slots<Result<u32, u32>>`) or LLVM gains a byte
  type.
- **Union layout for every multi-payload enum, memory-only.** Rejected: it
  would move today's register-returned `Result` and `PutOutcome` values to a
  destination pointer, undoing the measured register-return gain of
  compiler/result-registers, and force the checked-arithmetic producers
  through memory.
- **Niche encoding** (tag in invalid payload values, such as a null `Box`
  pointer for `Option<Box<T>>`). Deferred: the language has no refined
  integer domains yet ([docs/ideas.md](../../../docs/ideas.md#narrow-semantic-domains-and-automatic-niches)),
  and a `Box` niche changes `match_tag`, construction, release and the host
  ABI for a saving of one word per value; it composes with this layout later.
- **Narrower tag or field reordering inside a view.** Deferred: an `i8` tag
  shrinks only views whose first field is less than 4-aligned (not
  `Component`, whose largest view is 8-aligned), and reordering gives
  `Component` no saving (40 bytes either way) while making host mirrors and
  view offsets depend on a sort. Both fit under the OP-9 ceiling and can be
  measured separately.
- **Tighten OP-9's ceiling to the per-variant maximum.** Not proposed: the
  ceiling bounds source allocation counts near `2^64 / stride`, far beyond
  any target's allocator, so a tighter ceiling admits no program a target can
  build; the current ceiling remains a correct upper bound.

## 6. Cost estimate

**Specification and conformance.** None. No rule changes; layout is not
observable, conformance cases judge source and program output, and the OP-9
ceiling formula is unchanged.

**Compiler** (safe Rust, backend only):

- eligibility predicate and `target.rs` union branch: about 80 lines;
- emitted view and value types: about 60 lines;
- view addressing replacing `variant_field_base` at the three memory sites,
  and the tag load: about 60 lines;
- whole-value transfers of eligible-containing types by `memmove`, including
  edge ordering, and the no-slot invariant for SSA construction: about 120
  lines;
- address-based release for eligible enums and their containers: about 150
  lines in `cleanup.rs`;
- ABI leaf counting: about 15 lines.

About 450–550 changed source lines, plus the host mirror (about 25 header
lines and the result-field accesses in C). Tests: about 300–400 lines — a layout
property test (union never larger than product, equal alignment, LLVM size
equal to the target layout for every emitted enum and view), runtime tests for
construction, by-reference match, overwriting with another variant, release of
`Box` payloads in two different variants, an eligible enum crossing a block
edge through `value_match`, a struct holding one released by value, and an
eligible element in `Slots` and in an inline `Array`; the existing I/O and
container programs cover the host mirror. Backend tests that assert product
LLVM text for eligible enums change with the representation.

## 7. Recommendation

Against the criterion:

1. **No language change** — met. Section 1: layout is a compiler choice,
   OP-9 is a ceiling the union layout never exceeds, and OWN-7 already
   defines different-variant payloads as one storage.
2. **Backend only** — met. Section 2: no checker, IR or conformance site
   depends on the product layout.
3. **Material gain** — met. `Component` 168 to 40 bytes (4.2 times),
   `IoError` 228 to 12, `IoError`-bearing results 236–352 to 16–112; no
   value grows.
4. **No regression of a recorded gain** — met by the eligibility rule:
   register-returned enums keep their layout and ABI.

Adopt the union layout for eligible enums as a compiler decision, proposed as
the amendment `design/amendments/compiler-payload-enum-layout.md`, and
implement it on its own branch after the owner's ruling. The investigation
did not prototype: the first-class carrier question (section 4) is a
representation choice with a refused sibling alternative in
compiler/result-registers, and the cleanup walk must become address-based
for these values, which is more than the memory forms the census shows are
already in place.

## Validation criterion for the implementation

Recorded before implementation. The implementation is accepted when:

1. **Layout.** Every emitted eligible enum has the union size computed above
   (for the probe types, the table in section 3), LLVM's size of each emitted
   value and view type equals `target.rs`, and no enum's size or alignment
   exceeds its product layout; every non-eligible enum's emitted type is
   unchanged.
2. **Correctness.** `make check` is green with no conformance verdict or
   case changed, and the new runtime tests pass.
3. **Unchanged emission elsewhere.** For a program whose function bodies
   touch no eligible enum, the emitted LLVM differs only in the declarations
   of the eligible types it instantiates (the standard library's I/O results
   are instantiated almost everywhere).
4. **Memory.** A Snowghost-shaped program that fills `Box<Slots<Component>>`
   with 8,000,000 components allocates `40 / 168` of the slot bytes (the
   allocation request is exact, so this is checked by the request size, not
   RSS).
5. **Time.** The Slab and priority-queue library comparisons and the I/O
   programs show no slowdown beyond run-to-run noise, and the Slab insertion
   result is 264 bytes. A slowdown on any of them refutes the memory-only
   choice for that path and is investigated before merge.

## Where the decision goes

The owner approved the amendment and both representation choices on
2026-09-28; the decision is the node
[compiler/payload-enum-layout](../../../design/compiler/payload-enum-layout.md).
The [docs/todo.md](../../../docs/todo.md) Slab item points here, and the
deferred refinements (word carrier for register-sized enums, niches, tag
width) and the pending timing validation are recorded there with their
reopening conditions.

## Implementation results

Measured on the implementing branch (x86-64 Linux, clang 18.1.3), with the
same probe types as section 3. "Before" is section 3's product layout at
`85e2c89bf`; "after" is LLVM's size of the emitted type, which the backend
test `union_layouts_match_the_target_computation_and_the_emitted_types`
compares with `target.rs` for every emitted enum value and view type.

| Type | Before (bytes) | After (bytes) | Align | Layout |
| --- | ---: | ---: | ---: | --- |
| Snowghost `Component` | 168 | 40 | 8 | union |
| DOM-shaped `NodeData` (this investigation's guess) | 84 | 28 | 4 | union |
| `std::io::IoError` | 228 | 12 | 4 | union |
| `std::io::ReadStop` | 232 | 16 | 4 | product (one payload variant) |
| `Result<u64, IoError>` | 248 | 16 | 8 | union |
| `Result<u64, ReadStop>` | 248 | 24 | 8 | union |
| `Result<unit, IoError>` | 236 | 16 | 4 | union |
| `Result<unit, ListStop>` | 240 | 20 | 4 | union |
| `(Result<unit, ListStop>, u64, u64)` (`directory_next`) | 256 | 40 | 8 | struct |
| `Result<ReadFile, IoError>` | 288 | 48 | 16 | union |
| `Result<HostString, ArgError>` | 64 | 48 | 16 | union |
| `Result<u64, CopyError>` | 32 | 24 | 8 | union |
| `Result<TcpConnection, IoError>` | 320 | 80 | 16 | union |
| `Result<AcceptedConnection, IoError>` | 352 | 112 | 16 | union |
| `Result<SlabHandle, Record>` | 280 | 264 | 8 | union |
| `Option<Record>` | 264 | 264 | 8 | product (one payload variant) |
| `Option<u64>` | 16 | 16 | 8 | product (returns in registers) |
| `Result<u64, u64>` | 24 | 24 | 8 | product (returns in registers) |
| `Result<u64, Utf8Error>` | 24 | 24 | 8 | product (returns in registers) |

The rows for `Result<HostString, ArgError>`, `Result<u64, CopyError>`,
`Result<TcpConnection, IoError>`, `Result<AcceptedConnection, IoError>` and
the directory result are the standard library's other linked results; their
"before" sizes are the static assertions the host mirror
`ordinary_values.h` carried at `85e2c89bf`, and for the first two, which had
none, the product rule of section 1.

Against the validation criterion:

1. **Layout** — met. Every eligible enum has the union size, LLVM's size and
   alignment of every emitted value and view type equal `target.rs`'s, no
   enum exceeds its product size or changes its alignment, and non-eligible
   enums keep their product declarations (the backend test above).
2. **Correctness** — met on the branch after it merged specification v0.78:
   the repository's static checks, conformance structure and coverage,
   clippy with warnings denied, the gate build, the library and binary tests
   with the backend native tests, the corpus with the complete conformance
   adapter, and the completion runtime group pass, and no conformance case
   or verdict changed. Three corpus cases whose fixtures are mode-000 files
   fail when the tests run as root, which can read them, and pass as an
   unprivileged user. The new backend tests execute a program that
   constructs every variant, matches by value and through references,
   copies, moves through calls, results, block edges and a loop that
   exchanges two union values, exchanges two with `swap`, overwrites one
   with another variant as a whole binding and as a struct field, writes
   through references into payloads, and holds union values in a struct, an
   `Option`, a `Box`, boxed and inline `Slots`, a `Ring` whose elements wrap
   around its end and an `Array`, with `Box` and `Slots` owners in several
   variants; and a waiting program that passes union values to and from
   waiting calls, bound `mustpar` starts, whose awaits move the result into
   the binding by memmove, and an unbound start that takes one by value. An
   allocation observer confirms every owner is released exactly once, under
   the ordinary and the overlap lowering.
3. **Unchanged emission elsewhere** — met, measured before the merge of
   specification v0.78. The 81 standalone
   `tests/programs` sources of section 3 were emitted by the base compiler
   (`85e2c89bf`) and by this implementation: one is byte-identical, 47
   differ only in the declarations of the eligible types they instantiate
   (the standard library's I/O results), and in the other 33 every function
   whose text changed names a memory-only type (a union enum or an aggregate
   holding one inline, including frame structs that hold one).
4. **Memory** — met. `box_slots_new::<Component>(capacity: 8000000)`
   requests 320,000,016 bytes (16 header bytes and 40 per slot) where the
   product layout requested 1,344,000,016 (backend test
   `a_window_of_union_enums_requests_the_union_stride`).
5. **Time** — met for the containers; the I/O half waived. With the Slab
   and priority-queue harnesses repaired to `std::collections` (PR #179),
   two blocks under the one-minute load limit of 2.0 found no cell outside
   the identical-binary control's noise range: normalized branch/base ratios
   of 0.986 to 1.006 across the slab paths and 0.972 and 0.979 across the
   priority paths, against controls of 0.979 to 0.997, with 3 of 421 slab
   and 1 of 457 priority functions differing by a few instructions, none on
   a measured hot path (PR #174). Slab insertion is 264 bytes. The named
   I/O programs under `research/experiments/io-completion-bench/programs/`
   no longer compile (retired `&uniq` syntax), and on 2026-09-29 the owner
   waived that half rather than port them, judging the union layout correct
   on its grounds.
