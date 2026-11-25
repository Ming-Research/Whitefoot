# Certificate fold

How a written certificate scaled by a named multiplicity folds its degree-two products back into affine terms.

### decision fold-matches-by-declaration
A certificate's nonlinear monomials fold to admitted multiplications matched by declaration, with degree at most two, and any monomial left unfolded rejects.
- rejected: an agent proposed folding by the value an operand is known to equal; the owner rejected it, because `let stride = width + padding;` gives the product and the certificate different arithmetic at the fold, so only a stride copied straight from a parameter matched.

### decision fold-handle-only-at-fold
A binding used in a fold is an opaque handle only between the fold and the residual, and stands for its known value everywhere else.
- rejected: an agent proposed the opaque handle as the binding's form for every reader; the owner rejected it, because it closed the fold but broke ordinary premises about the binding, moving the failures without changing what was accepted.

### decision unsigned-multiplicities-only
A named multiplicity must have an unsigned type.
- rejected: an agent proposed signed multiplicities; the owner rejected it, because a signed one would need its own nonnegativity obligation at the scaling step, and no program checked so far has needed one.

### decision one-monomial-per-fold
A product operand may itself be a product, and each fold removes one finite monomial.
- rejected: an agent proposed expanding product operands; the owner rejected it, because expanding can exceed degree two or replace a provable affine residual with one that cannot be proved.