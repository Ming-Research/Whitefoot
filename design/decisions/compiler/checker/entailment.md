# Entailment

The entailment engine that discharges the checker's obligations: its parts, how its proof queries are prepared, and how it closes and keeps its difference-bound fact states.

### decision the-flow-analyzer-is-split-by-writer
The flow analyzer holds typed sub-contexts by writer and is split into modules by area, with one `prove` dispatcher, one ledger, one walker and one fact-state closure.
- replaced: one analyzer of 37 fields with a 15,040-line impl was on main; the owner changed it, because shared mutable state, not call structure, held it together, so moving its methods between files would change nothing a reader could rely on.
- rejected: an agent proposed splitting `flow.rs` along its section markers or its line counts; the owner rejected it, because child modules reach every field through `use super::*` and the markers no longer match the code, so the cuts would fall in the wrong places.
- rejected: an agent proposed splitting the analyzer into separate passes; the owner rejected it, because each pass would need a second event order.

### decision proof-queries-reuse-a-prepared-view
Proof queries over the same entering facts and value images reuse one L0 closure and ordered affine index, rebuilt when term information or existing goal metadata changes.
- replaced: preparing the closure again for each premise query was on main; the owner changed it, because the query-cost study found that repeated preparation was the cost (data: `research/investigations/proof-certificate-architecture/CHECKING-COST.md`).
- replaced: rebuilding a closed view whenever goals changed was on main; the owner changed it, because checking only appended goals for a new contradiction keeps every judgment and lets the redundancy queries share their preparation.

### decision affine-postcondition-sums-are-ordinary-terms
The one-parameter affine side of a postcondition is an internal sum term in the ordinary relation and proof-publication path.
- rejected: an agent proposed a second publication domain for postconditions; the owner rejected it, because the sum term keeps the ordinary return, caller, kill and retained-evidence machinery.
- rejected: an agent proposed an inference rule for the scan alone; the owner rejected it, because the sum term keeps the ordinary return, caller, kill and retained-evidence machinery.

### decision affine-queries-demand-their-index
Affine queries and certificates share a memo of unchanged states, and on a miss pair only the candidates a query demands, building the complete index when the final family is reached.
- replaced: rebuilding every candidate pair eagerly was on main; the owner changed it, because demanding the index cut the wasm interpreter's check to 0.215 of its base, where reuse alone recovered about 10% (data: `research/investigations/check-time/DESIGN.md`).
- rejected: an agent proposed pruning the pairs by overlap with the target; the owner rejected it, because it loses disjoint images that compose.
- replaced: demanding every vector lazily through the final family was on main; the owner changed it, because it made Halo's check 12-13% slower than the complete index (data: `research/investigations/check-time/DESIGN.md`).

### decision a-refuting-integer-point-ends-the-search
Before AUTO's final L0-image family, the prover looks for an integer point that meets every premise it reads; when that point makes the target false, the query ends unproved.
- rejected: an agent proposed memoizing whole AUTO results; the owner rejected it, because the residual targets of step 6 never repeat.
- rejected: an agent proposed bounding the traversal's work; the owner rejected it, because a cap would then select acceptance.

### decision op5-truth-views-share-only-when-exact
OP-5's True and False queries share one origin view only when the negation is a new class of its own and the view is not contradictory; preparation caches use exact keys.
- rejected: an agent proposed sharing one view for both truths of every condition; the owner rejected it, because it can bring in a contradiction and judge a condition that should be allowed as always true.
- replaced: caches keyed by the closed state's allocation was on main; the owner changed it, because keys of the exact inputs keep every disposition and retained parent, as the exact-recovery argument shows.

### decision the-final-affine-family-skips-failing-images
The final AUTO family skips an image only where a recorded deduction shows it cannot prove the target, and judges every target afresh with no cache of failed queries.
- rejected: an agent proposed skipping images that share no atom with the target; the owner rejected it, because a disjoint image can supply a GCD-tightened exact-L0 complement, and disjoint terms can repair an overflowing target interval.
- rejected: an agent proposed reusing the answers of failed AUTO queries; the owner rejected it, because the hosted measurement found only 310 exact repeats and no gain beyond noise (data: `research/investigations/check-time/AFFINE-BRIDGES.md`).
- rejected: an agent proposed certifying countermodels in this change; the owner rejected it, because the bounded shadow probe left all 176 observed groups unknown (data: `research/investigations/check-time/AFFINE-BRIDGES.md`).

### decision closure-products-scan-contiguous-rows
Each transitive product of the ENT-4 fixed point runs over contiguous bound, proof and freshness rows and scans only the fresh middle-row columns, keeping its order and ties.
- replaced: boxed optional cells behind asserted indexes was on main; the owner changed it, because billions of rejected triple visits made the cost, and contiguous rows with stale-column scans gave about 1.4x (data: `research/investigations/proof-certificate-architecture/CHECKING-COST.md`).

### decision closure-skips-products-that-cannot-improve
The fixed point skips a transitive product only when conservative numeric and proof-depth row bounds prove it cannot improve a fully populated destination row.
- replaced: visiting every product that cannot improve its row was on main; the owner changed it, because applying the scalar rejection tests to whole products saved the work the row comparison isolates (data: `research/investigations/proof-certificate-architecture/CHECKING-COST.md`).

### decision fact-maps-use-the-fixed-word-hasher
The fact, closed-state, candidate, term-index and affine maps hash with the fixed word hasher of the derivation intern index.
- replaced: SipHash with a seed per process was on main; the owner changed it, because hashing small keys with it was a leading cost of real programs, and its order was already random, so no output depended on it (data: `research/investigations/proof-certificate-architecture/CHECKING-COST.md`).

### decision fact-states-close-incrementally
A fact state records its completed ENT-4 closure and repairs it in place for new relations, fresh or killed terms and removed candidates, falling back to a seeded fixed point.
- replaced: recomputing the complete closure after every kill, join and query was on main; the owner changed it, because the closure study found repeated complete closure to be the dominant cost (data: `research/investigations/proof-certificate-architecture/INCREMENTAL-CLOSURE.md`).
- replaced: the complete unseeded fixed point for every state with no recorded closure was on main; the owner changed it, because closing from the remembered view took the slowest Snowghost module check from 41 s to 23 s (data: `research/investigations/compile-speed/DESIGN.md`).
- rejected: an agent proposed a second difference-bound implementation with its own kill rules; the owner rejected it, because one flow implementation already records changed terms and cells, and a copy would be a second source of acceptance behavior.

### decision equal-bounds-may-keep-different-witnesses
Equal L0 bounds may keep different valid derivations, and a join first reuses a shared explicit proof that every non-contradictory predecessor selects.
- rejected: an agent proposed forcing the witness the complete traversal selects; the owner rejected it, because ENT-4 fixes what is derivable, not which witness is chosen.

### decision joins-stop-at-the-implicit-bound
A join stops scanning a pair's predecessors at the first input that reaches the pair's implicit-only closed bound and records an implicit derivation for it.
- replaced: scanning every input and building join parents for each pair was on main; the owner changed it, because every closed state already entails the implicit facts, so the weakest common bound cannot exceed that value, and doubling the arms cost about ten times the check.

### decision bounds-are-dense-matrices-over-own-terms
Selected bounds and closed views are dense matrices over their own terms in term order, and the term table keeps its implicit structure up to date from a log of changed terms.
- replaced: hashed bounds rebuilt at each closure, materialization and join was on main; the owner changed it, because map building, key sorting and closing unchanged states were the remaining costs (data: `research/investigations/proof-certificate-architecture/INCREMENTAL-CLOSURE.md`).
- replaced: matrices over every registered term, with implicit bounds derived again at each closure, was on main; the owner changed it, because these dominated a straight-line body (data: `research/investigations/compile-speed/DESIGN.md`).
- rejected: an agent proposed sharing matrix rows between the states that meet at a join; the owner rejected it, because a kill or materialization in one arm rewrites a column in every row, so few rows would still be shared.

### decision snapshots-leave-implicit-proofs-unwrapped
A snapshot keeps a closed relation's proof unwrapped when it rests on ENT-2 implicit bounds alone, and wraps every other relation, while S11's preheader snapshot wraps every relation.
- replaced: a materialized node for every closed cell at every snapshot was on main; the owner changed it, because wrapping the nearly n-squared implicit cells dominated the slowest real functions, though no kill or join can remove them (data: `research/investigations/compile-speed/DESIGN.md`).

### decision closures-compute-rows-for-terms-with-facts
A closure computes rows only for zero and the terms a live fact reaches through more than zero, and reads every other term through zero from its implicit bounds.
- replaced: rows for every registered term was on main; the owner changed it, because Halo's widest closures held 879 terms, only 263 with facts, and a path through a term with no fact can go through zero instead (data: `research/investigations/compile-speed/DESIGN.md`).
- rejected: an agent proposed rows for every endpoint of a stored cell and of every implicit edge; the owner rejected it, because snapshots storing implied cells kept every earlier arm's terms in the closure (data: `research/investigations/compile-speed/DESIGN.md`).
- replaced: the contradiction probe of a state with no recorded closure over all terms was on main; the owner changed it, because it kept checks cubic, and the wasm interpreter's check fell from 73.71 to 3.89 s (data: `research/investigations/proof-certificate-architecture/INCREMENTAL-CLOSURE.md`).
- rejected: an agent proposed leaving out single cells implied through zero between terms that have a row; the owner rejected it, because kills, joins and delivery would each need a separate argument for the cells left out.