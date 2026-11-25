# Construction form

How operands are named: in construction, in match binders, in call arguments, and in a destructuring consume.

### decision operands-are-named-in-order
Construction, match binders and ordinary call arguments name every field in declared order, and a missing, extra, repeated, misspelled or out-of-order label is rejected.
- rejected: an agent proposed positional construction, binders and operands; the owner rejected it, because labels in declared order catch a label or order mismatch even between operands of the same type, which positional operands cannot show.

### decision even-one-field-is-named
Naming applies even to a variant with a single payload field.
- rejected: an agent proposed naming fields only when two of them share a type; the owner rejected it, because such a rule flips when a field is added, and irregularity, not verbosity, is what breaks writers.

### decision a-consume-may-end-with-rest
A destructuring consume may end its binders with `..`, which stands for the fields it does not name; each such field must be copy or affine, and the compiler releases it there.
- replaced: Naming every field of a consumed owner was on main; the owner changed it, because a binder for each released field would be a name nothing reads, and a marker that binds nothing allows none of the silent transposition that rules out positional binders.
- rejected: an agent proposed the rest marker in construction and call arguments too; the owner rejected it, because there an unnamed field has no value to take, while in a consume it has a value and a defined fate, which is release.
