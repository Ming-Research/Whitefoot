# Checks and proofs

How a Whitefoot program proves each partial operation's domain before it is accepted: the automatic facts, written certificates, contracts, type invariants and range facts.

### decision fixed-families-no-solver
Automatic derivation uses fixed families that always run to completion, and a harder proof is a finite written certificate that the checker checks as written, with no SMT solver.
- rejected: an agent proposed SMT-backed acceptance; the owner rejected it, because acceptance must not depend on solver state, time, machine speed or a work budget.
- rejected: an agent proposed limiting the certificate language to what the automatic families can already prove; the owner rejected it, because a certificate that can only restate an automatic proof adds no proof.

### decision domains-only-from-proof
A partial operation is admitted only where the specification's fixed proof system proves its domain from types, declarations, control-flow edges, verified contracts and checked invariants.
- rejected: an agent proposed writer assertions or runtime traps as a source of a domain; the owner rejected it, because an unproved domain cannot authorize a partial operation.

### decision redundant-certificate-rejected
A nonempty written certificate is rejected when automatic derivation already proves its target [PRF-1].
- replaced: tolerating an unused written certificate as advice, not an error, was on main; the owner changed it, because where automatic proof ends and written proof begins must follow from the language rules, so the writer's choice is tied to the language version, not to compiler behavior.

### decision structural-proof-ceilings
One `proof_use` block holds at most 4096 entries, and one invariant's affine formation at most 4096 expression nodes, 4096 input terms and 4096 result terms.
- rejected: an agent proposed a time or work budget, or no structural ceiling; the owner rejected it, because the specification must fix these ceilings on the source, apart from any machine's capacity, even though the number itself is arbitrary.

### decision bounded-range-facts
Beside affine facts, requirements, postconditions, loop header invariants and struct type invariants may state bounded range facts over element reads, which a separate range judgment proves.
- rejected: an agent proposed quantified invariants that the affine judgment instantiates at each element read and re-establishes at each write; the owner rejected it, because every element write would owe each such fact again, and the fact domain has no element-read terms to carry it.
- rejected: an agent proposed keeping no quantified storage facts and copying level-ordered results back into document order; the owner rejected it, because the copy costs a pass over every element on every style pass, and the renderer needs the document-order write it avoids.
- rejected: an agent proposed a ghost model of the whole document as a forest, proved through library contracts; the owner rejected it, because the parallel passes never need its acyclicity proof, and it adds ghost types, induction and witnesses where range facts add bounded clauses.
- rejected: an agent proposed quantifiers decided by search; the owner rejected it, because binding each fact to the storage version it was stated over lets a fixed derivation establish it again after a write.

### decision readonly-subscripted-terms
A subscripted place is a fact-language term when its last step selects a readonly integer field, a measure or a writer's field alike, and each offset is a tracked place or a constant.
- rejected: an agent proposed admitting every subscripted integer place; the owner rejected it, because its facts would die at almost every element write not proved distinct.
- rejected: an agent proposed admitting only the prelude's measures below a subscript; the owner rejected it, because a writer's readonly field changes by exactly the events that change a measure, and an arena tree could otherwise state no relation about one element's field.
- rejected: an agent proposed a let-bound copy of the field instead of a term; the owner rejected it, because a copy is cut off from the storage it was read from, so a requirement cannot relate an element's field to another storage.
- rejected: an agent proposed admitting any offset and reporting one it cannot represent as a compiler limit; the owner rejected it, because an offset whose changes no term records, such as an element read, lets the place come to select another element with no kill.

### decision one-proved-separation
Index and range separation is proved by the ordinary automatic families and written proof steps, an unproved pair stays overlapping, and a counted loop's `apart` certificate separates iterations.
- rejected: an agent proposed a separate alias solver, or separation that depends on the checker; the owner rejected it, because access and parallel permission must use the same proved distinctness.

### decision result-field-postconditions
An ensures clause may name an integer field or measure reached from a result, or from a routed variant field, through struct fields and Box contents; the caller reads it at its destination.
- replaced: naming only the bare result or its measures in an ensures clause was on main; the owner changed it, because a constructor must state its result's measures, and an identifier validated inside a returned struct made every caller repeat the callee's check.
- rejected: an agent proposed a new projected-datum family or hidden identity fields; the owner rejected it, because a field changes only by events that already kill the facts over it, so no new kill rule is needed.
- rejected: an agent proposed integer-tuple results in place of a struct result; the owner rejected it, because an unrouted clause must hold on the error exit too, each routed clause binds only one field, and the caller's rebuilt struct drops the facts again.
- rejected: an agent proposed admitting result fields without carrying them through placements; the owner rejected it, because a body that builds its result by construction could not prove the clause, and the caller's first rebinding of the struct would drop the fact.
- rejected: an agent proposed carrying every integer field at every placement; the owner rejected it, because carrying only fields whose source place is already a term proves the same facts at less cost.
- rejected: an agent proposed getters with postconditions; the owner rejected it, because a getter's ensures constrains its call's result, not the field at later reads, and calls stay out of contract expressions.
- rejected: an agent proposed struct field atoms in loop invariants; the owner rejected it, because a loop can carry the fields as scalars and build the value after it.

### decision routed-variant-field-postconditions
A written routed postcondition binds one declared data field of any variant of an own enum result, after generic substitution; further fields take separate clauses.
- replaced: writing routes only through Result's `Ok` and Option's `Some` was on main; the owner changed it, because Halo's array-slot and node-slot indices need bounds against different tables, and type invariants already carry per-field conditional relations.
- rejected: an agent proposed routed clauses over several fields; the owner rejected it, because a relation between variant fields needs a shared per-variant context, not the independent per-field contexts chosen here.

### decision widening-conversion-terms
A widening conversion, a bare `cvt` total over its integer pair, stands for its operand's value in a relation term and in a comparison's L0 projection.
- rejected: an agent proposed storing every identifier as `u64`, or a family of conversion terms; the owner rejected it, because relations are over mathematical values, and a `u32` identifier otherwise has no way to relate to a `u64` length.

### decision contracts-name-accessible-paths
Contracts, invariants and proof premises name only accessible paths; getters stay runtime calls, calls stay out of contract expressions, and privacy adds no implicit type invariant.
- rejected: an agent proposed observe modifiers and finite use views, with generated getter equations and a termination checker; the owner rejected it, because published fields give contracts and proofs direct names for the state they need, with no logical function kind.
- rejected: an agent proposed treating a private representation as a proved invariant; the owner rejected it, because a callable's own contract and its types' declared invariants are the only facts that cross a call.
- rejected: an agent proposed checker-only private terms with no spelling in annotations; the owner rejected it, because an external wrapper, loop invariant or certificate must be able to state the condition it requires.
- rejected: an agent proposed annotation-only access to private fields; the owner rejected it, because contracts would then depend on fields the interface does not publish.
- rejected: an agent proposed general mathematical functions as a prerequisite for modules; the owner rejected it, because direct structural contracts cover the chosen witnesses.
- rejected: an agent proposed replacing operations with one setter per private field; the owner rejected it, because separate setters set up no relation between fields and no checked invariant protocol.

### decision struct-type-invariants
A struct may declare affine and range invariants over its own fields, owed at each construction and hand-off and assumed at each function entry; only its declaring module writes its fields.
- replaced: refusing type invariants on structs was on main; the owner changed it, because a relation such as a cursor below its window's length otherwise has to be restated on every function that takes the value.
- rejected: an agent proposed an invariant owed at every construction and field write; the owner rejected it, because a relation between two fields cannot hold while they are written one at a time, and no code but the writing body can see the value between the writes.
- rejected: an agent proposed type invariants that relate a value to another value, such as an arena node's index into another table; the owner rejected it, because a type cannot name the other value, so such relations stay contracts.

### decision explicit-invariant-binder
A type invariant is written `invariant name(binder): left op right;` after its struct's fields, and its relation is judged as a requirement of a function whose one parameter is the binder.
- rejected: an agent proposed an implicit `self` or `this` binder; the owner rejected it, because no other construct has a receiver, so `self` would be a reserved word that names a value in this one clause.
- rejected: an agent proposed bare field names in the relation; the owner rejected it, because a field name alone is a place in no other clause, so judging the relation would need a second name-resolution rule.

### decision invariants-through-enum-payloads
For an own enum parameter or result whose variant field holds a struct with affine invariants, each invariant is a requirement or postcondition only for a value holding that variant.
- rejected: an agent proposed a written contract form saying that every exit of a function is `Err`, alone; the owner rejected it, because every producer of a valid struct would hand-write the invariant, and every function taking a Result or Option of it would restate it.
- rejected: an agent proposed both the generated routes and a written every-exit-is-`Err` form; the owner rejected it, because a generated route that no exit selects already holds vacuously, so the written form would be a second spelling of one fact.
- rejected: an agent proposed establishing an enum parameter's routed invariant at body entry as an ordinary fact; the owner rejected it, because an invariant no value satisfies would make the entry state contradictory for another variant, and every proof in the body would pass unchecked.

### decision invariants-at-substituted-types
A type invariant's function-boundary positions are judged on each parameter's and result's type after generic substitution, so a position written `T` at T = Span takes Span's invariants.
- rejected: an agent proposed judging a direct position by its written type; the owner rejected it, because a wrapper such as `fn wrap<T>(item: T) -> Option<T>` at an invariant struct would owe the invariant at its exit with nothing at its entry.
- rejected: an agent proposed covering a bare type-parameter parameter while judging a bare result by its written type; the owner rejected it, because a result written `Span` would promise the invariant and one written `T` at T = Span would promise nothing.

### decision supplied-callables-promise-no-invariants
A prelude or host callable, whose definition the build supplies, owes its parameters' type invariants as requirements and promises none as postconditions.
- rejected: an agent proposed trusting a supplied definition with generated postconditions as with its written ones; the owner rejected it, because no body proves such a promise, and `take_back` or `remove_at` would hand out a stored element that no write checked.

### decision payload-invariant-positions
The enum-payload invariant rule reaches only an own enum's variant field holding a struct; container elements, nested payload positions, enum references and range invariants are refused.
- rejected: an agent proposed extending the rule to every such position at once; the owner rejected it, because refusing these positions is sound, while carrying the invariant there needs a rule of its own for which writes owe it and which reads receive it.

### decision proof-only-data-is-ordinary
Data that only proofs read is ordinary data with no declaration of its own, and removing its run-time cost is left to the compiler.
- rejected: an agent proposed proof declarations on parameters, bindings and fields, erased in lowering; the owner rejected it, because every name gains a second kind to keep apart, for a saving below 1% of Snowghost's style stage (data: `research/investigations/proof-only-data/DESIGN.md`).
- rejected: an agent proposed proof-only parameters alone; the owner rejected it, because they keep the split for parameters while the data is still computed and stored.
- rejected: an agent proposed a logical model of ghost types, sequences, logical functions and lemmas; the owner rejected it, because the witnesses need only ordinary storage, while the model adds a second language of logical types and functions.
