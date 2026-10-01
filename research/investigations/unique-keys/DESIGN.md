# Indices a program knows are distinct

Status: language-gap investigation, with a worked library-forest model below.
The required direction is a user-defined structure with checked, erased
proofs. The proof language, invariant boundary and simultaneous assignment
remain proposals; no specification or compiler change is selected here.

## Question

A program often writes through indices it knows are distinct because its
own code produced them: every non-root node occurs exactly once in exactly
one parent's child sequence. Whitefoot proves the
independence of a counted loop's iterations from the places they write, and
an index read from storage is an integer it knows nothing about, so such a
loop is denied. How can a program carry the fact that a set of indices is
distinct from where it was produced to where it is used, so that the loop is
proved independent, with no check at run time?

The owner set three requirements for an answer (Snowghost's vocabulary
handoff, ruling on Q41, 2026-10-01):

- **R1. Any container.** The indices may live in an array, a hash map, an
  arena or a list of lists; the fact must not belong to one container type.
- **R2. No restricted container operations.** A container holding the
  indices keeps its ordinary operations; the mechanism does not ask for an
  append-only container or for one container per use.
- **R3. Nothing at run time.** No added branch, check, copy or word.
- **R4. Library-defined properties.** The writer can define and prove a
  Forest over ordinary storage, and extend its representation and properties
  for another application. A compiler-owned Forest is not the requested
  endpoint. This is the direction clarified during the investigation;
  the earlier candidates below remain comparisons, not selected mechanisms.

R2 preserves the ordinary operation vocabulary, not a false proposition:
duplicating an index cannot preserve a no-duplicates proof. An operation
must establish the invariant it promises, or cease to expose that guarantee.

## Minimal witnesses

- **W1, the cascade written back in document order.** `cascade.wf` in this
  directory: a counted loop over one node's children writes
  `results^[child]` for each child index read from the node's children
  list. The executable caller supplies distinct indices `[3, 1, 6, 2]`.
  With the compiler at `22d0923b`, `whitefootc --par --par-ledger`
  reports `PAR loop cascade.wf:10 loop denied condition 2: the body writes
  storage that is neither introduced by the iteration nor the accumulator`.
  In Snowghost this keeps the style stage's cascade a sequential pass in
  document order (`design/pipeline/style.md` there): a level's elements lie
  scattered in document order, so cascading a level in parallel writes
  through their indices.
- **W2, a map from one order to another.** Snowghost's layout builder
  numbers styles in document order and nodes in their own order; filling a
  per-node array from per-style results is the same write through indices
  that are a permutation by construction (Snowghost's concurrency record,
  "Box tree construction"). Its full construction-and-use witness remains
  to be written; this is a motivating use, not an executed positive case.

Both are written today either sequentially or through a level-ordered copy
gathered back into document order, which costs a copy per use and breaks R3.

## Constraints from the language as it stands

- **No quantified facts in automatic derivation.** The checker's derivation
  is fixed, deterministic and terminating; a fact about every element of an
  array is outside it. The segmented-storage investigation refused "a proved
  fact that an array of offsets does not decrease" on this ground
  ([rejected alternatives](../segmented-storage/DESIGN.md#rejected-alternatives)).
- **One overlap relation.** Every consumer separates places by one relation
  on complete paths: different roots, different fields, or proved distinct
  indices or ranges (`design/language/ownership.md`). A new source of
  distinct indices must enter that relation, not a loop-private clause.
- **Pools and arenas are ordinary storage with integer handles**
  (`design/language/ownership/pools-and-arenas.md`): the language refused
  arena and pool types with confined content, and compiler-maintained slot
  identity, because no memory-safety property needs them.
- **Kernel minimality.** The kernel owns a shape only when a fact about
  storage can be established by the storage alone, as `Segments` owns its
  boundaries.

### What earlier refusals cover and what is new

The segmented-storage investigation refused an offsets witness type with a
permission clause of its own, because `Segments` gave the same disjointness
from a storage shape the element rule already trusts. That need was
contiguous output per item, which a storage shape can own. W1 and W2 write
to positions scattered through the target, which no storage shape around
the target can describe, and the indices are produced by an algorithm (a
tree's construction, an ordering) rather than by the storage. The
pools-and-arenas refusal concerned memory safety; the property here is
proved independence for parallel work, which that decision did not weigh.
Any candidate below that adds a type or a rule says how it fits those
grounds.

## Candidates

**K. Keys minted once.** A key is an affine value, never copied, that only a
minting operation creates, each mint yielding a key no other mint of the
same source has yielded; a program reads a key's integer for ordinary use
but cannot forge a key from an integer. Two different places can then never
hold equal keys, so a write `results[key]` through keys read from two
different places writes two different positions. The tree in W1 keeps each
child's key in its parent's children list, the one place a node belongs,
and keeps parent and sibling links as plain integers.

- Potentially meets R1 and R2 through single ownership. Moving keys during
  traversal and copying their integer observations still require a precise
  proof-lifetime rule.
- A key can have the integer's representation. R3 also requires the mint,
  exhaustion and reuse protocol to add no state or work to the baseline;
  equal key sizes alone do not establish that.
- Adds: a key type, a minting operation, and one clause in the overlap
  relation ("two places holding keys of one source hold different keys").
- **The open problem is the source's identity.** Two mints of the same key
  type can yield equal integers, so the overlap clause must know that two
  keys share one source. Known answers: a type-level brand created fresh
  per source (Rust's generativity through invariant lifetimes, Haskell's
  `ST` and "ghosts of departed proofs"), which needs a form of existential
  or rank-2 generics Whitefoot lacks; or provenance the checker tracks per
  value, close to the origin discipline the language retired. Choosing
  between them does not settle the library-proof direction below.

**M. One structure that only grows.** A structure of several lists whose
only insertion appends a value greater than every value appended so far,
under a `requires value > max` on one scalar field. Its values are then
distinct across all its lists and increasing within each, facts the
structure's type carries. W1's children lists are built in one pass in
index order, each `append` proved by the loop's own counter.

- Needs no source identity: distinctness holds inside one structure.
- Breaks R1 and R2: one container kind, append only, and removal or
  reordering needs an answer of its own.
- Close to the refused offsets witness type.

**P. A partition owned by the kernel.** A storage shape whose content is a
partition of `0..n` into segments, built only by kernel operations (a stable
grouping by key, an inverse), so that writes through one segment's values,
or through all of them, are distinct and in bounds.

- Fits kernel minimality as `Segments` does, and needs no source identity.
- Breaks R1 and R2, puts a sort in the trusted base, and must be rebuilt
  when the tree changes.

**R. No language change: recursion over subtrees.** In document order every
subtree is a contiguous range, so cascading by recursion over children's
ranges is provable today. Snowghost's measured halving shape lost to the
runtime's fixed recursion budget on unbalanced trees (a four-worker speedup
of 1.11 against 3.62 for flat matching on apollo11); a budget that
follows the tree's shape would remove that limit. It answers W1 only where
the targets are subtree ranges, not W2.

**G. The workaround.** Compute in a level-ordered array and gather back
into document order in a second parallel loop. Provable today; breaks R3.

## Source identity for K

Single ownership gives "two places hold two different key values", not
"two different integers": keys from two sources of one key type can carry
the same integer. Whitefoot's identity for storage is the place, and a key
moved from one structure into another keeps no record of its source, so the
overlap clause needs the source in the key's type. What the language offers
and lacks:

- **Generics.** Type, const and function parameters on functions and
  nominals, instantiated explicitly and erased by monomorphization; a
  function-kind parameter's `fn_sig` takes no type parameters of its own,
  so no function is polymorphic in a type its caller does not name
  (no rank-2 types), and there are no existential types.
- **No global mutable state.** A source created at the program root and
  passed as an ordinary value does not violate this rule
  (`design/language/ownership.md`). Enforcing its uniqueness would still
  need a checked construction protocol; the global-state prohibition does
  not itself exclude that direction.

Three directions follow:

1. **A brand fresh per scope.** A block form introduces a type `B` that no
   other block shares, and a value whose type mentions `B` cannot leave the
   block, as Haskell's `runST` and Rust's generativity through invariant
   lifetimes do. Keys are `Key<B>`, sources `Source<B>`, and only arrays
   made for `B` accept `Key<B>`. `B` is phantom and erased. It costs one
   scoping construct and its escape check; a document kept across frames
   puts its frame loop inside the block.
2. **Provenance the checker tracks.** Each key value carries, as a proof
   fact, the place of the source that minted it, and storing it keeps that
   fact. It needs no new type form but brings back origin tracking through
   storage, which the language retired with stored references.
3. **Give up source identity.** Distinctness is promised only inside one
   structure (M) or one kernel shape (P), at the cost of R1 and R2.

Direction 1 is a candidate for isolating sources. It has not established
R1--R3 for the whole construction and mutation path, and it adds a type
scoping form the current generics do not provide. The library-proof model
below instead relates ordinary integers to the particular storage state
being proved, without selecting brands for the runtime index type.

## Worked derivation: a library Forest

This section identifies necessary capabilities by following operations,
rather than treating a name such as `forest_valid` as an assumed theorem.
All mathematical forms and assignment sketches in this section are proposals,
not accepted WF syntax, apart from the explicitly runnable Pair probe.
The model is one way to expose the obligations, not a selected representation
for either runtime data or proof terms.

### What "the invariant always holds" would require

[TYPE-11](../../../spec/kernel-spec.md) currently checks direct construction,
direct struct parameters/results, written-reference exits and shared-object
boundaries. It admits only non-generic, non-opaque structs and difference
bounds over selected fields and measures. It does not promise the invariant
at every statement boundary or transport it through every nested store/read.
The current limitation is already recorded under "Type invariants stop at
the direct struct type" in [the TODO](../../../docs/todo.md).

This complete current-WF probe demonstrates the difference:

```wf
struct Pair {
  left: u64;
  right: u64;
  invariant equal(pair): pair.left == pair.right;
}

fn restore(value: &u64) -> result: unit writes(value) contract {
  ensures value^ == entry(value)^;
} {
  let old = value^;
  set value^ = 1_u64;
  set value^ = old;
  return unit;
}

fn main() -> status: std::process::ExitStatus pure {
  let pair = Pair(left: 0_u64, right: 0_u64);
  restore(value: &pair.left);
  return std::process::exit_status(code: 0_u8);
}
```

The caller starts and ends with `(0, 0)`, but after the first store inside
`restore` the stored pair is `(1, 0)`. A postcondition about the final value
cannot establish preservation at each intermediate source statement. Adding
tuple assignment does not change this program's behavior or admission.

A stronger, still-unselected rule could require every live invariant-bearing
value to satisfy its invariant at each source commit boundary. Its proof
would be induction over program transitions, with these obligations:

- Construction establishes every applicable invariant, including nested
  values; generic storage admits only valid elements.
- Every write, storage operation, exchange, replacement and move preserves
  the invariants of affected enclosing values. Updating `f.nodes[i].links`
  owes the invariant of `f`, not only that of `links`.
- A simultaneous assignment is one transition: evaluate/capture the targets
  and RHS safely, establish the final state, then commit. Prove target
  separation and ordinary ownership transfer; it exposes no partially
  committed state to user code. This is not cross-thread atomicity.
- Passing a writable interior reference cannot forget its enclosing
  invariant. Ordinary final-state `ensures` is insufficient, as `restore`
  shows. Either the call boundary carries a compositional preservation
  obligation, or such writable projections are unavailable and helpers
  compute replacements for an owner-level commit. This choice is open.
- A read can obtain an invariant only because every creation/mutation route
  established it. Moves transport its state-relative proof, copies establish
  it for the copied value, and replacement invalidates old observations.
- All exits, including propagation, and shared-object publication preserve
  the chosen boundary. Existing effects and ownership must prevent another
  context or callback from observing a partial update.

Another possible boundary is validity whenever a complete value is handed
to an observer, with explicit checked intermediate states inside an update.
That is weaker than validity at every source statement. Tuple assignment
makes many fixed-size link updates compatible with the stronger boundary;
it does not choose between these two meanings or solve interior-reference
composition by itself. The source rule must settle this before an invariant
is freely assumed on any read.

### Runtime representation and an independent finite model

Use the actual representation pressure from Snowghost
([DOM interface](https://github.com/mbbill/Snowghost/blob/7503e37b887b79da66a7c2161551fc1c9175df6a/renderer/dom/module.wfm#L46)):
ordinary `NodeId` integers, an array/prefix window of nodes, and `parent`,
`first_child`, `last_child`, `previous_sibling`, `next_sibling` links.
Its parser also reparents existing nodes and moves children
([adoption](https://github.com/mbbill/Snowghost/blob/7503e37b887b79da66a7c2161551fc1c9175df6a/renderer/html/tree_builder/adoption.wf#L329)).
Node payloads are independent of the topology. Detached nodes and their
descendants remain allocated, so the model must be a forest, not one tree.

A candidate proof witness is an ordinary mathematical finite tree:

```text
Tree   = Node(id, finite sequence of Tree)
Forest = finite sequence of Tree
```

These are erased logical values. They are not additional runtime objects,
an index cache or a traversal buffer. Library definitions over this model
recurse on proper subtrees/sequences, not on unverified runtime links.

Define `Rep(f, M)` by:

1. Flattening model M visits every integer in `0 .. f.nodes.len` exactly
   once. The library predicate includes both coverage and no duplication.
2. Each actual node's five links equal the links induced by its position in
   M: parent, ordered child endpoints and neighboring children. Model roots
   have no parent or siblings; the model's ordering of roots is ghost only.
3. No payload field is read by this relation.

The proposed struct invariant means `there exists a finite M: Rep(f, M)`.
Construction supplies that witness and mutation constructs its successor.
An implementation could carry an erased model explicitly or hide it behind
an existential predicate; the representation of proof state is not selected.
Callers need a sound rule for opening and repackaging the existential.

For the running example the model is:

```text
nodes: [div, section, article, span, img, p, a]
M = [Node(0, [Node(1, [Node(3, []), Node(4, [])]),
              Node(2, [Node(5, []), Node(6, [])])])]
```

It now makes sense to define height, depth, child count and remaining
siblings by total structural recursion over M. Defining `height(f, n)` by
recursing on arbitrary raw links before proving acyclicity would be circular.
Bounds alone do not prove a forest: nodes 1 and 2 can point to each other
while every index remains in bounds. Unique parents alone also allow cycles.

The parked [termination proposal's Forest](https://github.com/mbbill/Whitefoot/blob/5e33f2930b596b19eb1b4ee6e3f5d7ce1f1a7d8f/research/investigations/termination/ARENA.md#form-1-the-forest-storage-shape)
provided such measures through trusted link primitives. Here their definitions
and accessor lemmas must be supplied and checked in the library. That record
is useful prior design evidence, not an implemented capability or a source
of axioms for this proposal.

### Empty construction, growth and a fresh node

An empty storage has model `[]`; both parts of Rep hold vacuously. For a
storage of length N with model M, append a node numbered N with five empty
links and arbitrary valid payload, and use `M' = M ++ [Node(N, [])]`.

The proof has concrete steps: existing model labels are below N; N is
therefore fresh; the new flattening covers exactly `0 .. N+1`; old links
are unchanged; the new node's links describe a detached root. Reallocation
may change addresses but must preserve every existing element value and
logical index. No source reference survives a destructive reallocation
unless the existing reference rules permit it.

This exposes a missing contract capability. Length postconditions alone do
not say what `place_back` wrote or that `grow` kept the old elements. The
proof layer needs the logical content transitions of primitive storage, and
source generic containers need to publish and prove equivalent content
contracts. Otherwise the proof stops at the first library push even when
the bounds and capacities are known. The standard-library implementation
must establish its contract; naming it in an interface establishes nothing.

### Detach, attach and reparent

First detach a middle child x between a and b of p. In this case a, b and x
are distinct by the old model. The runtime update is:

```text
set (nodes[a].next, nodes[b].previous,
     nodes[x].parent, nodes[x].previous, nodes[x].next)
  = (Some(b), Some(a), None, None, None);
```

The model update removes the whole subtree rooted at x from p's child
sequence and appends that subtree to the model's roots. Its descendants and
payloads remain unchanged. The library proves:

- the same labels still occur exactly once and every slot remains covered;
- the five written fields match the new model;
- every other link field is unchanged and still matches it.

For a first child, last child or only child, the statement also updates p's
corresponding endpoints and omits a nonexistent neighbor. These are finite
source branches. They need separate checked target lists, not a tuple with
conditionally invalid targets. Source tuple arity is finite and static.

Attaching a detached root x before child y of p removes x's subtree from
the model roots and inserts it in p's child sequence. Require p to be outside
x's subtree, with x itself included in that subtree. Unique labels and this
condition make the model edit well founded and preserve the forest. The
runtime commit writes the new neighboring links, x's parent and p's affected
endpoints, with the same changed-field/unchanged-field proof as detach.

For a fresh leaf N, the condition follows from old labels being below N.
For an arbitrary existing subtree, it must come from a verified caller fact
or the existing DOM cycle-refusal walk. A successful walk must publish the
logical non-ancestry fact: its loop proof relates the cursor to the remaining
ancestor path and records that every inspected ancestor differs from x.
This uses the program's existing validation, not a new scan inserted to
satisfy the proof system. Whether all existing guard results can be
transported at equal runtime cost is still unverified.

Reparenting can validate first, detach in one invariant-preserving commit,
then attach in another. The detached state is itself a valid forest, so no
broken Forest need cross that boundary. Same-parent relocation and insertion
before oneself need their own ordinary cases: a blanket increment of the
new parent's child count is false when the old and new parent are equal.
Aliases among old/new neighbors must be resolved or proved apart before
forming a tuple's target list. Every failure exit must satisfy the advertised
invariant; whether failure leaves the original tree unchanged is a separate
API promise that must also be proved if made.

For `move_all_children(from, to)`, each successful iteration detaches and
attaches one subtree. Its ghost rank, length of from's model child sequence,
decreases only when `from != to`. With equal parents a nonempty sequence can
be rotated forever. This is a falsifier, not a condition that the forest
invariant alone could prove. A finite tuple handles each link edit; it does
not replace this data-dependent loop.

### The finite proof operations these steps actually use

The derivation needs user definitions and checked lemmas, not a compiler
intrinsic named `Rep`, `NoDup`, `descendants` or `Forest`. In particular:

- finite logical sequences/inductive data, mathematical measures and total
  structural definitions;
- bounded universal introduction and explicit instantiation, existential
  witness introduction/elimination, Boolean case reasoning and equality
  substitution;
- explicitly checked structural induction, and loop invariants relating
  current data to the logical model; a recursive proof call needs a checked
  smaller argument, not merely a finite function body;
- read-after-write and unchanged-content equations for storage operations,
  with old and new logical states, and extensional reasoning for unchanged
  fields/elements;
- module-visible predicates and lemma signatures, with verified bodies and
  dependencies, so clients need not reopen a mutable representation.

The append proof splits an arbitrary index into `i < N` and `i == N`. The
detach proof splits an arbitrary field into the finite written set and its
complement. Structural induction proves that extracting/reinserting a
subtree preserves the model's other labels and links. These are finite
certificates over symbolic data, not compile-time enumeration of the runtime
tree. A source declaration or proof name never supplies an unchecked axiom.
Automatic derivation can remain fixed while explicit rules are extended;
termination of this checker still requires specified definition admissibility
and induction rules. No such calculus has been implemented or proved sound
here. This reopens the current refusal of quantified storage facts; it is
not expressible by the existing weighted-affine PRF-1 certificate alone.

### Moving proof state and preserving unrelated facts

A theorem about f at state S is not a theorem about arbitrary future f, or
about another document with the same integer indices. The proof layer must
capture reads as values in S and relate S to each verified successor state.
Moving the owner transports its invariant with the moved value; it does not
retarget existing runtime references. An old observation retains its old
meaning and cannot silently become a fact about a replacement node.

Rep depends on topology and node extent. Editing a payload should preserve
it. Relinking changes it and requires the new model. Shrinking, reordering
or reusing slots requires the corresponding remapping proof; no operation
gets preservation merely because it belongs to an ordinary container. A
function returning an arbitrary index list must prove that list's contents
and no-duplication, even if it obtained every entry from a valid Forest.
These support and content contracts are essential for R1 and R2, not optional
optimizer precision. Broad `writes(document)` by itself supplies neither
the changed fields nor the content relation needed to retain Rep.

### From model to traversal, termination and parallelism

For a sibling walk, the loop's proof carries a suffix S of the fixed model
child sequence. `first_child` yields its head; `next_sibling` advances to its
tail. The accessors must prove those facts from Rep and their actual loads.
Length(S) decreases, so the walk cannot revisit a node while topology stays
unchanged. The proof is erased; no runtime suffix or rank is stored.

For counted scatter over an actual index array, the contract must relate
that array's captured contents A to a sequence with no duplicates. Given
two distinct iteration positions i and j, explicit instantiation proves
`A[i] != A[j]`; each target's bounds are proved separately. This feeds the
shared OWN-7 overlap judgment. PAR-2 also needs a specified way to consume
that cross-iteration proof: its existing affine-element family does not
admit it. A false claim about duplicate indices cannot authorize parallelism;
ordinary source with no claimed proof may still be accepted sequentially.

Every write/read pair across iterations still needs separation where either
writes, including hidden callee effects and aliasing actual arguments. The
captured sequence and target layout must remain stable for the permitted
overlap. A proof of write/write separation alone is insufficient.

For recursive subtree work, a function must be able to state a footprint
such as `results at labels(subtree(M, root))`. Different sibling subtrees
have disjoint labels by NoDup(M); distinct nodes alone do not suffice when
one is an ancestor of the other. Current scalar/range effect paths and a
whole-root `writes(results)` cannot express this footprint. A general
logical-index-set effect, or another representation with the same checked
meaning, is a separate needed capability for this case. A source-contract
body must prove its actual accesses stay within that footprint.

Linked-list discovery remains dependent on reading the previous link. A
proof does not create random access, an index buffer or a task schedule.
Permitting independent subtree calls and efficiently scheduling a sibling
walk are distinct questions. Forest descent also does not establish progress
of the HTML token-reprocessing state machine. A runtime termination judgment
would need a consumer for the supplied ranks, separately from termination
of proof checking; PR #199 has not landed that judgment.

### Capability ledger and discriminating examples

| Capability absent or incomplete today | Where the worked derivation stops without it | Required negative case |
|---|---|---|
| Generic and recursively contained struct invariants, with a complete mutation/observation boundary | `Forest<T>`, stored invariant-bearing values, and a write through an interior reference | `restore(&pair.left)` cannot count as preserving the invariant at every internal commit merely from its exit contract |
| Simultaneous multi-place assignment and its invariant/ownership judgment | A bidirectional detach changes several links together | Repeated/overlapping targets and an omitted reverse link cannot pass the proposed final-state proof |
| User-defined logical predicates, finite models and checked structural lemmas | Defining Rep, proving no duplication and deriving non-ancestry | A cyclic raw graph cannot acquire height by assuming its own well-foundedness; a circular lemma cannot prove itself |
| Relational content contracts, result/entry snapshots and generic-library transport | First push/grow, next-sibling accessor, cycle-walk success, and a returned index array | Correct lengths with a duplicated or changed old element must not satisfy the content contract |
| Proof-state support, framing and move/store transport | Keep topology facts across payload changes; replace them after reparent | Reuse an old sibling or document proof after changing its supporting links |
| Shared separation facts and a cross-iteration proof consumer | Counted scatter with a proved injective index sequence | Distinct writes beside a cross-iteration conflicting read must deny permission |
| Noncontiguous effect footprints and their body checks | Parallel recursive subtree helpers | Two distinct roots where one is a descendant must not count as disjoint subtrees |

The first six reach the counted child-index witness; the seventh is needed
for recursive subtree calls. New runtime rank consumers and a linked-walk
scheduling strategy are additional consumers, not prerequisites for a checked
Forest value. A permutation mapping (W2) should reuse the same sequence
proofs and content contracts without any forest-specific checker rule.

Still unverified: an exact proof grammar and checking calculus, modular
interior-write preservation, all mutation/error paths of the actual SG
Forest library, proof-checking cost, and generated-code equality under R3.
The finite-model witness is a constructive paper route, not evidence that
this whole capability set has already been implemented or minimized.

### Observations with the current compiler

Compiler source: `22d0923bdf5ce7dbf4752cf9a302dc4154254869`, built with
`make -C compiler build`. No compiler source differs on this research branch.
Each probe uses that build with `--check`; each scatter uses
`--par --par-ledger --emit-llvm`.

- The runnable Pair probe above exits 0. Removing `set value^ = old;`
  exits 1 with FN-9 `UndischargedPostcondition`. Changing the construction
  to `(1, 0)` exits 1 with TYPE-11 `UndischargedTypeInvariant`. Thus the
  positive result does not mean the declared invariant is ignored everywhere.
- Both the original caller filled with four zeros and the corrected
  distinct caller of `cascade.wf` compile with exit 0 and deny the loop's
  parallel permission at condition 2. The original was a duplicate-write
  example, not a distinct-by-construction witness. The retained source now
  uses distinct indices; restoring the zero fill supplies the negative
  data case for future permission experiments.
- The proof/model/tuple sketches above are not compilable in current WF.
  No performance experiment or full gate was rerun for these observations.

## Criteria

Recorded before any prototype; a candidate is proposed to the owner only if
it meets every one:

1. **Expressible.** W1 and W2 are written so that `--par-ledger` permits
   the scatter loop, with no runtime check or copy in the loop beyond those
   of the sequential program (LLVM inspected for added branches).
2. **Sound.** A false distinctness claim is rejected; an otherwise legal
   sequential duplicate-write program receives no independence permission.
   For K, cross-source confusion must not authorize separation.
3. **Within the language's derivation.** No quantified fact enters automatic
   derivation, and the overlap relation stays one relation.
4. **No cost elsewhere.** Unaffected programs and cases keep their verdicts
   and runtime behavior; investigate any changed generated code. A newly
   selected invariant rule may deliberately change a verdict only through
   a stated specification amendment and matching evidence, never by silently
   weakening a requirement or rewriting an expected result to go green.
5. **The owner's requirements.** R1, R2 and R3, each judged on the
   witnesses and on one further container from R1 (indices held as hash map
   values), across construction, mutation and use. R4 additionally requires
   a second user-defined property or representation without a new kernel
   container or a trusted user axiom.

## Plan

1. Settle the invariant observation/mutation boundary, including writable
   interior references; simultaneous assignment alone does not settle it.
2. Specify the finite proof operations needed by the worked derivation and
   write explicit certificates for empty/create/detach/attach and one
   traversal. Keep source identity for K as a comparison, not a prerequisite.
3. Connect the resulting separation theorem to one counted scatter and the
   permutation witness, then address subtree effects separately.
4. Bring concrete specification sketches and their limitations to the owner
   before any compiler change. Record a selected mechanism in the design tree.
