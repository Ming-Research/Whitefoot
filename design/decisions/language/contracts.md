# Contracts

How generic code receives behavior such as hashing and comparison, and the callable boundary an interface member's row and contract set.

### decision behavior-as-function-kind-arguments
A container receives hash, equality and comparison as explicit function-kind arguments, with interface and binding groups that abbreviate their signatures and arguments.
- rejected: an agent proposed runtime dictionaries or callable values for container behavior; the owner rejected it, because the map and the queue need calls specialized to the selected behavior, with no runtime storage or dispatch.

### decision formal-member-sets-the-boundary
A formal member's row and contract are the callable boundary; an actual may refine it with a subset row, a weaker `requires` and each `ensures` met by its frame or its own stronger `ensures`.
- rejected: an agent proposed requiring the actual's row to equal the formal's; the owner rejected it, because an actual that touches only a declared subfield fits the caller's contract with no dummy reads or runtime adapter.
- rejected: an agent proposed deriving the caller's boundary from the selected body; the owner rejected it, because a generic caller must keep one written boundary it can read without knowing which implementation was selected.
- rejected: an agent proposed an implication solver or search to decide that an actual's `requires` is weaker and its `ensures` stronger; the owner rejected it, because a refinement admitted at a binding must be decided by a fixed finite check, not a search whose success selects acceptance.
- replaced: Requiring an owned result of a function formal to be fresh was on main; the owner changed it, because it rested on the removed history of owned values and kept ordinary functions that return ownership out of the common callable boundary.

### decision frame-discharges-unwritten-places
A binding discharges an interface `ensures` that one integer place or measure ends equal to its entry value when the actual's declared row writes nothing overlapping that place.
- replaced: Requiring the actual to state each such equality in its own `ensures` was on main; the owner changed it, because Halo's host call writes the stack while keeping its length, and a pure Stop implementation keeps that length with no entry clause it may write.
- rejected: an agent proposed letting implementations declare wider rows than their bodies show; the owner rejected it, because it weakens exact effects for every function only to express a guarantee the absence of writes already gives.
- rejected: an agent proposed making stubs perform a dummy write; the owner rejected it, because a conforming program would then change runtime state only to justify an erased frame guarantee.
- rejected: an agent proposed discharging frame equalities only for actuals that write nothing; the owner rejected it, because Halo's reads-only host function writes its environment and could not restate the guarantee, since an entry clause is admitted only for written parameters.

### decision frame-aliasing-judged-at-each-call
Aliasing between a framed place and another parameter's write is left to each call through the formal, whose contract-read footprint must separate the framed place from the other writes.
- rejected: an agent proposed requiring, at the binding, a formal access covering the framed place whenever the actual writes through another parameter; the owner rejected it, because each call's footprint already separates that place, so the condition only refused bindings no call could misuse.

### decision member-calls-use-the-formal
A call to a member uses the instantiated formal signature, row and contracts, while the selected actual is checked on its own.
- rejected: an agent proposed bringing an actual body's smaller row or stronger inferred facts into its callers; the owner rejected it, because a container's written boundary must not vary with which implementation is selected.

### decision supplied-behavior-has-no-laws
Supplied behavior carries no assumed equality, comparison or hashing laws.
- rejected: an agent proposed using an unchecked law as a safety premise; the owner rejected it, because hostile behavior must still keep ownership and proved bounds even when the container holds unexpected contents.

### decision member-references-through-effect-paths
A formal member relates references to its caller through substituted effect paths and optional `held` requirements, with no heap region, loan lifetime or store brand.
- rejected: an agent proposed attaching runtime region identities or storage brands to reference types; the owner rejected it, because references stay local path names in one heap, and a coherent observation needs only a finite caller-statement origin and protected paths.
- replaced: An effect row as the whole contract of a member was on main; the owner changed it, because reads describe accessed paths but cannot require that two arguments come from one caller-held statement and stay unchanged through it.