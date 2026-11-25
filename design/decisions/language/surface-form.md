# Surface form

How Whitefoot source is written: one spelling for each construct, the form of computation, binders, callable boundaries, loops and numeric literals.

### decision one-spelling-per-construct
Each construct has one spelling and one byte-level formatting, counted per construct, and non-canonical input is rejected rather than reformatted.
- rejected: an agent proposed counting one spelling per value, which would allow only one of `'a'_u8` and `97_u8`; the owner rejected it, because a text literal denotes text and a numeric literal a number, and forcing one where the other is meant makes either read as the wrong construct.
- rejected: an agent proposed relief by position, an element forbidden wherever the checker can rebuild it and required elsewhere; the owner rejected it, because legality must follow from the grammar class alone, and a rule keyed on inference makes the writer simulate the checker.

### decision no-comments-only-doc-fields
There are no comments, and documentation lives in a declaration's `doc` field.
- rejected: an agent proposed free-form comments; the owner rejected it, because a comment is text the checker cannot see, while a doc field is a checked part of the declaration.

### decision computation-is-three-address-form
Computation is flat three-address form: one operation per expression, with every intermediate named by a binding.
- rejected: an agent proposed nested expressions with precedence; the owner rejected it, because the flat form is performance-neutral by measurement and removes precedence, nesting and lookahead from the language.

### decision body-binders-are-never-annotated
A body binder's mode and type come from its self-typed right-hand side and are never written, while callable parameter and result types stay written.
- rejected: an agent proposed annotating body binders; the owner rejected it, because the annotation repeats what is already known at the declaration.

### decision boundaries-write-no-own-keyword
Callable boundaries write value parameters as `name: T`, references as `name: &T` or `name: &[T]`, and results as owned types with no qualifier.
- replaced: A written `own` keyword on value parameters and results was on main; the owner changed it, because the reference marker alone decides a parameter's kind, and references cannot be results, so `own` marked no further difference.

### decision two-iteration-forms
Iteration has two forms: an ordinary loop with an optional label, and an ascending half-open counted loop whose binder the compiler updates and the source cannot assign.
- replaced: `loop` with `break` as the only iteration form was on main; the owner changed it, because every bounded walk spelled its own counter, guard and increment, and three real SHA-256 index walks and three of four hostile writer probes each chose the same counted shape.

### decision numeric-literals-carry-a-suffix
Every numeric literal carries a type suffix; the generic identities `0_T` and `1_T` under a numeric bound are the only relief.
- rejected: an agent proposed inferring a literal's type from its context; the owner rejected it, because the language is as explicit as it can be by design.
