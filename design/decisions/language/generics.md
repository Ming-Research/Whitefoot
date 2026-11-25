# Generics

Type, const and function parameters: how they are instantiated, bounded, grouped and checked.

### decision monomorphization-only-generics
Generics are expanded before IR from explicit type, const and function arguments, and each inhabited instance is checked as concrete code.
- rejected: an agent proposed inferred instantiation; the owner rejected it, because inference and bound solving add complexity and unpredictability to the trust path.
- rejected: an agent proposed runtime bodies that take dictionaries; the owner rejected it, because selected behavior needs direct calls.

### decision instantiation-cycle-arguments
Each argument on an instantiation cycle is the caller's parameter at the same position and of the same kind, forwarded unchanged, or a closed term with none of the caller's parameters in it.
- replaced: Requiring every edge on a cycle to forward the whole parameter vector unchanged was on main; the owner changed it, because it refused finite nested callbacks, such as Snowghost's changing callback and Firn's nested `hash_map_lookup`.
- rejected: an agent proposed admitting finite permutations through a more precise termination analysis; the owner rejected it, because same-position forwarding and closed terms establish finiteness directly, with no search for a terminating instance graph.
- rejected: an agent proposed a depth, time or fuel limit on instantiation; the owner rejected it, because a limit makes acceptance depend on a budget instead of a structural rule over the written program.

### decision type-arguments-hold-no-references
Type arguments to functions, nominals and interface groups contain no reference or range reference at any depth after substitution.
- rejected: an agent proposed an exclusion list of special leaf types; the owner rejected it, because the rule that values hold no references must stay closed under generic wrapping.
- rejected: an agent proposed refusing only instances that happen to store their argument; the owner rejected it, because the rule that values hold no references must stay closed under generic wrapping.

### decision bounds-are-capability-filters
A type parameter's bound says what its argument must be able to do: no bound requires nothing, `T: drop` requires a droppable type and `T: copy` a copyable one; a bound is never inferred.
- replaced: The class-named bounds `T: copy`, `T: affine` and `T: linear`, with a bound on every parameter, was on main; the owner changed it, because they read backwards: `T: linear` read as a demand that T be linear while it accepted every class.
- rejected: an agent proposed a word for the case of no capability, such as `T: any`; the owner rejected it, because the absence of a bound already says that nothing is required of the argument.

### decision interface-prefix-on-group-headers
Parameter groups are declared with `interface` and argument groups with `binding`, and a group introduced in a function or nominal header is prefixed `interface`.
- rejected: an agent proposed deciding whether a bare header name brings in a group by looking that name up; the owner rejected it, because an unbounded type parameter and a group with no parameters both spell `<F>`, and the grammar would then depend on name resolution.
- rejected: an agent proposed giving a group import priority over an unbounded type parameter where the grammar's lookahead overlaps; the owner rejected it, because parser priority would change a declaration's meaning by an unrelated declaration and leave `<T>` with no one syntactic meaning.

### decision group-uses-take-no-prefix
Forwarded groups, concrete argument groups and qualified member calls keep their argument-position spelling, with no `interface` prefix.
- rejected: an agent proposed a marker at every use of a group; the owner rejected it, because those positions declare no type parameters and have no ambiguity to resolve.

### decision type-argument-judged-at-each-placement
A type argument is judged at every placement its parameter takes in the instantiated declaration [TYPE-9, FN-2].
- rejected: an agent proposed refusing a restricted type merely because it is written as a type argument; the owner rejected it, because a generic reference or shared handle keeps the storage's admitted home, while a by-value parameter, result or field stores it.

### decision const-argument-type-and-range
A const argument's names must have exactly its parameter's integer type and its value must lie in that type's range, judged where it is written, or at each instance when it names a const generic.
- rejected: an agent proposed checking every const argument only at concrete instantiation; the owner rejected it, because a body accepted at its symbolic instance could then be refused at a caller's instance for a fault visible in the body.
- rejected: an agent proposed requiring a literal const argument to have exactly the parameter's type; the owner rejected it, because a const literal has no suffix to state a type, so every literal for a parameter narrower than u64 would be refused.
- rejected: an agent proposed restricting const parameters to u64; the owner rejected it, because a narrower written type is a range fact a body proves with, and the type check at the argument and the range check at each value already close the hole.
- rejected: an agent proposed admitting a name whose type's range lies within the parameter type's range; the owner rejected it, because it widens a typed value implicitly, which no value position admits.
- rejected: an agent proposed admitting an operation over names of any type for a u64 parameter; the owner rejected it, because `m + 0` would then widen a u8 parameter `m` that the exact-type rule refuses when written alone.
- rejected: an agent proposed admitting an operation over names only when every name and the parameter are u64; the owner rejected it, because it refuses natural forms such as `m / 2` for a u8 parameter `m`, whose value can never leave the range.