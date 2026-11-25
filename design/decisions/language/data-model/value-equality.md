# Value equality

What `==` and `!=` compare, how floats and generic code stand to them, and how proofs read them.

### decision equality-types-follow-structure
`==` and `!=` are one value equality over the equality types, decided from a type's structure: integers, `unit`, and enums, structs and `Array<T, N>` whose parts are all equality types.
- rejected: an agent proposed an equality that each type declares, like Rust's derive; the owner rejected it, because downstream code compares almost every struct field by field, so every type would need an annotation and the language a new declaration form.
- rejected: an agent proposed content equality of boxes and runtime-sized sequences in the same step; the owner rejected it, because whether capacity, a ring's start and segment boundaries count was not settled, and it doubled the scope.
- rejected: an agent proposed leaving opaque structs out of the equality types; the owner rejected it, because `Array` is itself opaque and comparable, and equality reveals only whether two values are the same.

### decision floats-are-not-equality-types
Floats and every type with a float part are not equality types, and floats keep `feq` and its family.
- rejected: an agent proposed comparing float parts by their bits; the owner rejected it, because +0 and -0 would differ and so would NaN payloads, which disagrees with `feq`, and one `==` would mean different things on floats and integers.
- rejected: an agent proposed comparing float parts by IEEE equality, as Go and Swift do; the owner rejected it, because `==` would then not be reflexive, so proofs could not substitute one side for the other.

### decision tag-only-enums-compare-with-value-equality
Tag-only enums, `Bool` included, compare with `==` and `!=`, and `eeq` and `ene` are retired.
- replaced: Comparing tag-only enums with `eeq` and `ene` beside `==` was on main; the owner changed it, because a tag-only enum is an equality type with no payload, so keeping them would leave two spellings of one equality.
- rejected: an agent proposed converting a tag-only discriminant to an integer and comparing the integers; the owner rejected it, because it exposes the tag representation, invites integer operations and ordering, and widens the conversion proof surface.
- rejected: an agent proposed a pure source mapper per type to integer codes with a separately reviewed injectivity guard; the owner rejected it, because it duplicates a per-type ordinal convention and needs a guard that exhaustiveness alone cannot discharge.
- rejected: an agent proposed exhaustive nested matches over every ordered pair of variants; the owner rejected it, because the repeated identity test would have needed a projected 6,952 source-level variant-pair arms.

### decision proofs-read-equality-by-its-definition
The checker reads a value equality as the executable operation computes it: an established struct equality gives its field equalities, and a struct equality goal holds when every field equality does.
- rejected: an agent proposed reading value equality in proofs as an opaque predicate; the owner rejected it, because it gives no field fact, so a proof that needs one, such as that a filled array field is zero, cannot be made.
- rejected: an agent proposed expanding `==` into integer projections only inside generic range clauses; the owner rejected it, because proofs would then hold an equality that the executable language does not have.

### decision generic-equality-uses-the-eq-bound
A type parameter reaches `==` only through the built-in bound `Eq`, which admits exactly the equality types and implies copy, or through `Int`.
- rejected: an agent proposed `==` on every copy type, with floats compared by total order; the owner rejected it, because a float would then have two equalities that disagree on NaN and signed zero.
- rejected: an agent proposed `==` on a `copy` type parameter checked only at each instantiation, as Zig does; the owner rejected it, because the generic body would go unchecked and the error would land at a caller.
