# Indices a program knows are distinct

Status: question, requirements, candidates and criteria, recorded before any
prototype. Nothing here is decided; the owner selects the direction, and its
surviving decision goes to the design tree.

## Question

A program often writes through indices it knows are distinct because its
own code produced them: each node of a tree has one parent, so the indices
in the children lists of a tree's nodes never repeat. Whitefoot proves the
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

## Minimal witnesses

- **W1, the cascade written back in document order.** `cascade.wf` in this
  directory: a counted loop over one node's children writes
  `results^[child]` for each child index read from the node's children
  list. With the compiler at `22d0923b`, `whitefootc --par --par-ledger`
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
  "Box tree construction").

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

- Meets R1 and R2: the fact rides on the key's type and on single
  ownership, which every container already respects.
- Meets R3: a key is its integer at run time.
- Adds: a key type, a minting operation, and one clause in the overlap
  relation ("two places holding keys of one source hold different keys").
- **The open problem is the source's identity.** Two mints of the same key
  type can yield equal integers, so the overlap clause must know that two
  keys share one source. Known answers: a type-level brand created fresh
  per source (Rust's generativity through invariant lifetimes, Haskell's
  `ST` and "ghosts of departed proofs"), which needs a form of existential
  or rank-2 generics Whitefoot lacks; or provenance the checker tracks per
  value, close to the origin discipline the language retired. Choosing
  between them, or finding a third, is this investigation's main work.

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

## Criteria

Recorded before any prototype; a candidate is proposed to the owner only if
it meets every one:

1. **Expressible.** W1 and W2 are written so that `--par-ledger` permits
   the scatter loop, with no runtime check or copy in the loop beyond those
   of the sequential program (LLVM inspected for added branches).
2. **Sound.** A variant of each witness that writes one index twice is
   rejected; for K, a key minted by another source of the same type is
   rejected at the write.
3. **Within the language's derivation.** No quantified fact enters automatic
   derivation, and the overlap relation stays one relation.
4. **No cost elsewhere.** Every maintained program and conformance case
   keeps its verdict and its generated code.
5. **The owner's requirements.** R1, R2 and R3, each judged on the
   witnesses and on one further container from R1 (keys held as hash map
   values).

## Plan

1. Survey source identity for K: generativity and branded indices (Rust's
   `indexing` crate and GhostCell), `ST`-style rank-2 brands, existential
   types, and checker-tracked provenance, each judged against Whitefoot's
   generics and its retired origin discipline.
2. Write each candidate as a specification sketch with W1 and W2 in it, and
   the criteria's verdict on paper.
3. Bring the sketches to the owner as decision cards before any compiler
   change.
