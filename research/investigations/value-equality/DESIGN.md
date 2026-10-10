# Value equality

Status: the owner ruled on all six choices on 2026-10-10 (status board item
"值类型的 == 完整支持"); the selected design is specified in v0.120 as [OP-16]
and its consumers, and implemented on the branch of pull request #323.
Specification references below are to v0.119, the version this record
investigated.

## Rulings

Each choice took the recommended option, with two changes to the proposed
direction below:

- Opaque structs that remove no capability are equality types. The proposal
  excluded them, but `Array` is itself opaque and comparable, and equality
  reveals only whether two values are the same.
- The generic bound is spelled `Eq`, like `Int` and `Float`, rather than a
  new lowercase word, so no identifier becomes reserved. A parameter bounded
  `Int` also admits `==`.

The other rulings match the proposal: floats are not equality types,
`eeq`/`ene` retire into `==`/`!=`, proofs decompose a value equality by its
definition, and #323 was rewritten in place into the implementation.

## Question

Whitefoot has no equality on structs, payload enums or Arrays. `==` and `!=`
compare two values of one integer type [OP-1, OP-7], tag-only enums
(including `Bool`) compare with the separately named `eeq`/`ene` [OP-8], and
floats with the IEEE `feq`/`fne` [OP-3, OP-8]. Writers compare aggregates
with hand-written helpers, and a generic contract such as `array_filled`'s
`result[k] == value` states nothing when the element is a struct [RANGE-1].

Pull request #323 let `==` between two values of one copy struct, enum or
Array appear in generic range clauses only, read there as the conjunction of
their integer projections. That gives proofs an equality the executable
language lacks. The question is the complete design:

1. which types `==` compares, and by what rule;
2. what it means on floats, references, handles, boxes and containers;
3. whether `eeq`/`ene` remain;
4. how generic code reaches it;
5. how the checker's proofs use the same definition the executable code
   computes; and
6. what becomes of #323.

## Current state

Equality and comparison today, by rule:

| Rule | What it says about equality |
|---|---|
| OP-1, GRAM-5, GRAM-6 | The six comparison symbols are integer-only table rows; a symbol resolves to exactly one row; nothing is overloaded. The `i`-prefixed names are retired. |
| OP-7 | Prefixes separate domains (`f` float, `b` Bool logic, `e` tag-only enum); the symbols are the one prefix-free class and are integer-only. |
| OP-8 | `eeq`/`ene` compare declared-variant identity of one exact nominal tag-only enum; `feq` is ordered IEEE equality, `fne` its complement. |
| FN-8 | A requirement may be any pure total Bool operation over admitted datums, so `requires eeq(a, b)` and `requires feq(a, b)` already relate non-integer values as opaque predicates; a call to a hand-written helper is not admitted. |
| FN-9, CALL-4 | Postcondition relations compare integer terms (fragment integers and measures, through fields and `Box` `inner`). |
| ENT-2 to ENT-4 | Integer equality is a pair of difference bounds; `Bool` facts are signed exact goals; `eeq` is an opaque goal with no L0 relation. |
| INV-1, PRF-1 | Invariant `==` is a proof-domain integer equality. |
| RANGE-1 to RANGE-3 | Range relations compare mathematical integer terms; at a concrete non-integer instance of a generic clause the clause states nothing. |
| GRAM-4, GRAM-10 | Patterns select variants and bind payloads; there are no literal or value patterns, so matching invokes no equality. |
| GRAM-2, FN-2 | A type parameter has at most one bound of `Int`, `Float`, `copy`, `drop`; there is no equality bound and no trait system. Behavior is supplied by explicit function-kind parameters (the standard `HashMapKey` interface declares an `equal` callback). |

Lowering: structs are LLVM structs in field order with target padding; fixed
Arrays are `[N x T]`; tag-only enums are `i1` or `i32`; payload enums use a
product layout or a per-variant union layout. Construction zero-initializes
in most paths, but no rule makes padding or inactive union bytes canonical
after every producer, so a bytewise comparison is not a valid definition
(compiler/src/backend/emitter/places.rs:384-489, :1009-1051;
compiler/src/target.rs:590-627).

## Demand

A lexical screen for functions named `*eq*`, `*equal*` or `*same*` that
return `Bool` found 9 in `tests/programs`, 5 in conformance cases, 6 in
Halo-wf, 7 in Firn-wf and 74 in Snowghost-wf (definitions, not distinct
needs). Snowghost's `renderer/style/intern.wf` alone has 11 whole-struct
comparators, each comparing every field with `==`, `eeq` and `feq` joined by
`band`; `renderer/layout/structure.wf:94` compares two payload enums by
nested `match`; Firn compares fixed `Array<u8, 40>` values and byte windows
by hand-written loops. Snowghost's incremental investigation records the
missing aggregate equality as a gap
(Snowghost-wf research/investigations/incremental/DESIGN.md:653-657).

Not every comparator is structural: the owning hash-map test compares only
child identifiers, and one test comparator is deliberately non-reflexive. A
structural `==` does not replace such application-defined relations; they
stay ordinary functions passed where a callback is wanted.

`eeq`/`ene` appear 37 times in this repository (9 files), 38 times in
Halo-wf (4 files), 267 times in Snowghost-wf (91 files) and not in Firn-wf.

## Other languages

| Language | Which types have `==` | Floats inside aggregates | References and handles | Generic access |
|---|---|---|---|---|
| Go | automatic structural rule ("comparable": a struct if all fields are, an array if its element is) | IEEE, so a struct holding NaN is unequal to itself | pointers and channels by identity; interfaces may panic | `comparable` constraint, which can still panic |
| Rust | opt-in `derive(PartialEq)`, or a hand-written impl | `PartialEq` without `Eq` | `&T` compares referents; raw pointers by address | `T: PartialEq` bound |
| Swift | `Equatable` conformance, synthesized for structs and enums | IEEE, with a NaN exception written into the protocol | `===` for class identity | `T: Equatable` |
| C++20 | `operator== = default`, memberwise in declaration order | IEEE | pointers by address | concepts |
| Zig | scalars and packed structs only | IEEE | address | comptime, errors at instantiation |
| Ada 2012 | predefined `=` on every nonlimited type; limited types (files, tasks) have none | numeric | access values by designation | generic formal `"="` |
| OCaml | structural `=` on every type | IEEE `=`, total `compare` | `==` is physical identity | none needed; functions raise at run time |
| Dafny | all types in specifications; compiled `==` only on `T(==)` types | n/a | class identity | `T(==)` |
| SPARK | Ada `=`; contracts use the executable `=` | Ada | equality on owning access types refused | Ada |

Lessons, with the semantic difference recorded before borrowing anything:

- Decide comparability from type structure at compile time (Go, Swift and
  Rust synthesis, Dafny `T(==)`). Go's rule differs in admitting IEEE floats,
  identity on pointers and channels, and a run-time panic through
  interfaces; Whitefoot has no run-time trap, so none of these carry over.
- One definition in code and in contracts is the SPARK model. Dafny's ghost
  equality on every type, and the F*/Lean split between a logical and an
  executable equality, are the proof-only equality this design refuses.
- IEEE equality is not reflexive, so an aggregate `==` that compares floats
  by IEEE rules cannot be used as an equivalence in proofs. Rust excludes it
  from `Eq`; Java value classes compare float bits; Java records compare by
  total order.
- Identity and content need different spellings (Swift `===`, OCaml `==`
  and `=`); Haskell's identity `Eq` on `IORef` is the counterexample to
  avoid.
- User-defined equality that stands in for the structural one breaks
  composition (Ada's re-emerging predefined `=`, fixed for records only in
  Ada 2012) and is unchecked in Rust and Haskell.
- When equality is pure and total, comparison order is unobservable, so the
  definition need not fix one (Ada); Go and C++ fix it only because a
  comparison can panic or run user code.

## Proposed direction

The cards below decide each choice; this section states the recommended
combination as one design.

**Equality types.** A type is an equality type when it is:

- an integer type;
- `unit`;
- an enum, `Bool` and `Option`/`Result` instances included, every payload
  field of which has an equality type;
- a source struct that is not `opaque` and does not remove `copy`, every
  field of which has an equality type; or
- `Array<T, N>` with T an equality type.

Every equality type is copy [OWN-1]. Floats, `Box`, runtime-sized storage
(`Array<T>` in a `Box`, `Slots`, `Ring`, `Segments`, `Paged`), shared state,
host handles, opaque and `nocopy` structs, reference kinds and function
kinds are not equality types.

**Meaning.** For two values of one equality type, `a == b` is `True()`
exactly when:

- for an integer, the two values are equal;
- for `unit`, always;
- for an enum, both hold the same declared variant and each pair of
  corresponding payload fields is equal;
- for a struct, each pair of corresponding fields is equal; and
- for `Array<T, N>`, each pair of corresponding elements is equal.

`a != b` is its complement. Both are pure and total and evaluate no user
code; the order in which parts are compared is not observable and is not
specified. This relation is an equivalence: reflexive, symmetric and
transitive. `<`, `<=`, `>`, `>=` stay integer-only.

**One spelling.** `==`/`!=` replace `eeq`/`ene`, which retire: a tag-only
enum is an equality type whose values have no payload. The earlier decision
that gave tag-only equality its own name rejected widening an *integer*
comparison; the symbols are now the value-equality symbols, prefix-free
because the meaning (same value) is one across the domain, while the
ordering symbols keep the integer-only rule of OP-7.

**Generic code.** A new bound `eq` admits `==` and `!=` on a type parameter
in code and contracts and implies `copy`; an instantiation with a type that
is not an equality type is refused where it is written. A contract clause of
a generic function whose parameter is only `copy` may still relate values of
that parameter with `==`; at a concrete instance whose type is an equality
type the clause states that executable equality, and at any other instance
it states nothing, as RANGE-1 already does for non-integer instances. This
keeps `array_filled<T: copy>`'s content fact for structs without splitting
the fill constructors by bound.

**Proofs.** The checker reads `a == b` by the same definition:

- an established `a == b` at a struct, enum or Array type yields the
  equality of each part (an enum's tags equal, each payload pair equal where
  the tag is that variant, each element pair equal);
- a goal `a == b` holds when every part's equality holds; and
- `a != b` stays an opaque signed goal, since its decomposition is a
  disjunction.

In ordinary entailment the parts are integer relations and Bool goals; in the
range judgment they are the projected reads #323 already forms. An Array's
element equalities enter the range judgment as one element-wise fact over
its index rather than N separate facts, so a large Array costs one clause,
not N literals.

**Lowering.** Fieldwise comparison, element loops for Arrays, and a tag test
before payloads; the backend may lower a type whose representation has no
padding and only integer leaves to a block comparison, as an optimization
proven equivalent per type, never as the definition.

## Specification impact

| Rule | Change |
|---|---|
| OP-1, OP-7, GRAM-6 | `==`/`!=` rows over equality types; the integer-only convention narrows to the four ordering symbols; `eeq`/`ene` rows retire. |
| OP-8 | The tag-only equality text becomes the enum case of the value-equality definition. |
| New rule (OP family) | The definition of equality types and of `==` above, stated once and cited by every consumer. |
| GRAM-2, FN-2 | The `eq` bound and its implication of `copy`; instantiation checking. |
| FN-8, FN-9, CALL-4, INV-1 | Requirements, postconditions and invariants may relate equality-type values with `==`; postcondition relation data widen to such values. |
| ENT-2 to ENT-4 | Decomposition of an established aggregate equality and the conjunctive goal rule. |
| RANGE-1 to RANGE-3 | A range relation `==` over equality-type terms reads by the definition; replaces #323's RANGE-1 expansion. |
| PRE-1 | Prelude signatures; the fill constructors' content facts read through the definition. |

## Evidence plan

Before implementing, record for each card:

- the proposition the implementation must make true;
- the comparison that could show the choice wrong; and
- the result that would reject it.

- **Check cost.** Decomposing an established equality must not multiply
  check time on large values. Compare the checker's work counters and wall
  time on a program establishing and using `a == b` for `Array<u8, 4096>`
  and for a 64-field struct. The design is rejected if either needs time
  that grows with N per use rather than per clause, beyond the gate budget.
- **Runtime cost.** Generated code for `==` on a struct of integers and on
  `Array<u8, 40>` must compare with the hand-written comparators it
  replaces. Use a paired measurement on the 14900K through CI. A slowdown
  beyond noise rejects the lowering, not the semantics.
- **Migration.** Respelling `eeq`/`ene` must stay mechanical: every one of
  the 342 uses must become `==`/`!=` with the same verdicts. The release
  note gives downstream sessions the rule.
- **Proof reuse.** At least one existing downstream comparator (Snowghost's
  `Sizing` or a Firn fixed-array key) must be replaceable by `==`, with the
  facts the caller needs still provable. If none is, the proof
  decomposition is too weak.

## Open questions

- Content equality through references and `Box`, and of runtime-sized
  sequences (`&[u8]` byte windows, Firn's most common comparator), is
  deferred. It needs choices about capacity, origin and segment boundaries.
  Reopen when a downstream program needs `==` on a run or a boxed value;
  the board backlog records it.
- Identity comparison of shared objects and host handles is not proposed.
- Whether a struct may opt out of structural equality (its fields equal but
  its values meant to differ) is not proposed. The `opaque` and `nocopy`
  exclusions cover the representation and resource cases found so far.
