# Range judgment

The walk over the checked program that proves range facts after ordinary entailment: its state, joins, loops, solver and the obligations it takes over.

### decision range-judgment-is-a-separate-post-pass
The range judgment is its own forward walk over the checked program, run after every function's ordinary entailment, with its own state of storage versions and joins and its own fixed solver.
- rejected: an agent proposed keeping range facts as new families inside the entailment flow's fact domain; the owner rejected it, because every element write would kill them through the ordinary overlap events, and the flow's families relate no element reads.

### decision range-walk-reads-recorded-operand-intervals
At each [ENT-3.S7] operation in a function that takes part, ordinary entailment records in the proof receipt the operand intervals S7 read, and the walk forms that row's relations from them.
- rejected: an agent proposed that the walk compute the intervals from its own state; the owner rejected it, because those differ from the ones S7 reads and would change which row conditions hold.
- rejected: an agent proposed that ordinary entailment hand over the derived relations; the owner rejected it, because that would fix the walk's result names inside the flow.

### decision range-walk-joins-through-selectors
The walk forks at a branch and joins each differing value through one selector per join; a loop header forgets what dry walks of its body write, and walks again until nothing new is recorded.
- rejected: an agent proposed enumerating paths; the owner rejected it, because one walk keeps the cost linear in the body while the selectors still give each path's write and condition to a backedge proof.
- rejected: an agent proposed taking the forgotten header state after a loop; the owner rejected it, because that loses the loop's invariants, while a fact the loop establishes is needed after a loop left only by a `break`.
- rejected: an agent proposed forgetting every container at a loop header; the owner rejected it, because a set of written storage taken from the body forgets only the storage the body touches.

### decision atomic-targets-checked-per-partition
At an atomic statement the range walk checks every partition of the possibly aliased whole-object targets, giving the targets of one object one storage location, with no work cutoff.
- replaced: reporting such a statement as an unsupported compiler capability was on main; the owner changed it, because checking each allowed partition covers both cases the specification allows, and no known program pays the cost.
- rejected: an agent proposed treating possible aliases as always the same object, or always distinct; the owner rejected it, because [SHARE-2] permits both, so either one misjudges one of the two cases.
- rejected: an agent proposed adding conditional alias handling to every kind of mutation; the owner rejected it, because that changes far more of the walk, while one location per object lets the existing write, call, branch and exit judgments keep the relation.

### decision guarded-facts-survive-a-join
A fact some arms of a join hold keeps, for each arm that lacks it, the guard that the join's selector differs from that arm; a bare selector is split over the arm numbers alone.
- rejected: an agent proposed one guarded copy of the fact per arm that holds it; the owner rejected it, because that multiplies the rules each tuple adds.
- rejected: an agent proposed splitting a bare selector over the arms' full conditions; the owner rejected it, because that asserts an inner join's conditions on paths that bypassed it.

### decision joins-record-their-bypasses
A join records each earlier join that some of its arms bypassed, and a value that join selects gains one `selector == arm` alternative per bypass, transitively.
- rejected: an agent proposed a free not-reached alternative for a bypassed join; the owner rejected it, because it would also be open on paths that passed the join and lose its arms' values there.

### decision range-walk-depth-ceilings
The walk follows loop nests eight deep and gives one header eight walks; beyond either it forgets everything, and a fact then left unproved is an unsupported capability, not a rejection.
- rejected: an agent proposed following loop nests to any depth; the owner rejected it, because each nesting level multiplies the dry walks of the levels inside it.
- rejected: an agent proposed rejecting a program when a ceiling is reached; the owner rejected it, because a verdict reached by forgetting more is the checker's limit, not the language's.

### decision range-solver-backjumps
The solver records the case splits each literal and each contradiction rests on, and a branch refuted without its own split's case closes that split's other branches untried.
- rejected: an agent proposed the full case analysis; the owner rejected it, because it multiplied independent splits, and `owner_loop.wf` took 12.3 to 12.7 s against 0.96 to 1.03 s with every case output unchanged (data: `research/investigations/unique-keys/POINTWISE.md`).
- rejected: an agent proposed a memo of refuted literal sets; the owner rejected it, because it was slower (data: `research/investigations/unique-keys/POINTWISE.md`).
- rejected: an agent proposed splitting first on the read pairs the open conclusion's atoms reach; the owner rejected it, because that orders the same tree without pruning it.

### decision element-atoms-carry-projections
An element atom carries its struct-field and Box-content projection; a located write redefines overlapping projections and keeps disjoint projections and other index tuples.
- rejected: an agent proposed conflating an element's projections; the owner rejected it, because a stored inverse field must survive writes of sibling payload fields.
- rejected: an agent proposed forgetting every field of an element on each sibling write; the owner rejected it, because a stored inverse field must survive writes of sibling payload fields.

### decision formed-range-call-footprint
A call through a formed range uses the first captured range as its write footprint and keeps the complete frames only as diagnostic provenance.
- rejected: an agent proposed versioning storage by the full path; the owner rejected it, because how such versions correspond to the fixed derivation's read congruence and accounting had not been established.
- rejected: an agent proposed reporting a requirement whose reads lost their provenance as unsupported; the owner rejected it, because that changes the prover's core join and provenance accounting for an interim report that the real fix replaces.
- replaced: Forgetting the whole element a call's argument names was on main; the owner changed it, because the call was the one place that did not tell paths apart, so a call writing only `.g` lost the facts on `.f`.

### decision representation-limits-judged-per-obligation
A call's representation limit is diagnosed per failed obligation from its own conclusion, bounds, guards and read dependencies, and the restoring queries never help acceptance.
- replaced: Replacing a function's whole verdict after any unresolved write was on main; the owner changed it, because losing precision at one write does not explain an unrelated false requirement.

### decision range-judgment-discharges-deferred-obligations
Ordinary entailment runs first; the walk then proves each undischarged deferred record whose goal compares range terms, and the ordinary rejection selector runs over the rest unchanged.
- rejected: an agent proposed deferring every shape of record and reporting the ones the walk cannot express as unsupported; the owner rejected it, because that turned ordinary rejections into checker gaps.

### decision range-scope-symbolic-or-concrete
The range walk takes its scope explicitly: symbolic validation still discharges deferred ordinary obligations, while range-clause obligations are judged only at concrete instances.
- rejected: an agent proposed rejecting a generic body for a range clause; the owner rejected it, because [RANGE-1] forms the clause symbolically while a concrete substitution decides what it states, and its concrete instance may omit it.
- rejected: an agent proposed turning the symbolic range walk off; the owner rejected it, because deferred ordinary obligations need it.

### decision referenced-scalar-bindings-are-exposed
A scalar binding the walk holds as a value becomes exposed when a reference to it is formed, and every write the walk cannot place and every `atomic_stmt` forgets exposed bindings.
- rejected: an agent proposed giving a borrowed scalar binding a walk location, so that a write through the reference is placed; the owner rejected it, because it is more precise but changes how the walk models every binding.

### decision entry-places-use-the-ordinary-clause
An entry storage place has an entry-qualified binding root, and an `unchanged` conclusion is an equality of exit and entry reads with a whole-selection marker.
- rejected: an agent proposed a second clause representation for `unchanged`; the owner rejected it, because the ordinary one keeps its binder, guard, definedness and instance accounting, and the marker states a version claim without a type equality.

### decision entry-places-frozen-before-effects
The range walk freezes the places an entry-qualified postcondition names at function entry and before a call's effects, and forms owed or imported clauses from those frozen views.
- rejected: an agent proposed resolving an entry-qualified place against the exit state; the owner rejected it, because a reference argument's location and descriptor can change during a write.
- rejected: an agent proposed relying on the ordinary entailment walk's measure snapshots; the owner rejected it, because a retained value alone does not keep the version it first denoted.

### decision imported-frame-equalities-expand-on-demand
An imported whole-selection frame equality is expanded only for the projection reads already present in the problem, within the same instantiation rounds and allowances.
- rejected: an agent proposed adding every projection of the element type at once; the owner rejected it, because it would charge for fields the caller never reads.

### decision copied-containers-get-fresh-versions
A copied container gets a fresh version whose value projections come from its source, while its whole-selection identity stays unknown.
- replaced: A copy sharing every projection of its source's version was on main; the owner changed it, because a copied Array assigned back writes the whole container and could then prove its elements unchanged even when they were rewritten and restored.

### decision integer-match-exclusion-as-one-literal
The range world represents an integer match's `else` condition as one whole-set literal over the scrutinee and a shared sorted exclusion set, counted per [RANGE-3].
- rejected: an agent proposed an auxiliary zero-or-one predicate for the exclusion; the owner rejected it, because its atom and its inactive case exceed the counts [RANGE-3] specifies.
