# Generic validation

How the checker validates generic code: the inventory its symbolic and concrete checks share, which symbolic bodies it analyzes, and how generic instances, contracts and provenance are judged.

### decision validation-shares-one-grow-only-inventory
Symbolic validation, selector preflight and concrete checking use views over one structurally keyed, grow-only type and function inventory within a check.
- replaced: a scratch inventory discarded after validation, with bodies replayed to rebuild its identities, was on main; the owner changed it, because its rollback and structural type mirror forced repeated construction.
- rejected: an agent proposed reusing an analysis because its function id is unchanged; the owner rejected it, because the symbolic context and the consumed callee claims can differ even for a shared declaration or instance.

### decision symbolic-entailment-covers-postcondition-components
Symbolic entailment analyzes only the postcondition components of canonical generic bodies and, transitively, the callee components that contain a postcondition.
- replaced: analyzing every body in symbolic validation was on main; the owner changed it, because judged bodies read other analyses only through callee summaries, and the concrete phase analyzes the rest again (data: `research/investigations/proof-certificate-architecture/CHECKING-COST.md`).
- replaced: analyzing every component the canonical bodies reach was on main; the owner changed it, because a component without a postcondition publishes no summary, and leaving them out removed 315 of 1227 analyses of Halo's `pkg::vm` (data: `research/investigations/compile-speed/DESIGN.md`).
- rejected: an agent proposed leaving every nongeneric function out of validation; the owner rejected it, because a judged generic body can discharge an obligation only through a reachable callee's published postcondition.

### decision renamed-symbolic-instances-take-canonical-outcomes
A symbolic instance that renames its declaration's canonical instance one to one forms only its contract and takes the canonical instance's body disposition, invariants and postcondition proofs.
- replaced: checking and analyzing such an instance's body again was on main; the owner changed it, because that took 95% of the body-check time of Halo's `pkg::vm`, and the analyses alone exhausted 12 GB (data: `research/investigations/compile-speed/DESIGN.md`).

### decision symbolic-publication-starts-at-canonical-instances
Symbolic publication starts from each canonical generic instance and admits every clause expressible at its selected shape, results with a symbolic type included.
- rejected: an agent proposed deferring every obligation or clause because its enclosing function is generic; the owner rejected it, because a generic caller needs its callees' verified summaries just as a concrete caller does.

### decision operand-directed-generics-use-a-fixed-table
A fixed declaration table says which operand supplies each parameter of an operand-directed generic and which type component it selects, read by a type-only place walk.
- rejected: an agent proposed inferring the parameter from a value argument; the owner rejected it, because the chosen window defines its element type, and a mismatched value argument would change the intended instance.
- rejected: an agent proposed keeping anonymous signatures for each call; the owner rejected it, because lowering cannot resolve them.
- rejected: an agent proposed enumerating every operand-directed instance over every shape a unit names; the owner rejected it, because operations over several windows make an unused cross product, while the body checker already finds the selected operand types.

### decision type9-placement-is-checked-before-and-after-substitution
Written TYPE-9 placement is checked apart from inline storage after substitution, which checks formed own parameters and nominal fields and payloads again.
- rejected: an agent proposed enforcing both boundaries from the written syntax alone, or from the formed types alone; the owner rejected it, because generic indirection can bring in a forbidden representation without writing that shape at the storage site.

### decision contract-implication-reuses-ordinary-queries
FN-4 contract implication runs S4/MSR-4/AUTO in isolated declaration-local queries with their own entry, exit, result and payload identities and their own derivations.
- rejected: an agent proposed a second solver for declaration-contract implication; the owner rejected it, because S4/MSR-4/AUTO already fixes the admitted proof family.
- rejected: an agent proposed sharing identities between the independent queries; the owner rejected it, because the hypothetical premises of an implication must not become facts of callable bodies.

### decision generic-provenance-is-indexed
Generic argument provenance is indexed by the ordered substitution without its region axis and by parameter key, keeping the minimum source-node index of each pair.
- replaced: scanning every recorded site at registration and lookup was on main; the owner changed it, because Halo's profile found repeated linear provenance searches (data: `research/investigations/check-time/DESIGN.md`).