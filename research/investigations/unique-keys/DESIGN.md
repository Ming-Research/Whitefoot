# Indices a program knows are distinct

Status: language-gap investigation, with a worked library-forest model below.
The required direction is a user-defined structure with checked, erased
proofs. The proof language remains a proposal; no specification or compiler
change is selected here. The current derivation uses boundary contracts.
Stronger lifetime validity and its surface spelling are deferred questions,
not prerequisites for proving the Forest.

## Question

A program often writes through indices it knows are distinct because its
own code produced them: every non-root node occurs exactly once in exactly
one parent's child sequence. Whitefoot proves the
independence of a counted loop's iterations from the places they write, and
an index read from storage is an integer it knows nothing about, so such a
loop is denied. How can a program carry the fact that a set of indices is
distinct from where it was produced to where it is used, so that the loop is
proved independent, with no check at run time?

The owner initially set three requirements for an answer (Snowghost's vocabulary
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
- **R5. Deferred lifetime-strengthening question.** Earlier discussion asked
  for validity of every live struct value, including during mutation. After
  distinguishing boundary proof obligations from continuous validity, the
  owner deferred that question and returned to the Forest proof. The stronger
  alternative is retained below, but does not gate this derivation. Neither
  a stronger rule nor an invariant-check exemption has been selected.

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

**R. No language change: recursion over contiguous subtree ranges.** When
the target layout makes every subtree contiguous, cascading by recursion
over children's ranges is provable today. Arena allocation order need not
have that property after reparenting. Snowghost's measured halving shape lost to the
runtime's fixed recursion budget on unbalanced trees (a four-worker speedup
of 1.11 against 3.62 for flat matching on apollo11); a budget that follows
the tree's shape could address that scheduling limit. It has not established
R3 or separation for scattered subtree targets. It answers W1 only where
the targets are subtree ranges, not W2.

**G. The workaround.** Compute in a level-ordered array and gather back
into document order in a second parallel loop. Provable today; breaks R3.

## Source identity for K

Single ownership gives "two places hold two different key values", not
"two different integers": keys from two sources of one key type can carry
the same integer. Whitefoot's identity for storage is the place, and a key
moved from one structure into another keeps no record of its source, so the
overlap clause needs checked source identity, whether in the key's type or
in another retained fact. What the language offers and lacks:

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
not accepted WF syntax, apart from the explicitly runnable Pair probes.
The model is one way to expose the obligations, not a selected representation
for either runtime data or proof terms.

### Candidate source form: an erased field tied to storage by an invariant

The mathematical model below was not a WF language form. This section makes
one possible source design explicit: an erased `ghost` field holds a finite
tree value, ordinary library logical functions define its correspondence to
runtime fields, and a generalized struct invariant states that relation at
the existing boundaries. There is no `model` keyword, implicit field-name
matching, or compiler-owned Forest predicate. This is an unselected syntax
proposal, not accepted source or an implemented proof checker.
Besides quantified facts, this candidate reopens the current restriction on
logical functions in contracts and invariants. It must distinguish total
logical calls from ordinary runtime calls; the Forest's representation and
algorithmic contracts are the concrete consumer the earlier abstraction
refusal left as a reopening condition.

The spellings in this section have these proposed meanings:

- `ghost` on a type, function, field or local makes it proof-only. Logical
  functions are total, effect-free definitions over immutable logical values.
  A runtime value may be observed by proof code; a ghost value cannot choose
  a runtime branch, index, return value, allocation or write.
- `Seq<T>`, `Nat`, sequence literals, `nat(u64)`, bounded `forall`/`exists`, Boolean
  combinations and logical function calls belong to the proposed proof
  expressions. `Nat` is mathematical natural arithmetic. `nat` embeds a
  runtime unsigned value without changing it. Indexing a Seq requires a
  proved natural index below its length; guards delimit where an indexed
  expression is formed.
- `contents(place)` captures the current logical element sequence of storage,
  without a runtime copy. In this example Node has only integer and Option
  fields, so its logical image has those same values. This is proposed
  storage semantics, not a user-defined snapshot function whose name supplies
  a contract. Each primitive write/append must have specified content laws.
- A `lemma` has logical parameters, requirements, an ensured proposition and
  a checked proof body. `use unfold f(args)` expands that one total definition
  at the stated arguments; `use lemma(args)` applies a previously checked
  lemma after proving its requirements. Logical elimination, substitution
  and induction need specified rules. These are proposed extensions to
  `use`, not meanings of today's weighted-affine PRF-1.

All example declarations are in one module, so no private representation is
exposed through an annotation-only visibility exception. Separate modules
would need visible abstract predicates and verified public lemma contracts.
The fixed capacity only keeps the example small; generic invariant support
is a separate part of the desired library interface.

```text
struct Node {
    parent: Option<u64>;
    first_child: Option<u64>;
    last_child: Option<u64>;
    previous: Option<u64>;
    next: Option<u64>;
    payload: u64;
}

ghost struct Tree {
    id: u64;
    children: Seq<Tree>;
}

struct Forest {
    nodes: Slots<Node, 64>;
    ghost shape: Seq<Tree>;
    invariant valid(f): rep(contents(f.nodes), f.shape);
}
```

`shape` is the previously unnamed model witness. It exists in the checking
state and is carried by logical constructors, moves and function contracts;
it contributes no bytes or argument to runtime layout/ABI. A constructor must
supply it and prove `rep`, rather than asking the checker to invent a tree.
The proposed invariant is still one proposition at TYPE-11-like boundaries,
but allowing its logical call and ghost field is a new rule. Current TYPE-11
admits neither this predicate nor this field kind.

Here is the actual field correspondence. List literals, expression-returning
`if`, omitted type arguments on Option constructors and positional logical
arguments are notation of this candidate, not a claim about current grammar.

```text
ghost fn first_id(ts: Seq<Tree>) -> Option<u64> =
    if ts.len == 0 { None } else { Some(ts[0].id) };

ghost fn last_id(ts: Seq<Tree>) -> Option<u64> =
    if ts.len == 0 { None } else { Some(ts[ts.len - 1].id) };

ghost fn tree_matches(ns: Seq<Node>, t: Tree,
                      parent: Option<u64>, previous: Option<u64>,
                      next: Option<u64>) -> Bool =
    if nat(t.id) >= ns.len { false } else {
        ns[nat(t.id)].parent      == parent &&
        ns[nat(t.id)].previous    == previous &&
        ns[nat(t.id)].next        == next &&
        ns[nat(t.id)].first_child == first_id(t.children) &&
        ns[nat(t.id)].last_child  == last_id(t.children) &&
        forall k: Nat where k < t.children.len {
            tree_matches(ns, t.children[k], Some(t.id),
                if k == 0 { None }
                    else { Some(t.children[k - 1].id) },
                if k + 1 == t.children.len { None }
                    else { Some(t.children[k + 1].id) })
        }
    };

ghost fn ids(t: Tree) -> Seq<u64> =
    [t.id] ++ all_ids(t.children);

ghost fn all_ids(ts: Seq<Tree>) -> Seq<u64> =
    match ts {
        [] => [],
        [head, ..tail] => ids(head) ++ all_ids(tail)
    };

ghost fn no_dup(xs: Seq<u64>) -> Bool =
    forall i, j: Nat where i < xs.len && j < xs.len {
        i != j implies xs[i] != xs[j]
    };

ghost fn rep(ns: Seq<Node>, roots: Seq<Tree>) -> Bool =
    no_dup(all_ids(roots)) &&
    (forall slot: Nat where slot < ns.len {
        exists k: Nat where k < all_ids(roots).len {
            nat(all_ids(roots)[k]) == slot
        }
    }) &&
    (forall k: Nat where k < roots.len {
        tree_matches(ns, roots[k], None, None, None)
    });
```

Concatenation and Seq constructor/index laws are general sequence
definitions/laws, not forest axioms. The bounded existential explicitly
requires a model occurrence for every natural-numbered storage slot, even
when rep is considered over an arbitrary logical sequence. All concrete nodes must be covered,
and every model node is bounded by tree_matches. Roots have no parent or
sibling links; their sequence order is ghost only. Payload is not constrained
by any of these definitions. The Tree/Seq types must denote finite inductive
values, with checked admissibility of their recursive occurrences. The
recursive calls descend into proper finite Tree/Seq subvalues; admission
must check this joint structural ordering,
including the mutual ids/all_ids calls. A user cannot define a nonterminating
logical function and use its alleged return value as evidence.

For `shape = [Tree(0, [Tree(2, []), Tree(1, [])])]`, unfolding these library
definitions produces, among other equalities:

```text
nodes[0].first_child == Some(2)    nodes[0].last_child == Some(1)
nodes[2].parent      == Some(0)    nodes[1].parent     == Some(0)
nodes[2].previous    == None       nodes[2].next       == Some(1)
nodes[1].previous    == Some(2)    nodes[1].next       == None
```

Changing the member names in Node requires changing this function's field
selections. Nothing associates a mathematical parent with a member merely
because both are called `parent`.

A small lemma shows how a relation yields a usable field fact. Let
`t = Tree(id: p, children: cs)` be ordinary notation for a logical constructor:

```text
lemma root_first(ns: Seq<Node>, p: u64, cs: Seq<Tree>)
    requires rep(ns, [Tree(id: p, children: cs)]);
    ensures if nat(p) < ns.len {
        ns[nat(p)].first_child == first_id(cs)
    } else { false };
{
    use unfold rep(ns, [Tree(id: p, children: cs)])
        as [unique, coverage, root_match];
    use instantiate root_match at 0 as matched;
    use unfold tree_matches(ns, Tree(id: p, children: cs),
                            None, None, None) in matched;
}
```

For this illustrative proof body, `as [...]` names the three conjuncts
produced by the first unfold; `instantiate ... at 0` specializes its bounded
universal, whose singleton bound is proved. The ensured proposition is guarded
so its subscript is formed only where the bound holds. Unfolding the guarded
definition gives that bound and its first_child conjunct.
Naming projected premises and guarded goals are part of this
candidate proof notation and still need a grammar/judgment; an unchecked
lemma signature is never usable.

An ordinary function can load `let actual = f^.nodes[p].first_child;` after
establishing its bounds. The proposed content-read law connects that load to
`contents(f^.nodes)[nat(p)].first_child`. Applying root_first and substituting
the load equality proves `actual == first_id(cs)` when its singleton-root
premise holds. Arbitrary model nodes need a separately proved lookup lemma;
the singleton example does not implicitly supply that general theorem.

Mutation must update both descriptions and prove their agreement. This small
case links two previously detached leaves, showing the exact places changed:

```text
fn link_two(f: &Forest) -> result: unit writes(f) contract {
    requires f^.nodes.len == 2_u64;
    requires f^.shape == [Tree(0, []), Tree(1, [])];
    ensures f^.shape == [Tree(0, [Tree(1, [])])];
} {
    ghost let before = contents(f^.nodes);
    set f^.nodes[0_u64].first_child = Some(1_u64);
    set f^.nodes[0_u64].last_child = Some(1_u64);
    set f^.nodes[1_u64].parent = Some(0_u64);
    set f^.shape = [Tree(0, [Tree(1, [])])];
    // The implicit exit obligation is rep(contents(f^.nodes), f^.shape).
    return unit;
}
```

This body identifies the runtime/ghost statements, not a finished source
certificate. The exit derivation is finite and concrete: the entry rep says
both leaves' five links are None; content-write laws set exactly the three
listed fields and preserve all other fields and the extent; the new shape
has labels `[0,1]`, its root endpoints both equal 1, its child parent equals
0 and all its other links are None. These are precisely the final field
values. A source certificate must express those unfoldings, content equations
and substitutions; its complete command syntax is still unspecified here.
Omitting either root endpoint or the child parent fails this derivation.
Changing only shape supplies no fact about the runtime fields. Changing only
runtime fields fails against the old shape. Proof-only assignment is not a
trusted operation that restores rep by itself.

The ghost field is part of the logical owner across calls and moves, not an
untracked external witness keyed only by node numbers. Writes through any
alias invalidate dependent current-state facts. A before snapshot keeps its
old meaning; its rep cannot be reused as rep of current contents. Ordinary
effects still constrain actual writes, and logical updates must also appear
in the verified logical contract. These phase/transport rules are additional
work, not consequences already provided by erasing a field.

At a use site, the eventual separation certificate would have this form:

```text
invariant distinct: a != b {
    use sibling_indices_distinct(contents(f.nodes), f.shape, p, i, j, a, b);
}
```

That library lemma must require evidence that a and b were obtained from
positions i and j of p's model child sequence, that both positions exist,
and that i differs from j; it must prove the sequence
and actual-load correspondence, and then use no_dup. No lemma with only
`rep` and two arbitrary node arguments could conclude a != b. For a counted
loop the checker additionally needs a source interface supplying that proof
for two arbitrary distinct iterations, plus all conflicting read/write pairs.
This section specifies the model/field connection, not that still-open
cross-iteration certificate interface. It does not claim that a local
inequality alone extends current PAR-2.

An existential alternative could put `exists shape. rep(contents(f.nodes),
shape)` directly in the invariant and avoid a named ghost field. It would
instead require explicit witness introduction and elimination at construction,
mutation and use. The earlier mathematical prose did not choose between these
forms; this section spells out the ghost-field candidate so that its added
syntax and obligations can be assessed. Neither has been selected.

### A concrete proof from storage to independent work

Use the existing boundary discipline as the working assumption: a mutation
function receives a proved property, invalidates facts about current contents
as it writes, and proves its promised property on every exit. This does not
require a tuple assignment or a `change` scope. An internal helper taking the
whole Forest would still owe its type invariant at the call; a helper taking
raw fields instead needs explicit contracts for the partial state it accepts
and produces. A reference never makes an invalidated fact usable again.
Extending TYPE-11 to the logical predicates below is still a language change;
the current difference-bound invariant cannot express this property.

Consider ordinary node storage with parent, first/last-child and previous/
next-sibling indices. For example:

```text
actual storage indices: 0  1  2  3  4  5  6
logical tree:           0
                       / \
                      1   2
                     / \ / \
                    3  4 5  6
```

The intended recursive jobs rooted at 1 and 2 write payloads at `{1,3,4}`
and `{2,5,6}`. Those sets are not contiguous array ranges. Distinct roots
alone would not suffice: the jobs rooted at 1 and 3 overlap at 3.

Define a finite logical tree `Node(index, children)` and a forest as a finite
sequence of these trees. In this example its witness is
`[Node(0, [Node(1, [Node(3, []), Node(4, [])]),
           Node(2, [Node(5, []), Node(6, [])])])]`.
The witness is erased; it is not another allocated tree. The relation
`Rep(S, M)` ties a captured storage state S to that model M:

1. The flattened model labels have no duplicates and cover exactly
   `0 .. S.nodes.len`.
2. Every stored topology link equals the link induced by M, including empty
   links and both directions of each nonempty link.
3. Payload contents are unconstrained by Rep.

A finite model with unique labels and matching links excludes cycles in
the stored graph and gives each index one occurrence. Both matter:
`1 -> 2 -> 1` has unique parents but a cycle;
`children(0) = [1,1]` gives node 1 only one parent but visits it twice.
Array slots having distinct addresses proves neither property of the stored
indices. Coverage can later be replaced by a relation to live slots if the
library adds deletion; that changes the library predicate, not a Forest
primitive in the compiler.

Here is a finite library proof, rather than an assumed `disjoint` oracle.
Define `NoDup([]) = true` and
`NoDup(x :: xs) = (x not in xs) and NoDup(xs)`. Prove the sequence lemma
`NoDup(A ++ B) -> disjoint(labels(A), labels(B))` by structural induction on A:

- For `A = []`, the left set is empty.
- For `A = x :: rest`, unfolding NoDup gives `x not in rest ++ B`
  and `NoDup(rest ++ B)`. Membership in concatenation gives `x not in B`.
  The induction hypothesis, used only on the strictly smaller `rest`, gives
  `disjoint(labels(rest), labels(B))`. These two facts prove the conclusion.

Membership in concatenation and preservation of NoDup by taking a contiguous
subsequence also have structural sequence proofs. Flattening siblings puts
their subtree sequences in separate blocks of the parent's flattening.
For arbitrary distinct child positions i and j, split on `i < j`, decompose
the flattening around those two blocks, and apply these sequence lemmas.
Thus every label in child subtree i differs from every label in child
subtree j. Rep transfers those logical labels to actual storage indices.
These proofs inspect symbolic constructors, not every runtime node during
compilation; a recursive proof body must pass the smaller-argument check.

Construction must establish the premise rather than declare it. For example,
start with the valid leaf `M = [Node(0, [])]` and append a fresh leaf as 0's
child. The general proof uses old length N and parent p:

```text
before: Rep(S, M), p occurs in M, N = S.nodes.len
runtime: append node N; set its parent and the affected child/sibling links
model:  insert Node(N, []) at the selected child position of p in M
after:  Rep(S', M')
```

All old labels are below N, so N occurs nowhere in M. Inserting that one
label preserves no duplication and changes coverage to `0 .. N+1`.
The append contract preserves old elements and describes the new element;
each link write supplies a read-after-write equation. To prove the link
clause for an arbitrary node, split into the changed nodes and all others.
The former use those equations; the latter use preserved contents and the
old model relation. Failure routes must return a state satisfying the
advertised contract too. This proof requires storage-content contracts;
a postcondition about length alone is insufficient.

Ordinary sequential writes can implement that runtime update. Rep(S, M)
remains a theorem about the old captured state, while Rep of the partially
updated current state is unavailable. The exit proof constructs Rep(S', M').
A mutable reference parameter changes where these writes land, not these
proof obligations. All aliasing writes must participate in the same support
and effect rules; a hidden reference mutation cannot preserve a false fact.

To consume the proof, distinguish two interfaces:

- **Counted scatter.** Given actual captured index contents A, prove bounds
  and `forall i,j in 0..A.len: i != j -> A[i] != A[j]`. Instantiating at two
  arbitrary iterations establishes their target-index inequality. The
  compiler must check every conflicting read/write pair using that evidence,
  not just the two output writes. An accessor or list-building operation
  owes the relation between A and the relevant model labels; a Forest does
  not certify any arbitrary list derived from it.
- **Recursive subtree work.** A helper's proposed effect contract bounds
  its writes by payloads at `labels(subtree(M, root))`. Its body must prove
  that bound: write the root's payload, recursively use each child's bound,
  and prove their union stays in the parent's set. Reads may share immutable
  topology; any parent payload read must refer to data ready before the
  sibling jobs start. Other reads must also avoid another job's writes.
  The sibling theorem then establishes the common overlap judgment's
  separation premise. An unrestricted `writes(results)` does not express
  this footprint, and a function contract alone does not grant permission
  for overlapping recursive invocations or assume its own truth: recursive
  contract checking needs a sound rule, such as induction on the proper
  model subtree. That rule is not present in this derivation's baseline.

Keeping topology unchanged during these jobs preserves the supporting model.
Linked sibling discovery still reads one link before discovering the next;
the proof authorizes independent work on discovered subtrees but creates no
index buffer or scheduling strategy. Runtime progress and proof termination
also differ. Descending a proper finite subtree gives a mathematical rank;
using it to justify runtime recursion needs its own language judgment.

This gives a concrete dependency order for a prototype: checked logical
definitions and sequence induction; storage-content transitions and contract
transport; the fresh-node construction proof; a no-duplicates scatter proof
consumed by the shared separation rule and PAR-2. Detach/attach and recursive
footprints then exercise preservation and generality. No step may be
replaced by an unchecked user axiom or a runtime validation scan. This is a
paper derivation; no proposed proof syntax or checker is implemented here.

### Deferred alternative: validity at every source commit

The following comparison addresses the stronger R5 alternative, not a
prerequisite of the boundary-contract derivation above. Its executable probes
remain evidence about the distinction between the two guarantees.

[TYPE-11](../../../spec/kernel-spec.md) currently checks direct construction,
direct struct parameters/results, written-reference exits and shared-object
boundaries. It admits only non-generic, non-opaque structs and difference
bounds over selected fields and measures. It does not promise the invariant
at every statement boundary or transport it through every nested store/read.
The current limitation is already recorded under "Type invariants stop at
the direct struct type" in [the TODO](../../../docs/todo.md).

The gap is observable even without a helper. This complete current-WF
program constructs a valid Pair, modifies one field, then reads both:

```wf
struct Pair {
  left: u64;
  right: u64;
  invariant equal(pair): pair.left == pair.right;
}

fn main() -> status: std::process::ExitStatus pure {
  let pair = Pair(left: 0_u64, right: 0_u64);
  set pair.left = 1_u64;
  let left = pair.left;
  let right = pair.right;
  if left == right {
    return std::process::exit_status(code: 0_u8);
  } else {
    return std::process::exit_status(code: 1_u8);
  }
}
```

The baseline compiler accepts it, and the native executable exits 1: the
live Pair can be observed with unequal fields. Current TYPE-11 requires no
invariant obligation at that field write or those reads, so this is a
specified weaker boundary, not evidence of a compiler/specification
discrepancy. It fails R5's proposed requirement. Changing the construction
itself to `Pair(left: 1_u64, right: 0_u64)` remains a TYPE-11 rejection.

The existing [type-invariant boundary decision](../../../design/language/checks-and-proofs.md)
deliberately rejects an obligation at every field write because only the
updating body can observe the intermediate state. R5 rules out tolerating
an invalid live struct even inside that body. That decision and its refused
alternative must therefore be reopened alongside TYPE-11; this research
does not amend either, and an implementation-only repair would not select
the new language behavior.

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

Meeting R5 requires every live invariant-bearing value to satisfy its
invariant at each source commit boundary. The corresponding preservation
argument would be induction over program transitions, with these obligations:

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

Another boundary is validity whenever a complete value is handed
to an observer, with explicit checked intermediate states inside an update.
That is weaker than validity at every source statement. Tuple assignment
makes many fixed-size link updates compatible with the stronger boundary
required by R5; it does not enforce that boundary or solve interior-reference
composition by itself. A complete source rule is needed before an invariant
is freely assumed on any read.
Under R5, an intermediate state cannot remain a live value of the original
struct type with a false invariant, even if a scope hides it. An admissible
representation conversion must end that typed value's lifetime and expose
ordinary values of other types, each obeying its own invariants, before any
otherwise-invalid update. Reconstructing the original type owes its full
invariant. An unusable binding may be retained to name the eventual
replacement place; it must not count as a surviving invalid typed value.

An extensibility example makes the distinction consequential. Add a cached
`subtree_size` to every node and require it to equal the node count of its
logical subtree. Attaching a new leaf changes that count for every ancestor.
Updating the counts first violates their old-tree meaning; changing the
links first violates their new-tree meaning. The number of ancestors is
determined at runtime, so no fixed source-arity tuple covers all such edits.
A loop of bounded-size commits cannot perform this direct update while
establishing the complete invariant after every commit. Tuple assignment
therefore does not by itself support this natural extension at R3's cost.

One candidate is a checked, erased unpack/update/repack protocol. Consuming
the valid Forest exposes its exclusively owned raw representation and a
proof about the old state. During the update there is no usable Forest value
whose full invariant may be assumed. Helpers operate on that raw state with
their stated partial properties; a loop can update the ancestor counts.
Repacking establishes the complete new invariant before producing a Forest
again. This preserves validity of every usable value of the public type,
while permitting intermediate bytes that do not represent such a value.

Such a protocol must consume or suspend all authority to observe the value
as a Forest, prevent aliases or callbacks from exposing the intermediate
state, and require each return/propagation path either to re-establish the
public invariant or to dispose of the consumed representation where the
API permits that. Old-state proofs must not be treated as current-state
facts. Private visibility alone establishes none of these obligations.
The protocol might use ordinary consuming representation conversion or a
scoped proof form; neither syntax nor a lowering guarantee is selected here.
It is the checked-intermediate-state alternative above, not an unchecked
escape. Whether to provide this consuming conversion, a more general
simultaneous update form, or only operations that preserve the invariant
without a representation conversion is an open choice that the fixed-link
example alone cannot settle. Each candidate must meet R5.

### A scoped change and an ordinary representation conversion

Consider the candidate spelling `change(s) as raw { ... }`. Its useful
interpretation is a scoped conversion from a valid value to an exclusively
owned representation, followed by a checked reconstruction. Merely delaying
the final invariant check while retaining all ordinary facts about `s` would
be insufficient: a helper taking `&Forest` could assume the very invariant
the block has temporarily broken.

For that interpretation the scope needs the following rules, regardless of
its eventual spelling:

- Entry requires the current invariant and grants the update exclusive
  access. To meet R5, conversion consumes the original typed value; the body
  has ordinary representation values of different types. The ordinary `s`
  and aliases through which an observer could use its invariant are
  unavailable during the update. This changes usable authority, not the
  truth of a theorem about the entry state.
- The body operates on a representation without the opened invariant. It
  retains applicable bounds, initializedness, ownership, arithmetic and
  other value-type obligations. Writes invalidate current-state facts by
  their support; an old-state theorem cannot prove the final-state goal.
- Helpers accept that representation and state the partial properties they
  need and preserve. An ordinary `&Forest` helper remains callable only when
  its normal preconditions, including the complete invariant, are proved;
  lexical presence in `change` grants no exemption to a callee.
- Every exit that returns the updated object to a caller or observer owes
  its complete invariant. A propagated error has this obligation too. A
  consuming API may instead dispose of the representation if its ownership
  and resource obligations permit that. There is no implicit rollback.
- Opening a nested field does not silently waive an invariant of its live
  enclosing value. Any enclosing invariant affected by the writes must also
  be preserved, or its owning value must participate in the conversion.
  Reopening the same ordinary owner cannot create another independent
  entitlement to its original invariant.
- A change block provides no cross-thread atomicity, lock or scheduling
  edge. Shared storage still requires the language's ordinary exclusive
  access/synchronization rules. A long update need not be a machine atomic
  operation merely because its proof has one boundary.

An alternative already has a small executable witness in current WF:
ordinary consuming destructuring [PROV-6], followed by construction
[TYPE-11]. No special scope is required for this example:

```wf
nocopy struct Pair {
  left: u64;
  right: u64;
  invariant equal(pair): pair.left == pair.right;
}

fn rewrite(pair: Pair) -> result: Pair pure {
  let Pair(left: left, right: right) = move pair;
  set left = 1_u64;
  set right = 1_u64;
  return Pair(left: left, right: right);
}

fn main() -> status: std::process::ExitStatus pure {
  let pair = Pair(left: 0_u64, right: 0_u64);
  set pair = rewrite(pair: move pair);
  return std::process::exit_status(code: 0_u8);
}
```

There is no surviving Pair inside `rewrite` after the destructuring; there
are two ordinary integers. Construction proves their required relation
before another Pair exists. The caller uses the existing in-place update
form [OP-12]. Its restrictions on result types and failure exits still
apply. This probe establishes neither arbitrary Forest proofs nor a complete
invariant discipline for every other mutation route; TYPE-11 still has the
limitations identified above.

A larger library can give the representation its own ordinary type,
`ForestData`, and keep `Forest` as a private wrapper with the stronger
invariant. Destructuring moves out ForestData, helpers accept ForestData with
explicit content contracts, and construction owes `Rep(data, new_model)`.
The representation keeps all of its own type invariants. A boxed node array
can remain owned by that representation; the approach need not construct a
second array. Equal generated code under R3 remains a separate observation
to make, not a consequence of writing `move`.

The comparison is semantic rather than a keyword count:

| Candidate | Dynamic update set | Helper interface | Obligation exposed to the writer |
|---|---|---|---|
| Finite tuple commit | Only its statically listed targets | Compute operands or preserve the invariant at calls | Establish the simultaneous final state |
| Scoped conversion `change(s) as raw` | Loops and calls inside the scope | A nameable representation or an explicitly specified open-state parameter | Close every publishing exit and account for enclosing owners |
| Consuming destructure and reconstruct | Loops and calls between the two | Ordinary representation type and its contracts | Reconstruct every promised valid result; obey ordinary move and exit rules |
| Every function body implicitly opens the struct | Loops within a body | Still needs a distinction for helpers allowed to receive partial state | Function privacy or body entry alone cannot supply that distinction |

For further investigation, consuming representation conversion is the useful
baseline: it gives partial states and helper boundaries explicit meanings
using existing ownership forms. A scoped form should be compared against
that baseline for borrowed/in-place mutation, nested owners and error exits.
This does not select a new language rule or establish that no scoped form
is needed. It narrows the unresolved requirement from "an invariant-off
scope" to a compositional representation-state boundary.

There is related prior art in
[Viper's explicit predicate fold/unfold](https://viper.ethz.ch/tutorial/predicates.html):
predicate authority is exchanged with the resources and properties of its
body, rather than keeping both independently. This is a comparison of proof
organization, not adoption of that verifier's inference or permission model.
[Pulse's shared invariant scopes](https://fstar-lang.org/tutorial/book/pulse/pulse_atomics_and_invariants.html)
instead restrict observable computation to at most one atomic step, with
ghost or unobservable steps around it. Those shared-invariant scopes cannot
be copied as the meaning of an arbitrarily long exclusive local update.

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
are distinct by the old model. With boundary contracts the runtime update
can use ordinary sequential writes, shown in mathematical path notation:

```text
set nodes[a].next = Some(b);
set nodes[b].previous = Some(a);
set nodes[x].parent = None;
set nodes[x].previous = None;
set nodes[x].next = None;
```

The model update removes the whole subtree rooted at x from p's child
sequence and appends that subtree to the model's roots. Its descendants and
payloads remain unchanged. The library proves:

- the same labels still occur exactly once and every slot remains covered;
- the five written fields match the new model;
- every other link field is unchanged and still matches it.

For a first child, last child or only child, the body also updates p's
corresponding endpoints and omits a nonexistent neighbor. These are finite
source branches; each target must exist and be in bounds. The body proves
the new Rep before returning. A tuple could group the same writes if added,
but that syntax is not needed for this boundary proof.

Attaching a detached root x before child y of p removes x's subtree from
the model roots and inserts it in p's child sequence. Require p to be outside
x's subtree, with x itself included in that subtree. Unique labels and this
condition make the model edit well founded and preserve the forest. The
runtime update writes the new neighboring links, x's parent and p's affected
endpoints, with the same changed-field/unchanged-field proof as detach.

For a fresh leaf N, the condition follows from old labels being below N.
For an arbitrary existing subtree, it must come from a verified caller fact
or the existing DOM cycle-refusal walk. A successful walk must publish the
logical non-ancestry fact: its loop proof relates the cursor to the remaining
ancestor path and records that every inspected ancestor differs from x.
This uses the program's existing validation, not a new scan inserted to
satisfy the proof system. Whether all existing guard results can be
transported at equal runtime cost is still unverified.

Reparenting can validate first, call a proved detach, then a proved attach.
The detached state is itself a valid forest, so the attach call's boundary
requirement can hold. Same-parent relocation and insertion
before oneself need their own ordinary cases: a blanket increment of the
new parent's child count is false when the old and new parent are equal.
Aliases among old/new neighbors must be resolved or proved apart when
establishing the writes' content transitions and call separation. Every
failure exit must satisfy the advertised
invariant; whether failure leaves the original tree unchanged is a separate
API promise that must also be proved if made.

For `move_all_children(from, to)`, each successful iteration detaches and
attaches one subtree. Its ghost rank, length of from's model child sequence,
decreases only when `from != to`. With equal parents a nonempty sequence can
be rotated forever. This is a falsifier, not a condition that the forest
invariant alone could prove. Proving each link edit preserves the forest
does not prove this data-dependent loop terminates.

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

### How a certificate checker could run without proof search

The ghost-field sketch is a substantial extension, not a few more affine
rules. Termination of its logical definitions alone does not establish fast
checking: evaluating a total recursive function or normalizing two arbitrary
logical expressions can take enormous time. A candidate implementation must
keep recursive calls opaque until one explicitly named unfolding and avoid
general definitional-equality normalization. The following is an internal
certificate design to assess, not a selected language rule or implementation.

Keep the existing specification-fixed numeric derivation for its admitted
goals. Add a typed first-order logical certificate layer whose primitive
rules inspect explicitly supplied evidence. Its input consists of immutable
term/formula nodes, scoped assumptions, checked lemma signatures and a finite
proof graph. A proof record names its rule, earlier premise IDs, substitution
arguments, exact rewrite occurrence when applicable, and claimed conclusion.
The checker verifies that one inference; it never chooses another lemma,
quantifier instance, rewrite position or induction argument after failure.

Representative primitive checks are:

| Certificate instruction | What the checker verifies |
|---|---|
| Conjunction elimination/introduction | Select the written component or combine the supplied component proofs in the declared order |
| Universal elimination | The cited premise is universal, the supplied term has the quantified type, and the claimed result is its capture-avoiding substitution; bounded quantifiers still require bound evidence |
| Universal introduction | Check one subproof with a fresh symbolic variable; assumptions and variables cannot escape their scopes |
| Implication introduction/elimination | Check a subproof under its explicit assumption, or check that the supplied premise exactly matches the implication's antecedent |
| Existential introduction/elimination | Check the written witness and its property, or open a fresh scoped witness that cannot escape into an unsupported conclusion |
| Equality rewrite | Check the supplied equality, direction and occurrence path, and that only the selected occurrence changes |
| Definition unfolding | Check one application against one definition body with the supplied arguments substituted; leave calls inside that body unexpanded |
| Constructor cases/induction | Check all required constructor cases, with only the datatype rule's smaller subvalues receiving induction hypotheses |
| Lemma application | Instantiate the previously checked signature and match a supplied proof for every requirement; do not expand or recheck its body at each use |
| Numeric certificate | Check the explicit admitted arithmetic rule and its typed premises; the current PRF-1 family supplies only its current numeric fragment |

These are rule schemas over types and propositions. There is no rule named
`ForestIsValid` or `TrustRep`. Formula equality for rule matching is syntactic
up to explicitly specified binder representation and simple canonical forms;
mathematical equivalence requires evidence. Hash-consing can share identical
terms, but a hash match alone is not equality. Memoization changes repeated
work, never which inference is legal. The earlier `use unfold ... as [...]`
surface sketch must elaborate to definition and logical-elimination steps;
it cannot mean "expand this and solve the rest".

#### A complete small inference chain

Let A be one captured actual index sequence, with already checked facts:

```text
H: forall i,j. (B(i) and B(j) and i != j) implies A[i] != A[j]
I: B(i)               // i is in the captured sequence's domain
J: B(j)
D: i != j
L: a == A[i]          // actual load at the matching storage state
R: b == A[j]
```

The certificate author supplies:

```text
1. universal_elim H with i
2. universal_elim 1 with j
3. and_intro I, J
4. and_intro 3, D
5. implication_elim 2, 4                 : A[i] != A[j]
6. rewrite 5 at left  using L, backwards : a != A[j]
7. rewrite 6 at right using R, backwards : a != b
```

The conjunction in H is left-associated as formed in steps 3 and 4.
Index terms are formed under the supplied domain facts. Each step consists
of type/scope checks and explicit formula substitution or selection. No loop
runs over A, and no search chooses i or j. This checks an application of H;
it does not establish H. The Forest's construction/mutation and sequence
lemmas must have already proved that premise about this captured content.
For parallel permission i and j must represent arbitrary distinct iterations,
and all other conflicting access pairs remain obligations.

#### Checking a quantified mutation proof and induction

For an element-local property `forall i in domain(A). P(i, A[i])`, whose
other parameters remain unchanged, introduce one fresh symbolic index i
after `A[k] = value`. The primitive write law connects the captured states:

```text
read(write(A, k, value), i)
    = if i == k then value else read(A, i)
```

The writer gives two proof branches, `i == k` and `i != k`; the checker
verifies both and the completeness of the split. One branch checks the new
element; the other instantiates the old universal property at i. There is
no iteration over the runtime array. The write and both reads require their
index bounds, and this write preserves the extent. A relation that depends
on several elements or on other changed state needs the corresponding
additional preservation proof; the pointwise example grants no general
frame rule for an arbitrary `P(A, i)`. A field write needs the corresponding
field/path law, including unchanged elements and other fields. Those laws
must follow the primitive's specified runtime semantics and actual resolved
write, not a writer's assertion that other storage was unchanged.

For a sequence lemma such as `NoDup(A ++ B) -> Disjoint(A,B)`, the writer
chooses induction on A and supplies the empty and cons cases. The cons case
receives a fresh head x, tail xs and the induction hypothesis for xs. It
cannot assume the theorem for `x :: xs`, an arbitrary other sequence, or an
unverified mutually recursive lemma. The checker checks these symbolic
cases once; it neither unrolls an actual sequence nor executes the proof
recursively for its runtime length.

Finite first-order inductive datatypes can supply these case/induction
schemas mechanically. For the Tree/Seq combination, the mutual structural
rule must account for both the Tree constructor and sequence constructors;
recursive type admissibility and the precise smaller-subvalue rule still
need specification. Merely observing that a source lemma body is finite
does not admit unrestricted recursive proof calls. Ordinary library lemma
dependencies can be acyclic, with induction hypotheses confined to this
explicit rule; stronger recursion principles are not implicit.

#### Internal representation and the runtime-state connection

An illustrative checking loop is:

```text
for each proof record in dependency order:
    verify its premise IDs are available in this assumption scope
    verify its term types and supplied substitutions
    expected = check_the_named_rule(record, premises)
    require exact correspondence with its claimed conclusion
    retain that checked conclusion with its dependencies
require the final conclusion to match the requested goal
```

Subproofs for cases, quantifier introduction and induction have explicit
scopes and are traversed as finite certificate syntax. A flat graph of
earlier IDs alone does not enforce assumption discharge; the checker must
track those scopes and dependencies. Logical definitions are checked for
their admissible structural recursion separately from checking lemma bodies.
One-layer unfold nodes refer to those checked definitions. Program loops
and recursive calls are not evaluated to check a certificate.

The existing structural/flow walk still owns storage identity, resolved
references, effects and source-point availability. A new content-state
extension would have to derive and validate logical images S0 and S1 from
those identities and events, connect them by the specified store transition
or verified call relation, and supply typed equalities for actual loads.
Current numeric snapshots do not provide these logical content images.
The logical checker could retain a theorem about S0, but could not silently
attach it to S1. Calls would contribute only their verified content contracts
and frame conditions. Facts established by this
layer enter the same derivation/overlap consumers as other checked facts,
after the ordinary support and alias judgments. This is an extension inside
the one acceptance path, not an external answer that bypasses the flow walk.
Its content-state bridge is new work and part of the correctness argument.

Current code illustrates the narrower version of this organization:
`compiler/src/semantic/check/control/proofs.rs` resolves and forms a source
certificate; `compiler/src/semantic/entailment/flow/certificates.rs` checks
its admitted premises, written multipliers and residual against the current
proof context. It does not implement the logical rules above. Reusing its
numeric routines and derivation infrastructure does not supply quantifiers,
induction, state-content terms or recursive contract checking for free.

#### What time guarantee this could establish

There are three different claims:

1. **No search-dependent acceptance.** Every submitted proof follows fixed
   inference rules with explicit arguments. Failure does not trigger a
   larger search, and timeout, fuel or machine speed selects no verdict.
2. **Finite and accountable checking.** The certificate graph, definitions
   and case subproofs are finite. Each primitive rule must have a terminating
   local algorithm; recursive normalization and unbounded elaboration are
   excluded. This permits an operation-count bound in terms of certificate
   size, formula/term size, substitution work and arithmetic bit lengths.
3. **Fast for the intended programs.** This is not established here. The
   proof may be huge, substitutions may grow terms, and mathematical integer
   arithmetic is not constant-time. Finite checking does not show practical
   authoring or compilation cost, nor a hardware-independent wall-time bound.

A useful candidate IR makes intermediate terms and formula sharing explicit,
checks substitutions against the written result graph, and does not use
semantic normalization to decide equality. Even then, a linear bound in the
number of proof commands alone would be false: one command may mention a
large definition or substitution. Any claimed complexity bound must include
surface elaboration, type formation, generated terms, scope checking and
numeric derivation, not only replay of an already expanded certificate.
No aggregate asymptotic or measured bound has been established for this
candidate. Fixed source structural ceilings could bound admitted forms, as
current PRF-1 caps one certificate's entries, but no ceiling for these new
forms has been selected, and a step-fuel cutoff is not a substitute.

Before choosing the surface mechanism, a discriminating prototype would
check explicit certificates for empty/create, one detach/attach, and one
actual-content scatter chain. Record certificate bytes/steps, term nodes,
substitution visits, arithmetic operand sizes, elaboration work and checking
time separately. Compare shared lemma calls against repeated expansion;
changing a quantified bound must not cause enumeration of that domain;
one named unfolding must not recursively evaluate its descendants. Include
wrong substitutions, escaping assumptions, circular lemma dependencies,
missing reverse links, duplicate indices, stale storage images and induction
on a non-smaller argument as rejecting cases. A practical budget and authoring
criterion still need to be stated before measurements select a design.
No prototype, timing result or completed calculus is claimed by this section.

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
| Logical predicates in explicit contracts or type invariants, including generic instantiation | State the Forest relation at construction, call and return boundaries; automatic attachment through containers is a further question | A caller cannot pass a partially repaired Forest to a function requiring its full predicate; omitting a reverse link fails the exit proof |
| User-defined logical predicates, finite models and checked structural lemmas | Defining Rep, proving no duplication and deriving non-ancestry | A cyclic raw graph cannot acquire height by assuming its own well-foundedness; a circular lemma cannot prove itself |
| Relational content contracts, result/entry snapshots and generic-library transport | First push/grow, next-sibling accessor, cycle-walk success, and a returned index array | Correct lengths with a duplicated or changed old element must not satisfy the content contract |
| Proof-state support, framing and move/store transport | Keep topology facts across payload changes; replace them after reparent | Reuse an old sibling or document proof after changing its supporting links |
| Shared separation facts and a cross-iteration proof consumer | Counted scatter with a proved injective index sequence | Distinct writes beside a cross-iteration conflicting read must deny permission |
| Noncontiguous effect footprints, body checks and recursive contract checking | Parallel recursive subtree helpers | Two distinct roots where one is a descendant must not count as disjoint subtrees; a recursive contract cannot establish itself without a sound recursive rule |

The first five reach the counted child-index witness; the sixth is needed
for recursive subtree calls. A local explicit contract can carry the Forest
predicate without first generalizing every struct-invariant boundary.
Simultaneous assignment, a `change` scope and continuous lifetime validity
are not prerequisites for this route. New runtime rank consumers and a
linked-walk scheduling strategy are additional consumers, not prerequisites for a checked
Forest value. A permutation mapping (W2) should reuse the same sequence
proofs and content contracts without any forest-specific checker rule.

Still unverified: an exact proof grammar and checking calculus, content and
effect transport through interior references, all mutation/error paths of
the actual SG Forest library, proof-checking cost, and generated-code equality
under R3.
The finite-model witness is a constructive paper route, not evidence that
this whole capability set has already been implemented or minimized.

### Observations with the current compiler

Compiler source: `22d0923bdf5ce7dbf4752cf9a302dc4154254869`, built with
`make -C compiler build`. No compiler source differs on this research branch.
Each probe uses that build with `--check`; each scatter uses
`--par --par-ledger --emit-llvm`.

- The direct-write Pair probe above passes `--check` and native compilation
  with `whitefootc -o executable source.wf`. Running it exits 1, the expected
  observation of unequal fields. This executable observation demonstrates
  that current TYPE-11 does not meet R5; it is not a proposed accepted case
  for the stronger invariant discipline.
- The runnable Pair probe above exits 0. Removing `set value^ = old;`
  exits 1 with FN-9 `UndischargedPostcondition`. Changing the construction
  to `(1, 0)` exits 1 with TYPE-11 `UndischargedTypeInvariant`. Thus the
  positive result does not mean the declared invariant is ignored everywhere.
- The complete consuming-destructure/reconstruct Pair probe exits 0.
  Removing `set right = 1_u64;` rejects its final construction with TYPE-11
  `UndischargedTypeInvariant`. Replacing `set left = 1_u64;` with
  `set left = pair.left;` rejects with OWN-1 `UseAfterMove`.
  This is existing language behavior, not a prototype of `change`.
- Reference replacement exposes a separate result-publication defect. Using
  the same Pair type, a `make() -> result: Pair pure` function returns
  `Pair(left: 1_u64, right: 1_u64)`. A helper with signature
  `update(pair: &Pair) -> result: unit writes(pair)` and body
  `set pair^ = make(); return unit;` rejects at the return with FN-9
  `UndischargedPostcondition`. Changing that body to
  `let next = make(); set pair^ = move next; return unit;` passes; directly
  constructing `Pair(left: 1_u64, right: 1_u64)` into `pair^` passes too.
  The rejection loses a verified result-only relation specified by FN-9 and
  CALL-4 for direct ordinary-set destinations. It is conservative rejection,
  not a counterexample to invariant safety; its implementation cause remains
  untraced and is recorded in the TODO.
- A boxed representation variant also exposes the content-transport limit:
  for `Parts { left: u64; right: u64; }`, binding `Parts(0, 0)` and passing
  it to `box_new::<Parts>` does not by itself establish the field equality
  needed to construct a wrapper whose invariant is
  `wrapper.data.inner.left == wrapper.data.inner.right`. That constructor
  rejects with TYPE-11 `UndischargedTypeInvariant`. This does not refute the
  conversion protocol; it is an additional witness for the logical-content
  transport requirement already recorded above.
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
   container or a trusted user axiom. The active boundary-contract route
   requires invalid construction/call/exit and stale-fact negative cases.
   If R5's stronger alternative is reopened, it additionally requires
   direct-mutation, projected-reference and nested-storage witnesses; a
   scoped update cannot keep a live invalid struct value under that rule.

## Plan

1. Use explicit boundary contracts for the Forest derivation, with support
   invalidation on all overlapping writes including interior references.
   Keep the stronger lifetime rule and its spelling deferred.
2. Specify the finite proof operations and storage-content contracts needed
   for empty/create and no duplication; write those certificates explicitly.
   Keep source identity for K as a comparison, not a prerequisite.
3. Connect that theorem to one counted scatter and the permutation witness,
   then exercise detach/attach and traversal. Address subtree effects and
   recursive contracts separately.
4. Bring concrete specification sketches and their limitations to the owner
   before any compiler change. Record a selected mechanism in the design tree.
