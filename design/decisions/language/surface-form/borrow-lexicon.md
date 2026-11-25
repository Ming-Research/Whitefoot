# Borrow lexicon

The words and marks that spell references: surface names, the reference marker, range references and the referent form `place^`.

### decision surface-names-label-invariants
Surface names label invariants the specification defines and never borrow backend vocabulary.
- rejected: an agent proposed backend terms at the surface, such as `noalias`; the owner rejected it, because such a name states a consequence for lowering, not the invariant the checker enforces.

### decision references-carry-no-marker
A reference is written `&place` with no permission, exclusivity or lifetime marker, and whether a callee may write through it is read from the declared effect row.
- replaced: A marker on the reference, the exclusive mode `&uniq`, was on main; the owner changed it, because references carry no mode any more, write permission is declared once per parameter in the effect row, and exclusivity follows from comparing one call's effect paths.

### decision range-references-are-spelled-amp-brackets
A reference to a run of positions in one storage is spelled `&[T]` and has no nominal type name of its own.
- replaced: Naming range views `Slice<T>` and `MutSlice<T>` was on main; the owner changed it, because the two names stood for two loan strengths of a value that could be copied, stored and returned, and a range reference is now a local name with none of that.

### decision caret-reaches-a-referent
`place^` spells the referent of a reference of type `&T` or `&[T]` and nothing else, joining fields and subscripts as `p^.field` and `p^[i]`.
- replaced: The prefix access form `deref(p)` was on main; the owner changed it, because `^` is one explicit step, one byte, for both whole access and the selectors after it.
- rejected: an agent proposed C-like access, `*p` and `p->field`; the owner rejected it, because it needs separate whole, member and indexed forms, and changes to the lexical and indexing rules.
- rejected: an agent proposed a prefix star or a prefix dollar; the owner rejected it, because either needs a binding rule against field and index selection, and a high-binding star would reverse C's precedence.
