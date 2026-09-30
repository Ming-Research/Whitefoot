# Forms for the acyclicity sites, shared ranks and table ranks

This record designs the forms the [Q16 rulings](DESIGN.md#owner-rulings)
call for:
- a ranked arena for the sites whose termination rests on acyclicity;
- shared ranks for mutual recursion;
- ranks read from a constant table;
- ranks derived from a loop's exit guard.

It changes no specification or compiler. Every form must keep checking
deterministic, add no search, and add no runtime work to a walk.

## Constraints from the current language

The design tree refuses three routes (`design/language/checks-and-proofs.md`):
- a quantified invariant over the elements of a storage, because "every write
  into the storage would owe the quantified fact again, and establishing it
  would require a derivation over elements that no terminating fixed family
  performs";
- a type invariant relating a value to another value, such as "an arena
  node's index into another table";
- calls in contract expressions.

Two mechanisms can carry the needed facts:
- **Measure terms.** `P.len`, `P.cap` and `P.head` are logical quantities
  that the specification's measure table owns [MSR-1]. The shape's
  operations publish their values: `take_back` ensures
  `window^.len + 1_u64 == entry(window)^.len`. Probe p5 proves a shrinking
  loop from exactly that fact.
- **Element field terms.** A field of an element below a subscript is
  admitted as a term. The tree cites "an index-based structure such as an
  arena tree" stating "a relation about one element's field in a
  requirement".

Probe `runs/probes/t2.wf` shows that a constant `Array`'s element values are
not known to the checker: `rank_table[2_u64] < rank_table[0_u64]` is
unproved even with literal subscripts.

## The nine sites

| Site | Structure | Form | Measure | Source of the descent fact |
|---|---|---|---|---|
| loop 28 `move_all_children` | DOM, nodes relinked freely | Forest | children of `from_node` | the link primitive's `ensures`, which needs `from_node != to_node` |
| comp 17 `build_element` | DOM | Forest | height of the node | the first-child and next-sibling accessors' `ensures` |
| loop 347 `match_complex` | DOM, walked up and backwards | Forest, lexicographic | depth and siblings before, per frame | the parent and previous-sibling accessors' `ensures` |
| loop 85 `attribute_is_duplicate` | hash chains relinked in ascending order | index-relative link, plus one comparison per insertion | `cursor` | `links[at].next <= at` |
| loop 217 `parent_table` | parent table written in index order | index-relative link, with parents stored plus one so that 0 is the top level | `container` | `table[at] <= at` |
| comp 1 selector matcher | nested alternative ranges flushed before their parent's | index-relative link, possibly with a clamp as in loop 85 | position in the alternative order | nested range before the parent's own position; not verified: the parser stores ranges into instructions and flushes orders separately (`css/selectors/parse.wf`, `types.wf`), so whether the fact is local at the write is open |
| loop 490 `bfs_sparse` | a frontier list threaded through an index array | rewrite: a `Slots` stack | stack length | `take_back` (C3) |
| loop 234 `expand_cp` | decompositions read from an input file | input validation | expansions left | a bounded expansion count with an error for cyclic input |
| comp 8 `decompose` | a generated decomposition table | rewrite: the generator emits full decompositions | none (no recursion) | none |

Every row now has a form.
- **One row is not verified.** The selector matcher (comp 1) has a form,
  but whether its fact is local at the parser's write is open.
- **Two rows are rewrites that change the data structure.** A frontier
  stack replaces the threaded list, and full decompositions replace
  recursion. Neither adds work.
- **`expand_cp` must validate its input.** It reads an external file that
  can be cyclic, and the constitution requires defined behavior for
  expected input failures. The bound is an input check, not a proof
  stand-in: the checker cannot rule out a cyclic file, so the error outcome
  is live.

## Form 1: the Forest storage shape

This form covers the DOM sites.

**The shape.** `Forest<T>` is a storage shape beside `Slots`, `Ring` and
`Segments`. Each node holds one `T`. The shape owns every node's parent,
first-child, last-child, previous-sibling and next-sibling links; source
reads a link only through an accessor and writes one only through a link
primitive.

**Measures.** The measure table gains a row for a forest node place `f[n]`
with proof-only measures:
- `depth`, the distance to its root;
- `height`, the longest distance to a leaf below it;
- `before` and `after`, its siblings before and after it;
- `children`, its number of children.

**Proof-only.** No source expression reads these measures, and lowering
stores none of them. They are logical functions of the links, well defined
because the link primitives keep the links a forest. As with `len`, the
trusted party is the specification's record for the primitives, not a
library body.

**Accessors.** Each accessor publishes one descent fact:
- `forest_parent(f, n)` returns `Some(p)` with `f[p].depth + 1 == f[n].depth`.
- `forest_first_child(f, n)` returns `Some(c)` with
  `f[c].height < f[n].height` and `f[c].before == 0`.
- `forest_next_sibling(f, n)` returns `Some(s)` with
  `f[s].after + 1 == f[n].after`.
- `forest_previous_sibling(f, n)` returns `Some(s)` with
  `f[s].before + 1 == f[n].before`.

**Link primitives.** `forest_append(f, parent, child)` and
`forest_insert_before(f, parent, child, reference)` perform the cycle check
that the DOM standard requires of insertion:
- **Fast path.** A child with no children closes a cycle only by being the
  parent itself, so one comparison decides it in constant time. This keeps
  tree construction, which appends fresh nodes, constant-time per append.
- **Otherwise.** The primitive walks up from the parent, at most
  `f[parent].depth` steps, which is the walk Snowghost's `refuse_cycle`
  performs today.

A cyclic link is refused with an error. On success the primitive publishes
the changed measures:
- the old parent's `children` is one lower, provided the old parent differs
  from the new parent;
- the new parent's `children` is one higher.

**Kills.** Every link primitive writes `f`, so it kills every standing
measure fact about `f` [MSR-2], as a window write kills its references.

**Falsifier.** `move_all_children` (loop 28) appends `from_node`'s first
child to `to_node` until `from_node` has no child.
- Written with the rank `f[from_node].children`, its descent needs the
  primitive's "old parent differs" condition, so it proves only after
  `from_node != to_node` is established.
- As Snowghost wrote it, with no such fact, it is rejected. That is correct,
  since the loop never ends when the two nodes are the same.
- Any Forest design that accepts the loop unchanged is unsound.

**Cost.**
- Walks: no runtime work.
- Links: the cycle check the DOM standard already requires, constant-time
  for a fresh child.
- The Snowghost DOM would move its link fields into the shape. That is a
  migration of one module, not a cost the rule adds to the program.

## Form 2: an index-relative link

This form covers arenas built in index order.

**The form.** An element field may carry a relation to the element's own
index, stated once on the struct that owns the storage. The binder `at` is
the index:

    invariant backward(index, at): index.links[at].next <= at;

A read of element `i` publishes the relation with `at = i`. A write owes
it, as follows:

| Operation | Owes |
|---|---|
| element write at index `i` | the relation for `i` only, one comparison with a known index |
| `place_back` | the relation for the new index `len` |
| `take_back` | nothing |
| operations that move elements between indices (`insert_at`, `remove_at`, front operations, swapping two elements) | refused on such a storage |
| replacing the whole storage field (`set owner.links = move fresh`) | refused: it would owe the relation for every element of the new value, the refused quantified derivation, so a rebuild writes element by element into the existing storage |

The linked field is a `public readonly` element field, because [ENT-2]
clause (b) makes a subscripted field a term only when it is readonly. Its
every change is then an element write that the table covers.

**Why the refusal does not apply.** The tree refuses quantified storage
invariants because a write would owe a derivation over other elements. An
index-relative relation names one element and its own index. No write owes
anything about another element, and no read needs a derivation, so the
refusal's reason does not hold for this form. It is a proposal to admit a
narrower form, and it needs the owner's ruling (Q17).

**Cost.** The cost depends on what the writer knows when it writes a link.

- **Zero, when the written link's bound is a local fact.** In
  `parent_table`, the parent is an earlier position of a cursor that only
  rises, so the loop proves `container < at` with a scalar invariant.
- **One comparison per insertion, when the link comes from other storage.**
  In the attribute index, a new link's `next` is read from a `heads` array.
  That every head is at most the number of links is a fact over all of
  `heads`' elements, the refused quantified form. The insertion therefore
  clamps the head to its own index, once per insertion and never in a walk.
  The clamp never changes a value, and the checker cannot prove that it
  does not.

**Alternative.** The same three sites could use the Forest shape. It would
cost a constant-time check per link, and the hash chains and nested ranges
would have to be modeled as trees.

## Shared ranks for mutual recursion

The rule is:
- Every member of a recursive component declares a rank of the same arity
  in its contract.
- Every call inside the component proves the callee's rank, instantiated
  at the evaluated actual arguments, below the caller's rank at its own
  entry.

It is a caller-side obligation over the callee's declared rank, which is
part of the callable boundary. It needs no callee postcondition, so the
FN-9 rule that withholds same-component summaries stays as it is. A module
checks its own members' calls against the declared ranks of members in
other modules, which the interfaces carry, so the module-verdict rule keeps
holding.

## Ranks read from a constant table

The tree builder's measure is
`(templates, R[token class][mode], stack depth)`.

- **A table read denotes its element.** A `const` `Array` read at an index
  the checker knows exactly (a literal, or a value with an exact fact)
  would denote that element's value. It is a fixed evaluation of static
  storage with no search, but it is new, because probe t2 shows the
  checker does not know it today.
- **The mode as an index.** The mode is an enum, so the writer keeps its
  ordinal as a `u64` beside it, or derives it with a `match` whose arms
  give literals.
- **The template count.** It is a writer-maintained counter, and the end-of-file
  edge must prove it positive before it falls. It therefore needs the
  relation `templates <= open elements` as a type invariant of the parser
  state. The current type-invariant rule [TYPE-11] admits that form: the
  conformance case `type11-pos-by-value-parameter.wf` states
  `table.next < table.slots.len`.
- **Lexicographic comparison.** A rank of several components compares
  lexicographically. At each edge the checker proves, in order, that each
  component is unchanged or falls, and that one falls.

## Ranks derived from the exit guard

A loop needs no written rank when its exit test has one of these forms. The
exit test is the first `if` of the body with an arm that breaks the loop,
preceded only by `let` bindings. It continues when `c` in
`if c { ... } else { break; }`, and breaks when `c` in `if c { break; }`.

| Continuing when | Breaking when | Derived rank |
|---|---|---|
| `a < b` | `a >= b` | `b - a` |
| `a > b` | `a <= b` | `a - b` |
| `a != b` with a header fact `a <= b` | `a == b` | `b - a` |
| `x != 0` or `x > 0` | `x == 0` | `x` |

**What the derived rank owes.** The obligation is not that the rank stays
nonnegative at every header. A cursor may step past its bound, as loops 152
(`slot +wrap step`) and 170 (a step of 1 or 2) do, and the next exit test
then ends the loop. For `a < b`, each backedge owes only two things:

- `a` strictly rises;
- `b` does not rise.

The continuing values of `a` then form a rising sequence below `b`, which is
finite. A wrapped addition that falls below `a` fails the first
obligation, as it should. The other forms owe the mirror obligations. The
fixed-resource draft's header range proof, `0 <= R`, is not needed for a
derived rank.

**Operands.** The operands are what a local invariant admits [INV-1]:
- bare own-mode integer values;
- measure terms, including through a reference (`p^.len`);
- literals and const generics.

Contract clauses admit more [ENT-2]: fields reached through `^`, such as
`buffer^.index`, and readonly fields below a subscript. The sample's three
`buffer^.index` loops fail on exactly that difference ("an affine factor
reads a referent that is not a measure").

**Coverage in the sample.** By reading the exit test of each of the 20
sampled loops (`runs/sample/`), 19 have one of these forms, and the derived
rank is the measure the census recorded. The exception is loop 74, which
ends on an end-of-file sentinel character; it needs a written rank.

- Three of the 19 compare `buffer^.index`, which needs the vocabulary
  extension.
- The derived rank decides what must fall; the descent itself is proved as
  before. In the sample, 10 prove with no written step.

## Open choices for the owner

- **Q17.** Whether to admit the index-relative link form, a narrower
  relative of the refused quantified storage invariant, or to use the
  Forest shape for arenas built in index order as well.
- **Q18.** Whether to make `Forest` a specification-owned storage shape with
  proof-only measures, or to express it as a library type over existing
  storage. A library type cannot state proof-only per-node measures without
  the refused per-element facts.
- **Q19.** Whether a `const` array read at an exactly known index denotes
  its element in proofs.
- **Q20.** Whether local invariants and ranks admit the [ENT-2] terms that
  contract clauses already admit. These are fields through `^`, such as
  `buffer^.index`, and readonly fields below a subscript. They would keep
  the existing kill rules.
