<!-- External consultation: a survey written by a separate model session from a fixed prompt, kept as research input. Its claims were spot-checked (probe q1, the FN-9 conformance case); unverified items are marked in the text. -->

# Provable termination over index-linked, mutable data in Whitefoot: the design space

Scope: a first-principles survey of how a program over index-linked, mutated
data can carry a checked progress proof under Whitefoot's constraints, ranked
performance first. Written against `spec/kernel-spec.md` v0.83,
`design/language/checks-and-proofs.md`, `docs/constitution.md`,
`research/investigations/termination/{DESIGN,ARENA}.md` and their `runs/`,
`research/investigations/readonly-field-terms/DESIGN.md`, and the Snowghost
sites at 09d33ba. Nothing in the repository was edited. Items marked
**unverified** were not checked against a source in the repository.

## 0. What the constraints leave open

A progress proof is a term of the fact language that falls on every backedge
or call inside a cycle. Whitefoot's fact language is difference bounds and
disequalities over [ENT-2] terms: tracked places, subscripted readonly fields
with tracked-place or constant offsets, constants, and the spec-owned measures
`len`/`cap`/`head`. It has no quantifiers, no inductive predicates and no
functions of storage contents. So for a walk over index links the falling
quantity must be one of exactly four things:

| where the rank lives | example | who maintains the order |
|---|---|---|
| the index itself | `links[i].next < i` | the storage's operation set (append-only) |
| a spec-owned logical measure of a storage shape, like `len` | `f[n].depth` | the shape's primitives, trusted as [MSR-1] already trusts `take_back` |
| a stored field kept ordered by checked writes | `nodes[c].height < nodes[p].height` | a runtime comparison or renumbering at each write |
| ownership structure | `Option<Box<Node>>` | the type system (C5, already covered) |

Every path below is one of these four, or a way of turning a site into one of
them (representation change, phase split, restructuring), or a runtime bound
(the counted walk). Nothing else is expressible without a quantified fact, and
that family is refused for a reason that is sound: re-establishing a
per-element fact after a bulk write is a derivation over elements that no
fixed family performs.

Two facts about the current fact system shape the whole analysis:

- **Measures over subscripted places already enter local invariants.** Probe
  `q1.wf` (scratchpad, compiled with `compiler/target/gate/whitefootc --check`,
  exit 0): `let p` bound by `Some(value: p)` from a callee's routed `ensures`,
  then `invariant direct: table^[p].len <= before;` is accepted. A match
  binder is an [ENT-2] clause (a) place, so a proof-only per-node quantity
  modelled as an [MSR-1] measure of an element place needs no new INV-1
  admission (Q20 is not needed for measures; it is needed only for writer
  readonly fields, which [INV-1] still excludes).
- **Postconditions cannot yet name a subscripted place.** The conformance
  case `fn9-neg-readonly-field-below-subscript-relation`
  (`tests/conformance/manifest.jsonl:1317`) pins that an FN-9 relation datum
  is a parameter datum with field/Box projections or a measure member, never
  a subscripted place of a formal. Any accessor that publishes
  `f[p].depth + 1 == f[n].depth` therefore needs the FN-9 extension ARENA.md
  calls Q21, whichever path is chosen. That extension is a substitution
  problem at the boundary (the formal offset must be substituted on both
  sides), not a term-formation problem at the caller.

Cost tiers, as the owner set them: tier 1 adds no runtime work; tier 2 adds
only a check the program needs anyway.

## 1. The paths

### P1. Monotone arenas: the index is the rank

**Idea.** A storage whose link fields may only point at strictly smaller
indices (or strictly larger; one direction per field), enforced not by an
invariant on the element type but by the storage's operation set: elements
are appended, a link is written only when the element is placed or through a
primitive whose target is proved or checked below the source, and nothing
moves elements between indices. A read of a link publishes `target < source`
as the operation's own postcondition, exactly as `take_back` publishes
`len + 1 == entry len`. Walks then carry the index as their scalar rank and
need no new rank form at all: they are C1/C2 loops.

**Sketch.**

```wf
// prelude shape; `Down` is Option<u64> restricted to targets below the holder
struct Chain<T> { public readonly items: Box<Slots<T>>; /* link words owned by the shape */ }

fn chain_push<T>(chain: &Chain<T>, value: T, next: Down) -> made: u64 writes(chain) contract {
  requires next.is_none or next.value < chain^.items.inner.len;   // stated as two rows in practice
  ensures made == entry(chain)^.items.inner.len;
}
fn chain_next<T>(chain: &Chain<T>, at: u64) -> result: Option<u64> reads(chain) contract {
  requires at < chain^.items.inner.len;
  ensures when Some(value: below): below < at;
}

// hash-chain walk (tags.wf attribute_is_duplicate), rank = cursor
loop @scan {
  match chain_next(chain: &index^.links, at: cursor) {
    None() => { break @scan; }
    Some(value: below) => { set cursor = below; }     // below < cursor from the ensures
  }
}
```

The `heads` table stays an ordinary `Slots<u64>`; the walk's first read
`heads[chosen]` needs a range fact, which it gets from the bounds check the
walk already performs (`rel < links.len`, present in the current code) or
from pushing before setting the head (then `head == old len < new len` is
provable at the write and the read still needs its bounds check). That is
tier 2 and it is already paid.

**Soundness.** A link written at placement time targets an existing element
(`< len`), so it targets a smaller index. Later link writes are either
refused (no `set` on the link word outside the prelude) or go through a
primitive with the same requirement. No operation renumbers: `insert_at`,
`remove_at`, `append`, `split_off`, `swap` of elements and front operations
are outside the shape's operation set; `take_back` is admitted only if the
shape guarantees no link targets the removed slot, which it cannot, so the
shape is append-only or clears whole. The relation `target < holder` is then
an invariant of every reachable state, and the shape publishes it at each
read as a spec-owned fact, the same trust class as `len`.

**Sites covered.** Hash chains (loop 85: the new link's source is the element
being placed and its target is the old head, in range). `parent_table` (loop
217) after re-encoding the two sentinels out of the link word: `Down` is an
`Option`, so `top_level` is `None`, and `blocked_start` becomes a separate
kind field or a second table. Selector nested ranges (comp 1) **only after
merging alts, compounds and instrs into one index space**: today the fact
"an instr's nested `alt_end` is at most the alt that holds it" relates three
storages and no per-storage order states it; with one `Slots<SelNode>` where
every child range lies below the holder, every edge of the component falls in
node index. Unicode decomposition tables (comp 8, loop 234): the generator can
emit entries in topological order so every part index is below its holder;
`expand_cp` reading raw UCD input still needs input validation, as ARENA.md
already concludes. The frozen preorder DOM of P8. **Not covered:** the live
DOM under adoption or foster parenting (a moved subtree's parent is newer
than its children in creation order, so no fixed direction holds), the BFS
frontier list (its links follow discovery order, not index order), and
anything that relinks.

**Runtime cost.** Walks: zero. Writes: zero when the target is the previous
length or an index the checker already bounds; otherwise one comparison the
program can usually prove away. Memory: zero.

**Spec/checker cost.** One prelude shape (or one modifier on `Slots`), its
operation rows and one accessor postcondition per link field. No new rank
form, no new fact family. The FN-9 datum for `ensures when Some(value: b): b < at`
is a bare payload and a parameter: admitted today.

**Refusal conflict.** ARENA.md's Form 2 states the same relation as an
element invariant instantiated at every read, and Q17 asks to revisit the
refusal. Framed as a shape, the refusal's reason does not apply: the reason
is that every write owes the quantified fact again and re-establishing it is
a derivation over elements; here no element write exists outside the
primitive, the primitive owes one fact at one index, and bulk fills are not
operations of the shape. The remaining consequence, "instantiated at each
read", is exactly how `len` facts arrive today. So this path is consistent
with the refusal as a *shape* and conflicts with it as a *type invariant*.
The decision "type invariants relating a value to another value" is also
untouched: the shape, not the element type, owns the relation.

**Experiment.** Add the shape to the prelude behind a probe build; compile
(a) loop 85 with `cursor` as its rank, (b) `parent_table` re-encoded, (c) a
control that pushes a self-link (`next == len`), which must be refused at the
push; measure `attribute_is_duplicate` on the tokenizer benchmark before and
after (expected: no change; any change is the reordering of push and head
write).

### P2. Spec-owned linked shapes with proof-only per-node measures (Forest as one row)

**Idea.** Generalize [MSR-1]: a storage shape may own per-element logical
quantities that are functions of its link words, maintained by its
primitives and never stored; each accessor publishes a fixed fact about them
and each link primitive performs whatever runtime check the shape's
invariant needs. `Forest<T>` is the row for ordered trees with parent, child
and sibling links; its measures are `depth`, `height`, `before`, `after`,
`children`, `parent_index`; its primitives do the ancestor check the DOM
standard requires. The trust class is the one [MSR-1] already uses: the
specification's record of the prelude operation is what says `take_back`
lowers `len`.

**Sketch.** ARENA.md Form 1 is the sketch; the one addition worth making is
to define the shape family as a *table*, like the measure table, so that
`Chain` (P1) and `Forest` are two rows of one rule rather than two rules:

```
| shape      | link words                              | proof-only measures                              | link primitives and their check |
| Chain<T>   | next: Down                               | none (the index is the rank)                     | push: target < len, static |
| Forest<T>  | parent, first, last, prev, next          | depth, height, before, after, children, parent_index | append/insert_before: childless child is O(1); otherwise walk parents from the target, at most depth steps |
```

A walk to the root:

```wf
let cursor = start;
loop @up (invariant ranked: forest^[cursor].depth <= start_depth) {   // measure atom, admitted today (probe q1)
  match forest_parent(forest: forest, node: cursor) {
    None() => { break @up; }
    Some(value: above) => { set cursor = above; }   // ensures: forest[above].depth + 1 == forest[cursor].depth
  }
}
```

**Soundness.** The measures are well-defined because the primitives keep the
link words a forest: every parent chain ends, every sibling list is a finite
path, the child lists and parent words agree. That invariant is maintained by
the primitives' code and checked by their runtime cycle refusal, which is the
same invariant Snowghost's `refuse_cycle` maintains today. The published facts
are then true statements about the current link words, and [MSR-2]'s kill
rule already kills them at every write to the forest. One thing the owner
should see stated rather than infer: the ancestor walk inside
`forest_append` would be the first prelude primitive whose own body loops on
data, with its termination resting on the forest invariant holding before
the write. That is sound by induction over the primitives, but it is a new
class of prelude operation; today's window operations are straight-line.

**Sites covered.** DOM parent walks, sibling walks, subtree recursion
(comp 17, with the lexicographic shared rank ARENA.md gives), the layout
`build_children` walk without its counted wrapper and `inconsistent` branch,
`move_all_children` with rank `f[from].children` once `from != to` is a
requirement of the function (see the note below), `match_complex` after P9's
restructuring (rank per call `(i, f[current].depth, f[current].before)` or
the preorder position of P8). Not covered: hash chains and parent tables
(they are not trees; modelling them as trees pays the O(depth) check for
nothing) — that is why the family has two rows.

**`move_all_children`.** The loop as written does not terminate when
`from == to`, so any sound rule rejects it, and the requirement is the
correct repair. There is a representation that removes the requirement and
the loop: `forest_take_children(f, from) -> Run` detaches the whole child run
in O(1) and `forest_append_run(f, to, run)` relinks it, walking the run with
rank `after` to rewrite parent words. That walk is over a detached run, so
no aliasing of `from` and `to` can affect it, and the parent rewrite is the
same O(children) the current loop pays through `append_child`.

**Runtime cost.** Walks: zero (tier 1). Links: the DOM standard's check, O(1)
for a childless child and at most `depth` steps otherwise (tier 2, already
paid in Snowghost). Memory: zero; nothing proof-only is stored. This is the
only path that gives a freely relinkable tree tier-1 walks.

**Spec/checker cost.** The largest of the paths: a new measure-table section
with per-shape rows; four accessors and two to four link primitives with
contracts; Q21 (FN-9 datums over a result-payload offset and over an
`entry(f)[child]` offset). No new rank form beyond the lexicographic one the
tree builder needs anyway. Checking stays L0: every published fact is a
difference bound between measure terms.

**Refusal conflict.** None with the storage-element refusal: the measures are
not quantified facts in the fact language; they are terms with support in
the forest's descriptor storage. The design tree's "no added termination
checker" and "pure promises nothing about termination" decisions are already
slated for rewriting by the mandatory rule.

**Is it "hacky"?** It is special-purpose in the way `Ring` is: one row for
one structure. The argument that it is *principled* rather than ad hoc is
the cost argument in P3: the O(depth) check is cheap only because the four
link words form one tree whose consistency the primitives keep, and no
mechanism that treats the fields independently can exploit that. A generic
mechanism is not a generalization here; it is a slower check.

**Experiment.** Implement `Forest` in the prelude; port `renderer/dom/dom.wf`
(597 lines) to it; compile comp 17, loop 28 with and without the requirement
(the version without must be refused), and the walk-to-root loops; run the
tree-construction oracle and the layout prototype on the same inputs before
and after and compare wall time (expected within noise, since the check
exists today). The one measured quantity that could surprise: the atom
count of the checker's fact state per function when six measures are minted
per node read; record it.

### P3. Generic "acyclic link fields" on an arbitrary arena (candidate 3)

**Idea.** Any arena declares a set of index fields acyclic; the language
keeps the union of those fields acyclic (statically when the source node has
no incoming links, by a reachability check otherwise) and exposes a proof-only
longest-path measure that falls on each link read.

**Sketch.** `acyclic(nodes.parent, nodes.previous_sibling);` on the struct
that owns `nodes`; reads of either field publish
`rank(target) < rank(source)`.

**Where it fails.**

1. *The runtime check is the wrong one.* On a relink whose source has
   incoming links, the language must decide whether the target reaches the
   source over the *declared union*. For `{parent, previous_sibling}` on a
   DOM the set reachable from the new parent is its ancestors and every
   preceding sibling of each of them: O(depth x width), not the O(depth)
   ancestor walk the DOM standard specifies. The ancestor walk suffices only
   because a detached node is nobody's sibling and the child lists agree with
   the parent words — tree facts the generic mechanism does not have. A
   walk that mixes `parent` and `previous_sibling` (`match_complex`) needs
   the union declared, so the DOM would pay the expensive check. This is a
   tier-3 cost on the mutation path.
2. *"Fresh source" is not free.* A source with no incoming links can be
   linked to anything without closing a cycle only if no earlier element
   already holds the source's index in a link word. Link values are
   integers; `nodes[3].next = 7` while `len == 5` is representable unless
   every link write range-checks its target. So the static case needs the
   same write discipline as P1 (targets `< len` at every write), which the
   candidate did not state.
3. *Operation-level facts are still absent.* `children` for
   `move_all_children`, `after` for sibling walks and `height` for subtree
   recursion are tree measures, not longest-path measures of a declared
   union; the candidate covers the parent walk and nothing else on the DOM.
4. *Multi-field unions multiply the reachability work* (already noted in the
   summary), and the declaration has to be repeated per walk pattern.

**Verdict.** Dominated: by P1 for append-only structures (where its static
case is P1 with a heavier spec) and by P2 for trees (where its runtime case
is slower than the standard's check). Its one merit, that a writer can name
any field set, earns its place only for a structure that is neither a chain
nor an ordered tree; that is the reopen condition.

**Experiment.** The discriminating measurement is the union-reachability
walk against the ancestor walk on adoption-heavy inputs (deep trees with
wide sibling lists under the moved nodes); the structural argument predicts
the union walk grows with width and the ancestor walk does not.

### P4. Stored ranks with checked writes

**Idea.** Keep an explicit `rank` word per node and let the only link
primitive compare `rank[target] < rank[source]` at runtime; walks read the
stored ranks as readonly fields and descend on them.

**Sketch.** `struct Node { public readonly height: u32; ... }` and
`link(nodes, child, parent)` that raises `height` along the ancestor chain.

**Analysis.** *Depth* as the rank needs O(subtree) renumbering when a subtree
moves (adoption agency moves subtrees). *Height* needs raising along the new
parent's ancestor chain, which is exactly the cycle-check walk plus a store
per step — no cheaper than P2's check, and it never needs lowering (an upper
bound on height is a valid rank), so it is sound. But siblings have no
height order: `next_sibling` walks need a sibling label, and `insert_before`
then needs a label between two neighbours, the order-maintenance problem
(relabelling runs, Dietz–Sleator style; **unverified** which browsers do
this, but it is a known cost). *Discovery stamps* for the BFS list need a
compare per link write and a stamp word per vertex, work the program does not
otherwise do (tier 3).

**Verdict.** Dominated by P2 on trees (same check, plus memory and stores)
and by P1 on chains (where the index is the stamp for free). It is the right
shape only for a structure whose links are written in an order the program
already tracks and stores for its own reasons; none of the sites is one.

**Experiment.** Stored-height maintenance against P2's check on the same
adoption-heavy inputs, plus the memory per node; the argument predicts equal
walk counts and a strictly larger footprint, so the run would only confirm
the structural comparison.

### P5. Ownership as structure: owned spine, zipper, derived parents

**Idea.** Make the DOM an owned tree (`children: Box<Slots<Node>>`
recursively), so every subtree recursion is C5 for free; keep parent and
sibling links out of the data and navigate upward with a zipper or an
explicit path.

**Where it fails.** Node identity. The tree builder's open-element stack, the
active-formatting-elements list, foster parenting and the adoption agency
all hold `NodeId`s to nodes that other operations move; style caches and
selector `positions` tables key on them too. A path (sequence of child
offsets) is invalidated by every insertion before it; an owned tree with
stable identities needs an id-to-path index that every move must repair.
Upward navigation costs O(depth) path re-walks from the root or a zipper
that can hold only one focus at a time, while the DOM standard's algorithms
hold several. Rust's owned-tree crates avoid the problem only with `Rc`/weak
parents or arenas — the arena being what Snowghost already has.

**Verdict.** Fails performance-first on hot upward walks and fails the
identity requirement. It is the right shape for owned trees that need no
stable identity across mutation (layout contexts, the ordered map), and for
nothing else.

**Experiment.** Upward-navigation cost under a `NodeId`-to-path index on
selector matching (the hottest parent-walk consumer), against the arena's
O(1) parent read; the argument predicts O(depth) per step and index repair
on every insertion.

### P6. Storage length as the universal bound: counted walks and visited sets

**Idea.** A walk that visits distinct nodes of an `n`-node arena takes at
most `n` steps. Either count (`for step in 0..nodes.len`, the idiom
`refuse_cycle` and `build_children` use today) or prove distinctness.

**Analysis.** Proving distinctness statically is a per-slot occupancy fact
("each node visited at most once"), the refused family, so the static form
reduces to a rank (P1/P2). The counted form costs one decrement and compare
per step in the hot loop plus a fall-through outcome the checker cannot
prove dead. The owner refused it (Q16). It remains the honest floor in two
places: `expand_cp` (a cyclic input file is a live failure and a bounded
expansion with an error is the required behaviour; ARENA.md agrees) and the
BFS frontier list (P7 explains why nothing static and free exists there).

### P7. Linear freshness handles (typestate on the index)

**Idea.** The O(1) fast path of the DOM check ("a childless child closes a
cycle only by being the parent itself") has a static form: allocation returns
a `nocopy` wrapper on the index that witnesses "no incoming links and no
children"; `append_fresh(f, parent, fresh: FreshNode)` consumes it and owes
only `parent != fresh.index`, which follows from the allocator's
`ensures made == entry len` and the caller's bound on `parent`. Ownership
does the work; no proof machinery. The same token discharges the P1 push
requirement "the source is the element being placed".

**Sketch.**

```wf
nocopy struct Fresh { public readonly index: u32; }
fn forest_new_node<T>(f: &Forest<T>, data: T) -> made: Fresh writes(f) contract {
  ensures made.index == entry(f)^.len;      // u32/u64 by widening conversion
}
fn forest_append_fresh<T>(f: &Forest<T>, parent: u32, child: Fresh) -> result: u32 writes(f) contract {
  requires parent < f^.len;
}
```

**Move as detach + attach with typestate.** A `Detached` token (the result of
`unlink`) witnesses no parent and no siblings, not no children, and a
detached subtree can still contain the target parent; so it cannot replace
the ancestor check, only `Fresh` (no children, no incoming links) can. That
is why the static fast path is keyed on childlessness, exactly as
`refuse_cycle` keys its constant-time path today. A `Detached` token still
earns something: `forest_append` on a detached node skips the unlink and owes
no "child of another parent" fact.

**Limits.** Freshness in BFS is data-dependent: `distance[neighbor] == count`
is a runtime test, so the token cannot be minted without that test. A shape
that fuses the test and the link (`claim_and_link(marks, v, to)`: if `v` is
unclaimed, mark it, link it to `to`, return true) keeps the invariant "every
link's source was unclaimed when written and every target was claimed", so
claim order is a proof-only rank and the walk is tier 1. But the mark must be
the shape's own word, and the program today folds it into the `distance`
sentinel; the shape either owns `distance` as `Option<u64>` (a tag byte per
vertex unless the representation gives `Option<u64>` a niche, which the spec
does not promise — **unverified**) or carries a separate mark bit. That is
memory the program does not spend today. So for BFS the choice is: a mark
word (tier 2 at best, memory), a counted inner walk (P6, one compare per
step), or a refused fact family. There is no tier-1 static proof. I would
say so plainly in the record rather than leave the site "open".

**Verdict.** Not a path on its own; the cheapest static form of P2's fast
path and of P1's push discipline. Adopt with either.

### P8. Phase separation and representation change

**Idea.** Build the tree in a `Forest` (P2), then freeze it once into
preorder-numbered arrays for the read-only phases: `parent[i] < i`,
`prev[i] < i`, `first_child[i] == i + 1` or none, `next[i] > i`,
`subtree_end[i]`. Every step of every traversal is then index-monotone and
the rank is the index or `len - index` (P1's rule, no new mechanism), and
`match_complex`'s backtracking gets a scalar per frame. The freeze is O(n)
per mutation batch.

**Analysis.** For Snowghost's current pipeline (parse, then style, then
layout on a finished document) this is tier 1 on the walks and the freeze is
one linear pass that current engines' restyle passes already amortize
against. For a live DOM with script mutation between frames it turns every
mutation batch into O(n), which is why real engines keep pointer trees and
incremental restyle; that is a performance commitment the browser target
cannot make in general. The two forms coexist well: `Forest` for the live
tree, the frozen form for bulk traversals. The frozen arrays are also the
faster representation for those traversals (sequential access, four words
per node), so where a phase can afford the freeze this path is not a
concession.

**Also here:** the selector storages merged into one index space (P1) and the
Unicode generator emitting topological order are representation changes of
the same kind: the writer chooses a representation whose links point one
way, and the language needs nothing beyond P1.

**Verdict.** A strong writer pattern for `docs/patterns.md`, not a language
mechanism. It does not replace P2 for the live tree.

### P9. Backtracking over a frame stack (`match_complex`)

**Option A: restructure to recursion.** One call per unresolved combinator;
the frame's `last_tried` is the loop variable of that call's candidate loop,
falling in `depth` (ancestors) or `before` (preceding siblings) under P2, or
in preorder index under P8; the recursion falls in `i`. Rank per call
`(i, position)`, both scalar. The current stack of 64 frames becomes at most
64 nested calls, a depth the compound count already bounds. No new checker
form.

**Option B: a sequence order as a fixed family.** For a loop over a
bounded-capacity window whose element has a readonly rank field, order
states lexicographically by position with an absent position counting as
top; then any push descends, and a pop followed by a push of a strictly
smaller value descends, while a bare pop ascends (correctly: push/pop
alternation must be refused). It is well-founded (a lexicographic product of
`cap` copies of ω+1) and each backedge obligation reduces to L0 facts about
the popped value (`take_back`'s result, a tracked place) and the pushed
value. It is a real fixed family with no search. It is also a new rank form,
a new placement rule for `place_back`'s value, and a `cap`-unchanged side
condition, where Option A needs no new checker form and bounds the recursion
depth by the compound count, a bound the stack capacity already imposes.

**Verdict.** Option A on merit: the same proof with less mechanism. Keep
Option B in the record as the form to reopen when an explicit-stack search
over ranked data cannot be written as recursion (a search whose frames must
outlive the call that pushed them would be the trigger).

### P10. Checker forms the arena-free residue needs

- **Lexicographic ranks** (fixed arity, each component an admitted atom):
  standard, deterministic, needed by the tree builder, comp 17 and the C7
  tokenizer loops.
- **Constant-table ranks (Q19).** A `const` array read at an index the
  checker knows exactly denotes its element. Inside a `match` arm on the
  token kind and on the mode both indices are literals, so
  `R[class][from] > R[class][to]` is a constant comparison: evaluation, not
  search. Probes t2/t3/t4 show today's gap. The alternative — a `match`
  giving the rank as a literal in each arm — loses the fact at the join
  (only common facts survive), which is why the table is the right form. The
  EOF edge additionally needs the template-mode stack nonempty as a guard,
  as ARENA.md records.
- **Shared ranks for mutual recursion.** Each member declares a rank of the
  component's arity in its contract; every intra-component call proves the
  callee's rank at the actuals below the caller's entry rank. The rank is
  part of the callable boundary, like `requires`, so [FN-9]'s withholding of
  same-component summaries is untouched and no circularity arises. Descent
  must follow from the caller's own facts about the actuals — which is
  exactly why the arena facts must arrive from accessors (P1/P2) rather
  than from member postconditions. Function-kind edges: [FN-3] makes the
  actual a compile-time parameter, so the obligation is judged at the
  concrete instance where the edge exists; the module-verdict decision's
  conservative component formation only enlarges components, which only
  withholds summaries and never admits a circular proof. The
  instantiation-cycle rule keeps the instance set finite.
- **Derived ranks from exit guards** (ARENA.md): unchanged by any path here.

### P11. What other languages do, and why it does not transfer

- Dafny `decreases` with tuples is P10; termination over heap structures
  uses `Repr` sets and quantified invariants, discharged by SMT — the
  refused family twice over. SPARK `Loop_Variant`/`Subprogram_Variant` and
  ACSL `loop variant` confirm the written lexicographic form; SPARK's
  ownership makes lists owned (WF's C5) and has no arena story.
- Coq/Agda/Lean structural recursion is WF's C5 for free; well-founded
  recursion via `Acc` is a written rank whose witness, for an array-encoded
  graph, is a ghost proof of acyclicity carried alongside the array — ghost
  data WF has no place for, which is why P2 puts the witness in the shape.
- Liquid Haskell's `{v:Int | v < i}` on an element is P1 as a refinement
  type, proved by SMT; WF gets the same fact from the shape's contract.
- Sized types (Agda, Idris — **unverified** details) index types by a size;
  WF's `len` measures are that for windows, but a DOM height is a runtime
  quantity no const generic can carry.
- Rust arenas (`slotmap`, `generational-arena`, `petgraph`) solve handle
  validity, not termination; generations are P4 without the ordering.
- Koka's `div` is the owner-rejected opt-out. F\*, ATS, Viper's termination
  plugin: **unverified**; as far as I know they add nothing beyond written
  lexicographic metrics plus (for Viper) heap-based measures needing
  permissions reasoning.
- Separation-logic tools (Iris, VST) express linked structures as inductive
  predicates unfolded by the prover: the inductive/quantified family. WF's
  substitute is a spec-owned shape whose primitives are the only writers —
  the same move the prelude makes for `len`.

## 2. Site coverage

| site | P1 monotone | P2 Forest (+P7) | P8 frozen | P9 | P10 | honest residue |
|---|---|---|---|---|---|---|
| DOM parent / sibling walks | – | yes, tier 1 | yes, read-only phases | – | – | – |
| DOM subtree recursion (comp 17) | – | yes, lexicographic shared rank | yes | – | lexicographic | – |
| `move_all_children` | – | yes with `requires from != to`, or the run-splice form | – | – | – | as written it must be refused |
| `match_complex` backtracking | – | after P9 A | after P9 A | A (recommended) or B | – | – |
| hash chains (loop 85) | yes, tier 1 walks; head read uses the existing bounds check | possible, pays a tree check for nothing | – | – | – | – |
| `parent_table` (loop 217) | yes after sentinel re-encoding | – | – | – | – | – |
| selector nested ranges (comp 1) | yes after one index space | – | – | – | shared rank | cross-storage form stays open |
| BFS frontier list (loop 490) | – | – | – | – | – | mark word (memory) or counted walk (P6) or refused family |
| tree-builder reprocess | – | – | – | – | Q19 + lexicographic + template guard | – |
| Unicode tables (comp 8, loop 234) | yes, generator order | – | – | – | – | `expand_cp` validates its input (P6, required behaviour) |

## 3. Ranking, performance first

1. **P1 monotone arenas.** Tier 1 on walks and writes, one shape row, no
   new rank form, consistent with the refusals once framed as a shape.
   Covers four of the nine sites outright and two more with a
   representation change.
2. **P2 Forest as a second row of the same family, with P7's fresh token.**
   Tier 1 walks; the only runtime work is the DOM standard's own check.
   Highest spec cost (measure rows, six accessors/primitives, Q21). Covers
   every DOM site. Nothing cheaper exists for a freely relinkable tree.
3. **P8 freezing** as a documented pattern: tier 1 and faster traversals
   where a phase can afford O(n) per mutation batch; not a language change.
4. **P9 A** (recursion) and **P10** (lexicographic, Q19, shared ranks): the
   checker forms the residue needs; all deterministic, all L0.
5. **P6 counted walk**: the floor for input validation and for the BFS list.
6. **P3 generic acyclic fields**: dominated on cost by 1 and 2.
7. **P4 stored ranks**: dominated; memory and stores for the same check.
8. **P5 ownership-as-structure**: fails identity and upward navigation.

## 4. Recommendation

Adopt one specification rule, **linked storage shapes**, as table data beside
the measure table: each row names a shape, its link words, its proof-only
measures, its accessors' published facts and its primitives' checks. Start
with two rows, `Chain` (P1: append-only, index-ordered, no measures, no
check) and `Forest` (P2: ordered tree, six measures, the ancestor check).
Add P7's `Fresh` token to both as the static fast path. Add Q19 and
lexicographic shared ranks (P10). Restructure `match_complex` as recursion
(P9 A). Record P8 in `docs/patterns.md`. For BFS, state in the investigation
that the site has no tier-1 static proof under the refused-family boundary
and choose between a mark word and a counted inner walk on measurement.

Why one family rather than "Forest alone" or "generic acyclic fields":
`Forest` alone forces chains and parent tables to pay a tree check they do
not need; generic acyclic fields force the DOM to pay a union-reachability
check slower than the standard's. Two rows with one rule form is the
smallest design that gives every census site its cheapest sound check, and
the table form is how the specification already avoids a rule per shape for
`len`/`cap`/`head`.

**Confidence:** moderate-high on the ranking of P1 and P2 over P3–P5 (the
cost arguments are structural, not empirical); moderate on the claim that
two rows suffice, since the census is of programs written to exercise the
compiler; low-moderate on the BFS verdict being the final word (a niche
representation for `Option<u64>` would make the mark word free, and I did
not verify the spec's layout rules for that).

**Evidence that would change it:**
- A measured slowdown of the DOM build after porting to `Forest` (would
  mean the primitives' contracts force extra bounds work; the fix is in the
  primitives' rows, not the path).
- A checker-time blowup from six measure terms per node read (would argue
  for fewer measures per row: `depth`, `height`, `after` are enough for the
  census; `before`, `children`, `parent_index` can wait for a site).
- A real program whose linked structure is neither a chain nor an ordered
  tree (a DAG with sharing, a doubly linked ring with rotation): then a
  third row or P3's generic form earns its place, and P9 B's sequence order
  is the form to reopen for explicit-stack searches.
- A demonstration that the union-reachability check of P3 can be
  specialized to O(depth) for trees without the shape knowing it is a tree
  (I do not see how, but it would remove P3's main cost).

## 5. Experiments, with their criteria fixed first

1. **Chain row.** Criterion: loop 85 and re-encoded loop 217 prove their
   descent with no `use` step; the self-link control is refused at the push;
   the tokenizer benchmark is within noise. Failing the third means the
   push/head reordering costs something and must be measured further.
2. **Forest row.** Criterion: comp 17, the walk-to-root loops and
   `build_children` without its counted wrapper prove; `move_all_children`
   without the requirement is refused and with it proves; the oracle driver
   and layout prototype match their previous output on the six hang inputs
   and the conformance corpus; wall time within noise. Also record checker
   time per function before and after.
3. **Q19.** Criterion: t3 proves, t2 stays unproved (variable index), and the
   tree builder's reprocess loop proves with the 21x13 table and the EOF
   guard; the pre-fix builder at 09d33ba is refused.
4. **P9 A.** Criterion: the recursive `match_complex` passes the selector
   oracle and proves with rank `(i, position)`; its time on the selector
   benchmark is within noise of the stack form.
5. **BFS.** Criterion: measure the counted inner walk against the current
   program on the bfs fixtures; if the difference is within noise, the
   counted walk is the answer for that program and the record says why.

## 6. What was verified and what was not

Verified in the repository: the rule texts cited ([ENT-1], [ENT-2], [MSR-1],
[MSR-2], [INV-1], [PRF-1], [TYPE-11], [STOR-5], [FN-3], [FN-9], [WAIT-1],
[WAIT-2], [OP-10], [SET-1]); the refusals and their reasons in
`design/language/checks-and-proofs.md`; the readonly-field-terms record's
three boundaries; the conformance case
`fn9-neg-readonly-field-below-subscript-relation`; probe `q1.wf` (measure
term with a match-binder offset in a local invariant: accepted); the
Snowghost sites `dom.wf` (`append_child`, `refuse_cycle`, accessors),
`insert.wf` (`move_all_children`), `match.wf` (`match_complex`, `backtrack`),
`tags.wf` (`attribute_is_duplicate`, `attribute_index_insert`),
`css_rules.wf` (`parent_table`), `build.wf` (`build_children`), the WF
`bfs.wf` frontier loop, and the reprocess measure table.

Not verified: whether the compiler represents a match-binder offset in an
FN-9 datum after a Q21 extension (only the current refusal is pinned); the
`header_invariant` placement in the P2 sketch (probe `q1.wf` tested an
`invariant_stmt`; a header placement owes the subscript's bound at the
header on both base and backedge, which was not probed); the
layout of `Option<u64>` (niche or tag byte); the order-maintenance cost
claim for sibling labels in P4; the details attributed to Viper, F\*, ATS,
Idris and Agda in P11; that a topological emission order is available for
the Unicode generator's table (the decomposition relation is acyclic per the
census note, which suffices in principle).
