# Integer fields below range elements

Status: investigation and proposed rule text, awaiting the owner's choices.
Baseline: Whitefoot `1a6a96c5bba34cac2ed056a4c49d3073e0f1a0a7`, specification
v0.101. No implementation, specification amendment, conformance change or
measurement accompanies this draft. Code below is specification fragments,
not compiled complete programs.

## Question and evidence

Can the range judgment read an integer stored inside an owned element, so a
stored left inverse certifies independent scatter writes without duplicating
the inverse into another array?

Snowghost-wf's `research/m2-frag-a` record at
`f119db22f97f4fdf54710be9dead792ebf6a5a4d`,
[SPLIT-CONTRACT.md, independent owner-motion writes](https://github.com/Ming-Research/Snowghost-wf/blob/f119db22f97f4fdf54710be9dead792ebf6a5a4d/research/investigations/m2-edit-cost/SPLIT-CONTRACT.md#q139-independent-owner-motion-writes-and-the-stored-field-proof-gap),
reports that following sibling blocks move after an insertion. Their payload
writes are independent, but the bounded left/node/right traversal shares
context and touched-slot lists. The record identifies this minimal semantic
witness, written here with reference parameters:

```text
requires forall inv(k in 0_u64..order^.len): payloads^[order^[k]].entry_slot == k;
```

`order` is a sequence of payload indices; each selected payload stores its
position back in that sequence. The statement implies injectivity of `order`
on positions for which its reads exist. It does not itself prove that every
stored index is in bounds. Snowghost declines a second inverse array and
sorting targets merely to obtain disjoint slices: those change the program's
representation or algorithm to avoid expressing its existing invariant.

The Snowghost passage was inspected from its local git object at the cited
revision; its original finding was also source/specification inspection, not
a compiler trial. The current Whitefoot rules independently exhibit the same
restriction. The maintained
[left-inverse scatter case](../../../tests/conformance/cases/range5-pos-scatter-through-left-inverse.wf)
uses `pos^[order^[k]] == k`, with `pos` a separate integer array, and an empty
`apart(i, j)` block. Its source demonstrates the intended proof shape, not
new execution evidence for this draft.

The comparison to decide the proposal is the same scatter through an integer
array versus through an integer field, with equal mathematical inverses and
equal runtime target indices. A field projection should change the selected
integer storage, not the injectivity argument. Mutation and wrong-selection
controls below can answer against that hypothesis. The rejection criteria
are fixed in [the experiment plan](#criterion-before-implementation).

### Snowghost's minimal case

Snowghost-wf branch `research/inverse-proof-case` at 552dcbf,
`research/investigations/m2-edit-cost/inverse-proof/natural.wf`, reduces the
sibling move to one owner's `Open(block: u32)` entries and one write of
another field of each target:

```text
struct Block { entry_slot: u32; normal_y: i32; }
enum Flow { Open(block: u32); }

fn translate_owner_suffix(order: &[Flow], targets: &[Block], first: u64, delta: i32) -> result: unit reads(order), writes(targets) contract {
  requires forall inv(k in first..order^.len) when order^[k].Open.block < targets^.len: targets^[order^[k].Open.block].entry_slot == k;
} {
  let count = order^.len;
  for (k in first..count, apart(i, j) { }) {
    let item = order^[k];
    match item {
      Open(block: b) => {
        let at = cvt::<u32, u64>(b);
        if at < targets^.len {
          set targets^[at].normal_y = targets^[at].normal_y +sat delta;
        }
      }
    }
  }
  return unit;
}
```

Hosted run 37771401960 at Snowghost's pin `wf-0b7f5c5b9854` refuses it at
`order^[k].Open.block` with `error[RANGE-1]: InvalidRangeClause`, "a range
term selects below an element", before `.entry_slot` or the certificate is
reached; the same function without the requirement and certificate,
`today.wf`, is accepted with serial writes.

The case adds three needs to the struct-field draft below:

1. **An enum payload step.** The order side reads `order^[k].Open.block`. The
   renderer's full `Flow` also has `Text(paragraph)`, `Child(context)`,
   `Float(context)`, `Out(context)` and `Close(block)`; Child, Float and Out
   select the same children store. One fact per variant suffices: when
   iteration `i` holds `Child(c)` and iteration `j` holds `Float(c)`, the two
   instances read the same `children^[c].entry_slot` and give `i == j`, so
   collisions across variants are excluded by read congruence with no
   further rule. The struct-only scope therefore cannot accept this case.
2. **Copy provenance.** `let item = order^[k]` copies the element, and
   [RANGE-2] makes a copy new storage whose contents are unknown, so the
   match binder `b` is unrelated to `order^[k].Open.block`. The
   [range-facts decision](../../../design/language/checks-and-proofs/range-facts.md)
   that made copies unknown reopens "when a program must keep a fact across a
   copy"; this program is that case. Proposed rule: a copy's contents are
   defined by the source's version and index tuple at the copy, so a
   projection of the copy reads the same projection of the source version;
   later writes make versions of the copy only. Source versions never
   change, so this is sound, and a `match` arm that takes a variant of the
   copy constrains the source element's variant at that version.
3. **Precise field support.** The loop writes `normal_y` and needs the
   `entry_slot` fact afterwards when a callee requires it again, which is
   the precise-support option below.

The ordinary-obligation route ([ordinary range obligations](../ordinary-range-obligations/DESIGN.md))
adds a fourth: a measure of an element, `rows^[k].len`, is a projection below
an element too, and the same step admits it.

The owner approved range facts over a guarded variant's integer fields on
2026-10-07 (Q139 A, recorded in the match-dispatch plan of PR #262), for
validated interpreter code, so the enum step is an approved direction, not a
new one.

### Current rule boundary

Rule IDs below refer to the active
[kernel specification](../../../spec/kernel-spec.md). These are descriptions
of the baseline, not proposed additional definitions of its rules.

| Rule | Current behavior and consequence for this witness |
|---|---|
| [RANGE-1] | An element-read atom has integer element type. A range-term place “selects nothing below an element.” `payloads^[order^[k]]` selects a struct, and `.entry_slot` descends below it: the witness fails both admission conditions. The grammar already has field suffixes; a new token is unnecessary. |
| [RANGE-2] | A separate forward walk follows ordinary entailment. Facts denote storage versions, writes define new versions, calls and loop headers forget written locations. Range facts supply no ordinary obligation. |
| [RANGE-3] | Fixed instantiation, write/join definitions, integer equality and read congruence, rational inequality feasibility and exhaustive finite case splits prove range obligations. Instances require their ranges, guards and every read's existence. There is currently no field-read atom to feed that derivation. |
| [RANGE-4] | A written `use inv(i)` instantiates an eligible named requirement or enclosing-loop range invariant only in an `apart` certificate. This rule owns instantiation, not support kills. |
| [RANGE-5] | The walk compares the body's two executions from loop entry. It proves every element write apart from every other execution's access to that storage. A written use requires its fact active at that entry. |
| [ENT-5] | Ordinary L0 and opaque-goal facts use resolved storage support, offset support, reference/Box dependencies and overlap kills. Immutable captured theorems retain their former meaning. This is not a license to identify old and new range storage versions. |
| [MSR-1], [MSR-2] | Measures have declared meanings and descriptor-word support, separate from element storage. Replacing `p[i]` kills a measure of that element, not `p.len`; an integer payload field is content, not a measure of the outer collection. |
| [OP-4] | Executed subscripts require ordinary bounds proofs. [RANGE-1] separately gives mathematical reads no OP-4 obligation where written; their existence is a premise of instantiation. |
| [TYPE-2] | `readonly` is module-relative write control, not immutability. A declaring module can write its field, and whole-value replacement can replace it. Making `entry_slot` readonly would not amend [RANGE-1]. |
| [REF-1], [TYPE-8], [TYPE-9] | References resolve to paths; owned aggregates contain no references. `Box.inner` is an owned field selection. An ordinary enum payload selection requires a current variant refinement. |
| [OWN-7] | Resolved prefix overlap includes replacements of ancestors. Different struct fields are disjoint; different variants' payload storage overlaps. Unknown index separation stays overlapping. |
| [PAR-2] | A holding certificate supplies certified elements, subject to the rest of the loop permission conditions. Its success alone grants neither ordinary bounds nor permission for whole-root effects, incompatible reads or forbidden control flow. |

Ordinary [ENT-2] already admits subscripted readonly integer fields, with
ordinary offset and support restrictions. That does not provide quantified
range terms or a route from range facts into ordinary entailment.

### Prior decisions and what changes their grounds

The [range-facts decision](../../../design/language/checks-and-proofs/range-facts.md)
refused field and enum range terms in a proposal that would use written
instances to discharge ordinary bounds. Its reason included the order of the
two judgments and the tradeoff between guards and producer proof work. This
proposal reopens the field restriction for the existing range-only consumer,
certified scatter; it preserves that judgment order and the bounds boundary.
The prior objection therefore still governs ordinary bounds, but does not
settle this different consumer.

The [range implementation decision](../../../design/compiler/range-judgment.md)
specifically rejects inserting range facts into ordinary entailment, because
ordinary element kills and its proof families cannot carry them. Keep the
separate walk. The [loop permission decision](../../../design/language/parallelism/loop-permission.md)
already recognizes owned descendants of elements and certified scatter.
These decisions and their ancestors were read as rationale, not language
authority. A later approved amendment would update their field-admission
claims together; this draft changes none of them.

The [constitution](../../../docs/constitution.md) calls for practical proof
and runtime costs under the safety constraint. It does not uniquely select
field granularity or enum admission. The technical grounds here are that
owned field projection supplies the same integer-valued function that a
separate array supplies, and existing resolved paths describe its mutations.

## Proposed rule draft

This package admits finite struct-field and owned `Box.inner` suffixes,
with precise field support. Enum payload steps are a separate rule below,
recommended because Snowghost's case needs them. Mutable and readonly integer fields have the same range-term
formation rule. Visibility and declared-type selection still apply.

The following quoted paragraphs are candidate normative text. Each belongs
to its indicated existing rule; unmentioned sentences remain in that rule.
The explanations and tables outside quotations are analysis. Existing rule
IDs are used rather than introducing a second definition of any rule.

### [RANGE-1]: formation and read domain

Replace the atom-list sentence and the following place-formation sentence:

> A range term is formed from `affine_expr` syntax over these atoms: an
> integer literal or integer-typed named const, read as its mathematical
> value; a bound variable of the clause, or one of a certificate's two
> iteration names [RANGE-5]; a live own-mode integer binding, read as its
> value where the clause is formed; a measure `p.len` or `p.cap` of a place
> p; the length `s[d].len` of one segment of a `Segments` place s; and an
> element projection as defined below.
>
> A collection place starts at a live binding and follows a finite sequence
> of reference-referent, struct-field and `Box` `inner` steps
> [GRAM-5, TYPE-7, TYPE-9]. An element projection is `p[i]` where p selects
> an `Array`, a `Slots` or the run a range reference names, or `s[d][k]`
> where s selects a `Segments`, followed by a finite sequence of
> struct-field and `Box` `inner` steps. Every subscript operand is a range
> term. Every field step resolves to a field of the preceding selected type
> and obeys [MOD-5]. The final selected type of an element projection is an
> integer type [TYPE-1]. The empty suffix selects the element itself.
>
> A measure place in a range term is a collection place; the segment-length
> atom has the form stated above. Result roots have the admission stated
> below. Place formation admits exactly these forms.

This retains the collection kinds and special two-index `Segments` form.
`p[i].header.entry_slot`, `p[i].cell.inner.entry_slot` and
`s[d][k].entry_slot` become candidates. A second indexed collection below
an element, a range step, an entry-image read, and a `^` below an element
are outside the admitted sequence. Nested *terms* such as `order^[k]` in
another projection's index remain admitted. `Box.inner` stays an ordinary
owned step, including when the element itself is a Box; it introduces no
reference alias or new storage-sharing form.

Replace the generic-formation sentence to distinguish the selected value
from its containing aggregate:

> In a generic function a range clause is formed at its symbolic instance,
> treating a value of type-parameter type in an integer-atom position as an
> integer, and again at each concrete instance. Field selection requires
> the preceding selected type's declared fields. At a concrete instance the
> clause contributes a fact or obligation, and a use contributes instances,
> exactly when every such integer-atom position has integer type.

Thus `Array<Record<T>>` with a declared field `value: T` can form
`p[i].value` symbolically; `p[i].unknown_field` on an unconstrained T cannot.
A concretely declared non-integer final field is a formation rejection, not
the generic integer-specialization case.

Replace the final read-domain sentence:

> A read in a range term executes nothing and owes no [OP-4] obligation
> where written. A read is defined exactly when every element selection in
> that read and its subscript operands selects an existing element. An
> instance's read domain is the conjunction of definedness of all its
> reads; [RANGE-3] consumes that domain.

Struct fields and Box content exist when their containing owned value does.
Definedness is evaluated in the versions to which the clause is bound.
The domain neither establishes an ordinary bounds fact nor promises that an
out-of-range element exists. The empty range and a clause instance with an
undefined read make no assertion about that nonexistent selection.

### [RANGE-2]: versions, projections and mutation

Replace the write-version sentence with the following definition, and use
its projection vocabulary for the existing evaluation, join, call and
loop-forgetting sentences:

> An element-read value is identified by its resolved collection location,
> storage version, element index tuple and ordered field projection.
> Reference aliases and range-reference offsets resolve under [REF-1] and
> [REF-4]. Projection steps identify their declared fields, including owned
> `Box` content. Two different projections denote separate selected values.
>
> A located write defines a new version by these cases. At an index tuple
> different from the written tuple, a projection takes its previous-version
> value. At the written tuple, a projection disjoint from the written place
> under [OWN-7] takes its previous-version value. At the written tuple, a
> projection overlapping the written place takes the written value's
> corresponding projection when the walk can name that value, and a fresh
> unknown of its selected type when the walk cannot name it. Whole-place
> replacement defines all projections below the replaced place. Reads of
> these definitions follow [RANGE-3].
>
> A projection depends on its selected content, the values read in its
> offsets, and the reference and Box selections used to resolve its path.
> Its read domain also depends on the descriptors determining element
> existence. Measures retain [MSR-2]'s support. Mutation events and projected
> call effects are classified by [ENT-5], with resolved overlap from
> [OWN-7]. The range walk applies the version definitions here to located
> writes and its forgetting rules to affected locations and projections
> whose new contents it cannot name.

Replace the location-granularity portions of call and loop-header forgetting
with this text, retaining their existing event sites and other state changes:

> At a call or loop header, forgetting a written location makes each
> projection overlapping its projected write footprint unknown and retains
> each projection proved disjoint from that footprint. The footprint is
> resolved by the event and overlap rules above.

A whole-element footprint overlaps every field projection. This is a
specification choice about precision, not a checker optimization that may
alter acceptance. Unknown writes retain [RANGE-2]'s existing
forget-every-location behavior.

“Kill” here means that a previous range fact does not directly establish the
same spelling over current storage. Its old-version theorem stays true;
the new version must be related through definitions, or be unknown after
forgetting. A scalar index binding captured in an old fact also retains its
old value; assignment does not retarget that fact. This is distinct from
deleting an ordinary mutable-support fact under [ENT-5]. No change to
ordinary [ENT-5] fact admission or killing is proposed.

The existing aggregate-copy rule remains: an aggregate copied from storage
has unknown contents in its new storage. Field projection must not silently
transport all source facts to such a copy. Known scalar construction
operands may name written projections; an unknown replacement cannot recover
an old inverse merely because its type and field names match.

### Support and kills: cases that distinguish the choices

The precise option above uses the selected field footprint. The conservative
option instead uses the entire selected element as content support: every
write below a potentially overlapping element advances all of its projection
versions, and unwritten projections become unknown there. It keeps
previous-version values at proved-distinct tuples. This is a deliberate
loss of information; it must not be described as [OWN-7] proving sibling
fields overlap. Either option includes offset and path-resolution
dependencies and the read domain, not just the leaf field's spelling.

| Event relative to a fact over `p[i].f` | Conservative whole-element support | Precise field support (recommended) |
|---|---|---|
| Write `p[j].f`, with potentially equal indices | New f value at the written tuple; no unconditional reuse of old f | Same |
| Write `p[j].g`, where f and g are distinct declared fields | Old f unavailable at that tuple; fresh unknown f | Old f retained, by field disjointness |
| Replace `p[j]` or an ancestor of f inside it | Every affected projection is replaced | Same for projections below that replacement |
| Replace the root or enclosing Box owner | All its current contents lose their old association | Same |
| Change an index source such as `order[k]` | The old theorem keeps its captured index; a new read uses the new index version | Same |
| Rebind a reference used to reach p | The new target gains none of the old target's facts automatically | Same |
| Window operation moving elements, such as `insert_at`/`remove_at` | Its [OP-10]/[PRE-1] projected writes invalidate affected element versions and descriptor facts | Same; field precision does not preserve an occupant across a slot move |
| Exchange `p[a]` and `p[b]` | [OP-11] supplies two reads and two writes, including the equal-target case; affected fields acquire exchanged contents when known, otherwise unknown | Same; old slot facts do not survive as current facts merely because both roots stay live |
| Write at a provably disjoint element | Retained at the other tuple | Retained at the other tuple |
| Consume or leave scope | Old location cannot supply a new live read after the event | Same |

A known write restoring the same value, or an exchange whose captured values
establish the desired relation, can prove a fresh current fact through
[RANGE-3]. Therefore negative mutation cases must destroy the relevant
information or make the new required relation false; a mutation alone is
not a universal rejection condition. Window operations follow their actual
rows, not a new blanket “all window calls kill all fields” rule. In
particular, appending does not move a proved-live earlier slot, but changes
the length and creates a new slot; an old fact says nothing about that new
slot's fields.

Readonly fields get the same treatment. Their declaring-module writes,
whole-element replacement, shifts and exchange remain changing events under
[TYPE-2]. A write to element contents does not by itself change the outer
descriptor words [MSR-2]. A write to an inner descriptor affects any range
atom selecting that integer field and any read domain that uses it.

### [RANGE-3]: use the same derivation on projected reads

Replace the owed-clause opening condition's “whose reads select existing
elements” with “whose read domain [RANGE-1] holds.” In step 1 replace its
read-existence premise with that same cross-reference, and add:

> Instantiation matches reads by resolved collection location and ordered
> projection [RANGE-2]; the empty projection is an integer-element read.
> The bound-variable index criterion applies to the element index tuple of
> a projected read. Projection steps add no bound variables and form no
> further instance from an instance's own reads.

Replace step 2's written-version definition with a reference to [RANGE-2]'s
version definitions, retaining its existing joined-arm definition. Qualify
“two reads of one version” in steps 3 and 4 as:

> Two reads of one resolved collection location and version with the same
> ordered projection are one value when their index tuples agree at every
> solution. That same identity determines the read pairs considered for
> an open-item split.

This qualifies the existing congruence and splitting clauses in place;
their integer solving, apart-position criterion and case coverage remain
unchanged. `p[a].f` and `p[b].f` unify after `a == b`; `p[a].f` and
`p[a].g` do not. Each projected scalar is an atom under the existing 4096
atom ceiling; the existing 256-instance ceiling per fact is unchanged.
No arbitrary field search, new solver, proof budget or recursive
instantiation is introduced.

### [RANGE-4], [RANGE-5] and [PAR-2]: consuming the inverse

[RANGE-4] needs no new proof form: `use inv(i); use inv(j);` denotes the
same written instances with the extended [RANGE-1] terms. [RANGE-5]'s
entry-state requirement continues to apply. A killed current-storage
association is not revived by naming the fact. An old-version fact may help
only through a valid [RANGE-3] definition connecting it to the problem.

Clarify [RANGE-5]'s recording of element accesses with this sentence:

> An executed access at or below an element through owned steps is recorded
> against the outermost element selection of [RANGE-1]'s `p[i]` or
> `s[d][k]` form on its resolved path, with that selection's index tuple.
> Its certified root is [PAR-2]'s root, reference resolution is [REF-1]'s,
> and a range-readable field projection identifies the selected value under
> [RANGE-2].

This keeps certificate separation at element granularity. It does not grant
two iterations permission merely because they write different fields of
the same element. [PAR-2]'s certified-element clause consumes the retained
holding certificate without a new permission family or bounds route.

For the Snowghost-shaped scatter, let `a = order^[i]` and
`b = order^[j]`. Both executions' ordinary guards establish the executed
payload selections exist. [RANGE-3] forms the two inverse instances (or
[RANGE-4] names them explicitly), giving
`payloads^[a].entry_slot == i` and
`payloads^[b].entry_slot == j` in their common entry version. In the
`a == b` case, read congruence for the same `entry_slot` projection gives
`i == j`, contradicting the certificate's `i != j`. Thus the target
elements differ. The argument covers field writes or whole-element writes
placed at those targets, provided the walk also separates every body read
and the remaining [PAR-2] conditions hold.

The certificate uses the loop-entry inverse even if an iteration later
changes its own payload's `entry_slot`. That is sound only with the full
two-execution access check: an iteration that mutates `order`, changes a
later target or reaches another iteration's payload must have those accesses
placed and separated as well. The extension does not freeze runtime state
or suppress an access from the certificate. Proof reads execute nothing
and add no runtime footprint.

## Separate option: enum payload projections

Without the enum option, payload suffixes stay refused in a range term.
They are outside the positive formation list in [RANGE-1], so the existing
formation-error sentence supplies a RANGE-1 rejection. An ordinary guarded
enum read in executable code keeps its current [REF-1] behavior.

If the owner selects enums as well, replace “struct-field and `Box` `inner`
steps” in the element-projection suffix definition with “struct-field,
`Box` `inner`, and declared enum-payload steps [GRAM-5].” Keep the
collection-place prefix unchanged. Each payload step names a variant of
the preceding enum type and a field of that variant; later suffixes may
select owned descendants, ending at an integer. For that option replace
the definedness sentence in [RANGE-1] with:

> A read is defined exactly when every element selection in that read and
> its subscript operands selects an existing element and every payload step
> selects the active variant of its containing enum in the read's version.

An inactive payload is not zero, an arbitrary readable integer, a trap or
evidence of its variant. A fact with that read claims no conclusion for
the inactive instance. Even a guarded fact cannot publish its conclusion
until its entire read domain is established. A requirement made vacuous by
inactive payloads cannot establish injectivity for writes to those slots.
This mirrors the existing-element premise rather than silently assuming all
elements hold the named variant.

This choice requires coordinated wording beyond [RANGE-1]:

1. Replace [REF-1]'s payload-availability sentence with two positive domains:
   “A payload selection in an ordinary place requires the current variant
   refinement [ENT-3.S15]. A payload selection in a range element projection
   has the formation and read domain of [RANGE-1].” The latter is erased
   mathematical selection and establishes no reference witness or ordinary
   refinement.
2. Extend [RANGE-2]'s versioned projection data with the active variant at
   each enum prefix occurring in a projected read. Known constructions and
   taken match arms establish it; replacement, call effects, joins and
   loop forgetting follow the existing variant-state rule. A payload value
   depends on both its selected content and the selection's variant.
3. Extend [RANGE-3] with the tag of each enum selection as an integer read,
   as [the tag as an integer read](#the-tag-as-an-integer-read) states; the
   derivation's theory and split rule are unchanged.
4. Extend support with variant identity. Whole-enum replacement changes
   both tag and payload versions; payload steps of different variants use
   [OWN-7]'s overlapping-storage rule. A fact cannot cross a replacement
   that changes the tag by retaining only the old numeric leaf.

These are necessary semantics for the option, not claims that v0.101
already supplies tag premises inside quantified read instantiation.

### The tag as an integer read

Item 3's separate tag cases are unnecessary: the tag can be one more
projection of the existing theory. Number an enum's variants `0..V` in
declaration order. A tag selection `tag(e)` of an enum selection `e` is an
integer read of `e`'s version at `e`'s index tuple, with projection "tag" and
type range `0 <= tag(e) < V`.

- **Read domain.** A payload read `e.W.f` is defined where `e` exists and
  `tag(e) == W`. [RANGE-3] step 1 already makes an instance assert its
  conclusions only where its reads' domains hold, so the domain adds the
  equality literal `tag(e) == W` to the instance's premises. A premise is
  entailed when the literals with each of its negations are contradictory,
  and the negation `tag(e) != W` is an ordinary disequality, split as `<`
  and `>` like any other. No new kind of open item exists.
- **Congruence.** Two tag reads of one version at index tuples that agree at
  every solution are one value, and two at tuples not held apart are an open
  item, by step 3 and step 4 as they stand, because a tag read is a read.
  Two payload reads of one version, index tuple and projection, variant
  included, are one value the same way. Payload reads of different variants
  at one tuple are never both defined, since their domains need different
  tag values.
- **Sources of tag literals.** A construction stored at a tuple defines the
  written tag as a constant; a `match` arm that takes `W` on a value the
  walk holds as `tag(e)` adds the path condition `tag(e) == W`; a write of a
  whole enum defines the new version's tag at the written tuple and its
  payload projections from the written value, unknown where the walk cannot
  name it.
- **Order independence.** The problem's literals are the existing ones plus
  equalities, disequalities and bounds over tag atoms, which are integer
  atoms; the theory, the split rule and its order-independence argument in
  [RANGE-3] and the
  [range facts decision](../../../design/language/checks-and-proofs/range-facts.md)
  cover integer atoms of any origin, so they cover tags. The 4096-atom
  ceiling counts tag atoms like any read.
- **Wrong and unknown variant.** If `order^[k]` may hold `Close`, the
  instance at `k` has the premise `tag(order^[k]) == Open`; a certificate
  that needs its conclusion fails unless the path conditions entail that
  premise, which is the intended refusal of the wrong- and unknown-variant
  controls below.
- **Variants sharing a store.** With facts per variant, instances at `i`
  (`tag == Child`, target `c`) and `j` (`tag == Float`, target `c'`) read
  `children^[c].entry_slot` and `children^[c'].entry_slot`; where `c == c'`
  those are one read, so `i == j`, contradicting `i != j`. Collisions across
  variants are excluded by congruence alone.

Copy provenance composes with this: a copy's tag and payload projections
read the source version at the source tuple, so a `match` arm taken on the
copy constrains the source's tag. The struct proposal needs none of this.
The enum trial pairs an active-arm certificate with the wrong-variant and
unknown-variant controls below.

## Alternatives and selection grounds

| Alternative | Benefit | Cost and disposition |
|---|---|---|
| Keep refusing; add a second ordinary array read only by proofs | Uses the existing integer-element inverse form | Duplicates an invariant already stored in the payload, adds maintenance and potentially allocation/stores, and makes language expressibility depend on a representation change. Declined for this experiment, as Snowghost requests. No unmeasured runtime cost is claimed. |
| Add a ghost/proof-only array feature | Could erase duplicated proof data reliably | Requires a separate data/flow discipline and still duplicates the inverse. The [proof-only-data investigation's ruling](../proof-only-data/DESIGN.md#ruling) refused that discipline because writers must track two kinds of name and one-way flow. This witness supplies no changed ground for reopening it. |
| Struct fields and Box content now, enums later | Expresses the given struct inverse using total owned projection and existing integer congruence | Leaves enum-backed inverses open, and Snowghost's minimal case reads an enum payload, so it is not selected. |
| Struct fields, Box content and enum payloads together | Expresses variant-specific inverses over actual payload storage | Requires a conditional read domain, quantified tag identity and exhaustive tag cases. Recommended, with the extra rule draft above and copy provenance, because the reported case needs it. |
| Sort or copy targets into an independently partitioned representation | Can expose ordinary disjoint ranges | Changes the target algorithm to avoid stating the stored invariant; it does not answer this investigation and is outside its acceptance criterion. |

## Proposed conformance observations

These are planned cases, not additions to the manifest or claims of green
results. All executed subscripts, arithmetic, effects and construction must
first satisfy ordinary rules so a negative reaches its intended judgment.
Use plain structs with mutable integer fields for the primary positive;
then include a readonly variant so admission is not accidentally tied to
the ordinary [ENT-2] restriction.

| Case to add | Observation and control | Intended rule and verdict |
|---|---|---|
| Struct inverse requirement | The exact Snowghost clause over `&[Payload]`, with `Payload.entry_slot: u64`; a caller supplies a separately established valid inverse | [RANGE-1] forms it; [RANGE-3] proves it: accept |
| Certified field scatter | Adapt the maintained integer inverse case to payload records and writes to their content fields, keeping executed bounds guards; test empty and written `apart` instances | [RANGE-3], [RANGE-4], [RANGE-5]: accept; [PAR-2] reports the target loop permitted |
| Nested owned projection | Select `p[i].header.slot`, `p[i].cell.inner.slot` and `s[d][k].slot`; replace a containing Box as the mutation control | [RANGE-1] accepts each finite owned projection; [RANGE-2]/[RANGE-3] do not reuse a replaced owner's current contents |
| Inverse field overwritten | From a known valid nonempty inverse, overwrite one selected `entry_slot` with a distinct value, then call a callee requiring that inverse; keep all bounds and integer domains proved | [RANGE-2] changes the projection version; [RANGE-3] rejects the call's now-false requirement |
| Whole element overwritten | Replace a selected payload with a record carrying the wrong `entry_slot`, then owe the inverse again | [RANGE-2], [RANGE-3]: reject at the range obligation, including when `entry_slot` is readonly |
| Sibling-field write | Change only `.geometry` before owing the `.entry_slot` relation again; use an unknown geometry value so copying constants cannot mask the path distinction | Precise option: [RANGE-2]/[RANGE-3] accept. Conservative option: an otherwise unavailable inverse at the affected tuple remains unproved and rejects |
| Relocation and exchange | Shift a Slots window or exchange two unequal payloads while leaving order unchanged, then owe their old slot relation | [OP-10]/[OP-11] identify writes; [RANGE-2]/[RANGE-3] reject the false current relation |
| False scatter after index mutation | Change `order` so two in-bounds iterations target the same payload before the certificate, with only the old inverse available | [RANGE-3]/[RANGE-5]: reject the overlapping certificate; this directly detects stale inverse reuse |
| Projection congruence | Use two fields f and g with different values at the same index; a proposed proof needs them equal | [RANGE-3] keeps their identities distinct: reject the false obligation |
| Read through a reference element | Attempt to declare a stored reference element and then read through `p[i]^` | [GRAM-3]/[TYPE-8]: reject the element type before range formation. This is intentionally a storage-boundary case; no well-typed reference-element program exists to isolate RANGE-1. A separate owned-integer element followed by `^` tests the forbidden suffix without inventing reference storage |
| Non-integer final field | A concrete `.flag: Bool` or `.weight: f64` in an otherwise formed range relation | [RANGE-1]: reject the final atom; do not silently treat it as a generic inactive clause |
| Generic leaf | `Record<T>.value` for integer and non-integer concrete T, alongside an attempt to select an undeclared field of bare T | [RANGE-1]: integer instance contributes its clause, non-integer specialization contributes none, undeclared symbolic field rejects |
| Read domain boundary | Out-of-range symbolic projection in erased syntax and an empty quantified range, paired with an executed unbounded subscript | Mathematical form owes no OP-4 at writing and asserts nothing outside its domain; executable negative still rejects under [OP-4] |
| Payload projection, first-scope choice | `p[i].Live.entry_slot` on a declared enum | Struct-only option: [RANGE-1] rejects the payload step |
| Active payload, enum option | A certificate whose actual target accesses and match paths establish the selected variant and element existence for both instances | Enum option: [RANGE-1]/[RANGE-3]/[RANGE-5] accept; [PAR-2] permission still checked separately |
| Wrong payload, enum option | Store another variant in an in-bounds target; the only inverse clause selects `.Live.entry_slot`, while two loop iterations write that same element | The inverse instance asserts nothing; [RANGE-5] rejects the overlapping certificate. Formation of the conditional clause itself is valid |
| Unknown payload, enum option | Same duplicate-target certificate with an unconstrained variant and no matching-arm premise; separately replace Live with another variant after establishing a field fact | [RANGE-3]/[RANGE-5]: reject reliance on an unestablished or stale tag; ordinary direct wrong-arm payload reads remain [REF-1] errors |

The overwrite tests use a subsequent range obligation because losing an
inverse field's value need not invalidate injectivity already proved about
unchanged `order`. The index-mutation test supplies the distinct negative
certificate observation. Formal cases will own minimal standalone inputs
and oracles under `tests/conformance/`; they will not import this research
directory or Snowghost's fixtures.

## Criterion before implementation

Primary criterion: with an experiment compiler release implementing the
selected amendment, Snowghost's frozen certified sibling-move loop is
accepted and its [PAR-2] permission is granted with no source change beyond
the `requires` clause above. The experiment release pin is a recorded
toolchain condition. No extra inverse storage, sorting, target copying,
changed algorithm, added runtime checks, extra proof annotations or widened
effect promises counts as success.

The cited Snowghost revision documents a bounded traversal and a desired
independent preparation/write/reduction shape; it does not itself establish
that a ready certified counted loop exists. Before the future trial, freeze
and identify that loop's source and its existing `apart` block as the
baseline. If making that baseline requires transforming the traversal, the
no-source-change criterion has not yet been met: record that prerequisite
separately, not as a silent relaxation or an achievement of this extension.

The prospective comparison is:

1. Inspect and record the frozen program, compiler revisions, release
   identities and the source delta. Submit the same field-clause source to
   baseline and candidate compilers in authorized CI. Baseline refusal must
   be the identified [RANGE-1] boundary; candidate success must retain a
   checked certificate and a permitted target loop, not merely acceptance
   with sequential permission denied.
2. Use the integer inverse conformance case as the control for the existing
   derivation, and the planned field/scatter cases as the projection
   comparison. Include the mutations and projection-identity negatives
   before claiming a successful rule implementation. Their normative oracle
   is the selected rule text and the explicit counterexamples, not the old
   compiler's answer.
3. Run the applicable compiler/conformance gate and Snowghost's existing
   correctness oracles in CI, retaining revision-specific results and the
   permission record. Start with a small focused sample before choosing a
   larger validation batch. This draft requests no run or release now.

Reject the proposed solution for this witness if the natural inverse cannot
be expressed under the selected scope, the frozen sibling loop still lacks
permission, the producer cannot establish the required inverse, or success
requires any source change beyond the stated clause. Treat an additional
independent compiler or contract gap as a reported blocker, not grounds to
weaken this criterion. Reject the rule design if it permits stale or
wrong-projection facts, lets an inactive payload prove injectivity, admits
overlapping writes, changes ordinary bounds/ownership obligations, or makes
acceptance depend on a solver heuristic or work limit. Failure to establish
the existing rule's finite, deterministic derivation for the extended terms
also rejects the draft as ready for amendment.

Acceptance and permission are the result under study. No speedup, overhead
or checking-cost improvement is predicted from source inspection. A later
performance claim needs its own same-source interleaved comparison, base
twin and falsifier under [the research method](../../README.md); it is not a
condition silently added to this language-expression trial.

## Open choices for the owner

---

**Admit struct fields and Box content first, or also enum payloads?**

- **Background.** The current integer-element rule refuses
  `payloads^[order^[k]].entry_slot`. Struct and Box paths select owned
  content that exists with the element. Enum payloads additionally need a
  versioned active-variant premise for each quantified instance.
- **A — Struct fields and Box content first.** Select the main rule draft;
  keep payload suffixes refused. Snowghost's minimal case reads its order
  side through `order^[k].Open.block`, so this scope refuses it; not
  recommended.
- **B — Include enum payloads and copy provenance (recommended).** Select
  the additional conditional-domain and tag-case rules and the copy rule of
  [Snowghost's minimal case](#snowghosts-minimal-case). This accepts the
  case the gap was reported with and is the scope the owner approved for
  validated code (Q139 A), at the cost of a finite-case soundness argument
  and variant-aware mutation coverage. The risk is unjustified conclusions
  from vacuous instances, which the wrong- and unknown-variant cases test.
- **Confidence 4/5.** Snowghost's minimal case settles that the enum step is
  needed. The tag-case argument is not yet written out; a counterexample to
  it would narrow the scope.

---

**Use whole-element or precise field support for range projections?**

- **Background.** Writing `p[i].geometry` changes no byte of
  `p[i].entry_slot`, while replacing `p[i]` changes both. [OWN-7] already
  distinguishes those paths. [RANGE-2] must say which current projection
  versions remain related to the entry facts.
- **A — Precise field support (recommended).** Select the version draft
  and retain facts across proved-disjoint field writes. This composes with
  mutation of real payloads and existing overlap rules. Its cost is tracking
  full projections and owner, index and descriptor dependencies; missing
  any changing dependency would be a correctness defect.
- **B — Conservative whole-element support.** Apply the conservative
  definition above. This is sound with fresh unknowns at changed tuples
  and makes no precision promise for sibling writes. Its cost is rejecting
  otherwise valid preservation proofs and potentially the target trial;
  implementing a simpler representation is not by itself a language-design
  reason to choose it.
- **Confidence 4/5.** Resolved field separation supplies the semantic
  ground. Counterexamples involving owner replacement, window movement or
  unknown paths would require repairing the precise dependency definition;
  the proposed controls have not been executed.

---

## Proposed specification delta and remaining evidence

| Rule | Before | Proposed after | Selection |
|---|---|---|---|
| [RANGE-1] | Integer elements only, no selection below one | Integer projections through the selected owned suffix vocabulary; final-leaf generic formation; one definedness domain | Scope choice above |
| [RANGE-2] | Element versions without field projection semantics | Version identity, write definitions and forgetting include projections and their dependencies | Support choice above |
| [RANGE-3] | Congruence and instantiation of integer-element reads | Same derivation keyed by resolved location, version, index tuple and projection; read-domain premise | Consequence of scope/support choices |
| [RANGE-5] | Records element accesses | Explicitly places owned descendant accesses at their containing element | Clarification needed by the selected scope; certificate still separates whole elements |
| [REF-1] | Payload selection requires ordinary current refinement | Separate ordinary-place and erased range-projection domains if enums are selected | Enum option only |
| [RANGE-4], [PAR-2], [ENT-5], [MSR-1], [MSR-2], [OP-4], [TYPE-2], [OWN-7] | Existing instantiation, permission, support, measure, bounds, field and overlap rules | No new ordinary proof route or independent replacement rule | Retained boundaries; analysis above states their consequences |

No normative tokens or grammar productions are added. All specification
wording here is proposed; no owner ruling or version archive has been
written. Before implementation, the selected draft still needs a soundness
review of versioned projection and domain handling, followed by the
independent conformance observations and frozen Snowghost trial. The enum
option additionally needs the integrated tag-case argument stated above.

Findings retained in this document within the requested single-file scope:
the earlier field refusal has a different consumer premise; range facts
must not be described as ordinary ENT-5 facts; a stored-reference negative
cannot reach RANGE-1 as a well-typed program; and the cited Snowghost
traversal is not evidence of a prepared certified loop. Their dispositions
are, respectively, a proposed reopening, explicit separate version
semantics, correctly assigned negative-test ownership, and an unverified
experiment prerequisite. No implementation or measured outcome is claimed.

An independent read-only agent reviewed the complete draft against the
baseline, the relevant specification and design passages, and checklist
groups A, D and V plus applicable design-correspondence checks. Its one
concrete finding was ambiguity in which containing element [RANGE-5]
records; the draft now names the outermost admitted selection and certified
root, and the reviewer reread that repair. No concrete finding remains
within that scope. This review is not a proof of the proposed semantics.
No build, compilation, test, lint or compiler execution was performed.
