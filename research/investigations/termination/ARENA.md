# Forms for the acyclicity sites, shared ranks and table ranks

This record designs the forms the [Q16 rulings](DESIGN.md#owner-rulings)
call for:
- a ranked arena for the sites whose termination rests on acyclicity;
- shared ranks for mutual recursion;
- ranks read from a constant table;
- ranks derived from a loop's exit guard.

It changes no specification or compiler. Every form must keep checking
deterministic, add no search, and add no runtime work to a walk. Where a
site has no complete form, this record says so.

## Constraints from the current language

The design tree refuses or narrows several routes
(`design/language/checks-and-proofs.md`):

- **Storage-element facts.** A decision keeps "no quantified storage-element
  or per-slot occupancy facts" and refuses "element invariants that must be
  instantiated at each read and re-established at each write". A rejected
  alternative refuses "quantified invariants over the elements of a
  storage", because "every write into the storage would owe the quantified
  fact again, and establishing it would require a derivation over elements
  that no terminating fixed family performs".
- **Cross-value type invariants.** It refuses type invariants relating a
  value to another value, such as "an arena node's index into another
  table".
- **Field atoms in loop invariants.** It refuses these "because a loop can
  carry the fields as scalars and construct the value after it; reopen when
  a program needs a loop-carried struct field in an invariant".
- **Calls.** Calls are excluded from contract expressions.

The mechanisms that can carry the needed facts:

- **Measure terms.** `P.len`, `P.cap` and `P.head` are logical quantities
  that the specification's measure table owns [MSR-1]. The operations of a
  shape publish their values: `take_back` ensures
  `window^.len + 1_u64 == entry(window)^.len`, and probe p5 proves a
  shrinking loop from exactly that fact.
- **Contract terms.** A contract clause admits fields reached through `^`
  and, below a subscript, readonly fields [ENT-2]. A local invariant admits
  less: bare own-mode integer values, measure terms and literals [INV-1].

**Constant arrays.** A constant `Array`'s element values are unknown to the
checker (`runs/probes/`):
- t2 reads with a variable index and is unproved;
- t3 reads `rank_table[0_u64]` and `rank_table[2_u64]` and is unproved;
- the control t4 states the same values as literals and is proved.

## The nine sites

| Site | Structure | Form | State |
|---|---|---|---|
| comp 17 `build_element` | DOM | Forest, lexicographic rank | complete form, given the Forest measures below |
| loop 28 `move_all_children` | DOM, relinked freely | Forest | complete form, given a parent-index measure; rejected unless `from_node != to_node` is established |
| loop 347 `match_complex` | DOM, backtracking over a stack of up to 64 frames | none | open: its measure orders a sequence of per-frame positions |
| loop 85 `attribute_is_duplicate` | hash chains in a links storage | index-relative link | partial: the insertion goes through a generic helper that cannot owe the relation |
| loop 217 `parent_table` | parent table in index order | index-relative link | partial: the table also stores two sentinels above every index, so it needs a re-encoding |
| comp 1 selector matcher | nested alternative ranges | index-relative link across two storages | open: the fact relates an instruction's range to another storage's order |
| loop 490 `bfs_sparse` | frontier list threaded through an index array | none | open: a stack rewrite adds an allocation, which the program's own documentation chose to avoid |
| loop 234 `expand_cp` | decompositions read from an input file | input validation | complete: a cyclic file is a live input failure, so a bounded expansion with an error is required behavior, not a proof stand-in |
| comp 8 `decompose` | generated decomposition table | rewrite | complete: the generator emits full decompositions, so the runtime needs no recursion |

The table has four complete rows (17, 28, 234, 8), two partial rows (85,
217) and three open rows (347, comp 1, 490), so criterion 1 is still not met.

## Form 1: the Forest storage shape

This form covers the DOM sites.

**The shape.** `Forest<T>` is a storage shape beside `Slots`, `Ring` and
`Segments`. It owns every node's parent, child and sibling links. Source
reads a link only through an accessor and writes one only through a link
primitive.

**Measures.** The measure table gains a row for a node place `f[n]` with
proof-only measures:
- `depth` and `height`: the distance to its root, and the longest distance
  to a leaf below it;
- `parent_height`: its parent's height, or its own height plus one at a
  root;
- `before` and `after`: its siblings on each side;
- `children`: its number of children;
- `parent_index`: the index of its parent.

**Proof-only.** No source expression reads these measures, and lowering
stores none of them. They are logical functions of the links, well defined
because the link primitives keep the links a forest. The trusted party is
the specification's record for the primitives, as it is for `len`.

**Accessors.** Each publishes facts about its result:

| Accessor | Result | Facts |
|---|---|---|
| `forest_parent(f, n)` | `Some(p)` | `f[p].depth + 1 == f[n].depth`; `f[n].parent_index == p` |
| `forest_first_child(f, n)` | `Some(c)` | `f[c].parent_index == n`; `f[c].parent_height == f[n].height`; `f[c].before == 0` |
| `forest_next_sibling(f, n)` | `Some(s)` | `f[s].parent_height == f[n].parent_height`; `f[s].after + 1 == f[n].after` |
| `forest_previous_sibling(f, n)` | `Some(s)` | the mirror, over `before` |

Every node also has the standing fact `f[n].height < f[n].parent_height`.

**New machinery.** A fact such as `f[p].depth` uses the accessor's result
payload `p` as a subscript offset. [ENT-2] admits only a tracked place or a
constant there, and [FN-9] states a result's measures only for a formal
place. Publishing measures at a result offset is therefore new (Q21).

**Link primitives.** `forest_append(f, parent, child)` and
`forest_insert_before` perform the cycle check that the DOM standard
requires of insertion:
- **Fast path.** A child with no children closes a cycle only by being the
  parent itself, so one comparison decides it. Tree construction appends
  childless nodes, so it keeps a constant-time check per append.
- **Otherwise.** Moving a node that has children walks up from the parent,
  at most `f[parent].depth` steps, as Snowghost's `refuse_cycle` does today.

A cyclic link is refused with an error. On success the primitive ensures:
- the old parent's `children` is one lower, when
  `entry(f)[child].parent_index` differs from `parent`;
- the new parent's `children` is one higher.

Every link primitive writes `f`, so it kills the standing measure facts
about `f` [MSR-2].

**Comp 17.** `build_element(n)` calls `build_children` on `n`'s first child,
which calls `build_element` on each child and itself on the next sibling.
The shared rank is lexicographic:

| Function | Rank |
|---|---|
| `build_element(n)` | `(f[n].height, 1, 0)` |
| `build_children(c)` | `(f[c].parent_height, 0, f[c].after)` |

Each edge falls:
- **element to children.** `parent_height` equals `height`, and the middle
  component falls from 1 to 0.
- **children to element.** `height` is below `parent_height`.
- **children to next sibling.** `parent_height` is unchanged, and `after`
  falls.

**Loop 28 (falsifier).** `move_all_children(from, to)` appends `from`'s
first child `c` to `to` until `from` has no child.
- `forest_first_child` gives `f[c].parent_index == from`.
- `forest_append` lowers `children` of that old parent only when it differs
  from `to`.
- So the rank `f[from].children` falls only after `from != to` is
  established. The census found that both callers pass distinct nodes, one
  of them a node created just before the call; each caller must state the
  fact.

As written in Snowghost, with no such fact, the loop is rejected. That is
correct, since it never ends when the two nodes are the same.

**Cost.**
- Walks: no runtime work.
- Links: the cycle check the DOM standard already requires, constant-time
  for a childless node and up to the parent's depth for a node with
  children.
- The measures are not stored.

**Loop 347.** Its measure is the sequence of the frames' last-tried
positions, compared bottom to top, and then `i`. A per-frame term such as
`f[stack[k].last_tried].before` uses an element read as an offset, which
[ENT-2] refuses because the offset "could come to select another element
with no event killing the term". A fixed-length lexicographic rank cannot
express a comparison over a stack of frames. The loop stays open:
- Its backtracking could be restructured as recursion, one call per frame,
  where each call's rank is the Forest depth and sibling position of its
  frame.
- Otherwise it needs a rank over a sequence, which is not designed.

## Form 2: an index-relative link

This form covers arenas built in index order.

**The form.** An element's readonly field may carry a relation to the
element's own index, stated once on the struct that owns the storage. The
binder `at` is the index:

    invariant backward(index, at): index.links[at].next <= at;

The field is `public readonly`, because [ENT-2] clause (b) makes a
subscripted field a term only when it is readonly.

**How it differs from the refused form.** The refused element invariant
must be "instantiated at each read and re-established at each write", and
establishing a quantified one needs "a derivation over elements". This form
differs in the second point only:
- It is still instantiated at each read. A read of element `i` publishes
  the relation with `at = i`.
- Each write owes the relation at one known index and no fact about any
  other element.

Whether per-read instantiation is acceptable is the question the decision
already answered "no" to, and Q17 asks the owner to revisit it for this
narrow form.

**What each write owes.** The form is usable only if every write path either
names one index or is refused:

| Operation | Owes |
|---|---|
| an element write at index `i` | the relation for `i` |
| `place_back` | the relation for the new index `len` |
| `take_back` | nothing |
| `insert_at`, `remove_at`, `append`, `split_off`, front operations | refused: they move runs of elements between indices or storages |
| `grow` | refused, unless it is shown to keep every index |
| `swap` of two elements, or of fields of two elements [OP-11] | refused |
| constructions that fill every element with one value | refused, or owe the relation for every index, which is a derivation over elements |
| replacing the whole storage field | refused |
| a write through a reference to an element, a sub-path or a range [REF-4] | refused: its index is relative to the reference, not to the storage |
| passing the storage to a generic callee that writes it | refused: a callee generic in its element type cannot state the relation in its postcondition |

**Loop 85.** Snowghost inserts through the generic helper `bytes_push`, which
calls `grow` and `place_back` and can drop the value. Under this form the
attribute index needs its own insertion function. The head a new link
stores comes from a separate `heads` storage. That every head is at most
the number of links is a fact over all of `heads`' elements, so the
insertion clamps the head to its own index. That is one comparison per
insertion, which never changes a value and which the checker cannot prove
redundant.

**Loop 217.** The table stores `top_level` (`u64::MAX`) and `blocked_start`
(`u64::MAX - 1`) beside parent indices, and its elements are plain `u64`.
- The relation `table[at] <= at` holds for neither sentinel.
- A subscripted plain `u64` is not a readonly field.

The table needs an element struct with a readonly field and an encoding
that keeps the sentinels outside the related field, such as a separate kind
field. Its cost is not measured.

**Comp 1.** The needed fact relates an instruction's nested range, stored in
one storage, to an alternative's position in another. That is not a
relation between an element and its own index, so this form does not
cover it.

**Alternative.** These sites could also use the Forest shape.
- For a childless node, the link check is one comparison.
- The attribute index relinks nodes that already have children, which
  takes the walk path.
- Hash chains and nested ranges would have to be modeled as trees.

## Shared ranks for mutual recursion

**Candidate rule.**
- Every member of a recursive component declares a rank of the same arity
  in its contract.
- Every call inside the component proves the callee's rank, instantiated
  at the evaluated actual arguments, below the caller's rank at its own
  entry.

**What fits.** The obligation reads only the callee's declared rank, which
is part of its callable boundary. It needs no same-component postcondition,
so the rule that withholds same-component summaries [FN-9] is unchanged.

**Two consequences to settle (Q22):**
- **No member summaries.** A descent fact that one member's postcondition
  would supply to another stays unavailable inside the component, because
  those summaries are withheld. The rank's descent must follow from the
  caller's own facts about the arguments.
- **Function-kind edges.** The module-verdict decision treats a generic
  instance as calling every function-kind actual it names, with no call
  site in the module's bodies. Such an edge closes a component only at
  instantiation. Where its rank obligation is judged, at the instance or at
  the module that supplies the actual, is not designed.

## Ranks read from a constant table

The tree builder's measure is
`(template-mode stack length, R[token class][mode], open-element stack depth)`.

- **A table read denotes its element.** A `const` `Array` read at an index
  the checker knows exactly (a literal, or a value with an exact fact) would
  denote that element's value. It is a fixed evaluation of static storage
  with no search, and it is new (probes t2 and t3, Q19).
- **The mode as an index.** The mode is an enum, so the writer keeps its
  ordinal as a `u64` beside it, or derives it with a `match` whose arms give
  literals.
- **The template component.** It is the template-mode stack's length, a
  measure, rather than a count of template elements on the open-element
  stack.
  - The reprocess record measures templates on the open-element stack
    because that count needs no invariant. It notes that the template-mode
    stack depth works under the unenforced relation "templates on the stack
    ≤ template-mode depth".
  - With the length as the component, the end-of-file reprocess edge owes a
    fall. It therefore needs a guard that the template-mode stack is
    nonempty: today it is guarded by a template on the open-element stack,
    and its pop does nothing on an empty template-mode stack.
- **Lexicographic comparison.** At each edge the checker proves, in order,
  that each component is unchanged or falls and that one falls.

## Ranks derived from the exit guard

**The exit test.** A loop's exit test is the first `if` of its body with an
arm that breaks the loop. The test must satisfy two conditions:
- **Conditions.** Its condition is a comparison, or a Boolean `let` bound
  directly to one.
- **Preceding statements.** Only `let` bindings that write no operand of the
  comparison may precede it.

A compound condition, such as `band(closing, nested)`, falls outside the
forms and needs a written rank.

**The forms.**

| Continuing when | Breaking when | Operand that must move | Operand that must not rise |
|---|---|---|---|
| `a < b` | `a >= b` | `a` rises | `b` |
| `a > b` | `a <= b` | `a` falls | none; `b` must not fall |
| `a != b` with a header fact `a <= b` | `a == b` | `a` rises | `b` |
| `x != 0` or `x > 0` | `x == 0` | `x` falls | none |

**The obligation.** It runs from one evaluation of the exit test to the
next. On every path that continues past the test and returns to it, the
operands at the next test compare to those at this test as the table says.
The continuing values then form a strictly monotone sequence bounded by the
other operand, which is finite.
- A wrapped addition that falls below `a` fails the obligation, as it
  should.
- A cursor that steps past its bound, as loops 152 (`slot +wrap step`) and
  170 (a step of 1 or 2) do, meets it, and the next test exits.
- The fixed-resource draft's header range proof, `0 <= R`, is not needed.

**Operands.** The operands are what a local invariant admits [INV-1].
Contract clauses admit more [ENT-2]: fields reached through `^`, such as
`buffer^.index`, and readonly fields below a subscript. The sample's three
`buffer^.index` loops (172, 173, 181) fail on exactly that difference ("an
affine factor reads a referent that is not a measure").
- The tree refused field atoms in loop invariants until "a program needs a
  loop-carried struct field in an invariant".
- These loops meet that condition. The field is reached through a
  reference parameter, and a callee the loop calls (`set_glyph`,
  `decompose_current`) writes it through that reference, so the loop cannot
  carry it as a scalar.

**Sample.** Reading the exit tests of the 20 sampled loops (`runs/sample/`),
19 have a tabled form with a comparison or a Boolean `let` bound to one.
Loop 74 has no break; it exits by `return`, so it needs a written rank.
- This is weak evidence. The census put these loops in C1 to C3 because
  their measures have these forms, so the count shows only that the exit
  tests are written in the recognized syntax.
- The sample script checks only the moving operand, so "must not rise" was
  not tested.

## Open choices for the owner

- **Q17.** Whether to admit the index-relative link form, which is
  instantiated at each read, against the decision that keeps no element
  invariants, with the write restrictions in Form 2's table. The
  alternative is to use the Forest shape for arenas built in index order as
  well.
- **Q18.** Whether to make `Forest` a specification-owned storage shape with
  proof-only measures, or a library type over existing storage. A library
  type cannot state per-node proof-only measures without the refused
  element facts.
- **Q19.** Whether a `const` array read at an exactly known index denotes
  its element in proofs.
- **Q20.** Whether local invariants and ranks admit the [ENT-2] terms that
  contract clauses already admit. This reopens the refused field atoms in
  loop invariants, whose reopening condition the three `buffer^.index`
  loops meet.
- **Q21.** Whether an accessor's postcondition may state measures at its
  result payload as a subscript offset, such as `f[p].depth`.
- **Q22.** How mutual-recursion ranks handle withheld member summaries and
  function-kind edges.

Still without a form: loop 347, comp 1 and loop 490.
