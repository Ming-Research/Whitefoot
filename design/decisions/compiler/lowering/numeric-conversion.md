# Numeric conversion

How the compiler carries a numeric conversion's mode from checking to lowering, and what each mode lowers to.

### decision a-conversion-keeps-its-mode-until-lowering
A conversion's mode and endpoint types are kept explicitly from semantic formation through goals and checked expressions, and its domain is checked in the existing entailment walk.
- rejected: an agent proposed recovering the mode from whether the type pair is total or from the Result layout; the owner rejected it, because exact, checked and domain-query forms share type pairs but have different obligations and result shapes.

### decision a-bare-conversion-lowers-under-its-domain-proof
A bare conversion lowers to the plain value operation under its domain proof, while checked and domain-query conversions share one validity test and only the checked form builds a Result.
- rejected: an agent proposed lowering every form through a checked Result, or a general family of backend assumptions for this operation; the owner rejected it, because removing a failure path that cannot happen should not depend on the optimizer noticing it.

### decision sufficient-conversion-bounds-normalize-only-positively
Sufficient conversion bounds have only a positive normalization, and a checked integer conversion's value takes its facts in the existing private Result payload.
- rejected: an agent proposed giving a sufficient interval an equivalent negation; the owner rejected it, because failing to prove a sufficient interval does not show the value unrepresentable.
- rejected: an agent proposed a new SET commit identity for the saved value; the owner rejected it, because a conversion result is not a storage assignment.

### decision a-wrapping-conversion-uses-the-shared-integer-cast
A wrapping conversion is its own mode and lowers through the shared integer cast, extending by the source's signedness.
- rejected: an agent proposed a second conversion pipeline for wrapping; the owner rejected it, because the existing cast already does truncation, sign and zero extension and same-width bit copies, and wrapping has no domain obligation or Result payload.
- rejected: an agent proposed a checked Result as wrapping's fallback; the owner rejected it, because wrapping has no domain obligation or Result payload.
- rejected: an agent proposed reusing the exact conversion's evidence for wrapping; the owner rejected it, because proofs keep a wrapped value's modular identity without the input being equal to it.

### decision a-rounding-conversion-emits-the-exact-float-sequence
A rounding conversion keeps its own mode through checked expressions and IR, and the emitter checks its float destination and emits the exact conversion's float sequence.
- rejected: an agent proposed a software rounding sequence or constrained intrinsics; the owner rejected it, because LLVM's plain conversions already round to nearest, ties to even, in the default floating-point environment, which compiled programs never leave.
- rejected: an agent proposed mapping the rounding mode to the exact mode before the IR; the owner rejected it, because a backend fact later drawn from an exact conversion's proved domain would then attach to a rounding conversion that has no domain proof.