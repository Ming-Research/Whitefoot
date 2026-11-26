# Checker facts

The facts the checker keeps for the entailment engine: places and their identities, reference validity, terms and goals, kills and preservation, conditional enum states, delivery and match selections.

### decision checked-places-share-one-root-and-step-form
Checked places use one typed root-and-step form for reference resolution, effects, proof support and invalidation, keeping every possible target through joins.
- rejected: an agent proposed separate place identities for fields, contracts and references; the owner rejected it, because these consumers must agree on the target, and sharing one reading saves rebuilding it from the expression layout.
- rejected: an agent proposed reducing a joined reference to its first target, or taking a containing prefix as the exact target; the owner rejected it, because safety checks must cover every possible member, and overlap does not show that targets are equal.

### decision captured-offsets-have-stable-occurrence-identities
A captured offset pairs a term class with a stable occurrence identity: its offset syntax node, a finite loop-carried identity, or a call and position for an erased contract position.
- rejected: an agent proposed reading the mutable source binding again; the owner rejected it, because the checker must not equate values from different dynamic iterations.
- rejected: an agent proposed a new identity on every checker visit; the owner rejected it, because a repeated visit must recognize one position.
- rejected: an agent proposed a separate solver for contract positions; the owner rejected it, because contract-read separation must reuse the ordinary proof families without evaluating proof syntax or publishing caller facts.

### decision projected-offsets-use-interned-place-ids
Projected offsets use `CapturedPlaceId` for root-and-step proof paths and `FormalOffsetId` for parameter-relative suffixes, in one interned table of the compilation.
- rejected: an agent proposed reducing a projected offset to its root binding; the owner rejected it, because that would conflate a field with its sibling and a captured value with a later read.
- rejected: an agent proposed embedding recursive paths in each capture; the owner rejected it, because compact copyable ids let parallel checking and contract substitution share structural identity and complete support.
- rejected: an agent proposed separate identity and support tables for each consumer; the owner rejected it, because one shared table gives every consumer the same identity and complete support.

### decision reference-preservation-is-judged-at-the-use
Destructive writes add finite preservation questions to the reference-validity lattice, and only a use of the reference submits them to the flow ledger at their write sites.
- rejected: an agent proposed demanding separation at every write that could invalidate a reference; the owner rejected it, because a reference may legally become invalid when no later use depends on it.
- rejected: an agent proposed a separation cache for the whole function; the owner rejected it, because a proof about immutable captures holds only in the flow context that made it, which a sibling arm or a zero-trip loop cannot inherit.
- rejected: an agent proposed checking every structural body again after entailment; the owner rejected it, because ordinary functions need no replay, and the existing replay ends before the whole function inventory is analyzed.
- rejected: an agent proposed proving preservation at the reference use; the owner rejected it, because a later guard or postcondition cannot restore a validity fact an earlier write removed.
- replaced: invalidating every window reference at any call that writes the window's length, end or filled part was on main; the owner changed it, because the specification keeps a reference valid while its formation bounds hold.

### decision loop-headers-grow-path-summaries
The typed structural walk grows finite loop-header path summaries and restarts on a strict summary change before it publishes its checked result.
- rejected: an agent proposed a second path interpreter over syntax alone; the owner rejected it, because reusing place resolution keeps effects, invalidation and aliases on one semantic path.
- rejected: an agent proposed a pass budget on acceptance; the owner rejected it, because finite roots with shortening prefixes bound convergence.

### decision the-structural-checker-publishes-reference-origins
The structural checker publishes the paths it resolved for each reference holder in its final traversal as a function-wide origin inventory, which place summaries read.
- rejected: an agent proposed rebuilding origins from the spellings of checked expressions; the owner rejected it, because an incoming parameter root names entry storage even after the parameter is rebound.
- rejected: an agent proposed a depth limit on reference expansion; the owner rejected it, because replaying assignments as path equations can invent unbounded recursive descent, while the walk's resolved paths are finite.

### decision reference-places-carry-their-exact-path
A place written through a reference variable carries that variable's exact path at the occurrence as an optional proof base, which only term and Goal formation read.
- rejected: an agent proposed resolving the path through the origin inventory when a term is interned; the owner rejected it, because that inventory is not point-current, and only the structural walk knows the path at the occurrence.

### decision measure-terms-and-goals-share-support
Measure terms and goals share typed descriptor-word support and the ordinary resolved-place overlap machinery for kills.
- rejected: an agent proposed separate kill summaries for terms and goals; the owner rejected it, because they could keep an obsolete length beside a new one and falsely discharge later obligations by contradiction.
- rejected: an agent proposed one identity relation for every term; the owner rejected it, because it would either split a repeated term or equate references formed across a write.
- rejected: an agent proposed instantiating a call at the reference holder once a written binding no longer names its offset; the owner rejected it, because FN-8 and ENT-2 instantiate a reference formal at its resolved referent.

### decision growth-writes-are-relocations
Canonical prelude growth calls mark their proof write events as relocations, keeping their full resolved write places, and every fact component reads that mark.
- rejected: an agent proposed narrowing the effect of a growth call; the owner rejected it, because effects and reference validity must still see whole-cell writes.
- rejected: an agent proposed restoring selected facts after an ordinary destructive kill; the owner rejected it, because the relocation rule already keeps the facts of unchanged logical storage.

### decision reference-rebinding-is-its-own-event
Rebinding a reference has its own proof invalidation event, applied to each component's reference-holder support and kept unchanged by continuing-loop summaries.
- replaced: a synthetic whole-holder write resolved through the origin inventory was on main; the owner changed it, because REF-1 replaces only the holder and writes no storage, while that write killed facts about its old and new targets.

### decision ranges-have-their-own-checked-data
A range has its own checked formation and length data and a typed element place keeping every suffix; its measure is the captured endpoint difference.
- rejected: an agent proposed treating a range as an owned Array; the owner rejected it, because a pointer-and-count reference has no owned container descriptor.

### decision call-overlap-questions-go-to-the-flow
The structural checker owns effect covering, actual-path substitution and call-site overlap questions, and the flow proves each index or range question in the call's state.
- rejected: an agent proposed judging the substituted rows again in the flow; the owner rejected it, because the checker has the source modes and reference validity, while the flow has the arithmetic facts.
- rejected: an agent proposed a function-wide memo of separation proofs; the owner rejected it, because a proof about immutable captures holds only in the flow context that made it, which a sibling arm or a zero-trip loop cannot inherit.

### decision writes-ask-separation-on-the-same-place
Before a write removes facts, it asks index and range separation questions against live support on the same containing place, in the write's own flow context.
- rejected: an agent proposed relying only on separations that calls or references asked earlier; the owner rejected it, because ENT-5 keeps support that OWN-7 can prove disjoint even when no earlier judgment asked the question.
- rejected: an agent proposed publishing a proof about current values as an immutable capture relation; the owner rejected it, because a later rebinding of the index can invalidate the separation.
- rejected: an agent proposed leaving out the questions of support that already survives; the owner rejected it, because its answer can keep another support whose containing place cannot ask that question itself.

### decision lowering-keeps-declared-whole-state-coverage
Lowering records the declared whole-state read and write coverage of reference formals apart from their having no writes, and uses it only for scoped-access metadata.
- rejected: an agent proposed letting the backend infer full separation of physical accesses from readonly status; the owner rejected it, because a partial row and readonly status do not cover every descriptor access the compiler generates.

### decision boundary-clauses-keep-ordinals-and-paths
Boundary clauses keep parameter and result ordinals with their complete projection paths; a call substitutes its actuals, while the body reads its own bindings.
- rejected: an agent proposed rebuilding clauses from rendered source; the owner rejected it, because declaration templates must survive without a caller's binding identities, and a result projection must continue below its destination.

### decision allocation-is-a-checked-program-bit
Allocation is a separate checked-program bit, propagated to a least fixed point over the whole call graph from the five compiler-owned allocating declarations.
- rejected: an agent proposed deriving allocation from a writes row, from signature snapshots in source order or from the postcondition schedule; the owner rejected it, because effect rows have no allocation category, and forward calls, recursion and bound actuals still need the fact.

### decision equality-invariants-store-one-inequality
An equality invariant target stores one normalized inequality with a reverse flag, and proves and publishes both members together.
- rejected: an agent proposed storing two targets named apart; the owner rejected it, because the reverse has no written form of its own, and every invariant and certificate consumer would have to choose a member.

### decision value-placement-stops-at-a-repeated-nominal
Measure and integer-field placement follows owned fields, payloads and Box content, stops at a repeated nominal, and adds exact paths from the term inventory for recursive operands.
- rejected: an agent proposed unfolding recursive types without end; the owner rejected it, because the finite term inventory already holds the earlier numeric evidence a placement can carry.
- rejected: an agent proposed a value datum for every integer field at every placement; the owner rejected it, because a field that is no term has only its type's standing bounds, which hold at the destination without transport.

### decision symbolic-constants-keep-their-declared-type
A symbolic integer constant's term and affine image use its declaration identity and exact declared integer type, from one shared read-only inventory.
- rejected: an agent proposed giving every checked extent and captured-index constant a copied type; the owner rejected it, because a copy would be a different constant whose facts no longer meet the original's.

### decision own-enum-routes-keep-isolated-fact-states
The entailment walk gives each live local own enum an isolated conditional fact state for each admitted variant-field route, over a private payload root.
- rejected: an agent proposed instantiating a saved call's contract again when its outcome is matched; the owner rejected it, because mutable arguments may denote different values by then, and a dead support cannot be revived.
- rejected: an agent proposed combining the states of distinct routes or live outcomes before their variants are selected; the owner rejected it, because each state holds only for its own value and variant.

### decision route-states-only-for-demanded-values
Only values a later reader demands keep their conditional route states, while a Result with an integer success payload keeps them at every value.
- replaced: building route states wherever evidence first arose was on main; the owner changed it, because states nothing reads feed no proof or required root, and the Halo VM test host's check grew from about 2 s to 630 s (data: `research/investigations/enum-route-check-cost/DESIGN.md`).
- rejected: an agent proposed a conditional state for every admitted enum value; the owner rejected it, because values carrying no evidence would add closure and join work while proving nothing an absent state does not.

### decision result-evidence-is-a-flow-child-module
Result evidence construction and transport live in a private child of the flow module, while the walker keeps the event order.
- rejected: an agent proposed another analysis pass; the owner rejected it, because the shared flow already owns the call authority and the invalidation consumers the mechanism needs.
- rejected: an agent proposed writing every conditional-state operation inline in the statement walker; the owner rejected it, because the mechanism has one coherent representation of its own.

### decision result-transport-reuses-closure
Result transport reuses completed numeric closure through the copy-on-write fact stores, keeping isolated contexts, complete candidate sets and ordinary fallbacks.
- rejected: an agent proposed rebuilding numeric states for transport; the owner rejected it, because avoiding repeated closure and snapshot work meets the measured targets (data: `research/investigations/result-proof-transport/DESIGN.md`).
- rejected: an agent proposed projecting to the selected payload bounds or guessing the last use; the owner rejected it, because either needs a broader precision or lifetime argument, while closure reuse already meets the target.

### decision value-initializer-delivery-is-lean
Value-initializer delivery materializes only the relations the continuation's closure cannot derive, and a literal carrier's equality enters the edge's closure incrementally.
- rejected: an agent proposed materializing every joined consequence under a raised proof-size ceiling; the owner rejected it, because the roots grow with each literal initializer times the terms, tripling wfgrep's proof (data: `research/investigations/writer-lost-facts/DESIGN.md`).
- rejected: an agent proposed a full closure for each literal edge; the owner rejected it, because the given value adds one equality the closure inserts incrementally, and the full closure slowed wfgrep's check by 29 percent (data: `research/investigations/writer-lost-facts/DESIGN.md`).
- rejected: an agent proposed delivering a literal carrier as its equality alone; the owner rejected it, because it drops the edge's own bounds on other terms and gives an unreachable edge an image that is not neutral.
- replaced: delivering only the disequalities an edge image stores was on main; the owner changed it, because two edges bounding the receiver on either side of a term delivered none (data: `research/investigations/proof-certificate-architecture/INCREMENTAL-CLOSURE.md`).
- rejected: an agent proposed storing in an edge image every disequality the carrier's zero bound implies; the owner rejected it, because a constant far outside the other terms' ranges would hold a row for every registered term.

### decision delivery-tests-redundancy-by-derivability
Bounded relation delivery judges a substituted candidate redundant by exact derivability from implicit facts, the cached closures of standing measures included.
- replaced: judging candidates by the ancestry of their chosen proofs was on main; the owner changed it, because ENT-5 selects relations from the closed state, and DIAG-2 lets a redundant delivery root be absent.

### decision checked-matches-mark-their-selector
Checked matches tell an enum selector from an integer selector, and each arm carries an explicit variant, integer value or remaining-alternatives selection.
- rejected: an agent proposed giving integer arms synthetic enum tags; the owner rejected it, because the consumers must not mistake an integer command code for a payload tag.
- rejected: an agent proposed a second statement walk for integer matches; the owner rejected it, because the body, ownership and value-delivery paths can stay shared.

### decision integer-exclusions-keep-one-exact-set
Integer match exclusions keep one exact expression identity and one sorted set of integer values in the fact state, with their source admission views kept apart.
- rejected: an agent proposed a vector of established disequalities; the owner rejected it, because ENT-3.S17 supplies one complement fact.
- rejected: an agent proposed taking an admission view's id as the fact's exact identity; the owner rejected it, because two written views can name the same value without giving the same numeric projection.

### decision exclusion-tightening-walks-whole-runs
Exclusion endpoint tightening steps through a whole run of consecutive excluded integers before it publishes its final bound and closes again.
- rejected: an agent proposed a full closure for every excluded integer at the endpoint; the owner rejected it, because each step has the same premises, and the closures in between add no premise the next step needs.

### decision atomic-roots-keep-one-substitution
Atomic root comparisons keep one substitution across both state types and carry their possible aliases through every resolved-place projection.
- rejected: an agent proposed treating each occurrence of a repeated parameter as an independent wildcard; the owner rejected it, because repeated parameters must name one argument.
- rejected: an agent proposed dropping aliases when loop or window roots are taken out; the owner rejected it, because a prefix stays part of the same storage.
- rejected: an agent proposed atomic header provenance beside reference paths; the owner rejected it, because independent targets name map states directly, so no grant follows aliases.

### decision unresolved-const-expressions-may-be-equal
Unresolved symbolic const expressions compare as possibly equal in the atomic root judgment.
- rejected: an agent proposed taking different expression syntax as distinct state types; the owner rejected it, because no arithmetic unification judgment proves them distinct.

### decision rebinding-prescan-walks-the-control-graph
The reference-rebinding pre-scan builds the finite source control graph before binding allocation and walks predecessors back from the target iteration header.
- rejected: an agent proposed keeping rebindings only when a nested construct's ordinary suffix reaches the header; the owner rejected it, because a labeled continue can reach an outer header through a nested loop whose ordinary successor returns.

### decision loops-keep-their-normal-continuation
The checked ordinary-loop node keeps the normal-continuation result its resolved break transfers give, for proof reachability and lowering.
- rejected: an agent proposed finding loop exits again from label spellings; the owner rejected it, because these consumers must agree with function completion and statement reachability, even for breaks in nested loops.

### decision goal-data-carry-written-admission-views
A goal datum carries the place as written as an admission annotation outside its typed identity, and the goal table keeps one canonical goal with one view per annotation.
- rejected: an agent proposed refusing the goal datum of an inadmissible indexed field outright; the owner rejected it, because that removes the value identity a guard on a writable indexed field and a readonly field reached by struct equality need.