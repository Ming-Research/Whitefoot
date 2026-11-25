# Range facts

Bounded facts over many elements of storage, how they are written, carried across calls, joins and writes, and how the range judgment proves them.

### decision range-fact-two-binders
A range fact binds one or two variables and states guarded affine relations over constants, bound variables, live integers, measures, element reads and integer values below an element.
- rejected: an agent proposed arbitrary nesting or alternation of quantifiers; the owner rejected it, because the facts the level cascade and scatter producers need are pointwise or pairwise, and every further binder multiplies the instances a derivation forms.
- rejected: an agent proposed three or more bound variables, such as distinctness within each segment; the owner rejected it, because a left inverse over two variables states the same distinctness at a fraction of the instances.

### decision fixed-range-derivation
A range fact is proved where it is owed by one fixed derivation: instances at the values the problem's own reads select, case splits, integer equality solving, read congruence and elimination.
- rejected: an agent proposed an SMT solver with quantifier instantiation; the owner rejected it, because each step of the fixed derivation is bounded by what the problem already holds, so it searches nothing and always ends.
- rejected: an agent proposed written instances for every obligation, as a certificate gives a loop; the owner rejected it, because a preservation proof would restate at every write instances the problem's own reads already select.

### decision some-arm-facts-survive-joins
A fact active at the end of only some arms of a join stays active after it where one of those arms was taken, its instances holding only under that condition.
- replaced: dropping every fact some arm lacks was on main; the owner changed it, because a loop that calls a length-preserving helper in one arm, as v2h's slot move does, needs the helper's ensures on its back edge.

### decision range-problem-size-ceilings
The derivation runs to completion, and only the problem's size is bounded: more than 4096 atoms, or 256 instances of one fact, rejects the site naming the ceiling.
- rejected: an agent proposed ceilings on the branches a derivation opens and the inequalities one elimination holds; the owner rejected it, because both count work that depends on the split and elimination order, so two checkers could accept differently.

### decision tighten-literals-then-rational
The theory tightens each inequality a literal forms over the integers, and then asks only for a rational solution.
- rejected: an agent proposed tightening at every elimination step; the owner rejected it, because elimination that also tightens what it derives refutes some sets in one order and not another, an order the specification does not state.

### decision integer-equality-solving
The theory solves a set's equalities over the integers, by integer changes of variables, so its verdict depends only on the set of literals and a contradiction stays one as literals are added.
- replaced: solving only the equalities that already had a unit coefficient was on main; the owner changed it, because which had one depended on the solving order, so `x == 3z, x == -2y, x == 3` was refuted in one order and open in the other.
- rejected: an agent proposed stating an order of solving in the specification; the owner rejected it, because a verdict would then depend on where the walk places each literal, and an added literal could undo a refutation.
- rejected: an agent proposed solving equalities over the rationals with each literal tightened in the problem's own atoms; the owner rejected it, because it loses refutations such as `i == 2k, j == 2m + 1, i == j`.

### decision split-on-any-open-item
A branch is split on any open item into all of its cases, two reads kept from being one element split over their differing positions, so the verdict is the same in every split order.
- replaced: leaving unsplit a pair kept apart at two positions was on main; the owner changed it, because a refutation was found for a pair differing at one position and not at two, and a pair at strided indices was split again without end.
- rejected: an agent proposed a split order stated in the specification; the owner rejected it, because it would fix the checker's search and keep these differences.

### decision bool-guard-is-true
A range guard tests a Bool datum only as `e is True` or `e is False`, and an `if` on a Bool selection the walk holds enters each block with that tag.
- replaced: a bare Bool expression as a guard was on main; the owner changed it, because the variant relation `e is True` made it a second spelling of the same condition.
- rejected: an agent proposed payload-free variant constructions as comparison operands; the owner rejected it, because it keeps every guard a comparison but refuses the form the writer tests at run time.
- rejected: an agent proposed making payload-free variants named constants, or a Bool literal or integer tag; the owner rejected it, because constants serve no other need, and a literal or tag reverses the chosen data model.

### decision range-walk-operation-relations
An operation-table row the range walk does not name exactly gives a fresh value with the relations its row states between result and operands, under the operand intervals that row reads.
- rejected: an agent proposed adding only `imin` and `imax`; the owner rejected it, because the next masked or halved index would have the same gap.
- rejected: an agent proposed handing the walk every ordinary fact at each point; the owner rejected it, because it couples the two judgments' fact sets and enlarges every range problem.

### decision storage-copy-reads-source-version
An aggregate copied out of storage is new storage defined by the source's version and index tuple at the copy, while later writes make versions of the copy only.
- replaced: a copy out of storage with unknown contents was on main; the owner changed it, because a stored order read with `let item = order^[k];` and matched must keep the relation between the matched payload and the fact stated over `order`.
- rejected: an agent proposed treating the copy as its source; the owner rejected it, because a write to the copy would then be taken for a write to the source.

### decision range-facts-cross-calls
A range fact crosses a call both ways: as the callee's requirement, owed at the call, and as its postcondition, owed at each selecting exit and taken by the caller.
- rejected: an agent proposed range facts that cross a call only as requirements, with the fill constructors' contents as a separate source; the owner rejected it, because a producer's facts then never leave it, so a validating walk could not be one function several passes call.

### decision caller-takes-range-term-relations
After a call, the caller's range walk also takes each callee relation whose two sides are range terms, as a clause without bound variables.
- rejected: an agent proposed restating each such relation as a range clause; the owner rejected it, because writers would get clauses without bound variables only to say one fact twice.
- rejected: an agent proposed letting the range judgment read every fact ordinary entailment holds after a call; the owner rejected it, because it would couple the two judgments at every statement.

### decision routed-range-postcondition-selection
A routed range postcondition is skipped at a return whose value is, or the walk knows holds, another variant, and is owed at every other reached return and at an error exit for an `Err` route.
- rejected: an agent proposed selecting by the constructor tags ordinary entailment carries; the owner rejected it, because the range judgment does not hold them, so it would couple the two judgments.
- rejected: an agent proposed skipping only a construction of another variant; the owner rejected it, because it would owe a payload fact at a return of a value the walk knows holds no payload of that variant.

### decision range-postcondition-must-select
An inhabited instance must select each of its range postconditions at some exit the walk reaches.
- rejected: an agent proposed accepting an unselected range postcondition as vacuous; the owner rejected it, because a postcondition no exit selects promises a caller nothing and is a writer's mistake, such as a route that names the wrong variant.

### decision own-parameters-at-entry-value
A range postcondition names an `own` parameter only by its value at entry, and a written place rooted at an `own` aggregate parameter is refused.
- rejected: an agent proposed reading such a place at entry; the owner rejected it, because the call consumes that storage, so the caller holds nothing a fact about it could describe, and it would need an entry version of every consumed parameter's storage.

### decision entry-storage-of-written-references
A range postcondition may read `entry(p)` storage of a reference parameter whose row writes through p, each selection denoting its entry version.
- rejected: an agent proposed admitting only exit reads; the owner rejected it, because an ordinary reserve helper must relate its exit elements to the elements it received without exposing its body.

### decision unchanged-element-conclusion
A range postcondition may conclude `P unchanged` for an element projection rooted at a written reference parameter, with no equality required of its type.
- rejected: an agent proposed expanding executable equality into projections, or a two-sided element relocation relation; the owner rejected it, because a generic append helper must keep facts about elements that own boxes and have no value equality.

### decision unchanged-by-whole-selection-read
An `unchanged` conclusion is proved by equality of an abstract whole-selection read at its exit and entry versions; any write reaching an owned descendant makes that read unknown.
- rejected: an agent proposed comparing every field value; the owner rejected it, because the version derivation can then certify that no overlapping write happened without a new theory.
- rejected: an agent proposed treating restored equal contents as untouched storage; the owner rejected it, because an element written and then restored must stay unproved.

### decision caller-expands-unchanged-lazily
At the caller, an imported `unchanged` conclusion defines only the existing reads of its exit selection, within the two instantiation rounds and shared ceilings.
- rejected: an agent proposed expanding every possible projection at once, or adding instantiation rounds; the owner rejected it, because a caller needs only the projections it reads, and a generic instance's projections follow its element type.

### decision range-type-invariants
A struct may declare a range fact over its own stores as a type invariant, owed and assumed at exactly the sites an affine type invariant is.
- replaced: live storage carrying no range fact across passes was on main; the owner changed it, because a structure whose writers keep a pointwise relation could not state it where used, and the experiment compiler refused every caller of Snowghost's sibling move.

### decision placed-window-writes
`place_back` and `take_back` are writes the range walk places, and growth keeps facts as the relocation rule says; other calls forget what their rows write.
- replaced: forgetting the window at every window call was on main; the owner changed it, because a writer that appends cannot otherwise show that the element it appended satisfies a fact over the store.

### decision take-back-hands-over-element
The value `take_back` returns is defined by the window's version before the call at the old last index, so its fields, variant and owned parts read that element.
- replaced: taking `take_back`'s result as new storage with unknown contents was on main; the owner changed it, because a writer that moves a prepared element from one store to another had to rewrite every field it had already proved.

### decision generic-range-clause-instances
In a generic function a range clause is formed at the symbolic instance, type-parameter values taken as integers, and again at each concrete instance, where `==` is value equality.
- rejected: an agent proposed refusing range clauses over a type parameter's values; the owner rejected it, because the fill constructors' contents would then stay a rule of their own.
- rejected: an agent proposed a bound that admits only integer types; the owner rejected it, because a generic container's elements need not be integers, and value equality keeps `result[k] == value` meaningful for every equality type.
- rejected: an agent proposed a rule only for the fill constructors, their elements taken as copies of the value; the owner rejected it, because a user's generic function stating `result[k] == value` would still say nothing for a struct.

### decision range-judgment-discharges-leftovers
In a function with range facts, an ordinary comparison obligation that ordinary entailment leaves open is proved by the range judgment at the same point, and rejected only if that fails too.
- replaced: bounding a subscript at a stored position by a runtime guard was on main; the owner changed it, because a run-time test standing in for a fact the writer has stated is a workaround of the kind the language exists to remove.
- rejected: an agent proposed range facts inside ordinary entailment, with element-read terms and overlap kills; the owner rejected it, because every element write would kill a whole fact, and every goal of a function with range facts would pay for it.

### decision sibling-field-writes-keep-values
A write below an element keeps the range judgment's values of the element's other fields and of every other element, and redefines only the projections it overlaps.
- rejected: an agent proposed treating every write below an element as replacing the whole element; the owner rejected it, because two declared fields are disjoint storage, and a stored inverse must survive a write of a sibling field such as a block's position.

### decision variant-tag-integer-read
The variant of an enum selection is an integer read of the same version and index tuple, numbered in declaration order, and a payload read of a variant is defined only where that read equals it.
- rejected: an agent proposed a separate finite case split per variant; the owner rejected it, because with an integer read the equality solving, read congruence and order-free splitting apply to variants with no new kind of open item.
- rejected: an agent proposed struct fields and `Box` content first, with enum payloads later; the owner rejected it, because the reported stored inverse, Snowghost's sibling move, reads its order through an enum payload.

### decision two-instantiation-rounds
Instantiation runs two rounds, the second at the reads the first round's instances add.
- rejected: an agent proposed one round with a diagnostic pointing at the other spelling; the owner rejected it, because a writer's verdict would then depend on which of two equal spellings it chose.
- rejected: an agent proposed rounds until no new read appears; the owner rejected it, because that bounds work only by the ceilings, while a second round stays a fixed step that ends.

### decision match-exclusion-gaps
The range walk records an integer match's remaining arm as one exclusion set over the scrutinee's value, and an open exclusion splits into the set's sorted integer gaps.
- rejected: an agent proposed an independent binary split for each excluded equality; the owner rejected it, because those enumerate combinations that contradict each other, while the gaps cover the exact complement with one more case than listed values.

### decision variant-relation-is-v
A range guard or conclusion may state that an enum datum holds a variant, `e is V`, or does not, `e is not V`.
- rejected: an agent proposed `e == V` and `e != V`; the owner rejected it, because `==` is the one value equality, and a tag comparison under it would hold between unequal values of one variant.
- rejected: an agent proposed a pattern form such as `e is V(..)` or `matches(e, V)`; the owner rejected it, because a range clause reaches payloads through `.V.f` and binds nothing, so the parentheses would suggest a binding that does not exist.
- rejected: an agent proposed an integer tag term such as `e.tag == 2`; the owner rejected it, because it exposes declaration order to writers.

### decision conclusions-own-definedness
Each conclusion of a range clause holds where its own reads and the reads of the clause's ranges and guards are defined, not the reads of the other conclusions.
- replaced: one shared definedness condition for each clause instance was on main; the owner changed it, because a clause listing each branch variant's target bound was accepted and claimed nothing, since no element holds every variant.
- rejected: an agent proposed asking writers to split such a clause into one clause per variant; the owner rejected it, because it multiplies instances and leaves the vacuous form silently accepted.

### decision impossible-conclusion-error
A conclusion whose instance needs two variants of one element, or a variant its guard excludes, is a hard error, elements being compared only as identically written projections.
- rejected: an agent proposed accepting the vacuous conclusion silently; the owner rejected it, because it hides a writer's mistake such as `code[j].Jump.t < code[j].Branch.t`.
- rejected: an agent proposed deciding which index terms name one element; the owner rejected it, because two subscript terms that are merely equal would need the derivation to decide equality.

### decision payload-binder-referent
In a goal the range judgment reads, the referent of a payload binder that a `match` through a reference binds below an element is that element's payload projection.
- rejected: an agent proposed requiring the payload to be copied into an own-mode binding first; the owner rejected it, because it refuses the natural form, which branches on the variant and uses the field, for a value the walk already reads.